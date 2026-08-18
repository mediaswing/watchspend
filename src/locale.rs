//! Locale detection, and the formatting rules that follow from it.
//!
//! Everything the user reads or types that is a number or a date goes through
//! here, so that a person in London sees `£12.34` and `11/08/2026` while a
//! person in Berlin sees `12,34 €` and `11.08.2026` — from the same code, and
//! parsed back the same way it was printed.
//!
//! The rules come from a table of regions rather than from a full CLDR
//! database: a real internationalisation library is a large dependency, and
//! this table covers the locales a desktop budgeting app is likely to meet.
//! Anything not in the table falls back to a neutral profile (ISO dates, the
//! generic currency sign) rather than quietly pretending to be American.

use chrono::NaiveDate;

use crate::{t, tn};

/// Order of the parts in a numeric date.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DateOrder {
    /// 11/08/2026 — most of Europe, Latin America, Africa, Oceania.
    Dmy,
    /// 08/11/2026 — the United States and a few others.
    Mdy,
    /// 2026-08-11 — ISO order, used natively in Sweden, Japan, China, Canada.
    Ymd,
}

/// How digits are grouped to the left of the decimal separator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Grouping {
    /// Groups of three: 1,234,567.
    Western,
    /// Three, then twos: 12,34,567 (the lakh/crore system).
    Indian,
}

/// Where the currency symbol sits relative to the digits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SymbolPosition {
    /// £12.34
    Prefix,
    /// 12,34 €
    Suffix,
}

#[derive(Clone, Copy, Debug)]
pub struct Currency {
    pub code: &'static str,
    pub symbol: &'static str,
    /// Digits after the decimal separator: 2 for most, 0 for yen and won.
    pub decimals: u32,
}

/// Everything the app needs to know about how this user writes things down.
#[derive(Clone, Debug)]
pub struct Locale {
    /// The BCP 47 tag we resolved from, e.g. `en-GB`.
    pub tag: String,
    pub currency: Currency,
    pub symbol_position: SymbolPosition,
    /// A space between symbol and digits (a non-breaking one, when printed).
    pub symbol_space: bool,
    pub decimal_sep: char,
    pub group_sep: char,
    pub grouping: Grouping,
    pub date_order: DateOrder,
    pub date_sep: char,
}

/// One row of the region table. Kept flat and `const` so the whole table reads
/// as data rather than as code.
struct RegionRules {
    region: &'static str,
    currency: &'static str,
    symbol_position: SymbolPosition,
    symbol_space: bool,
    decimal_sep: char,
    group_sep: char,
    grouping: Grouping,
    date_order: DateOrder,
    date_sep: char,
}

use DateOrder::{Dmy, Mdy, Ymd};
use Grouping::{Indian, Western};
use SymbolPosition::{Prefix, Suffix};

/// A narrow no-break space, which is what French and Nordic locales actually
/// use to group digits.
const NNBSP: char = '\u{202f}';
/// A no-break space, used the same way in several other locales.
const NBSP: char = '\u{00a0}';

/// Nine arguments is a lot for a function and exactly right for a row: the
/// table below is readable precisely because each locale is one line.
#[allow(clippy::too_many_arguments)]
const fn r(
    region: &'static str,
    currency: &'static str,
    symbol_position: SymbolPosition,
    symbol_space: bool,
    decimal_sep: char,
    group_sep: char,
    grouping: Grouping,
    date_order: DateOrder,
    date_sep: char,
) -> RegionRules {
    RegionRules {
        region,
        currency,
        symbol_position,
        symbol_space,
        decimal_sep,
        group_sep,
        grouping,
        date_order,
        date_sep,
    }
}

