//! Reports: one set of figures, and four ways of writing it down.
//!
//! The [`Report`] is built once from a year's entries, and each writer turns
//! that same structure into a file. Nothing is queried per format, so a CSV
//! and a Word document made a second apart cannot disagree.

mod csv;
mod docx;
mod html;
mod json;

use chrono::{Datelike as _, Local, NaiveDate};

use crate::db::SpendEntry;
use crate::locale::Locale;

/// What a report can be written as.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    /// For a spreadsheet.
    Csv,
    /// For Word, and anything else that reads OOXML.
    Docx,
    /// For a browser — and, through one, for print and PDF.
    Html,
    /// For another program.
    Json,
}

impl Format {
    pub const ALL: [Self; 4] = [Self::Csv, Self::Docx, Self::Html, Self::Json];

    pub fn label(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Docx => "Word",
            Self::Html => "HTML",
            Self::Json => "JSON",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Docx => "docx",
            Self::Html => "html",
            Self::Json => "json",
        }
    }

    /// A line of prose for the pane, so the choice is not four bare acronyms.
    pub fn detail(self) -> &'static str {
        match self {
            Self::Csv => "Columns for a spreadsheet: Excel, Numbers, LibreOffice.",
            Self::Docx => "A Word document, with the tables laid out for reading.",
            Self::Html => "A page for the browser, and printable from there.",
            Self::Json => "The figures as data, for another program to read.",
        }
    }
}

/// Which sections to write. A report of one line is still a report.
#[derive(Clone, Copy, Debug)]
pub struct Sections {
    pub monthly: bool,
    pub entries: bool,
}

impl Default for Sections {
    fn default() -> Self {
        Self {
            monthly: true,
            entries: true,
        }
    }
}

/// One line of the category table.
#[derive(Clone, Debug)]
pub struct CategoryLine {
    pub name: String,
    pub entries: usize,
    pub total_minor: i64,
    /// Share of the year's spending, in per cent.
    pub share: f64,
}

/// A year's spending, arranged the way a report presents it.
#[derive(Clone, Debug)]
pub struct Report {
    pub year: i32,
    pub generated: NaiveDate,
    pub categories: Vec<CategoryLine>,
    /// Totals for January through December.
    pub months: [i64; 12],
    pub entries: Vec<SpendEntry>,
    pub total_minor: i64,
}

impl Report {
    /// Arrange a year's entries into the shape a report needs.
    ///
    /// Every figure here is derived from the same list of entries, including
    /// the category totals — a report that disagreed with itself between two
    /// of its own tables would be worse than no report.
    pub fn build(year: i32, mut entries: Vec<SpendEntry>) -> Self {
        // Both backends already return these in order, but a report that
        // listed a year out of sequence because a query changed would be a
        // bug found by a reader, not by a test. Ordering it here makes it a
        // property of the report rather than of the query.
        entries.sort_by(|a, b| {
            a.spent_on
                .cmp(&b.spent_on)
                .then_with(|| a.category.cmp(&b.category))
        });

        let total_minor: i64 = entries.iter().map(|e| e.amount_minor).sum();

        let mut months = [0i64; 12];
        for entry in &entries {
            months[entry.spent_on.month0() as usize] += entry.amount_minor;
        }

        // Group by category name, keeping the order stable and alphabetical so
        // two runs of the same report are the same document.
        let mut by_category: std::collections::BTreeMap<&str, (usize, i64)> =
            std::collections::BTreeMap::new();
        for entry in &entries {
            let line = by_category.entry(entry.category.as_str()).or_insert((0, 0));
            line.0 += 1;
            line.1 += entry.amount_minor;
        }

        let categories = by_category
            .into_iter()
            .map(|(name, (count, total))| CategoryLine {
                name: name.to_owned(),
                entries: count,
                total_minor: total,
                share: if total_minor == 0 {
                    0.0
                } else {
                    total as f64 / total_minor as f64 * 100.0
                },
            })
            .collect();

        Self {
            year,
            generated: Local::now().date_naive(),
            categories,
            months,
            entries,
            total_minor,
        }
    }

