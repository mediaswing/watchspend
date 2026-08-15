//! CSV, for spreadsheets.
//!
//! Three decisions here are about the spreadsheet rather than about CSV:
//! the delimiter follows the locale, because Excel reads a comma-decimal
//! locale's files as semicolon-separated; the file opens with a byte order
//! mark, without which Excel renders `£` as `Â£`; and the lines end with
//! CRLF, as RFC 4180 asks.

use super::{Report, Sections, share};
use crate::locale::Locale;

const BOM: &str = "\u{feff}";
const CRLF: &str = "\r\n";

pub fn render(report: &Report, locale: &Locale, sections: Sections) -> Vec<u8> {
    // A locale that writes 1,50 for one and a half cannot also use the comma
    // to separate columns, and its spreadsheet will expect a semicolon.
    let delimiter = if locale.decimal_sep == ',' { ';' } else { ',' };
    let mut out = String::from(BOM);

    let row = |cells: &[&str], out: &mut String| {
        let line: Vec<String> = cells.iter().map(|c| field(c, delimiter)).collect();
        out.push_str(&line.join(&delimiter.to_string()));
        out.push_str(CRLF);
    };

    row(&["Spending report"], &mut out);
    row(&["Year", &report.year.to_string()], &mut out);
    row(
        &["Generated", &locale.format_date(report.generated)],
        &mut out,
    );
    row(&["Currency", locale.currency.code], &mut out);
    row(
        &["Total", &plain_amount(report.total_minor, locale)],
        &mut out,
    );
    row(&["Entries", &report.entries.len().to_string()], &mut out);
    out.push_str(CRLF);

    row(&["Category", "Entries", "Total", "Share"], &mut out);
    for line in &report.categories {
        row(
            &[
                &line.name,
                &line.entries.to_string(),
                &plain_amount(line.total_minor, locale),
                &share(line.share, locale),
            ],
            &mut out,
        );
    }

    if sections.monthly {
        out.push_str(CRLF);
        row(&["Month", "Total"], &mut out);
        for (name, total) in report.months_with_spending() {
            row(&[name, &plain_amount(total, locale)], &mut out);
        }
    }

    if sections.entries {
        out.push_str(CRLF);
        row(&["Date", "Category", "Amount", "Description"], &mut out);
        for entry in &report.entries {
            row(
                &[
                    &locale.format_date(entry.spent_on),
                    &entry.category,
                    &plain_amount(entry.amount_minor, locale),
                    &entry.description,
                ],
                &mut out,
            );
        }
    }

    out.into_bytes()
}

/// An amount with no symbol and no digit grouping, so the spreadsheet reads it
/// as a number and can add it up. The decimal separator is still the locale's,
/// because that is what the spreadsheet on this machine expects.
fn plain_amount(minor: i64, locale: &Locale) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let scale = 10i64.pow(locale.currency.decimals);
    let whole = minor.abs() / scale;
    let frac = minor.abs() % scale;
    if locale.currency.decimals == 0 {
        format!("{sign}{whole}")
    } else {
        format!(
            "{sign}{whole}{}{frac:0width$}",
            locale.decimal_sep,
            width = locale.currency.decimals as usize
        )
    }
}

/// Quote a field as RFC 4180 asks, and defuse anything a spreadsheet would
/// treat as a formula.
///
/// A description of `=1+1` is text to this app and a formula to Excel; the
/// classic form of the attack is `=HYPERLINK(...)` or a call out to a shell,
/// in a file the recipient trusts because their colleague sent it. Prefixing
/// with an apostrophe is the usual defence: the spreadsheet shows the text and
/// evaluates nothing.
///
/// A leading `-` or `+` is on the dangerous list too, but [`plain_amount`]
/// legitimately produces `-12.34` for a refund, and quoting that turns it
/// into text a spreadsheet's `SUM()` skips over. A bare signed number cannot
/// itself be a formula — there is nothing after the sign for a spreadsheet to
/// evaluate — so it is let through unquoted; anything else with a dangerous
/// lead still is.
fn field(value: &str, delimiter: char) -> String {
    let dangerous_lead = matches!(
        value.chars().next(),
        Some('=' | '+' | '-' | '@' | '\t' | '\r')
    ) && !is_plain_number(value);
    let mut text = if dangerous_lead {
        format!("'{value}")
    } else {
        value.to_owned()
    };

    if text.contains(delimiter) || text.contains('"') || text.contains('\n') || text.contains('\r')
    {
        text = format!("\"{}\"", text.replace('"', "\"\""));
    }
    text
}