#[rustfmt::skip]
const REGIONS: &[RegionRules] = &[
    // Region  Currency  Symbol    Space  Dec  Group  Grouping  Date  Sep
    r("GB",    "GBP",    Prefix,   false, '.', ',',   Western,  Dmy,  '/'),
    r("IE",    "EUR",    Prefix,   false, '.', ',',   Western,  Dmy,  '/'),
    r("US",    "USD",    Prefix,   false, '.', ',',   Western,  Mdy,  '/'),
    r("CA",    "CAD",    Prefix,   false, '.', ',',   Western,  Ymd,  '-'),
    r("AU",    "AUD",    Prefix,   false, '.', ',',   Western,  Dmy,  '/'),
    r("NZ",    "NZD",    Prefix,   false, '.', ',',   Western,  Dmy,  '/'),
    r("DE",    "EUR",    Suffix,   true,  ',', '.',   Western,  Dmy,  '.'),
    r("AT",    "EUR",    Suffix,   true,  ',', '.',   Western,  Dmy,  '.'),
    r("CH",    "CHF",    Prefix,   true,  '.', '\u{2019}', Western, Dmy, '.'),
    r("FR",    "EUR",    Suffix,   true,  ',', NNBSP, Western,  Dmy,  '/'),
    r("BE",    "EUR",    Suffix,   true,  ',', '.',   Western,  Dmy,  '/'),
    r("NL",    "EUR",    Prefix,   true,  ',', '.',   Western,  Dmy,  '-'),
    r("LU",    "EUR",    Suffix,   true,  ',', '.',   Western,  Dmy,  '.'),
    r("ES",    "EUR",    Suffix,   true,  ',', '.',   Western,  Dmy,  '/'),
    r("IT",    "EUR",    Suffix,   true,  ',', '.',   Western,  Dmy,  '/'),
    r("PT",    "EUR",    Suffix,   true,  ',', '.',   Western,  Dmy,  '/'),
    r("GR",    "EUR",    Suffix,   true,  ',', '.',   Western,  Dmy,  '/'),
    r("FI",    "EUR",    Suffix,   true,  ',', NNBSP, Western,  Dmy,  '.'),
    r("SE",    "SEK",    Suffix,   true,  ',', NBSP,  Western,  Ymd,  '-'),
    r("NO",    "NOK",    Suffix,   true,  ',', NBSP,  Western,  Dmy,  '.'),
    r("DK",    "DKK",    Suffix,   true,  ',', '.',   Western,  Dmy,  '.'),
    r("IS",    "ISK",    Suffix,   true,  ',', '.',   Western,  Dmy,  '.'),
    r("PL",    "PLN",    Suffix,   true,  ',', NBSP,  Western,  Dmy,  '.'),
    r("CZ",    "CZK",    Suffix,   true,  ',', NBSP,  Western,  Dmy,  '.'),
    r("SK",    "EUR",    Suffix,   true,  ',', NBSP,  Western,  Dmy,  '.'),
    r("HU",    "HUF",    Suffix,   true,  ',', NBSP,  Western,  Ymd,  '.'),
    r("RO",    "RON",    Suffix,   true,  ',', '.',   Western,  Dmy,  '.'),
    r("BG",    "BGN",    Suffix,   true,  ',', NBSP,  Western,  Dmy,  '.'),
    r("HR",    "EUR",    Suffix,   true,  ',', '.',   Western,  Dmy,  '.'),
    r("SI",    "EUR",    Suffix,   true,  ',', '.',   Western,  Dmy,  '.'),
    r("RU",    "RUB",    Suffix,   true,  ',', NBSP,  Western,  Dmy,  '.'),
    r("UA",    "UAH",    Suffix,   true,  ',', NBSP,  Western,  Dmy,  '.'),
    r("TR",    "TRY",    Prefix,   false, ',', '.',   Western,  Dmy,  '.'),
    r("IL",    "ILS",    Prefix,   false, '.', ',',   Western,  Dmy,  '.'),
    r("AE",    "AED",    Prefix,   true,  '.', ',',   Western,  Dmy,  '/'),
    r("SA",    "SAR",    Prefix,   true,  '.', ',',   Western,  Dmy,  '/'),
    r("EG",    "EGP",    Prefix,   true,  '.', ',',   Western,  Dmy,  '/'),
    r("ZA",    "ZAR",    Prefix,   true,  ',', NBSP,  Western,  Ymd,  '/'),
    r("NG",    "NGN",    Prefix,   false, '.', ',',   Western,  Dmy,  '/'),
    r("KE",    "KES",    Prefix,   true,  '.', ',',   Western,  Dmy,  '/'),
    r("IN",    "INR",    Prefix,   false, '.', ',',   Indian,   Dmy,  '/'),
    r("PK",    "PKR",    Prefix,   true,  '.', ',',   Indian,   Dmy,  '/'),
    r("BD",    "BDT",    Prefix,   false, '.', ',',   Indian,   Dmy,  '/'),
    r("LK",    "LKR",    Prefix,   true,  '.', ',',   Western,  Dmy,  '/'),
    r("JP",    "JPY",    Prefix,   false, '.', ',',   Western,  Ymd,  '/'),
    r("CN",    "CNY",    Prefix,   false, '.', ',',   Western,  Ymd,  '/'),
    r("TW",    "TWD",    Prefix,   false, '.', ',',   Western,  Ymd,  '/'),
    r("KR",    "KRW",    Prefix,   false, '.', ',',   Western,  Ymd,  '.'),
    r("HK",    "HKD",    Prefix,   false, '.', ',',   Western,  Dmy,  '/'),
    r("SG",    "SGD",    Prefix,   false, '.', ',',   Western,  Dmy,  '/'),
    r("MY",    "MYR",    Prefix,   false, '.', ',',   Western,  Dmy,  '/'),
    r("TH",    "THB",    Prefix,   false, '.', ',',   Western,  Dmy,  '/'),
    r("VN",    "VND",    Suffix,   true,  ',', '.',   Western,  Dmy,  '/'),
    r("ID",    "IDR",    Prefix,   true,  ',', '.',   Western,  Dmy,  '/'),
    r("PH",    "PHP",    Prefix,   false, '.', ',',   Western,  Mdy,  '/'),
    r("BR",    "BRL",    Prefix,   true,  ',', '.',   Western,  Dmy,  '/'),
    r("MX",    "MXN",    Prefix,   false, '.', ',',   Western,  Dmy,  '/'),
    r("AR",    "ARS",    Prefix,   true,  ',', '.',   Western,  Dmy,  '/'),
    r("CL",    "CLP",    Prefix,   true,  ',', '.',   Western,  Dmy,  '-'),
    r("CO",    "COP",    Prefix,   true,  ',', '.',   Western,  Dmy,  '/'),
    r("PE",    "PEN",    Prefix,   true,  '.', ',',   Western,  Dmy,  '/'),
];

