//! Finding a date in a file name.

use chrono::NaiveDate;
use fancy_regex::{Captures, Regex};
use std::ops::Range;
use std::sync::OnceLock;

/// The shapes of date DateFix understands. When two shapes find a date at
/// the same place, the one listed first wins.
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
    /// 05-11-23, 5-11-23, 05.11.23: day first, or month first when asked
    NumbersShortYear,
}

const SEP: &str = r"[-._ ]"; // between numeric date parts
const GAP: &str = r"[-._ ,]*"; // around a month name
const ORD: &str = r"(?:st|nd|rd|th)?"; // 1st, 2nd, 3rd, 4th
/// A month name, full or short, and not the start of a longer word
/// ("Mar" but not "Mars" or "Market").
const MONTH: &str = r"(?i:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sept(?:ember)?|sep|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)(?![A-Za-z])";
/// Before a numeric date: not another digit, not "v" (v1.2.2023) and not
/// "digit." (the tail of 10.1.2.2023), so version numbers aren't dates.
const NUM_START: &str = r"(?<![\dvV])(?<!\d\.)";
/// After a numeric date: not another digit, and not ".digit" (5.11.2023.1).
const NUM_END: &str = r"(?!\d)(?!\.\d)";

fn patterns() -> &'static [(Shape, Regex)] {
    static PATTERNS: OnceLock<Vec<(Shape, Regex)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let p = |shape, re: String| (shape, Regex::new(&re).expect("valid date pattern"));
        vec![
            p(
                Shape::YearFirst,
                format!(r"{NUM_START}(?P<y>\d{{4}})(?P<s>{SEP})(?P<m>\d{{1,2}})\k<s>(?P<d>\d{{1,2}}){NUM_END}"),
            ),
            p(
                Shape::NumbersYearLast,
                format!(r"{NUM_START}(?P<a>\d{{1,2}})(?P<s>{SEP})(?P<b>\d{{1,2}})\k<s>(?P<y>\d{{4}}){NUM_END}"),
            ),
            p(
                Shape::DayThenMonthName,
                format!(r"(?<!\d)(?P<d>\d{{1,2}}){ORD}{GAP}(?P<mon>{MONTH}){GAP}(?P<y>\d{{4}}|\d{{2}})(?!\d)"),
            ),
            p(
                // The day and year must be apart, so "June 2024" isn't
                // read as day 20 of '24.
                Shape::MonthNameThenDay,
                format!(r"(?<![A-Za-z])(?P<mon>{MONTH}){GAP}(?P<d>\d{{1,2}})(?!\d){ORD}[-._ ,]+(?P<y>\d{{4}}|\d{{2}})(?!\d)"),
            ),
            p(
                Shape::Compact,
                r"(?<!\d)(?P<y>(?:19|20)\d{2})(?P<m>\d{2})(?P<d>\d{2})(?!\d)".to_string(),
            ),
            p(
                Shape::NumbersShortYear,
                format!(r"{NUM_START}(?P<a>\d{{1,2}})(?P<s>[-.])(?P<b>\d{{1,2}})\k<s>(?P<y>\d{{2}}){NUM_END}"),
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
    let text = |name: &str| caps.name(name).map(|m| m.as_str());
    let num = |name: &str| text(name)?.parse::<u32>().ok();
    let year = full_year(text("y")?)?;
    let real = |day, month| NaiveDate::from_ymd_opt(year, month, day);
    match shape {
        Shape::YearFirst | Shape::Compact => real(num("d")?, num("m")?),
        Shape::DayThenMonthName | Shape::MonthNameThenDay => {
            real(num("d")?, month_number(text("mon")?)?)
        }
        Shape::NumbersYearLast | Shape::NumbersShortYear => {
            let (a, b) = (num("a")?, num("b")?);
            let (day, month) = if month_first { (b, a) } else { (a, b) };
            match shape {
                // With a 4-digit year, if only the other order is a real
                // date (3.13.2024), use it.
                Shape::NumbersYearLast => real(day, month).or_else(|| real(month, day)),
                // 2-digit years are less certain: "1.2.23" is more likely a
                // version than a date, so with dots both parts need 2 digits,
                // and the order is never swapped.
                _ => {
                    let dots = text("s")? == ".";
                    if dots && (text("a")?.len() != 2 || text("b")?.len() != 2) {
                        return None;
                    }
                    real(day, month)
                }
            }
        }
    }
}

/// A date found in a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundDate {
    /// The date as YYYY-MM-DD.
    pub iso: String,
    /// The text in the name that held the date.
    pub matched: String,
}

/// A real date and where it is in the name.
struct Candidate {
    date: NaiveDate,
    span: Range<usize>,
    /// Has a 4-digit year, so it's more certain than a 2-digit one.
    full_year: bool,
}

/// Every real date `re` finds in `name`. After each match the search moves
/// on by one character, so a match that isn't a real date can't hide a real
/// date that overlaps it ("3 notes 14 March 2024").
fn all_in(name: &str, shape: Shape, re: &Regex, month_first: bool, out: &mut Vec<Candidate>) {
    let mut pos = 0;
    while let Ok(Some(caps)) = re.captures_from_pos(name, pos) {
        let (Some(whole), Some(year)) = (caps.get(0), caps.name("y")) else {
            return;
        };
        if let Some(date) = date_from(&caps, shape, month_first) {
            out.push(Candidate {
                date,
                span: whole.range(),
                full_year: year.as_str().len() == 4,
            });
        }
        let Some(first) = name[whole.start()..].chars().next() else {
            return;
        };
        pos = whole.start() + first.len_utf8();
    }
}

/// The date to fix: the first one in the name, but one with a 4-digit year
/// beats a less certain 2-digit one anywhere in the name. On a tie, the
/// shape listed first in [`Shape`] wins.
fn find(name: &str, month_first: bool) -> Option<Candidate> {
    let mut found = Vec::new();
    for (shape, re) in patterns() {
        all_in(name, *shape, re, month_first, &mut found);
    }
    found
        .into_iter()
        .min_by_key(|c| (!c.full_year, c.span.start))
}

/// Finds the date to fix in a name. With `month_first`, 05-11-2023 is read
/// as 11 May (US style) instead of 5 November.
pub fn extract_date(name: &str, month_first: bool) -> Option<FoundDate> {
    let found = find(name, month_first)?;
    Some(FoundDate {
        iso: found.date.format("%Y-%m-%d").to_string(),
        matched: name[found.span].to_string(),
    })
}

/// The fixed name, or `None` when the name has no date or is already fine.
pub fn new_name_for(name: &str, month_first: bool) -> Option<String> {
    let found = find(name, month_first)?;
    let iso = found.date.format("%Y-%m-%d").to_string();
    if name[found.span.clone()] == iso {
        return None;
    }
    Some(format!(
        "{}{iso}{}",
        &name[..found.span.start],
        &name[found.span.end..]
    ))
}
