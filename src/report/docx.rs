//! A Word document, written directly.
//!
//! A `.docx` is a zip of a few XML parts, and the handful this needs —
//! paragraphs, tables, page setup — is a small, stable corner of OOXML. The
//! crates that build these files carry an image codec and an XML reader for
//! the sake of features a spending report will never use, which is a lot of
//! dependency for text in a table.
//!
//! The escaping this shares with the HTML writer is the part that has to be
//! right: an unescaped `&` in a category name is the difference between a
//! document and a file Word refuses to open.

use std::io::{Cursor, Write as _};

use zip::write::SimpleFileOptions;

use super::{Report, Sections, escape_markup, share};
use crate::locale::Locale;

/// Twips — twentieths of a point — are OOXML's unit for anything on the page.
/// A4 is 210×297mm, and the margins are 2cm all round.
const PAGE_WIDTH: u32 = 11906;
const PAGE_HEIGHT: u32 = 16838;
const MARGIN: u32 = 1134;
/// The width tables and their columns share out.
const CONTENT_WIDTH: u32 = PAGE_WIDTH - 2 * MARGIN;

pub fn render(report: &Report, locale: &Locale, sections: Sections) -> Result<Vec<u8>, String> {
    let document = document_xml(report, locale, sections);
    // Packaging writes to a `Vec`, so this should not fail — but if it ever
    // did, a half-built document saved as though it were whole is the one
    // outcome worth ruling out.
    package(&document).map_err(|err| format!("Could not build the Word document: {err}"))
}

fn document_xml(report: &Report, locale: &Locale, sections: Sections) -> String {
    let mut body = String::with_capacity(8192);

    body.push_str(&heading(&format!("Spending report {}", report.year), 36));
    body.push_str(&muted(&format!(
        "Generated {} · {} · {} {} across {} {}",
        locale.format_date(report.generated),
        locale.currency.code,
        report.entries.len(),
        plural(report.entries.len(), "entry", "entries"),
        report.categories.len(),
        plural(report.categories.len(), "category", "categories"),
    )));
    body.push_str(&paragraph_runs(&[
        run("Total spent: ", false, 24),
        run(&locale.format_money(report.total_minor), true, 24),
    ]));

    body.push_str(&heading("By category", 28));
    let widths = share_out(&[46, 14, 22, 18]);
    body.push_str(&table(
        &widths,
        &[header_cells(
            &["Category", "Entries", "Total", "Share"],
            &[false, true, true, true],
        )],
        &report
            .categories
            .iter()
            .map(|line| {
                cells(
                    &[
                        line.name.clone(),
                        line.entries.to_string(),
                        locale.format_money(line.total_minor),
                        share(line.share, locale),
                    ],
                    &[false, true, true, true],
                    false,
                )
            })
            .chain(std::iter::once(cells(
                &[
                    "Total".to_owned(),
                    report.entries.len().to_string(),
                    locale.format_money(report.total_minor),
                    String::new(),
                ],
                &[false, true, true, true],
                true,
            )))
            .collect::<Vec<_>>(),
    ));

    if sections.monthly {
        body.push_str(&heading("By month", 28));
        let widths = share_out(&[60, 40]);
        body.push_str(&table(
            &widths,
            &[header_cells(&["Month", "Total"], &[false, true])],
            &report
                .months_with_spending()
                .into_iter()
                .map(|(name, total)| {
                    cells(
                        &[name.to_owned(), locale.format_money(total)],
                        &[false, true],
                        false,
                    )
                })
                .collect::<Vec<_>>(),
        ));
    }

    if sections.entries {
        body.push_str(&heading("Entries", 28));
        let widths = share_out(&[18, 26, 18, 38]);
        body.push_str(&table(
            &widths,
            &[header_cells(
                &["Date", "Category", "Amount", "Description"],
                &[false, false, true, false],
            )],
            &report
                .entries
                .iter()
                .map(|entry| {
                    cells(
                        &[
                            locale.format_date(entry.spent_on),
                            entry.category.clone(),
                            locale.format_money(entry.amount_minor),
                            entry.description.clone(),
                        ],
                        &[false, false, true, false],
                        false,
                    )
                })
                .collect::<Vec<_>>(),
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="{W}"><w:body>{body}<w:sectPr>\
<w:pgSz w:w="{PAGE_WIDTH}" w:h="{PAGE_HEIGHT}"/>\
<w:pgMar w:top="{MARGIN}" w:right="{MARGIN}" w:bottom="{MARGIN}" w:left="{MARGIN}" w:header="708" w:footer="708" w:gutter="0"/>\
</w:sectPr></w:body></w:document>"#
    )
    .replace("\\\n", "")
}

/// The WordprocessingML namespace, spelled once.
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}

/// Column widths in twips from a set of percentages.
fn share_out(percentages: &[u32]) -> Vec<u32> {
    percentages
        .iter()
        .map(|pct| CONTENT_WIDTH * pct / 100)
        .collect()
}

fn run(text: &str, bold: bool, half_points: u32) -> String {
    let bold = if bold { "<w:b/>" } else { "" };
    format!(
        r#"<w:r><w:rPr>{bold}<w:sz w:val="{half_points}"/><w:szCs w:val="{half_points}"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
        escape_markup(text)
    )
}

fn paragraph_runs(runs: &[String]) -> String {
    format!(
        r#"<w:p><w:pPr><w:spacing w:after="120"/></w:pPr>{}</w:p>"#,
        runs.concat()
    )
}

fn heading(text: &str, half_points: u32) -> String {
    format!(
        r#"<w:p><w:pPr><w:spacing w:before="240" w:after="120"/><w:keepNext/></w:pPr>{}</w:p>"#,
        run(text, true, half_points)
    )
}

fn muted(text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:spacing w:after="120"/></w:pPr><w:r><w:rPr><w:color w:val="595959"/><w:sz w:val="18"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
        escape_markup(text)
    )
}

