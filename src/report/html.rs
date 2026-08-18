//! HTML, for reading in a browser and printing from one.
//!
//! One self-contained file: no scripts, no external stylesheet, no fonts to
//! fetch. It can be opened from a memory stick on a machine with no network
//! and look the same, and printing it is how this app makes a PDF.

use super::{Report, Sections, escape_markup, share};
use crate::locale::Locale;
use crate::{i18n, t, tn};

pub fn render(report: &Report, locale: &Locale, sections: Sections) -> Vec<u8> {
    let mut out = String::with_capacity(4096);
    let title = t!("report.title", year = report.year);

    // The page says which language it is in, so a screen reader picks the right
    // voice for it and a browser offers the right translation — which it cannot
    // work out from the text, and would otherwise get wrong for every reader of
    // a French report.
    out.push_str(&format!(
        "<!doctype html>\n<html lang=\"{}\">\n<head>\n",
        escape_markup(&i18n::current_code())
    ));
    out.push_str("<meta charset=\"utf-8\"/>\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/>\n");
    out.push_str(&format!("<title>{}</title>\n", escape_markup(&title)));
    out.push_str(STYLE);
    out.push_str("</head>\n<body>\n");

    out.push_str(&format!("<h1>{}</h1>\n", escape_markup(&title)));
    out.push_str(&format!(
        "<p class=\"meta\">{}</p>\n",
        escape_markup(&meta(report, locale))
    ));
    out.push_str(&format!(
        "<p class=\"total\">{} <strong>{}</strong></p>\n",
        escape_markup(&t!("report.total_spent")),
        escape_markup(&locale.format_money(report.total_minor))
    ));

    out.push_str(&format!(
        "<h2>{}</h2>\n<table>\n<thead><tr><th>{}</th>\
         <th class=\"n\">{}</th><th class=\"n\">{}</th><th class=\"n\">{}</th></tr></thead>\n<tbody>\n",
        escape_markup(&t!("report.by_category")),
        escape_markup(&t!("report.column.category")),
        escape_markup(&t!("report.column.entries")),
        escape_markup(&t!("report.column.total")),
        escape_markup(&t!("report.column.share")),
    ));
    for line in &report.categories {
        out.push_str(&format!(
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td></tr>\n",
            escape_markup(&line.name),
            line.entries,
            escape_markup(&locale.format_money(line.total_minor)),
            escape_markup(&share(line.share, locale)),
        ));
    }
    out.push_str(&format!(
        "</tbody>\n<tfoot><tr><th>{}</th><th class=\"n\">{}</th><th class=\"n\">{}</th><th class=\"n\"></th></tr></tfoot>\n</table>\n",
        escape_markup(&t!("report.column.total")),
        report.entries.len(),
        escape_markup(&locale.format_money(report.total_minor)),
    ));

    if sections.monthly {
        out.push_str(&format!(
            "<h2>{}</h2>\n<table>\n<thead><tr><th>{}</th><th class=\"n\">{}</th></tr></thead>\n<tbody>\n",
            escape_markup(&t!("report.by_month")),
            escape_markup(&t!("report.column.month")),
            escape_markup(&t!("report.column.total")),
        ));
        for (name, total) in report.months_with_spending() {
            out.push_str(&format!(
                "<tr><td>{}</td><td class=\"n\">{}</td></tr>\n",
                escape_markup(&name),
                escape_markup(&locale.format_money(total))
            ));
        }
        out.push_str("</tbody>\n</table>\n");
    }

    if sections.entries {
        out.push_str(&format!(
            "<h2>{}</h2>\n<table>\n<thead><tr><th>{}</th><th>{}</th>\
             <th class=\"n\">{}</th><th>{}</th></tr></thead>\n<tbody>\n",
            escape_markup(&t!("report.column.entries")),
            escape_markup(&t!("report.column.date")),
            escape_markup(&t!("report.column.category")),
            escape_markup(&t!("report.column.amount")),
            escape_markup(&t!("report.column.description")),
        ));
        for entry in &report.entries {
            out.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"n\">{}</td><td>{}</td></tr>\n",
                escape_markup(&locale.format_date(entry.spent_on)),
                escape_markup(&entry.category),
                escape_markup(&locale.format_money(entry.amount_minor)),
                escape_markup(&entry.description),
            ));
        }
        out.push_str("</tbody>\n</table>\n");
    }

    out.push_str("<p class=\"foot\">Generic Accounting System</p>\n</body>\n</html>\n");
    out.into_bytes()
}

