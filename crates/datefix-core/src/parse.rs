//! Finding a date in a file name.

use chrono::NaiveDate;
use fancy_regex::{Captures, Regex};
use std::sync::OnceLock;

/// The shapes of date DateFix understands, tried in this order.
#[derive(Clone, Copy)]
enum Shape {
    /// 2023-11-05, 2023_11_05, 2023.11.05, 2023 11 05
    YearFirst,
    /// 05-11-2023, 5.11.2023: day first, or month first when asked
    NumbersYearLast,
    /// 11 November 2022, 3rd July 2021, 11Nov23
    DayThenMonthName,
    /// November 11 2022, Nov 22, 2022, Sept 3 2021
    MonthNameThenDay,
    /// 20231105 (years 1900-2099 only)
    Compact,
}

const SEP: &str = r"[-._ ]"; // between numeric date parts
const GAP: &str = r"[-._ ,]*"; // around a month name
const ORD: &str = r"(?:st|nd|rd|th)?"; // 1st, 2nd, 3rd, 4th

fn patterns() -> &'static [(Shape, Regex)] {
    static PATTERNS: OnceLock<Vec<(Shape, Regex)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let p = |shape, re: String| (shape, Regex::new(&re).expect("valid date pattern"));
        vec![
            p(
                Shape::YearFirst,
                format!(r"(?<!\d)(?P<y>\d{{4}})(?P<s>{SEP})(?P<m>\d{{1,2}})\k<s>(?P<d>\d{{1,2}})(?!\d)"),
            ),
            p(
                Shape::NumbersYearLast,
                format!(r"(?<!\d)(?P<a>\d{{1,2}})(?P<s>{SEP})(?P<b>\d{{1,2}})\k<s>(?P<y>\d{{4}})(?!\d)"),
            ),
            p(
                Shape::DayThenMonthName,
                format!(r"(?<!\d)(?P<d>\d{{1,2}}){ORD}{GAP}(?P<mon>[A-Za-z]{{3,9}}){GAP}(?P<y>\d{{4}}|\d{{2}})(?!\d)"),
            ),
            p(
                Shape::MonthNameThenDay,
                format!(r"(?<![A-Za-z])(?P<mon>[A-Za-z]{{3,9}}){GAP}(?P<d>\d{{1,2}}){ORD}{GAP}(?P<y>\d{{4}}|\d{{2}})(?!\d)"),
            ),
            p(
                Shape::Compact,
                r"(?<!\d)(?P<y>(?:19|20)\d{2})(?P<m>\d{2})(?P<d>\d{2})(?!\d)".to_string(),
            ),
        ]
    })
}

/// Month name (full, 3-letter, or "sept") to its number.
fn month_number(name: &str) -> Option<u32> {
    const NAMES: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    let name = name.to_ascii_lowercase();
    if name == "sept" {
        return Some(9);
    }
    NAMES
        .iter()
        .position(|full| *full == name || full[..3] == name)
        .map(|i| i as u32 + 1)
}

/// 2-digit years: 00-49 become 20xx, 50-99 become 19xx.
fn full_year(year: &str) -> Option<i32> {
    let n: i32 = year.parse().ok()?;
    Some(match year.len() {
        2 if n < 50 => 2000 + n,
        2 => 1900 + n,
        _ => n,
    })
}

fn date_from(caps: &Captures, shape: Shape, month_first: bool) -> Option<NaiveDate> {
    let num = |name: &str| caps.name(name)?.as_str().parse::<u32>().ok();
    let year = full_year(caps.name("y")?.as_str())?;
    let (day, month) = match shape {
        Shape::YearFirst | Shape::Compact => (num("d")?, num("m")?),
        Shape::NumbersYearLast if month_first => (num("b")?, num("a")?),
        Shape::NumbersYearLast => (num("a")?, num("b")?),
        Shape::DayThenMonthName | Shape::MonthNameThenDay => {
            (num("d")?, month_number(caps.name("mon")?.as_str())?)
        }
    };
    NaiveDate::from_ymd_opt(year, month, day)
}

/// A date found in a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundDate {
    /// The date as YYYY-MM-DD.
    pub iso: String,
    /// The text in the name that held the date.
    pub matched: String,
}

/// Finds the first real date in a name. With `month_first`, 05-11-2023 is
/// read as 11 May (US style) instead of 5 November.
pub fn extract_date(name: &str, month_first: bool) -> Option<FoundDate> {
    for (shape, re) in patterns() {
        for caps in re.captures_iter(name).flatten() {
            if let Some(date) = date_from(&caps, *shape, month_first) {
                return Some(FoundDate {
                    iso: date.format("%Y-%m-%d").to_string(),
                    matched: caps.get(0)?.as_str().to_string(),
                });
            }
        }
    }
    None
}

/// The fixed name, or `None` when the name has no date or is already fine.
pub fn new_name_for(name: &str, month_first: bool) -> Option<String> {
    let found = extract_date(name, month_first)?;
    if found.matched == found.iso {
        return None;
    }
    Some(name.replacen(&found.matched, &found.iso, 1))
}
