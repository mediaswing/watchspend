//! The optional backend: a MariaDB (or MySQL) server somewhere on the network.

use std::time::Duration;

use chrono::NaiveDate;
use mysql::prelude::Queryable as _;
use mysql::{Conn, OptsBuilder, SslOpts, params};
use serde::{Deserialize, Serialize};

use super::{CategoryTotal, Error, NewSpend, Result, SpendEntry, Store, clean_category_name};
use crate::t;

/// How long to wait on a server that is not answering. Short enough that a
/// wrong hostname is a mistake you notice, rather than a frozen window.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const IO_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MariaDbSettings {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    /// Only written to disk if the user asks for it; see `config`.
    pub password: String,
    pub use_tls: bool,
    /// Accept a server certificate that does not match its hostname. The
    /// issuer is still checked, so this is narrower than
    /// [`super::mssql::MsSqlSettings::tls_skip_verify`] despite the matching
    /// name. Off by default, and labelled plainly in the UI, because it is
    /// still a real hole.
    pub tls_skip_verify: bool,
    /// A certificate authority to trust on top of the public ones, as a path
    /// to a `.pem` or `.der` file. Empty for the usual case of a certificate
    /// from a public issuer.
    ///
    /// A database server on your own network is very often issued by your own
    /// authority, which no public list has ever heard of. The driver trusts a
    /// compiled-in copy of the public roots rather than asking the operating
    /// system, so unlike the rest of the machine it cannot be told about that
    /// authority by installing it — hence this.
    pub ca_cert_path: String,
}

impl Default for MariaDbSettings {
    fn default() -> Self {
        Self {
            host: "localhost".to_owned(),
            port: 3306,
            database: "accounts".to_owned(),
            username: String::new(),
            password: String::new(),
            use_tls: false,
            tls_skip_verify: false,
            ca_cert_path: String::new(),
        }
    }
}

impl MariaDbSettings {
    fn opts(&self) -> Result<OptsBuilder> {
        if self.host.trim().is_empty() {
            return Err(Error::Rejected(t!("database.needs_host")));
        }
        if self.database.trim().is_empty() {
            return Err(Error::Rejected(t!("database.needs_name")));
        }
        if self.username.trim().is_empty() {
            return Err(Error::Rejected(t!("database.needs_user")));
        }

        let ssl = self.use_tls.then(|| {
            let ssl = SslOpts::default().with_danger_skip_domain_validation(self.tls_skip_verify);
            match self.ca_cert_path.trim() {
                "" => ssl,
                ca => ssl.with_root_cert_path(Some(std::path::PathBuf::from(ca))),
            }
        });

        Ok(OptsBuilder::new()
            .ip_or_hostname(Some(self.host.trim()))
            .tcp_port(self.port)
            .db_name(Some(self.database.trim()))
            .user(Some(self.username.trim()))
            .pass(Some(self.password.clone()))
            .tcp_connect_timeout(Some(CONNECT_TIMEOUT))
            .read_timeout(Some(IO_TIMEOUT))
            .write_timeout(Some(IO_TIMEOUT))
            .ssl_opts(ssl))
    }
}

pub struct MariaDbStore {
    conn: Conn,
    label: String,
    /// The login this connection authenticated as. A server can be shared by
    /// several people, so every row is stamped with whoever wrote it and
    /// every read is filtered back down to just them — the database's own
    /// login is the only account system this needs.
    owner: String,
}

