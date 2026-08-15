//! Opening a database without freezing the window.
//!
//! A SQLite file opens in microseconds. A MariaDB server on the other end of a
//! network takes as long as it takes, and up to the five-second timeout when
//! the answer is that it is not there. Doing that on the thread that draws the
//! window means the window stops being drawn — a spinning cursor, a beachball,
//! and no way to press Cancel, all because someone typed the hostname wrong.
//!
//! So the attempt is made on its own thread and collected later. The [`Store`]
//! it produces is `Send` for exactly this reason.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use super::{
    Store, mariadb::MariaDbSettings, mariadb::MariaDbStore, mssql::MsSqlSettings,
    mssql::MsSqlStore, sqlite::SqliteStore,
};

/// A database to open.
#[derive(Clone, Debug)]
pub enum Target {
    Sqlite(PathBuf),
    MariaDb(MariaDbSettings),
    MsSql(MsSqlSettings),
}

impl Target {
    /// How to describe it before it is open, when there is no store to ask.
    pub fn label(&self) -> String {
        match self {
            Self::Sqlite(path) => format!("SQLite · {}", crate::config::tilde(path)),
            Self::MariaDb(settings) => format!(
                "MariaDB · {}@{}:{}/{}",
                settings.username, settings.host, settings.port, settings.database
            ),
            Self::MsSql(settings) => format!(
                "SQL Server · {}@{}:{}/{}",
                settings.username, settings.host, settings.port, settings.database
            ),
        }
    }

    /// Whether opening this is worth going to another thread for. A local file
    /// is not: the flicker of an empty window would last longer than the work.
    pub fn is_slow(&self) -> bool {
        !matches!(self, Self::Sqlite(_))
    }

    /// Open it, and prove it is usable before anyone relies on it.
    ///
    /// Connecting only shows that the login worked. It says nothing about
    /// whether this account can read the tables it just created, and finding
    /// that out after the switch means finding it out from an app that has
    /// already put its old, working database down.
    pub fn open_and_check(&self, year: i32, currency: &str) -> Result<Box<dyn Store>, String> {
        let mut store: Box<dyn Store> = match self {
            Self::Sqlite(path) => {
                if path.as_os_str().is_empty() {
                    return Err("Give the database file a path.".to_owned());
                }
                Box::new(SqliteStore::open(path).map_err(|e| e.to_string())?)
            }
            Self::MariaDb(settings) => {
                Box::new(MariaDbStore::connect(settings).map_err(|e| e.to_string())?)
            }
            Self::MsSql(settings) => {
                Box::new(MsSqlStore::connect(settings).map_err(|e| e.to_string())?)
            }
        };

        store
            .categories_with_totals(year, currency)
            .map_err(|err| format!("Connected, but could not read from it: {err}"))?;

        Ok(store)
    }
}

/// What to do with the connection once it is open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose {
    /// Report on it and throw it away.
    Test,
    /// Start using it.
    Adopt,
}

/// A connection being opened on another thread.
pub struct Attempt {
    receiver: Receiver<Result<Box<dyn Store>, String>>,
    pub purpose: Purpose,
    pub target: Target,
}

impl Attempt {
    pub fn start(target: Target, purpose: Purpose, year: i32, currency: &'static str) -> Self {
        let (sender, receiver) = channel();
        let work = target.clone();
        let spawned = std::thread::Builder::new()
            // Linux caps a thread name at 15 bytes and silently truncates
            // past that, which is a poor thing to discover in a backtrace
            // while working out why a connection is hanging. Short enough to
            // survive intact there and still mean something on macOS.
            .name("db-connect".to_owned())
            .spawn(move || {
                let _ = sender.send(work.open_and_check(year, currency));
            });

        if let Err(err) = spawned {
            // A machine that cannot spawn a thread has worse problems, but the
            // app should still say something rather than wait forever.
            let (sender, receiver) = channel();
            let _ = sender.send(Err(format!("Could not start the connection: {err}")));
            return Self {
                receiver,
                purpose,
                target,
            };
        }

        Self {
            receiver,
            purpose,
            target,
        }
    }

    /// The result, once it arrives.
    pub fn poll(&mut self) -> Option<Result<Box<dyn Store>, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            // The thread died without sending, which should not happen; say so
            // rather than leaving the app waiting on it.
            Err(TryRecvError::Disconnected) => Some(Err(
                "The connection attempt stopped unexpectedly.".to_owned(),
            )),
        }
    }
}