    /// Write the report out. The bytes are handed back rather than written
    /// here, so that the caller owns the decision about where they land.
    pub fn render(
        &self,
        format: Format,
        locale: &Locale,
        sections: Sections,
    ) -> Result<Vec<u8>, String> {
        match format {
            Format::Csv => Ok(csv::render(self, locale, sections)),
            Format::Docx => docx::render(self, locale, sections),
            Format::Html => Ok(html::render(self, locale, sections)),
            Format::Json => Ok(json::render(self, locale, sections)),
        }
    }

    /// The name to save under, e.g. `spending-report-2026.csv`.
    pub fn file_name(&self, format: Format) -> String {
        format!("spending-report-{}.{}", self.year, format.extension())
    }

    fn month_name(index: usize) -> &'static str {
        const MONTHS: [&str; 12] = [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];
        MONTHS[index]
    }

    /// Months that had something in them, as (name, total) pairs. A month with
    /// no spending is left out rather than printed as a row of zeroes.
    fn months_with_spending(&self) -> Vec<(&'static str, i64)> {
        self.months
            .iter()
            .enumerate()
            .filter(|(_, total)| **total != 0)
            .map(|(i, total)| (Self::month_name(i), *total))
            .collect()
    }
}

/// A share formatted for display, e.g. `12.5%`, using the locale's decimal
/// separator so it matches every other number in the document.
fn share(value: f64, locale: &Locale) -> String {
    let text = format!("{value:.1}");
    format!("{}%", text.replace('.', &locale.decimal_sep.to_string()))
}

/// Escape text for XML and HTML alike.
///
/// Category names and descriptions are whatever the user typed, and they end
/// up inside markup in three of the four writers. An ampersand in "Books &
/// Music" would produce a file Word refuses to open; a `<script>` in a
/// description would produce an HTML page that runs it.
fn escape_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // XML 1.0 forbids most control characters outright, and a stray
            // one would make the file unopenable rather than merely ugly.
            c if (c < '\u{20}' && c != '\t' && c != '\n' && c != '\r') || c == '\u{7f}' => {
                out.push(' ');
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn entry(date: (i32, u32, u32), category: &str, minor: i64) -> SpendEntry {
        SpendEntry {
            // No report format reads the id; it only matters to edit/delete.
            id: 0,
            spent_on: NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap(),
            category: category.to_owned(),
            amount_minor: minor,
            description: String::new(),
        }
    }

    pub(super) fn sample() -> Report {
        Report::build(
            2026,
            vec![
                entry((2026, 1, 14), "Groceries", 8734),
                entry((2026, 3, 2), "Groceries", 12250),
                entry((2026, 1, 1), "Rent", 95000),
                SpendEntry {
                    description: "birthday <dinner> & drinks".to_owned(),
                    ..entry((2026, 5, 30), "Eating Out", 4200)
                },
            ],
        )
    }

    #[test]
    fn totals_are_derived_from_the_same_entries_everywhere() {
        let report = sample();
        assert_eq!(report.total_minor, 8734 + 12250 + 95000 + 4200);

        let sum_of_categories: i64 = report.categories.iter().map(|c| c.total_minor).sum();
        let sum_of_months: i64 = report.months.iter().sum();
        assert_eq!(sum_of_categories, report.total_minor);
        assert_eq!(sum_of_months, report.total_minor);

        // Alphabetical, so the same data always makes the same document.
        let names: Vec<&str> = report.categories.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Eating Out", "Groceries", "Rent"]);
        assert_eq!(report.categories[1].entries, 2);
    }

    #[test]
    fn shares_add_up() {
        let report = sample();
        let total: f64 = report.categories.iter().map(|c| c.share).sum();
        assert!((total - 100.0).abs() < 0.001, "shares summed to {total}");
    }

    #[test]
    fn empty_years_do_not_divide_by_zero() {
        let report = Report::build(2026, Vec::new());
        assert_eq!(report.total_minor, 0);
        assert!(report.categories.is_empty());
        assert!(report.months_with_spending().is_empty());
    }

    #[test]
    fn markup_is_escaped_and_control_characters_removed() {
        assert_eq!(escape_markup("Books & Music"), "Books &amp; Music");
        assert_eq!(
            escape_markup("<script>alert('x')</script>"),
            "&lt;script&gt;alert(&apos;x&apos;)&lt;/script&gt;"
        );
        assert_eq!(escape_markup("a\u{0}b"), "a b");
        assert_eq!(escape_markup("keep\ttabs"), "keep\ttabs");
    }
}