impl MariaDbStore {
    pub fn connect(settings: &MariaDbSettings) -> Result<Self> {
        let conn = Conn::new(settings.opts()?).map_err(backend)?;
        let mut store = Self {
            conn,
            label: format!(
                "MariaDB · {}@{}:{}/{}",
                settings.username, settings.host, settings.port, settings.database
            ),
            owner: settings.username.trim().to_owned(),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        self.conn
            .query_drop(
                "CREATE TABLE IF NOT EXISTS categories (
                     id    INT AUTO_INCREMENT PRIMARY KEY,
                     owner VARCHAR(255) NOT NULL,
                     name  VARCHAR(64) COLLATE utf8mb4_unicode_ci NOT NULL,
                     CONSTRAINT categories_owner_name UNIQUE (owner, name)
                 ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4",
            )
            .map_err(backend)?;
        self.conn
            .query_drop(
                "CREATE TABLE IF NOT EXISTS spending (
                     id           INT AUTO_INCREMENT PRIMARY KEY,
                     owner        VARCHAR(255) NOT NULL,
                     category_id  INT NOT NULL,
                     spent_on     DATE NOT NULL,
                     amount_minor BIGINT NOT NULL,
                     currency     CHAR(3) NOT NULL,
                     description  VARCHAR(255) NOT NULL DEFAULT '',
                     INDEX spending_by_date (spent_on),
                     FOREIGN KEY (category_id) REFERENCES categories(id)
                 ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4",
            )
            .map_err(backend)?;
        self.claim_legacy_rows()
    }

    /// A database opened before per-owner data existed has no `owner` column,
    /// and a `categories.name` that had to be unique for everybody at once.
    /// Bring one up to date exactly once: add the column, hand whatever is
    /// already in it to whoever connects and finds it that way — there was
    /// only ever one owner before now, so this is just naming them — then
    /// swap the old database-wide uniqueness for one scoped to (owner, name)
    /// so different people can each have their own "Groceries".
    fn claim_legacy_rows(&mut self) -> Result<()> {
        let has_owner: Option<i64> = self
            .conn
            .query_first(
                "SELECT 1 FROM INFORMATION_SCHEMA.COLUMNS
                  WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'categories'
                    AND COLUMN_NAME = 'owner'",
            )
            .map_err(backend)?;
        if has_owner.is_some() {
            return Ok(());
        }

        self.conn
            .query_drop("ALTER TABLE categories ADD COLUMN owner VARCHAR(255) NOT NULL DEFAULT ''")
            .map_err(backend)?;
        self.conn
            .query_drop("ALTER TABLE spending ADD COLUMN owner VARCHAR(255) NOT NULL DEFAULT ''")
            .map_err(backend)?;
        self.conn
            .exec_drop(
                "UPDATE categories SET owner = :owner WHERE owner = ''",
                params! { "owner" => &self.owner },
            )
            .map_err(backend)?;
        self.conn
            .exec_drop(
                "UPDATE spending SET owner = :owner WHERE owner = ''",
                params! { "owner" => &self.owner },
            )
            .map_err(backend)?;

        // The old constraint was never named, so MariaDB picked a name for
        // it — look up whatever that was rather than guessing.
        let old_index: Option<String> = self
            .conn
            .query_first(
                "SELECT INDEX_NAME FROM INFORMATION_SCHEMA.STATISTICS
                  WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'categories'
                    AND COLUMN_NAME = 'name' AND NON_UNIQUE = 0 AND INDEX_NAME <> 'PRIMARY'
                  LIMIT 1",
            )
            .map_err(backend)?;
        if let Some(index_name) = old_index {
            self.conn
                .query_drop(format!("DROP INDEX `{index_name}` ON categories"))
                .map_err(backend)?;
        }
        self.conn
            .query_drop(
                "ALTER TABLE categories ADD CONSTRAINT categories_owner_name UNIQUE (owner, name)",
            )
            .map_err(backend)?;
        Ok(())
    }

    fn category_id(&mut self, name: &str) -> Result<Option<u64>> {
        self.conn
            .exec_first(
                "SELECT id FROM categories WHERE owner = :owner AND name = :name",
                params! { "owner" => &self.owner, "name" => name },
            )
            .map_err(backend)
    }
}

impl Store for MariaDbStore {
    fn categories_with_totals(&mut self, year: i32, currency: &str) -> Result<Vec<CategoryTotal>> {
        self.conn
            .exec_map(
                "SELECT c.name,
                        COALESCE(SUM(s.amount_minor), 0) AS total,
                        COUNT(s.id) AS entries
                   FROM categories c
                   LEFT JOIN spending s
                     ON s.category_id = c.id
                    AND s.currency = :currency
                    AND YEAR(s.spent_on) = :year
                  WHERE c.owner = :owner
                  GROUP BY c.id, c.name
                  ORDER BY c.name",
                params! { "owner" => &self.owner, "currency" => currency, "year" => year },
                |(name, total, entries): (String, i64, i64)| CategoryTotal {
                    name,
                    total_minor: total,
                    entries,
                },
            )
            .map_err(backend)
    }

    fn entries_in_other_currencies(&mut self, year: i32, currency: &str) -> Result<i64> {
        let count: Option<i64> = self
            .conn
            .exec_first(
                "SELECT COUNT(*) FROM spending
                  WHERE owner = :owner AND currency <> :currency AND YEAR(spent_on) = :year",
                params! { "owner" => &self.owner, "currency" => currency, "year" => year },
            )
            .map_err(backend)?;
        Ok(count.unwrap_or(0))
    }

