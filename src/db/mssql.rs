//! The optional backend: a Microsoft SQL Server somewhere on the network.
//!
//! Tiberius, the driver, is async-only, so this store carries its own small
//! single-threaded runtime and blocks on it for every call. That is no worse
//! than [`super::mariadb`], whose driver blocks the calling thread directly —
//! both leave the caller waiting for the network either way — and it keeps
//! [`Store`] itself synchronous, so the rest of the app does not need to know
//! that one of its three backends is async underneath.

use serde::{Deserialize, Serialize};
use tiberius::{AuthMethod, Client, Config, EncryptionLevel};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt as _};

use super::{CategoryTotal, Error, NewSpend, Result, SpendEntry, Store, clean_category_name};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MsSqlSettings {
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

impl Default for MsSqlSettings {
    fn default() -> Self {
        Self {
            host: "localhost".to_owned(),
            port: 1433,
            database: "accounts".to_owned(),
            username: String::new(),
            password: String::new(),
            use_tls: false,
            tls_skip_verify: false,
        }
    }
}

impl MsSqlSettings {
    fn config(&self) -> Result<Config> {
        if self.host.trim().is_empty() {
            return Err(Error::Rejected("Enter the server's host name.".to_owned()));
        }
        if self.database.trim().is_empty() {
            return Err(Error::Rejected("Enter the database name.".to_owned()));
        }
        if self.username.trim().is_empty() {
            return Err(Error::Rejected("Enter the user name.".to_owned()));
        }

        let mut config = Config::new();
        config.host(self.host.trim());
        config.port(self.port);
        config.database(self.database.trim());
        config.authentication(AuthMethod::sql_server(self.username.trim(), &self.password));
        // The login packet is encrypted either way; this only decides whether
        // the rest of the traffic is, same as the MariaDB pane's checkbox.
        config.encryption(if self.use_tls {
            EncryptionLevel::Required
        } else {
            EncryptionLevel::Off
        });
        if self.tls_skip_verify {
            config.trust_cert();
        }

        Ok(config)
    }
}

pub struct MsSqlStore {
    rt: Runtime,
    client: Client<Compat<TcpStream>>,
    label: String,
    /// The login this connection authenticated as. A server can be shared by
    /// several people, so every row is stamped with whoever wrote it and
    /// every read is filtered back down to just them — the database's own
    /// login is the only account system this needs.
    owner: String,
}

impl MsSqlStore {
    pub fn connect(settings: &MsSqlSettings) -> Result<Self> {
        let config = settings.config()?;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .map_err(|e| Error::Backend(format!("Could not start a connection thread: {e}")))?;

        let client = rt.block_on(async move {
            let tcp = TcpStream::connect(config.get_addr())
                .await
                .map_err(backend_io)?;
            tcp.set_nodelay(true).map_err(backend_io)?;
            Client::connect(config, tcp.compat_write())
                .await
                .map_err(backend)
        })?;

        let mut store = Self {
            rt,
            client,
            label: format!(
                "SQL Server · {}@{}:{}/{}",
                settings.username, settings.host, settings.port, settings.database
            ),
            owner: settings.username.trim().to_owned(),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        let client = &mut self.client;
        self.rt.block_on(async move {
            client
                .execute(
                    "IF NOT EXISTS (SELECT 1 FROM sys.tables WHERE name = 'categories')
                     CREATE TABLE categories (
                         id    INT IDENTITY PRIMARY KEY,
                         owner NVARCHAR(255) NOT NULL,
                         name  NVARCHAR(64) COLLATE Latin1_General_CI_AS NOT NULL,
                         CONSTRAINT categories_owner_name UNIQUE (owner, name)
                     )",
                    &[],
                )
                .await
                .map_err(backend)?;
            client
                .execute(
                    "IF NOT EXISTS (SELECT 1 FROM sys.tables WHERE name = 'spending')
                     CREATE TABLE spending (
                         id           INT IDENTITY PRIMARY KEY,
                         owner        NVARCHAR(255) NOT NULL,
                         category_id  INT NOT NULL REFERENCES categories(id),
                         spent_on     DATE NOT NULL,
                         amount_minor BIGINT NOT NULL,
                         currency     CHAR(3) NOT NULL,
                         description  NVARCHAR(255) NOT NULL DEFAULT ''
                     )",
                    &[],
                )
                .await
                .map_err(backend)?;
            client
                .execute(
                    "IF NOT EXISTS (
                         SELECT 1 FROM sys.indexes
                          WHERE name = 'spending_by_date'
                            AND object_id = OBJECT_ID('dbo.spending')
                     )
                     CREATE INDEX spending_by_date ON spending(spent_on)",
                    &[],
                )
                .await
                .map_err(backend)?;
            Ok(())
        })?;
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
        let client = &mut self.client;
        let owner = self.owner.as_str();
        self.rt.block_on(async move {
            let has_owner = client
                .query(
                    "SELECT 1 FROM INFORMATION_SCHEMA.COLUMNS
                      WHERE TABLE_NAME = 'categories' AND COLUMN_NAME = 'owner'",
                    &[],
                )
                .await
                .map_err(backend)?
                .into_row()
                .await
                .map_err(backend)?
                .is_some();
            if has_owner {
                return Ok(());
            }

            client
                .execute("ALTER TABLE categories ADD owner NVARCHAR(255) NOT NULL DEFAULT ''", &[])
                .await
                .map_err(backend)?;
            client
                .execute("ALTER TABLE spending ADD owner NVARCHAR(255) NOT NULL DEFAULT ''", &[])
                .await
                .map_err(backend)?;
            client
                .execute("UPDATE categories SET owner = @P1 WHERE owner = ''", &[&owner])
                .await
                .map_err(backend)?;
            client
                .execute("UPDATE spending SET owner = @P1 WHERE owner = ''", &[&owner])
                .await
                .map_err(backend)?;

            // The old constraint was never named, so SQL Server picked a
            // name for it — look up whatever that was rather than guessing.
            let old_constraint = client
                .query(
                    "SELECT kc.name
                       FROM sys.key_constraints kc
                       JOIN sys.index_columns ic
                         ON ic.object_id = kc.parent_object_id
                        AND ic.index_id = kc.unique_index_id
                       JOIN sys.columns col
                         ON col.object_id = ic.object_id
                        AND col.column_id = ic.column_id
                      WHERE kc.parent_object_id = OBJECT_ID('dbo.categories')
                        AND kc.type = 'UQ'
                        AND col.name = 'name'",
                    &[],
                )
                .await
                .map_err(backend)?
                .into_row()
                .await
                .map_err(backend)?;
            if let Some(row) = old_constraint {
                let constraint_name = column_string(&row, 0, "constraint name")?;
                client
                    .execute(
                        format!("ALTER TABLE categories DROP CONSTRAINT [{constraint_name}]"),
                        &[],
                    )
                    .await
                    .map_err(backend)?;
            }
            client
                .execute(
                    "ALTER TABLE categories ADD CONSTRAINT categories_owner_name UNIQUE (owner, name)",
                    &[],
                )
                .await
                .map_err(backend)?;
            Ok(())
        })
    }

    fn category_id(&mut self, name: &str) -> Result<Option<i32>> {
        let client = &mut self.client;
        let owner = self.owner.as_str();
        self.rt.block_on(async move {
            let row = client
                .query(
                    "SELECT id FROM categories WHERE owner = @P1 AND name = @P2",
                    &[&owner, &name],
                )
                .await
                .map_err(backend)?
                .into_row()
                .await
                .map_err(backend)?;
            Ok(row.and_then(|row| row.get::<i32, _>(0)))
        })
    }
}

impl Store for MsSqlStore {
    fn categories_with_totals(&mut self, year: i32, currency: &str) -> Result<Vec<CategoryTotal>> {
        let client = &mut self.client;
        let owner = self.owner.as_str();
        self.rt.block_on(async move {
            let rows = client
                .query(
                    "SELECT c.name,
                            COALESCE(SUM(s.amount_minor), 0) AS total,
                            COUNT_BIG(s.id) AS entries
                       FROM categories c
                       LEFT JOIN spending s
                         ON s.category_id = c.id
                        AND s.currency = @P2
                        AND YEAR(s.spent_on) = @P3
                      WHERE c.owner = @P1
                      GROUP BY c.name
                      ORDER BY c.name",
                    &[&owner, &currency, &year],
                )
                .await
                .map_err(backend)?
                .into_first_result()
                .await
                .map_err(backend)?;

            rows.into_iter()
                .map(|row| {
                    Ok(CategoryTotal {
                        name: column_string(&row, 0, "categories.name")?,
                        total_minor: column(&row, 1, "total")?,
                        entries: column(&row, 2, "entries")?,
                    })
                })
                .collect()
        })
    }

    fn entries_in_other_currencies(&mut self, year: i32, currency: &str) -> Result<i64> {
        let client = &mut self.client;
        let owner = self.owner.as_str();
        self.rt.block_on(async move {
            let row = client
                .query(
                    "SELECT COUNT_BIG(*) FROM spending
                      WHERE owner = @P1 AND currency <> @P2 AND YEAR(spent_on) = @P3",
                    &[&owner, &currency, &year],
                )
                .await
                .map_err(backend)?
                .into_row()
                .await
                .map_err(backend)?;
            match row {
                Some(row) => column(&row, 0, "count"),
                None => Ok(0),
            }
        })
    }

    fn spending_in_year(&mut self, year: i32, currency: &str) -> Result<Vec<SpendEntry>> {
        let client = &mut self.client;
        let owner = self.owner.as_str();
        self.rt.block_on(async move {
            let rows = client
                .query(
                    "SELECT s.spent_on, c.name, s.amount_minor, s.description
                       FROM spending s
                       JOIN categories c ON c.id = s.category_id
                      WHERE s.owner = @P1 AND s.currency = @P2 AND YEAR(s.spent_on) = @P3
                      ORDER BY s.spent_on, c.name, s.id",
                    &[&owner, &currency, &year],
                )
                .await
                .map_err(backend)?
                .into_first_result()
                .await
                .map_err(backend)?;

            rows.into_iter()
                .map(|row| {
                    Ok(SpendEntry {
                        spent_on: column(&row, 0, "spending.spent_on")?,
                        category: column_string(&row, 1, "categories.name")?,
                        amount_minor: column(&row, 2, "amount_minor")?,
                        description: column_string(&row, 3, "description")?,
                    })
                })
                .collect()
        })
    }

    fn add_category(&mut self, name: &str) -> Result<()> {
        let name = clean_category_name(name)?;
        if self.category_id(&name)?.is_some() {
            return Err(Error::Rejected(format!("“{name}” is already a category.")));
        }
        let client = &mut self.client;
        let owner = self.owner.as_str();
        self.rt.block_on(async move {
            client
                .execute(
                    "INSERT INTO categories (owner, name) VALUES (@P1, @P2)",
                    &[&owner, &name.as_str()],
                )
                .await
                .map_err(backend)?;
            Ok(())
        })
    }

    fn add_spend(&mut self, spend: &NewSpend) -> Result<()> {
        let category_id = self.category_id(&spend.category)?.ok_or_else(|| {
            Error::Rejected(format!("There is no category called “{}”.", spend.category))
        })?;
        let client = &mut self.client;
        let owner = self.owner.as_str();
        self.rt.block_on(async move {
            client
                .execute(
                    "INSERT INTO spending
                         (owner, category_id, spent_on, amount_minor, currency, description)
                     VALUES (@P1, @P2, @P3, @P4, @P5, @P6)",
                    &[
                        &owner,
                        &category_id,
                        &spend.spent_on,
                        &spend.amount_minor,
                        &spend.currency.as_str(),
                        &spend.description.as_str(),
                    ],
                )
                .await
                .map_err(backend)?;
            Ok(())
        })
    }

    fn describe(&self) -> String {
        self.label.clone()
    }
}

/// Read a column out, treating a NULL where the query never allows one as a
/// backend error rather than a panic.
fn column<'a, T: tiberius::FromSql<'a>>(
    row: &'a tiberius::Row,
    index: usize,
    what: &str,
) -> Result<T> {
    row.get::<T, _>(index)
        .ok_or_else(|| Error::Backend(format!("{what} came back unexpectedly empty")))
}

/// `String` has no `FromSql` impl of its own — only `&str` does — so text
/// columns go through this instead of [`column`].
fn column_string(row: &tiberius::Row, index: usize, what: &str) -> Result<String> {
    column::<&str>(row, index, what).map(str::to_owned)
}

fn backend_io(e: std::io::Error) -> Error {
    Error::Backend(e.to_string())
}

/// Turn a driver error into something worth putting in front of a person,
/// preferring the server's own message where there is one.
fn backend(e: tiberius::error::Error) -> Error {
    Error::Backend(match &e {
        tiberius::error::Error::Server(token) => token.message().to_owned(),
        other => other.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three fields with no sensible default are checked before any
    /// network call, so a half-filled form fails immediately and says which
    /// box is empty, rather than after a timeout against a hostname that was
    /// never there.
    #[test]
    fn incomplete_settings_are_refused_before_dialling_out() {
        let good = MsSqlSettings {
            username: "budget".to_owned(),
            ..Default::default()
        };
        assert!(good.config().is_ok());

        for missing in [
            MsSqlSettings {
                host: "  ".to_owned(),
                ..good.clone()
            },
            MsSqlSettings {
                database: String::new(),
                ..good.clone()
            },
            MsSqlSettings {
                username: String::new(),
                ..good.clone()
            },
        ] {
            assert!(
                matches!(missing.config(), Err(Error::Rejected(_))),
                "should have been refused: {missing:?}"
            );
        }
    }

    #[test]
    fn tls_is_off_unless_asked_for() {
        let settings = MsSqlSettings {
            username: "budget".to_owned(),
            ..Default::default()
        };
        assert!(!settings.use_tls);
        assert!(!settings.tls_skip_verify);
    }

    /// Not run as part of the normal suite: it needs a real server. Point it
    /// at one with:
    ///
    /// ```text
    /// MSSQL_TEST_HOST, MSSQL_TEST_PORT (default localhost:1433)
    /// MSSQL_TEST_DATABASE, MSSQL_TEST_USER, MSSQL_TEST_PASSWORD (required)
    /// ```
    ///
    /// then `cargo test --lib db::mssql::tests::live_server_round_trip -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_server_round_trip() {
        fn env(key: &str) -> Option<String> {
            std::env::var(key).ok().filter(|v| !v.is_empty())
        }

        let settings = MsSqlSettings {
            host: env("MSSQL_TEST_HOST").unwrap_or_else(|| "localhost".to_owned()),
            port: env("MSSQL_TEST_PORT")
                .map(|p| p.parse().expect("MSSQL_TEST_PORT must be a number"))
                .unwrap_or(1433),
            database: env("MSSQL_TEST_DATABASE").expect("set MSSQL_TEST_DATABASE"),
            username: env("MSSQL_TEST_USER").expect("set MSSQL_TEST_USER"),
            password: env("MSSQL_TEST_PASSWORD").expect("set MSSQL_TEST_PASSWORD"),
            use_tls: false,
            tls_skip_verify: true,
        };

        let mut store = MsSqlStore::connect(&settings).expect("connect and migrate");
        println!("connected: {}", store.describe());

        let marker = format!(
            "smoke-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let category = format!("Smoke {marker}");
        store.add_category(&category).expect("add_category");

        use chrono::Datelike as _;
        let year = chrono::Utc::now().date_naive().year();
        let spend = NewSpend {
            category: category.clone(),
            spent_on: chrono::NaiveDate::from_ymd_opt(year, 6, 15).unwrap(),
            amount_minor: 1234,
            currency: "GBP".to_owned(),
            description: marker.clone(),
        };
        store.add_spend(&spend).expect("add_spend");

        let totals = store
            .categories_with_totals(year, "GBP")
            .expect("categories_with_totals");
        let found = totals
            .iter()
            .find(|t| t.name == category)
            .expect("the category just added should be in the totals");
        assert_eq!(found.total_minor, 1234);
        assert_eq!(found.entries, 1);
        println!("totals: {found:?}");

        let entries = store
            .spending_in_year(year, "GBP")
            .expect("spending_in_year");
        let entry = entries
            .iter()
            .find(|e| e.description == marker)
            .expect("the entry just added should be in the year's spending");
        assert_eq!(entry.category, category);
        assert_eq!(entry.amount_minor, 1234);
        assert_eq!(entry.spent_on, spend.spent_on);
        println!("entry: {entry:?}");

        let other = store
            .entries_in_other_currencies(year, "GBP")
            .expect("entries_in_other_currencies");
        println!("entries in other currencies this year: {other}");

        println!("round trip through a real SQL Server succeeded");
    }
}
