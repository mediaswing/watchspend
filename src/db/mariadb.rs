//! The optional backend: a MariaDB (or MySQL) server somewhere on the network.

use std::time::Duration;

use chrono::NaiveDate;
use mysql::prelude::Queryable as _;
use mysql::{Conn, OptsBuilder, SslOpts, params};
use serde::{Deserialize, Serialize};

use super::{CategoryTotal, Error, NewSpend, Result, SpendEntry, Store, clean_category_name};

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
    /// Accept a server certificate that does not match its hostname. Off by
    /// default, and labelled plainly in the UI, because it is a real hole.
    pub tls_skip_verify: bool,
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
        }
    }
}

impl MariaDbSettings {
    fn opts(&self) -> Result<OptsBuilder> {
        if self.host.trim().is_empty() {
            return Err(Error::Rejected("Enter the server's host name.".to_owned()));
        }
        if self.database.trim().is_empty() {
            return Err(Error::Rejected("Enter the database name.".to_owned()));
        }
        if self.username.trim().is_empty() {
            return Err(Error::Rejected("Enter the user name.".to_owned()));
        }

        let ssl = self
            .use_tls
            .then(|| SslOpts::default().with_danger_skip_domain_validation(self.tls_skip_verify));

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
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        self.conn
            .query_drop(
                "CREATE TABLE IF NOT EXISTS categories (
                     id   INT AUTO_INCREMENT PRIMARY KEY,
                     name VARCHAR(64) COLLATE utf8mb4_unicode_ci NOT NULL UNIQUE
                 ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4",
            )
            .map_err(backend)?;
        self.conn
            .query_drop(
                "CREATE TABLE IF NOT EXISTS spending (
                     id           INT AUTO_INCREMENT PRIMARY KEY,
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
        Ok(())
    }

    fn category_id(&mut self, name: &str) -> Result<Option<u64>> {
        self.conn
            .exec_first(
                "SELECT id FROM categories WHERE name = :name",
                params! { "name" => name },
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
                  GROUP BY c.id, c.name
                  ORDER BY c.name",
                params! { "currency" => currency, "year" => year },
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
                  WHERE currency <> :currency AND YEAR(spent_on) = :year",
                params! { "currency" => currency, "year" => year },
            )
            .map_err(backend)?;
        Ok(count.unwrap_or(0))
    }

    fn spending_in_year(&mut self, year: i32, currency: &str) -> Result<Vec<SpendEntry>> {
        // The date comes back formatted rather than as a `DATE`, so that both
        // backends hand this code the same thing: `chrono` support is an
        // optional feature of the MySQL driver and not one worth taking on for
        // a single column.
        let rows: Vec<(String, String, i64, String)> = self
            .conn
            .exec(
                "SELECT DATE_FORMAT(s.spent_on, '%Y-%m-%d'), c.name, s.amount_minor, s.description
                   FROM spending s
                   JOIN categories c ON c.id = s.category_id
                  WHERE s.currency = :currency AND YEAR(s.spent_on) = :year
                  ORDER BY s.spent_on, c.name, s.id",
                params! { "currency" => currency, "year" => year },
            )
            .map_err(backend)?;

        rows.into_iter()
            .map(|(date, category, amount_minor, description)| {
                Ok(SpendEntry {
                    spent_on: chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(
                        |_| Error::Backend(format!("A stored date is not a date: {date:?}")),
                    )?,
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
            return Err(Error::Rejected(format!("“{name}” is already a category.")));
        }
        self.conn
            .exec_drop(
                "INSERT INTO categories (name) VALUES (:name)",
                params! { "name" => &name },
            )
            .map_err(backend)?;
        Ok(())
    }

    fn add_spend(&mut self, spend: &NewSpend) -> Result<()> {
        let category_id = self.category_id(&spend.category)?.ok_or_else(|| {
            Error::Rejected(format!("There is no category called “{}”.", spend.category))
        })?;
        self.conn
            .exec_drop(
                "INSERT INTO spending (category_id, spent_on, amount_minor, currency, description)
                 VALUES (:category_id, :spent_on, :amount_minor, :currency, :description)",
                params! {
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

    fn describe(&self) -> String {
        self.label.clone()
    }
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

fn backend(e: mysql::Error) -> Error {
    Error::Backend(e.to_string())
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
}