    fn spending_in_year(&mut self, year: i32, currency: &str) -> Result<Vec<SpendEntry>> {
        // The date comes back formatted rather than as a `DATE`, so that both
        // backends hand this code the same thing: `chrono` support is an
        // optional feature of the MySQL driver and not one worth taking on for
        // a single column.
        let rows: Vec<(i64, String, String, i64, String)> = self
            .conn
            .exec(
                "SELECT s.id, DATE_FORMAT(s.spent_on, '%Y-%m-%d'), c.name, s.amount_minor, s.description
                   FROM spending s
                   JOIN categories c ON c.id = s.category_id
                  WHERE s.owner = :owner AND s.currency = :currency AND YEAR(s.spent_on) = :year
                  ORDER BY s.spent_on, c.name, s.id",
                params! { "owner" => &self.owner, "currency" => currency, "year" => year },
            )
            .map_err(backend)?;

        rows.into_iter()
            .map(|(id, date, category, amount_minor, description)| {
                Ok(SpendEntry {
                    id,
                    spent_on: chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                        .map_err(|_| Error::Backend(t!("database.bad_stored_date", date = date)))?,
                    category,
                    amount_minor,
                    description,
                })
            })
            .collect()
    }

    fn add_category(&mut self, name: &str) -> Result<()> {
        let name = clean_category_name(name)?;
        if self.category_id(&name)?.is_some() {
            return Err(Error::Rejected(t!("category.already_exists", name = name)));
        }
        self.conn
            .exec_drop(
                "INSERT INTO categories (owner, name) VALUES (:owner, :name)",
                params! { "owner" => &self.owner, "name" => &name },
            )
            .map_err(backend)?;
        Ok(())
    }

    fn add_spend(&mut self, spend: &NewSpend) -> Result<()> {
        let category_id = self
            .category_id(&spend.category)?
            .ok_or_else(|| Error::Rejected(t!("category.no_such", name = spend.category)))?;
        self.conn
            .exec_drop(
                "INSERT INTO spending
                     (owner, category_id, spent_on, amount_minor, currency, description)
                 VALUES (:owner, :category_id, :spent_on, :amount_minor, :currency, :description)",
                params! {
                    "owner" => &self.owner,
                    "category_id" => category_id,
                    "spent_on" => as_mysql_date(spend.spent_on),
                    "amount_minor" => spend.amount_minor,
                    "currency" => &spend.currency,
                    "description" => &spend.description,
                },
            )
            .map_err(backend)?;
        Ok(())
    }

    fn update_spend(&mut self, id: i64, spend: &NewSpend) -> Result<()> {
        let category_id = self
            .category_id(&spend.category)?
            .ok_or_else(|| Error::Rejected(t!("category.no_such", name = spend.category)))?;
        self.conn
            .exec_drop(
                "UPDATE spending
                    SET category_id = :category_id, spent_on = :spent_on,
                        amount_minor = :amount_minor, currency = :currency,
                        description = :description
                  WHERE id = :id AND owner = :owner",
                params! {
                    "owner" => &self.owner,
                    "id" => id,
                    "category_id" => category_id,
                    "spent_on" => as_mysql_date(spend.spent_on),
                    "amount_minor" => spend.amount_minor,
                    "currency" => &spend.currency,
                    "description" => &spend.description,
                },
            )
            .map_err(backend)?;
        not_found_unless_changed(self.conn.affected_rows())
    }

    fn delete_spend(&mut self, id: i64) -> Result<()> {
        self.conn
            .exec_drop(
                "DELETE FROM spending WHERE id = :id AND owner = :owner",
                params! { "id" => id, "owner" => &self.owner },
            )
            .map_err(backend)?;
        not_found_unless_changed(self.conn.affected_rows())
    }

    fn rename_category(&mut self, old_name: &str, new_name: &str) -> Result<()> {
        let new_name = clean_category_name(new_name)?;
        let id = self
            .category_id(old_name)?
            .ok_or_else(|| Error::Rejected(t!("category.no_such", name = old_name)))?;
        if let Some(existing) = self.category_id(&new_name)?
            && existing != id
        {
            return Err(Error::Rejected(t!(
                "category.already_exists",
                name = new_name
            )));
        }
        self.conn
            .exec_drop(
                "UPDATE categories SET name = :name WHERE id = :id AND owner = :owner",
                params! { "name" => &new_name, "id" => id, "owner" => &self.owner },
            )
            .map_err(backend)?;
        Ok(())
    }

    fn delete_category(&mut self, name: &str) -> Result<()> {
        let id = self
            .category_id(name)?
            .ok_or_else(|| Error::Rejected(t!("category.no_such", name = name)))?;
        self.conn
            .exec_drop(
                "DELETE FROM categories WHERE id = :id AND owner = :owner",
                params! { "id" => id, "owner" => &self.owner },
            )
            .map_err(|e| category_delete_error(name, e))?;
        Ok(())
    }

