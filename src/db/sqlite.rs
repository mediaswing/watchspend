//! The default backend: a SQLite file in the user's data directory.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension as _, params};

use super::{CategoryTotal, Error, NewSpend, Result, SpendEntry, Store, clean_category_name};

pub struct SqliteStore {
    conn: Connection,
    path: PathBuf,
}

impl SqliteStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                Error::Backend(format!("Could not create {}: {e}", parent.display()))
            })?;
        }
        let conn = Connection::open(path)
            .map_err(|e| Error::Backend(format!("Could not open {}: {e}", path.display())))?;
        conn.pragma_update(None, "foreign_keys", "ON").ok();
        let mut store = Self {
            conn,
            path: path.to_path_buf(),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS categories (
                     id   INTEGER PRIMARY KEY,
                     name TEXT NOT NULL UNIQUE COLLATE NOCASE
                 );
                 CREATE TABLE IF NOT EXISTS spending (
                     id           INTEGER PRIMARY KEY,
                     category_id  INTEGER NOT NULL REFERENCES categories(id),
                     spent_on     TEXT NOT NULL,
                     amount_minor INTEGER NOT NULL,
                     currency     TEXT NOT NULL,
                     description  TEXT NOT NULL DEFAULT ''
                 );
                 CREATE INDEX IF NOT EXISTS spending_by_date ON spending(spent_on);",
            )
            .map_err(backend)
    }

    fn category_id(&self, name: &str) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM categories WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |row| row.get(0),
            )
            .optional()
            .map_err(backend)
    }
}

impl Store for SqliteStore {
    fn categories_with_totals(&mut self, year: i32, currency: &str) -> Result<Vec<CategoryTotal>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT c.name,
                        COALESCE(SUM(s.amount_minor), 0),
                        COUNT(s.id)
                   FROM categories c
                   LEFT JOIN spending s
                     ON s.category_id = c.id
                    AND s.currency = ?1
                    AND s.spent_on >= ?2 AND s.spent_on <= ?3
                  GROUP BY c.id, c.name
                  ORDER BY c.name COLLATE NOCASE",
            )
            .map_err(backend)?;

        let rows = stmt
            .query_map(
                params![
                    currency,
                    format!("{year:04}-01-01"),
                    format!("{year:04}-12-31")
                ],
                |row| {
                    Ok(CategoryTotal {
                        name: row.get(0)?,
                        total_minor: row.get(1)?,
                        entries: row.get(2)?,
                    })
                },
            )
            .map_err(backend)?;

        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(backend)
    }

    fn entries_in_other_currencies(&mut self, year: i32, currency: &str) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM spending
                  WHERE currency <> ?1 AND spent_on >= ?2 AND spent_on <= ?3",
                params![
                    currency,
                    format!("{year:04}-01-01"),
                    format!("{year:04}-12-31")
                ],
                |row| row.get(0),
            )
            .map_err(backend)
    }

    fn spending_in_year(&mut self, year: i32, currency: &str) -> Result<Vec<SpendEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT s.id, s.spent_on, c.name, s.amount_minor, s.description
                   FROM spending s
                   JOIN categories c ON c.id = s.category_id
                  WHERE s.currency = ?1
                    AND s.spent_on >= ?2 AND s.spent_on <= ?3
                  ORDER BY s.spent_on, c.name COLLATE NOCASE, s.id",
            )
            .map_err(backend)?;

        let rows = stmt
            .query_map(
                params![
                    currency,
                    format!("{year:04}-01-01"),
                    format!("{year:04}-12-31")
                ],
                |row| {
                    let date: String = row.get(1)?;
                    Ok((row.get(0)?, date, row.get(2)?, row.get(3)?, row.get(4)?))
                },
            )
            .map_err(backend)?;

        let mut entries = Vec::new();
        for row in rows {
            let (id, date, category, amount_minor, description) = row.map_err(backend)?;
            entries.push(SpendEntry {
                id,
                spent_on: parse_stored_date(&date)?,
                category,
                amount_minor,
                description,
            });
        }
        Ok(entries)
    }

    fn add_category(&mut self, name: &str) -> Result<()> {
        let name = clean_category_name(name)?;
        if self.category_id(&name)?.is_some() {
            return Err(Error::Rejected(format!("“{name}” is already a category.")));
        }
        self.conn
            .execute("INSERT INTO categories (name) VALUES (?1)", params![name])
            .map_err(backend)?;
        Ok(())
    }

    fn add_spend(&mut self, spend: &NewSpend) -> Result<()> {
        let category_id = self.category_id(&spend.category)?.ok_or_else(|| {
            Error::Rejected(format!("There is no category called “{}”.", spend.category))
        })?;
        self.conn
            .execute(
                "INSERT INTO spending (category_id, spent_on, amount_minor, currency, description)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    category_id,
                    spend.spent_on.format("%Y-%m-%d").to_string(),
                    spend.amount_minor,
                    spend.currency,
                    spend.description,
                ],
            )
            .map_err(backend)?;
        Ok(())
    }

    fn update_spend(&mut self, id: i64, spend: &NewSpend) -> Result<()> {
        let category_id = self.category_id(&spend.category)?.ok_or_else(|| {
            Error::Rejected(format!("There is no category called “{}”.", spend.category))
        })?;
        let changed = self
            .conn
            .execute(
                "UPDATE spending
                    SET category_id = ?1, spent_on = ?2, amount_minor = ?3,
                        currency = ?4, description = ?5
                  WHERE id = ?6",
                params![
                    category_id,
                    spend.spent_on.format("%Y-%m-%d").to_string(),
                    spend.amount_minor,
                    spend.currency,
                    spend.description,
                    id,
                ],
            )
            .map_err(backend)?;
        not_found_unless_changed(changed)
    }

    fn delete_spend(&mut self, id: i64) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM spending WHERE id = ?1", params![id])
            .map_err(backend)?;
        not_found_unless_changed(changed)
    }

    fn rename_category(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        let new_name = clean_category_name(new_name)?;
        let id = self
            .category_id(old_name)?
            .ok_or_else(|| Error::Rejected(format!("There is no category called “{old_name}”.")))?;
        if let Some(existing) = self.category_id(&new_name)?
            && existing != id
        {
            return Err(Error::Rejected(format!(
                "“{new_name}” is already a category."
            )));
        }
        self.conn
            .execute(
                "UPDATE categories SET name = ?1 WHERE id = ?2",
                params![new_name, id],
            )
            .map_err(backend)?;
        Ok(())
    }

    fn delete_category(&mut self, name: &str) -> Result<()> {
        let id = self
            .category_id(name)?
            .ok_or_else(|| Error::Rejected(format!("There is no category called “{name}”.")))?;
        self.conn
            .execute("DELETE FROM categories WHERE id = ?1", params![id])
            .map_err(|e| category_delete_error(name, e))?;
        Ok(())
    }

    fn describe(&self) -> String {
        format!("SQLite · {}", crate::config::tilde(&self.path))
    }
}

