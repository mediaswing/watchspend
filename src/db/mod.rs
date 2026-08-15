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

    /// A short description of where the data is, for the status bar.
    fn describe(&self) -> String;
}

/// Trim a category name and check it is usable. Shared by both backends so
/// they cannot disagree about what a valid name is.
pub(crate) fn clean_category_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Rejected("Give the category a name.".to_owned()));
    }
    if name.chars().count() > 64 {
        return Err(Error::Rejected(
            "That name is too long — 64 characters at most.".to_owned(),
        ));
    }
    Ok(name.to_owned())
}