/// A cell's paragraph: text, alignment, and whether it is bold.
fn cell(text: &str, right: bool, bold: bool, width: u32) -> String {
    let justify = if right {
        r#"<w:jc w:val="right"/>"#
    } else {
        ""
    };
    format!(
        r#"<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/></w:tcPr><w:p><w:pPr>{justify}<w:spacing w:after="0"/></w:pPr>{}</w:p></w:tc>"#,
        run(text, bold, 20)
    )
}

/// A row of body cells; `widths` is filled in later by [`table`].
fn cells(texts: &[String], right: &[bool], bold: bool) -> Vec<(String, bool, bool)> {
    texts
        .iter()
        .zip(right)
        .map(|(text, right)| (text.clone(), *right, bold))
        .collect()
}

fn header_cells(texts: &[&str], right: &[bool]) -> Vec<(String, bool, bool)> {
    texts
        .iter()
        .zip(right)
        .map(|(text, right)| ((*text).to_owned(), *right, true))
        .collect()
}

type Row = Vec<(String, bool, bool)>;

fn table(widths: &[u32], header: &[Row], rows: &[Row]) -> String {
    let grid: String = widths
        .iter()
        .map(|w| format!(r#"<w:gridCol w:w="{w}"/>"#))
        .collect();

    let render_row = |row: &Row, is_header: bool| {
        let cells: String = row
            .iter()
            .zip(widths)
            .map(|((text, right, bold), width)| cell(text, *right, *bold, *width))
            .collect();
        // A header marked as one repeats at the top of every page, which
        // matters as soon as the entries run past the first.
        let props = if is_header {
            "<w:trPr><w:tblHeader/></w:trPr>"
        } else {
            ""
        };
        format!("<w:tr>{props}{cells}</w:tr>")
    };

    let head: String = header.iter().map(|row| render_row(row, true)).collect();
    let body: String = rows.iter().map(|row| render_row(row, false)).collect();

    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders>\
<w:top w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>\
<w:left w:val="none" w:sz="0" w:space="0" w:color="auto"/>\
<w:bottom w:val="single" w:sz="4" w:space="0" w:color="BFBFBF"/>\
<w:right w:val="none" w:sz="0" w:space="0" w:color="auto"/>\
<w:insideH w:val="single" w:sz="4" w:space="0" w:color="D9D9D9"/>\
<w:insideV w:val="none" w:sz="0" w:space="0" w:color="auto"/>\
</w:tblBorders><w:tblCellMar>\
<w:top w:w="60" w:type="dxa"/><w:left w:w="80" w:type="dxa"/>\
<w:bottom w:w="60" w:type="dxa"/><w:right w:w="80" w:type="dxa"/>\
</w:tblCellMar></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{head}{body}</w:tbl>"#
    )
    .replace("\\\n", "")
}

/// Zip the parts into the package Word expects.
fn package(document: &str) -> Result<Vec<u8>, zip::result::ZipError> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();

    let parts: [(&str, &str); 4] = [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS),
        ("word/styles.xml", STYLES),
    ];
    for (name, contents) in parts {
        zip.start_file(name, options)?;
        zip.write_all(contents.as_bytes())?;
    }
    zip.start_file("word/document.xml", options)?;
    zip.write_all(document.as_bytes())?;

    Ok(zip.finish()?.into_inner())
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

const DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;

/// Only the document defaults: every other bit of formatting in this file is
/// applied directly, so there are no style names to keep in step.
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:cs="Calibri"/><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="120"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style></w:styles>"#;

#[cfg(test)]
mod tests {
    use super::super::tests::sample;
    use super::*;
    use std::io::Read as _;

    fn parts() -> std::collections::HashMap<String, String> {
        let bytes = render(&sample(), &Locale::from_tag("en-GB"), Sections::default()).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("a readable zip");
        let mut parts = std::collections::HashMap::new();
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).unwrap();
            let mut contents = String::new();
            file.read_to_string(&mut contents).unwrap();
            parts.insert(file.name().to_owned(), contents);
        }
        parts
    }

    #[test]
    fn the_package_holds_the_parts_word_looks_for() {
        let parts = parts();
        for name in [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/_rels/document.xml.rels",
            "word/styles.xml",
            "word/document.xml",
        ] {
            assert!(parts.contains_key(name), "missing {name}");
        }
    }

    #[test]
    fn the_document_is_well_formed_and_complete() {
        let parts = parts();
        let document = &parts["word/document.xml"];
        assert!(document.starts_with("<?xml"));
        assert!(document.ends_with("</w:document>"));
        // The line continuations in the templates must not survive into the
        // file: a stray backslash inside a tag would break the XML.
        assert!(!document.contains('\\'));
        assert_eq!(
            document.matches("<w:tbl>").count(),
            document.matches("</w:tbl>").count()
        );
        assert_eq!(
            document.matches("<w:tc>").count(),
            document.matches("</w:tc>").count()
        );
    }

    #[test]
    fn user_text_is_escaped_into_the_xml() {
        let parts = parts();
        let document = &parts["word/document.xml"];
        assert!(
            document.contains("birthday &lt;dinner&gt; &amp; drinks"),
            "{document}"
        );
        assert!(!document.contains("<dinner>"));
    }

    #[test]
    fn the_figures_are_in_there() {
        let parts = parts();
        let document = &parts["word/document.xml"];
        assert!(document.contains("£1,201.84"));
        assert!(document.contains("Groceries"));
        assert!(document.contains("Spending report 2026"));
    }
}