/// Symbols and minor-unit counts, keyed by ISO 4217 code.
#[rustfmt::skip]
const CURRENCIES: &[Currency] = &[
    Currency { code: "GBP", symbol: "£",   decimals: 2 },
    Currency { code: "EUR", symbol: "€",   decimals: 2 },
    Currency { code: "USD", symbol: "$",   decimals: 2 },
    Currency { code: "CAD", symbol: "$",   decimals: 2 },
    Currency { code: "AUD", symbol: "$",   decimals: 2 },
    Currency { code: "NZD", symbol: "$",   decimals: 2 },
    Currency { code: "CHF", symbol: "CHF", decimals: 2 },
    Currency { code: "SEK", symbol: "kr",  decimals: 2 },
    Currency { code: "NOK", symbol: "kr",  decimals: 2 },
    Currency { code: "DKK", symbol: "kr",  decimals: 2 },
    Currency { code: "ISK", symbol: "kr",  decimals: 0 },
    Currency { code: "PLN", symbol: "zł",  decimals: 2 },
    Currency { code: "CZK", symbol: "Kč",  decimals: 2 },
    Currency { code: "HUF", symbol: "Ft",  decimals: 2 },
    Currency { code: "RON", symbol: "lei", decimals: 2 },
    Currency { code: "BGN", symbol: "лв.", decimals: 2 },
    Currency { code: "RUB", symbol: "₽",   decimals: 2 },
    Currency { code: "UAH", symbol: "₴",   decimals: 2 },
    Currency { code: "TRY", symbol: "₺",   decimals: 2 },
    Currency { code: "ILS", symbol: "₪",   decimals: 2 },
    Currency { code: "AED", symbol: "AED", decimals: 2 },
    Currency { code: "SAR", symbol: "SAR", decimals: 2 },
    Currency { code: "EGP", symbol: "E£",  decimals: 2 },
    Currency { code: "ZAR", symbol: "R",   decimals: 2 },
    Currency { code: "NGN", symbol: "₦",   decimals: 2 },
    Currency { code: "KES", symbol: "KSh", decimals: 2 },
    Currency { code: "INR", symbol: "₹",   decimals: 2 },
    Currency { code: "PKR", symbol: "Rs",  decimals: 2 },
    Currency { code: "BDT", symbol: "৳",   decimals: 2 },
    Currency { code: "LKR", symbol: "Rs",  decimals: 2 },
    Currency { code: "JPY", symbol: "¥",   decimals: 0 },
    Currency { code: "CNY", symbol: "¥",   decimals: 2 },
    Currency { code: "TWD", symbol: "NT$", decimals: 2 },
    Currency { code: "KRW", symbol: "₩",   decimals: 0 },
    Currency { code: "HKD", symbol: "HK$", decimals: 2 },
    Currency { code: "SGD", symbol: "S$",  decimals: 2 },
    Currency { code: "MYR", symbol: "RM",  decimals: 2 },
    Currency { code: "THB", symbol: "฿",   decimals: 2 },
    Currency { code: "VND", symbol: "₫",   decimals: 0 },
    Currency { code: "IDR", symbol: "Rp",  decimals: 2 },
    Currency { code: "PHP", symbol: "₱",   decimals: 2 },
    Currency { code: "BRL", symbol: "R$",  decimals: 2 },
    Currency { code: "MXN", symbol: "$",   decimals: 2 },
    Currency { code: "ARS", symbol: "$",   decimals: 2 },
    Currency { code: "CLP", symbol: "$",   decimals: 0 },
    Currency { code: "COP", symbol: "$",   decimals: 2 },
    Currency { code: "PEN", symbol: "S/",  decimals: 2 },
];