    fn describe(&self) -> String {
        self.label.clone()
    }
}

/// A zero-row UPDATE/DELETE means the id was never there, or was already
/// gone — either way there is nothing to tell the two cases apart, and
/// nothing worth leaking either way.
fn not_found_unless_changed(changed: u64) -> Result<()> {
    if changed == 0 {
        return Err(Error::Rejected(t!("entry.no_longer_exists")));
    }
    Ok(())
}

/// The foreign key from `spending` is what actually stops a category with
/// entries in it from being deleted; this turns that specific failure (MySQL
/// error 1451, "Cannot delete or update a parent row") into something worth
/// reading rather than a raw constraint-violation message.
fn category_delete_error(name: &str, e: mysql::Error) -> Error {
    if let mysql::Error::MySqlError(inner) = &e
        && inner.code == 1451
    {
        return Error::Rejected(t!("category.has_entries", name = name));
    }
    backend(e)
}

/// `chrono` integration is an optional feature of the `mysql` crate, and the
/// only date this app sends is a plain calendar day, so pass it as one.
fn as_mysql_date(date: NaiveDate) -> mysql::Value {
    use chrono::Datelike as _;
    mysql::Value::Date(
        date.year() as u16,
        date.month() as u8,
        date.day() as u8,
        0,
        0,
        0,
        0,
    )
}

/// Turn a driver error into something worth putting in front of a person.
///
/// `mysql::Error`'s own `Display` wraps the useful part in the name of the
/// variant holding it — `DriverError { Could not connect: connection timeout }`
/// — which tells the reader about the crate rather than about their server.
fn backend(e: mysql::Error) -> Error {
    Error::Backend(match e {
        mysql::Error::DriverError(inner) => inner.to_string(),
        mysql::Error::MySqlError(inner) => inner.message,
        mysql::Error::IoError(inner) => inner.to_string(),
        other => other.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three fields with no sensible default are checked before any
    /// network call, so a half-filled form fails immediately and says which
    /// box is empty, rather than after a five-second timeout against a
    /// hostname that was never there.
    #[test]
    fn incomplete_settings_are_refused_before_dialling_out() {
        let good = MariaDbSettings {
            username: "budget".to_owned(),
            ..Default::default()
        };
        assert!(good.opts().is_ok());

        for missing in [
            MariaDbSettings {
                host: "  ".to_owned(),
                ..good.clone()
            },
            MariaDbSettings {
                database: String::new(),
                ..good.clone()
            },
            MariaDbSettings {
                username: String::new(),
                ..good.clone()
            },
        ] {
            assert!(
                matches!(missing.opts(), Err(Error::Rejected(_))),
                "should have been refused: {missing:?}"
            );
        }
    }

    #[test]
    fn tls_is_off_unless_asked_for() {
        let settings = MariaDbSettings {
            username: "budget".to_owned(),
            ..Default::default()
        };
        assert!(!settings.use_tls);
        assert!(!settings.tls_skip_verify);
        assert!(
            mysql::Opts::from(settings.opts().unwrap())
                .get_ssl_opts()
                .is_none()
        );
    }

    /// The driver trusts a compiled-in list of public authorities and never
    /// consults the operating system, so a server certificate issued in-house
    /// is only trusted if this path carries it through.
    #[test]
    fn an_in_house_authority_reaches_the_driver() {
        let settings = MariaDbSettings {
            username: "budget".to_owned(),
            use_tls: true,
            ca_cert_path: "  /etc/ssl/company-ca.pem  ".to_owned(),
            ..Default::default()
        };

        let opts = mysql::Opts::from(settings.opts().unwrap());
        let ssl = opts.get_ssl_opts().expect("TLS was asked for");
        assert_eq!(
            ssl.root_cert_path(),
            Some(std::path::Path::new("/etc/ssl/company-ca.pem")),
            "the path should arrive trimmed"
        );
    }

    /// Left blank it must stay absent rather than becoming an empty path the
    /// driver then fails to open.
    #[test]
    fn no_authority_named_means_none_is_sent() {
        let settings = MariaDbSettings {
            username: "budget".to_owned(),
            use_tls: true,
            ca_cert_path: "   ".to_owned(),
            ..Default::default()
        };

        let opts = mysql::Opts::from(settings.opts().unwrap());
        let ssl = opts.get_ssl_opts().expect("TLS was asked for");
        assert_eq!(ssl.root_cert_path(), None);
    }
}