fn backend(e: rusqlite::Error) -> Error {
    Error::Backend(e.to_string())
}

/// A zero-row UPDATE/DELETE means the id was never there, or was already
/// gone — either way there is nothing to tell the two cases apart, and
/// nothing worth leaking either way.
fn not_found_unless_changed(changed: usize) -> Result<()> {
    if changed == 0 {
        return Err(Error::Rejected("That entry no longer exists.".to_owned()));
    }
    Ok(())
}

/// The foreign key from `spending` is what actually stops a category with
/// entries in it from being deleted; this turns that specific failure into
/// something worth reading rather than a raw constraint-violation message.
fn category_delete_error(name: &str, e: rusqlite::Error) -> Error {
    if let rusqlite::Error::SqliteFailure(inner, _) = &e
        && inner.code == rusqlite::ErrorCode::ConstraintViolation
    {
        return Error::Rejected(format!(
            "“{name}” still has spending entries — delete or move them first."
        ));
    }
    backend(e)
}

/// SQLite has no date type, so dates are kept as `YYYY-MM-DD` text. Anything
/// else in that column was not put there by this app.
fn parse_stored_date(text: &str) -> Result<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|_| Error::Backend(format!("A stored date is not a date: {text:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn store() -> SqliteStore {
        let dir = std::env::temp_dir().join(format!("gas-test-{}", std::process::id()));
        let path = dir.join(format!(
            "{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        SqliteStore::open(&path).expect("open temp database")
    }

    fn spend(category: &str, minor: i64, date: (i32, u32, u32)) -> NewSpend {
        NewSpend {
            category: category.to_owned(),
            spent_on: NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap(),
            amount_minor: minor,
            currency: "GBP".to_owned(),
            description: String::new(),
        }
    }

    #[test]
    fn totals_cover_the_year_and_nothing_else() {
        let mut s = store();
        s.add_category("Groceries").unwrap();
        s.add_spend(&spend("Groceries", 1000, (2026, 1, 1)))
            .unwrap();
        s.add_spend(&spend("Groceries", 250, (2026, 12, 31)))
            .unwrap();
        s.add_spend(&spend("Groceries", 9999, (2025, 12, 31)))
            .unwrap();

        let totals = s.categories_with_totals(2026, "GBP").unwrap();
        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].total_minor, 1250);
        assert_eq!(totals[0].entries, 2);
    }

    #[test]
    fn empty_categories_still_appear_with_a_zero() {
        let mut s = store();
        s.add_category("Travel").unwrap();
        let totals = s.categories_with_totals(2026, "GBP").unwrap();
        assert_eq!(totals[0].total_minor, 0);
        assert_eq!(totals[0].entries, 0);
    }

    #[test]
    fn category_names_are_unique_whatever_the_case() {
        let mut s = store();
        s.add_category("Bills").unwrap();
        assert!(matches!(s.add_category(" bills "), Err(Error::Rejected(_))));
        assert!(matches!(s.add_category("   "), Err(Error::Rejected(_))));
    }

    #[test]
    fn other_currencies_are_counted_but_not_totalled() {
        let mut s = store();
        s.add_category("Books").unwrap();
        s.add_spend(&spend("Books", 500, (2026, 3, 1))).unwrap();
        let mut euro = spend("Books", 700, (2026, 3, 2));
        euro.currency = "EUR".to_owned();
        s.add_spend(&euro).unwrap();

        assert_eq!(
            s.categories_with_totals(2026, "GBP").unwrap()[0].total_minor,
            500
        );
        assert_eq!(s.entries_in_other_currencies(2026, "GBP").unwrap(), 1);
    }

    #[test]
    fn spending_needs_a_category_that_exists() {
        let mut s = store();
        assert!(matches!(
            s.add_spend(&spend("Nowhere", 100, (2026, 3, 1))),
            Err(Error::Rejected(_))
        ));
    }

    #[test]
    fn updating_a_spend_entry_changes_it() {
        let mut s = store();
        s.add_category("Books").unwrap();
        s.add_category("Games").unwrap();
        s.add_spend(&spend("Books", 500, (2026, 3, 1))).unwrap();
        let id = s.spending_in_year(2026, "GBP").unwrap()[0].id;

        let mut corrected = spend("Games", 750, (2026, 3, 2));
        corrected.description = "Actually a game".to_owned();
        s.update_spend(id, &corrected).unwrap();

        let entries = s.spending_in_year(2026, "GBP").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].category, "Games");
        assert_eq!(entries[0].amount_minor, 750);
        assert_eq!(entries[0].description, "Actually a game");

        let totals = s.categories_with_totals(2026, "GBP").unwrap();
        assert_eq!(
            totals.iter().find(|c| c.name == "Books").unwrap().entries,
            0
        );
        assert_eq!(
            totals.iter().find(|c| c.name == "Games").unwrap().entries,
            1
        );
    }

    #[test]
    fn deleting_a_spend_entry_removes_it() {
        let mut s = store();
        s.add_category("Books").unwrap();
        s.add_spend(&spend("Books", 500, (2026, 3, 1))).unwrap();
        let id = s.spending_in_year(2026, "GBP").unwrap()[0].id;

        s.delete_spend(id).unwrap();

        assert!(s.spending_in_year(2026, "GBP").unwrap().is_empty());
        assert_eq!(
            s.categories_with_totals(2026, "GBP").unwrap()[0].total_minor,
            0
        );
    }

    #[test]
    fn editing_or_deleting_a_missing_entry_is_rejected() {
        let mut s = store();
        s.add_category("Books").unwrap();
        assert!(matches!(
            s.update_spend(9999, &spend("Books", 100, (2026, 3, 1))),
            Err(Error::Rejected(_))
        ));
        assert!(matches!(s.delete_spend(9999), Err(Error::Rejected(_))));
    }

    #[test]
    fn renaming_a_category_keeps_its_spending() {
        let mut s = store();
        s.add_category("Books").unwrap();
        s.add_spend(&spend("Books", 500, (2026, 3, 1))).unwrap();

        s.rename_category("Books", "Reading").unwrap();

        let totals = s.categories_with_totals(2026, "GBP").unwrap();
        assert_eq!(totals.len(), 1);
        assert_eq!(totals[0].name, "Reading");
        assert_eq!(totals[0].total_minor, 500);
    }

    #[test]
    fn renaming_into_an_existing_name_is_rejected() {
        let mut s = store();
        s.add_category("Books").unwrap();
        s.add_category("Games").unwrap();
        assert!(matches!(
            s.rename_category("Books", "Games"),
            Err(Error::Rejected(_))
        ));
    }

    #[test]
    fn deleting_an_empty_category_succeeds() {
        let mut s = store();
        s.add_category("Books").unwrap();
        s.delete_category("Books").unwrap();
        assert!(s.categories_with_totals(2026, "GBP").unwrap().is_empty());
    }

    #[test]
    fn deleting_a_category_with_entries_is_rejected() {
        let mut s = store();
        s.add_category("Books").unwrap();
        s.add_spend(&spend("Books", 500, (2026, 3, 1))).unwrap();
        assert!(matches!(
            s.delete_category("Books"),
            Err(Error::Rejected(_))
        ));
        // Refusing to delete it must not have deleted it halfway.
        assert_eq!(s.categories_with_totals(2026, "GBP").unwrap().len(), 1);
    }
}
