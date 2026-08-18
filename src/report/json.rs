//! JSON, for another program to read.
//!
//! Amounts appear twice: as integer minor units, which is what a program
//! should compute with, and as the formatted string a person would recognise.
//! Dates are ISO, whatever the locale writes on screen, because a machine
//! reading this has no way to know which locale wrote it.

use serde_json::{Map, Value, json};

use super::{Report, Sections};
use crate::locale::Locale;

pub fn render(report: &Report, locale: &Locale, sections: Sections) -> Vec<u8> {
    let money = |minor: i64| {
        json!({
            "minor_units": minor,
            "formatted": locale.format_money(minor),
        })
    };

    let mut root = Map::new();
    root.insert("report".into(), json!("spending"));
    root.insert("year".into(), json!(report.year));
    root.insert(
        "generated".into(),
        json!(report.generated.format("%Y-%m-%d").to_string()),
    );
    root.insert(
        "currency".into(),
        json!({
            "code": locale.currency.code,
            "symbol": locale.currency.symbol,
            "minor_units_per_major": 10i64.pow(locale.currency.decimals),
        }),
    );
    root.insert("locale".into(), json!(locale.tag));
    root.insert("total".into(), money(report.total_minor));
    root.insert("entry_count".into(), json!(report.entries.len()));

    root.insert(
        "categories".into(),
        Value::Array(
            report
                .categories
                .iter()
                .map(|line| {
                    json!({
                        "name": line.name,
                        "entries": line.entries,
                        "total": money(line.total_minor),
                        "share_percent": (line.share * 10.0).round() / 10.0,
                    })
                })
                .collect(),
        ),
    );

    if sections.monthly {
        root.insert(
            "months".into(),
            Value::Array(
                report
                    .months
                    .iter()
                    .enumerate()
                    .map(|(i, total)| {
                        json!({
                            "month": i + 1,
                            "name": Report::month_name_in_english(i),
                            "total": money(*total),
                        })
                    })
                    .collect(),
            ),
        );
    }

    if sections.entries {
        root.insert(
            "entries".into(),
            Value::Array(
                report
                    .entries
                    .iter()
                    .map(|entry| {
                        json!({
                            "date": entry.spent_on.format("%Y-%m-%d").to_string(),
                            "category": entry.category,
                            "amount": money(entry.amount_minor),
                            "description": entry.description,
                        })
                    })
                    .collect(),
            ),
        );
    }

    let mut bytes =
        serde_json::to_vec_pretty(&Value::Object(root)).unwrap_or_else(|_| b"{}".to_vec());
    bytes.push(b'\n');
    bytes
}

#[cfg(test)]
mod tests {
    use super::super::tests::sample;
    use super::*;

    fn parsed(sections: Sections) -> Value {
        let bytes = render(&sample(), &Locale::from_tag("en-GB"), sections);
        serde_json::from_slice(&bytes).expect("valid JSON")
    }

    #[test]
    fn amounts_are_exact_integers_as_well_as_readable_strings() {
        let value = parsed(Sections::default());
        assert_eq!(value["total"]["minor_units"], 120_184);
        assert_eq!(value["total"]["formatted"], "£1,201.84");
        assert_eq!(value["currency"]["code"], "GBP");
    }

    #[test]
    fn dates_are_iso_whatever_the_locale_shows() {
        let value = parsed(Sections::default());
        assert_eq!(value["entries"][0]["date"], "2026-01-01");
    }

    #[test]
    fn every_month_is_present_so_a_reader_can_index_them() {
        let value = parsed(Sections::default());
        assert_eq!(value["months"].as_array().unwrap().len(), 12);
        assert_eq!(value["months"][0]["name"], "January");
    }

    #[test]
    fn sections_can_be_left_out() {
        let value = parsed(Sections {
            monthly: false,
            entries: false,
        });
        assert!(value.get("months").is_none());
        assert!(value.get("entries").is_none());
        assert!(value.get("categories").is_some());
    }
}