/// The line under the heading, shared with the Word writer so the two documents
/// say the same thing. Each of the two counts is a counted message in its own
/// right, because how a language words "one category" is that language's
/// business and not something a sentence built around it should decide.
pub(super) fn meta(report: &Report, locale: &Locale) -> String {
    t!(
        "report.meta",
        date = locale.format_date(report.generated),
        code = locale.currency.code,
        entries = tn!("report.entry_count", report.entries.len() as u64),
        categories = tn!("report.category_count", report.categories.len() as u64),
    )
}

/// Readable on screen, sane on paper, and legible in either colour scheme —
/// a report printed from a machine in dark mode should not arrive as white
/// text on white paper.
const STYLE: &str = r#"<style>
  :root { color-scheme: light dark; }
  body {
    font-family: Ubuntu, "Helvetica Neue", Arial, sans-serif;
    margin: 2rem auto; max-width: 50rem; padding: 0 1rem; line-height: 1.5;
  }
  h1 { font-size: 1.6rem; margin-bottom: 0.25rem; }
  h2 { font-size: 1.15rem; margin-top: 2rem; }
  .meta { color: #666; margin-top: 0; }
  .total { font-size: 1.1rem; }
  table { border-collapse: collapse; width: 100%; margin-top: 0.5rem; }
  th, td { text-align: left; padding: 0.4rem 0.6rem; border-bottom: 1px solid #ddd; }
  th.n, td.n { text-align: right; font-variant-numeric: tabular-nums; }
  tfoot th { border-top: 2px solid #999; border-bottom: none; }
  tbody tr:nth-child(even) { background: rgba(127, 127, 127, 0.08); }
  .foot { color: #666; font-size: 0.85rem; margin-top: 2.5rem; }
  @media print {
    body { margin: 0; max-width: none; }
    h2 { break-after: avoid; }
    tr { break-inside: avoid; }
  }
</style>
"#;

#[cfg(test)]
mod tests {
    use super::super::tests::sample;
    use super::*;

    fn page() -> String {
        String::from_utf8(render(
            &sample(),
            &Locale::from_tag("en-GB"),
            Sections::default(),
        ))
        .unwrap()
    }

    #[test]
    fn is_a_whole_self_contained_document() {
        let html = page();
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.trim_end().ends_with("</html>"));
        // Nothing to fetch: no scripts, no remote anything.
        assert!(!html.contains("<script"));
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
    }

    /// Walk the tags and check every one that opens is closed, in order.
    ///
    /// A browser will paper over a stray `</td>`; a report that is quietly
    /// missing its last three rows in one browser and not another is the kind
    /// of thing nobody notices until the figures are being argued about.
    #[test]
    fn every_tag_that_opens_is_closed_in_order() {
        let html = page();
        let mut stack: Vec<String> = Vec::new();
        let mut rest = html.as_str();

        while let Some(start) = rest.find('<') {
            rest = &rest[start + 1..];
            let Some(end) = rest.find('>') else { break };
            let tag = &rest[..end];
            rest = &rest[end + 1..];

            // Skip the doctype, comments and self-closing tags.
            if tag.starts_with('!') || tag.ends_with('/') {
                continue;
            }
            let name: String = tag
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if tag.starts_with('/') {
                assert_eq!(
                    stack.pop().as_deref(),
                    Some(name.as_str()),
                    "</{name}> closes something that was not open"
                );
            } else {
                stack.push(name);
            }
        }
        assert!(stack.is_empty(), "left open: {stack:?}");
    }

    #[test]
    fn user_text_cannot_become_markup() {
        let html = page();
        assert!(
            html.contains("birthday &lt;dinner&gt; &amp; drinks"),
            "{html}"
        );
        assert!(!html.contains("<dinner>"));
    }

    #[test]
    fn figures_are_the_locales_own() {
        let html = page();
        assert!(html.contains("£1,201.84"), "{html}");
        let german = String::from_utf8(render(
            &sample(),
            &Locale::from_tag("de-DE"),
            Sections::default(),
        ))
        .unwrap();
        assert!(german.contains("1.201,84\u{a0}€"), "{german}");
    }
}