/// What we use when the region is missing or unknown: ISO dates and the
/// generic currency sign, so nothing on screen claims to be a currency we did
/// not actually identify.
const UNKNOWN_CURRENCY: Currency = Currency {
    code: "XXX",
    symbol: "¤",
    decimals: 2,
};

impl Locale {
    /// Resolve the locale from the environment.
    ///
    /// `GAS_LOCALE` overrides the system setting, which is useful for trying
    /// the app as another locale would see it: `GAS_LOCALE=de-DE cargo run`.
    pub fn detect() -> Self {
        let tag = std::env::var("GAS_LOCALE")
            .ok()
            .filter(|t| !t.trim().is_empty())
            .or_else(sys_locale::get_locale)
            .unwrap_or_else(|| "und".to_owned());
        Self::from_tag(&tag)
    }

    pub fn from_tag(tag: &str) -> Self {
        // Accept both BCP 47 (`en-GB`) and POSIX (`en_GB.UTF-8`) spellings.
        let cleaned = tag.split(['.', '@']).next().unwrap_or(tag);
        let region = cleaned
            .split(['-', '_'])
            .find(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
            .filter(|part| part.chars().all(|c| c.is_ascii_uppercase()))
            .map(str::to_owned)
            // A bare `en` or `de` still tells us something; take the second
            // subtag whatever its case, then upper-case it.
            .or_else(|| {
                cleaned
                    .split(['-', '_'])
                    .nth(1)
                    .filter(|p| p.len() == 2)
                    .map(|p| p.to_ascii_uppercase())
            });

        let rules = region
            .as_deref()
            .and_then(|reg| REGIONS.iter().find(|r| r.region == reg));

        match rules {
            Some(rules) => Self {
                tag: cleaned.to_owned(),
                currency: currency_for(rules.currency),
                symbol_position: rules.symbol_position,
                symbol_space: rules.symbol_space,
                decimal_sep: rules.decimal_sep,
                group_sep: rules.group_sep,
                grouping: rules.grouping,
                date_order: rules.date_order,
                date_sep: rules.date_sep,
            },
            None => Self {
                tag: cleaned.to_owned(),
                currency: UNKNOWN_CURRENCY,
                symbol_position: Prefix,
                symbol_space: true,
                decimal_sep: '.',
                group_sep: ',',
                grouping: Western,
                date_order: Ymd,
                date_sep: '-',
            },
        }
    }

    // --- Money -------------------------------------------------------------

    /// Format an amount in minor units (pence, cents) with its symbol.
    pub fn format_money(&self, minor: i64) -> String {
        let sign = if minor < 0 { "-" } else { "" };
        let digits = self.format_digits(minor.unsigned_abs());
        let space = if self.symbol_space { "\u{00a0}" } else { "" };
        match self.symbol_position {
            Prefix => format!("{sign}{}{space}{digits}", self.currency.symbol),
            Suffix => format!("{sign}{digits}{space}{}", self.currency.symbol),
        }
    }

    /// The digits alone — grouped and with the right decimal separator, but no
    /// currency symbol. Used where the symbol is already in a column heading.
    pub fn format_digits(&self, minor: u64) -> String {
        let scale = 10u64.pow(self.currency.decimals);
        let whole = minor / scale;
        let frac = minor % scale;

        let mut out = self.group_digits(&whole.to_string());
        if self.currency.decimals > 0 {
            out.push(self.decimal_sep);
            out.push_str(&format!(
                "{frac:0width$}",
                width = self.currency.decimals as usize
            ));
        }
        out
    }

    fn group_digits(&self, digits: &str) -> String {
        // Build right to left, inserting a separator at each group boundary.
        let bytes: Vec<char> = digits.chars().collect();
        let mut out: Vec<char> = Vec::with_capacity(bytes.len() * 4 / 3);
        for (i, ch) in bytes.iter().rev().enumerate() {
            let boundary = match self.grouping {
                Western => i > 0 && i % 3 == 0,
                // Three digits, then groups of two: 1,23,45,678.
                Indian => i == 3 || (i > 3 && (i - 3) % 2 == 0),
            };
            if boundary {
                out.push(self.group_sep);
            }
            out.push(*ch);
        }
        out.iter().rev().collect()
    }

    /// The hint shown next to an amount field, e.g. `0.00`.
    pub fn amount_hint(&self) -> String {
        if self.currency.decimals == 0 {
            "0".to_owned()
        } else {
            format!(
                "0{}{}",
                self.decimal_sep,
                "0".repeat(self.currency.decimals as usize)
            )
        }
    }

    /// Parse an amount the user typed, in this locale's conventions, into
    /// minor units. Deliberately strict about the decimal separator: `1,50`
    /// means one and a half in Berlin and one hundred and fifty in London, and
    /// guessing which was meant is how money goes missing.
    pub fn parse_money(&self, input: &str) -> Result<i64, String> {
        let mut s: String = input.trim().to_owned();
        if s.is_empty() {
            return Err(t!("amount.enter"));
        }

        // Tolerate the symbol and the currency code being typed in.
        s = s.replace(self.currency.symbol, "");
        s = s.replace(self.currency.code, "");
        // Drop grouping separators and every kind of space.
        s.retain(|c| c != self.group_sep && !c.is_whitespace() && c != NBSP && c != NNBSP);

        let negative = s.starts_with('-');
        if negative || s.starts_with('+') {
            s.remove(0);
        }
        if s.is_empty() {
            return Err(t!("amount.enter"));
        }

        let (whole, frac) = match s.split_once(self.decimal_sep) {
            Some((w, f)) => (w, f),
            None => (s.as_str(), ""),
        };
        let whole = if whole.is_empty() { "0" } else { whole };

        if !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
            return Err(t!("amount.not_an_amount", separator = self.decimal_sep));
        }
        let decimals = self.currency.decimals as usize;
        if frac.len() > decimals {
            return Err(if decimals == 0 {
                t!("amount.no_decimals", code = self.currency.code)
            } else {
                tn!("amount.too_many_decimals", decimals as u64)
            });
        }

        let whole: i64 = whole.parse().map_err(|_| t!("amount.too_large"))?;
        let frac_padded = format!("{frac:0<decimals$}");
        let frac: i64 = if decimals == 0 {
            0
        } else {
            frac_padded.parse().map_err(|_| t!("amount.too_large"))?
        };

        let scale = 10i64.pow(self.currency.decimals);
        let minor = whole
            .checked_mul(scale)
            .and_then(|w| w.checked_add(frac))
            .ok_or_else(|| t!("amount.too_large"))?;

        if minor == 0 {
            return Err(t!("amount.greater_than_zero"));
        }
        Ok(if negative { -minor } else { minor })
    }

