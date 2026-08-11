//! The default backend: a SQLite file in the user's data directory.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension as _, params};

use super::{CategoryTotal, Error, NewSpend, Result, Store, clean_category_name};

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

    fn describe(&self) -> String {
        format!("SQLite · {}", crate::config::tilde(&self.path))
    }
}

fn backend(e: rusqlite::Error) -> Error {
    Error::Backend(e.to_string())
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
}