/// Whether `value` is nothing but an optional sign, digits, and at most one
/// decimal separator — the shape [`plain_amount`] produces. A formula needs
/// more than that to do anything, so this is enough to tell a real negative
/// number apart from `-HYPERLINK(...)` or `-cmd|'/c calc'!A1`.
fn is_plain_number(value: &str) -> bool {
    let body = value.strip_prefix(['+', '-']).unwrap_or(value);
    !body.is_empty()
        && body
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
        && body.chars().filter(|c| *c == '.' || *c == ',').count() <= 1
}

#[cfg(test)]
mod tests {
    use super::super::tests::{entry, sample};
    use super::*;
    use crate::db::SpendEntry;

    fn uk() -> Locale {
        Locale::from_tag("en-GB")
    }

    fn text(report: &Report, locale: &Locale) -> String {
        String::from_utf8(render(report, locale, Sections::default())).unwrap()
    }

    #[test]
    fn opens_with_a_bom_and_ends_lines_the_way_the_standard_asks() {
        let csv = text(&sample(), &uk());
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("\r\n"));
    }

    #[test]
    fn amounts_are_plain_numbers_a_spreadsheet_can_total() {
        let csv = text(&sample(), &uk());
        assert!(csv.contains("Groceries,2,209.84,"), "{csv}");
        assert!(!csv.contains('£'));
    }

    #[test]
    fn comma_decimal_locales_get_semicolons() {
        let csv = text(&sample(), &Locale::from_tag("de-DE"));
        assert!(csv.contains("Groceries;2;209,84;"), "{csv}");
    }

    #[test]
    fn fields_are_quoted_when_they_have_to_be() {
        assert_eq!(field("plain", ','), "plain");
        assert_eq!(field("a,b", ','), "\"a,b\"");
        assert_eq!(field("a,b", ';'), "a,b");
        assert_eq!(field("say \"hi\"", ','), "\"say \"\"hi\"\"\"");
        assert_eq!(field("two\nlines", ','), "\"two\nlines\"");
    }

    #[test]
    fn spreadsheet_formulas_are_defused() {
        // The description is the user's text, and it stays text.
        for lead in ["=", "+", "-", "@"] {
            let defused = field(&format!("{lead}HYPERLINK(\"http://x\")"), ',');
            assert!(
                defused.starts_with('\'') || defused.starts_with("\"'"),
                "{defused}"
            );
        }

        let report = Report::build(
            2026,
            vec![SpendEntry {
                description: "=cmd|'/c calc'!A1".to_owned(),
                ..entry((2026, 2, 2), "Odd", 100)
            }],
        );
        let csv = text(&report, &uk());
        assert!(!csv.contains("\r\n=cmd"), "a formula reached a cell: {csv}");
        assert!(csv.contains("'=cmd"), "{csv}");
    }

    #[test]
    fn negative_amounts_stay_plain_numbers() {
        assert_eq!(field("-12.34", ','), "-12.34");
        // A comma decimal separator only stays unquoted when it is not also
        // the delimiter — render() picks ';' for exactly that locale.
        assert_eq!(field("-12,34", ';'), "-12,34");
        assert_eq!(field("-0", ','), "-0");
        // Still caught: a dangerous lead followed by anything but digits.
        assert_eq!(field("-HYPERLINK(1)", ','), "'-HYPERLINK(1)");
        assert_eq!(field("-1+1", ','), "'-1+1");

        let report = Report::build(2026, vec![entry((2026, 2, 2), "Refund", -1234)]);
        let csv = text(&report, &uk());
        assert!(csv.contains(",-12.34,"), "{csv}");
        assert!(!csv.contains("'-12.34"), "{csv}");
    }

    #[test]
    fn sections_can_be_left_out() {
        let bare = String::from_utf8(render(
            &sample(),
            &uk(),
            Sections {
                monthly: false,
                entries: false,
            },
        ))
        .unwrap();
        assert!(!bare.contains("Month"));
        assert!(!bare.contains("Description"));
        assert!(bare.contains("Category"));
    }
}
