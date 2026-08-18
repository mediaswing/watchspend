//! Storage: the same small set of operations, over SQLite, MariaDB or SQL
//! Server.
//!
//! The app only ever talks to the [`Store`] trait, so the Database tab can
//! swap the backend underneath it without anything else noticing.

pub mod attempt;
pub mod mariadb;
pub mod mssql;
pub mod sqlite;

use chrono::NaiveDate;

use crate::t;

/// A category and what has gone into it so far this year.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryTotal {
    pub name: String,
    /// In minor units of [`Self::currency`] — pence, cents, whole yen.
    pub total_minor: i64,
    pub entries: i64,
}

/// One recorded entry, as it comes back out for a report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpendEntry {
    /// Stable across reads, and what `update_spend`/`delete_spend` take to
    /// say which row is meant. Not shown anywhere in the app.
    pub id: i64,
    pub spent_on: NaiveDate,
    pub category: String,
    pub amount_minor: i64,
    pub description: String,
}

/// One thing the user spent money on.
#[derive(Clone, Debug)]
pub struct NewSpend {
    pub category: String,
    pub spent_on: NaiveDate,
    pub amount_minor: i64,
    pub currency: String,
    pub description: String,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    /// The user asked for something the data will not allow — a duplicate
    /// category name, a category that has since been deleted. Shown as-is.
    Rejected(String),
    /// Anything the database itself complained about.
    Backend(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(msg) | Self::Backend(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for Error {}

/// Everything the app needs from a database.
pub trait Store: Send {
    /// Every category, with the total spent in it during `year`, in the given
    /// currency. Categories with nothing in them yet come back with a zero.
    fn categories_with_totals(&mut self, year: i32, currency: &str) -> Result<Vec<CategoryTotal>>;

    /// Entries in `year` that are in some *other* currency, and so are not
    /// part of the totals above. Non-zero only if the machine's locale has
    /// changed since the entries were recorded.
    fn entries_in_other_currencies(&mut self, year: i32, currency: &str) -> Result<i64>;

    /// Every entry in `year` in the given currency, oldest first. This is what
    /// a report is built from, so it is read once and worked on in memory
    /// rather than asked for a section at a time.
    fn spending_in_year(&mut self, year: i32, currency: &str) -> Result<Vec<SpendEntry>>;

    fn add_category(&mut self, name: &str) -> Result<()>;

    fn add_spend(&mut self, spend: &NewSpend) -> Result<()>;

    /// Change what an entry says. `id` comes from a [`SpendEntry`] returned
    /// by [`Self::spending_in_year`]. `Rejected` if the id no longer refers
    /// to a row — deleted since, or (on a shared server) never this login's
    /// to begin with — or if `spend.category` does not exist.
    fn update_spend(&mut self, id: i64, spend: &NewSpend) -> Result<()>;

    /// Remove an entry. Never removes a category. Same `Rejected` case as
    /// [`Self::update_spend`] for an id that is not there to remove.
    fn delete_spend(&mut self, id: i64) -> Result<()>;

    /// Change a category's name in place. Same rules as [`Self::add_category`]:
    /// trimmed, non-empty, at most 64 characters, and `Rejected` if another
    /// category already has the new name.
    fn rename_category(&mut self, old_name: &str, new_name: &str) -> Result<()>;

    /// Remove a category. `Rejected` if it still has any spend entries — in
    /// any year or currency — so this can never take spending history down
    /// with it.
    fn delete_category(&mut self, name: &str) -> Result<()>;

    /// A short description of where the data is, for the status bar.
    fn describe(&self) -> String;
}

/// Trim a category name and check it is usable. Shared by both backends so
/// they cannot disagree about what a valid name is.
pub(crate) fn clean_category_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Rejected(t!("category.needs_a_name")));
    }
    if name.chars().count() > 64 {
        return Err(Error::Rejected(t!("category.name_too_long")));
    }
    Ok(name.to_owned())
}