    // --- Dates -------------------------------------------------------------

    pub fn format_date(&self, date: NaiveDate) -> String {
        use chrono::Datelike as _;
        let (d, m, y) = (date.day(), date.month(), date.year());
        let sep = self.date_sep;
        match self.date_order {
            Dmy => format!("{d:02}{sep}{m:02}{sep}{y:04}"),
            Mdy => format!("{m:02}{sep}{d:02}{sep}{y:04}"),
            Ymd => format!("{y:04}{sep}{m:02}{sep}{d:02}"),
        }
    }

    /// The pattern to show as a hint next to a date field, e.g. `DD/MM/YYYY`.
    pub fn date_hint(&self) -> String {
        let sep = self.date_sep;
        match self.date_order {
            Dmy => format!("DD{sep}MM{sep}YYYY"),
            Mdy => format!("MM{sep}DD{sep}YYYY"),
            Ymd => format!("YYYY{sep}MM{sep}DD"),
        }
    }

    /// Parse a date in this locale's order. Any of `/ . -` is accepted as the
    /// separator whatever the locale prefers, since keyboards differ, but the
    /// *order* of the parts is the locale's and is not guessed at.
    pub fn parse_date(&self, input: &str) -> Result<NaiveDate, String> {
        let parts: Vec<&str> = input
            .trim()
            .split(['/', '.', '-', ' '])
            .filter(|p| !p.is_empty())
            .collect();
        if parts.len() != 3 || !parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
            return Err(t!("date.enter_as", format = self.date_hint()));
        }

        if parts.iter().any(|p| p.len() > 4) {
            return Err(t!("date.enter_as", format = self.date_hint()));
        }

        let n = |i: usize| parts[i].parse::<i32>().unwrap_or(-1);
        let (day_at, month_at, year_at) = match self.date_order {
            Dmy => (0, 1, 2),
            Mdy => (1, 0, 2),
            Ymd => (2, 1, 0),
        };
        let (d, m, mut y) = (n(day_at), n(month_at), n(year_at));

        // A year written with one or two digits means this century; anyone
        // budgeting for 1926 can type all four. It is the number of digits
        // that decides, not the value — `0000` is a typo, not the year 2000.
        if parts[year_at].len() <= 2 {
            y += 2000;
        }

        // Chrono is happy with the year 7, and so is SQLite; MySQL's DATE is
        // not, and neither is anyone's budget. Bound it to four-digit years so
        // a typo is caught here rather than by the database, or worse, stored.
        if !(1000..=9999).contains(&y) {
            return Err(t!("date.bad_year", format = self.date_hint()));
        }

        NaiveDate::from_ymd_opt(y, m as u32, d as u32)
            .ok_or_else(|| t!("date.no_such_date", format = self.date_hint()))
    }
}

fn currency_for(code: &str) -> Currency {
    CURRENCIES
        .iter()
        .copied()
        .find(|c| c.code == code)
        .unwrap_or(UNKNOWN_CURRENCY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uk() -> Locale {
        Locale::from_tag("en_GB.UTF-8")
    }

    #[test]
    fn uk_money_reads_as_sterling() {
        let l = uk();
        assert_eq!(l.currency.code, "GBP");
        assert_eq!(l.format_money(1234), "£12.34");
        assert_eq!(l.format_money(123_456_789), "£1,234,567.89");
        assert_eq!(l.format_money(-500), "-£5.00");
    }

    #[test]
    fn german_money_puts_the_symbol_last() {
        let l = Locale::from_tag("de-DE");
        assert_eq!(l.format_money(123_456), "1.234,56\u{a0}€");
        assert_eq!(l.parse_money("1.234,56"), Ok(123_456));
        // A full stop is the grouping separator here, not a decimal point.
        assert_eq!(l.parse_money("1.5"), Ok(1500));
    }

    #[test]
    fn indian_grouping_is_lakhs_and_crores() {
        let l = Locale::from_tag("hi-IN");
        assert_eq!(l.format_money(123_456_789), "₹12,34,567.89");
    }

    #[test]
    fn yen_has_no_minor_units() {
        let l = Locale::from_tag("ja-JP");
        assert_eq!(l.format_money(1234), "¥1,234");
        assert_eq!(l.parse_money("1234"), Ok(1234));
        assert!(l.parse_money("12.34").is_err());
    }

    #[test]
    fn unknown_locales_do_not_pretend_to_know_the_currency() {
        let l = Locale::from_tag("und");
        assert_eq!(l.currency.code, "XXX");
        assert_eq!(l.date_hint(), "YYYY-MM-DD");
    }

    #[test]
    fn money_round_trips_through_parsing() {
        let l = uk();
        for minor in [1, 99, 100, 4321, 10_000_000] {
            let text = l.format_money(minor);
            assert_eq!(l.parse_money(&text), Ok(minor), "round trip of {text}");
        }
    }

    #[test]
    fn money_parsing_rejects_the_ambiguous_and_the_impossible() {
        let l = uk();
        assert!(l.parse_money("").is_err());
        assert!(l.parse_money("twelve").is_err());
        assert!(l.parse_money("1.234").is_err()); // three decimal places
        assert!(l.parse_money("0.00").is_err()); // nothing was spent
        assert_eq!(l.parse_money("£1,234.50"), Ok(123_450));
        assert_eq!(l.parse_money(" 7 "), Ok(700));
        assert_eq!(l.parse_money(".5"), Ok(50));
    }

    #[test]
    fn dates_use_the_local_order() {
        let uk = uk();
        let us = Locale::from_tag("en-US");
        let date = NaiveDate::from_ymd_opt(2026, 8, 11).unwrap();
        assert_eq!(uk.format_date(date), "11/08/2026");
        assert_eq!(us.format_date(date), "08/11/2026");
        assert_eq!(uk.date_hint(), "DD/MM/YYYY");
        assert_eq!(uk.parse_date("11/08/2026"), Ok(date));
        assert_eq!(us.parse_date("08/11/2026"), Ok(date));
        // The same text means different days in the two locales.
        assert_ne!(uk.parse_date("03/04/2026"), us.parse_date("03/04/2026"));
    }

    #[test]
    fn dates_are_forgiving_about_separators_but_not_about_order() {
        let l = uk();
        let date = NaiveDate::from_ymd_opt(2026, 8, 11).unwrap();
        assert_eq!(l.parse_date("11-8-2026"), Ok(date));
        assert_eq!(l.parse_date("11.08.26"), Ok(date));
        assert!(l.parse_date("31/02/2026").is_err()); // no such day
        assert!(l.parse_date("2026/08/11").is_err()); // wrong order for en-GB
        assert!(l.parse_date("11/08").is_err());
    }

    #[test]
    fn years_outside_four_digits_are_refused() {
        let l = uk();
        assert!(l.parse_date("11/08/0000").is_err());
        assert!(l.parse_date("11/08/0999").is_err());
        assert!(l.parse_date("11/08/99999").is_err());
        assert!(l.parse_date("11/08/1000").is_ok());
        assert!(l.parse_date("11/08/9999").is_ok());
    }
}
