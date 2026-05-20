//! Date parsing, validation, and correction for email headers.
//!
//! Handles RFC 2822, RFC 3339, and various malformed date formats found in
//! mailing list archives. Includes millennium-year correction and fallback
//! date discovery from `Received` headers.

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use std::sync::LazyLock;

static DATE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    let rfc2822 = r"(?:(Sun|Mon|Tue|Wed|Thu|Fri|Sat),\s+)?(0[1-9]|[1-2]?[0-9]|3[01])\s+(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+(19[0-9]{2}|[2-9][0-9]{3})\s+(2[0-3]|[0-1][0-9]):([0-5][0-9])(?::(60|[0-5][0-9]))?\s+([-\+][0-9]{2}[0-5][0-9]|(?:UT|GMT|(?:E|C|M|P)(?:ST|DT)|[A-IK-Z]))";
    let rfc2822_loose = r"(?:(Sun|Mon|Tue|Wed|Thu|Fri|Sat),\s+)?(0[1-9]|[1-2]?[0-9]|3[01])\s+(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+([0-9]{2,4})\s+([0-9]{2}:[0-9]{2}(?::[0-9]{2})?)";
    let rfc1123 = r"\w{3}, \d{2} \w{3} \d{4} \d{2}:\d{2}:\d{2} \w{3}";
    let rfc1036 = r"\w+?, \d{2}-\w{3}-\d{2} \d{2}:\d{2}:\d{2} \w{3}";
    let ctime = r"\w{3}\s+\w{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}\s+\d{4}";
    let ctime_tz = r"\w{3}\s+\w{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}\s+\w{3,5}\s+\d{4}";
    let dash_date_tz = r"\d{1,2}-\w{3}-\d{4}\s+\d{2}:\d{2}:\d{2}\s+\w{3,5}";
    // rfc3339_loose allows for 2,3,4 digit yerars
    let rfc3339_loose =
        r"((?:\d{2,4}-\d{2}-\d{2})[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+\-]\d{2}:\d{2})?)";
    Regex::new(&format!(
        "(?:{})|(?:{})|(?:{})|(?:{})|(?:{})|(?:{})|(?:{})|(?:{})",
        rfc2822, rfc2822_loose, rfc1123, rfc1036, ctime, ctime_tz, dash_date_tz, rfc3339_loose
    ))
    .expect("DATE_REGEX MUST COMPILE")
});

fn find_date_in_string(text: &str) -> Option<String> {
    DATE_REGEX.find(text).map(|m| m.as_str().to_string())
}

/// Attempts to parse `date` with RFC 2822, RFC 3339, and fallback heuristics.
///
/// Returns `None` if no recognizable date could be extracted.
pub fn parse_date_string(date: &str) -> Option<DateTime<FixedOffset>> {
    if date.is_empty() {
        return None;
    }

    if let Some(found) = find_date_in_string(date) {
        // Handle "(" comments in date strings
        let cleaned = if let Some(pos) = found.find('(') {
            found[..pos].trim().to_string()
        } else {
            found.clone()
        };

        // Try RFC 2822 format first
        // This rust implemented parser handles "millennium dates",
        // These will be fixed here, not in `fix_millennium_date`
        if let Ok(dt) = DateTime::parse_from_rfc2822(&cleaned) {
            if has_valid_utc_offset(&dt) {
                return Some(dt);
            }
            return None;
        }

        // Try RFC 3339
        if let Ok(dt) = DateTime::parse_from_rfc3339(&cleaned) {
            if has_valid_utc_offset(&dt) {
                return Some(dt);
            }
            return None;
        }

        // Try RFC 3339 with zero-padded year (handles 2-3 digit millennium years)
        if let Some(first_dash) = cleaned.find('-') {
            let year_part = &cleaned[..first_dash];
            if !year_part.is_empty()
                && year_part.len() < 4
                && year_part.chars().all(|c| c.is_ascii_digit())
            {
                let padded = format!("{:0>4}{}", year_part, &cleaned[first_dash..]);
                if let Ok(dt) = DateTime::parse_from_rfc3339(&padded) {
                    if has_valid_utc_offset(&dt) {
                        return Some(dt);
                    }
                    return None;
                }
            }
        }

        return last_effort_date_finder(&found);
    }
    None
}

fn has_valid_utc_offset(dt: &DateTime<FixedOffset>) -> bool {
    let offset_secs = dt.offset().local_minus_utc();
    offset_secs > -24 * 3600 && offset_secs < 24 * 3600
}

static TZ_STRIP_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s+[A-Z]{2,5}(\s+\d{4}|$)").expect("TZ_STRIP_REGEX must compile")
});

fn last_effort_date_finder(date_text: &str) -> Option<DateTime<FixedOffset>> {
    let cleaned = if let Some(pos) = date_text.find('(') {
        date_text[..pos].trim().to_string()
    } else {
        date_text.to_string()
    };

    let tz_stripped = TZ_STRIP_REGEX.replace(&cleaned, "${1}").to_string();

    let attempts = vec![
        cleaned.clone(),
        tz_stripped.clone(),
        cleaned.replace('.', ":"),
        cleaned
            .chars()
            .take("Fri, 15 Jun 2012 16:52:52".len())
            .collect(),
        cleaned
            .chars()
            .take("Fri, 5 Jun 2012 16:52:52".len())
            .collect(),
    ];

    for attempt in &attempts {
        // Dash date format: "31-Oct-2005 11:20:23"
        for fmt in &["%d-%b-%Y %H:%M:%S", "%d-%b-%y %H:%M:%S"] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(attempt, fmt) {
                let dt = Utc
                    .from_utc_datetime(&naive)
                    .with_timezone(&FixedOffset::east_opt(0)?);
                return Some(dt);
            }
        }
        // Try with weekday prefix (works for 2-digit years)
        for fmt in &["%a, %d %b %Y %H:%M:%S", "%a, %d %b %y %H:%M:%S"] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(attempt, fmt) {
                let dt = Utc
                    .from_utc_datetime(&naive)
                    .with_timezone(&FixedOffset::east_opt(0)?);
                return Some(dt);
            }
        }
        // Try without weekday prefix (avoids chrono %a+%Y interaction bug)
        let without_weekday = attempt
            .find(", ")
            .map(|pos| &attempt[pos + 2..])
            .unwrap_or(attempt);
        for fmt in &["%d %b %Y %H:%M:%S", "%d %b %y %H:%M:%S"] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(without_weekday, fmt) {
                let dt = Utc
                    .from_utc_datetime(&naive)
                    .with_timezone(&FixedOffset::east_opt(0)?);
                return Some(dt);
            }
        }
        // Try ctime format: "%a %b %d %H:%M:%S %Y"
        for fmt in &["%a %b %d %H:%M:%S %Y", "%a %b %d %H:%M:%S %y"] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(attempt, fmt) {
                let dt = Utc
                    .from_utc_datetime(&naive)
                    .with_timezone(&FixedOffset::east_opt(0)?);
                return Some(dt);
            }
        }
        let without_weekday_ctime = attempt
            .find(' ')
            .map(|pos| &attempt[pos + 1..])
            .unwrap_or(attempt);
        for fmt in &["%b %d %H:%M:%S %Y", "%b %d %H:%M:%S %y"] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(without_weekday_ctime, fmt) {
                let dt = Utc
                    .from_utc_datetime(&naive)
                    .with_timezone(&FixedOffset::east_opt(0)?);
                return Some(dt);
            }
        }
    }

    // last effort lib
    if let Ok(date) = dateparser::parse(date_text.trim()) {
        log::warn!("the external dateparser lib was able to parse this date: {date_text}");
        Some(date.into())
    } else {
        None
    }
}

/// Returns `true` if the date's year is before 1900 (likely malformed).
pub fn is_date_too_old(date_obj: &DateTime<FixedOffset>) -> bool {
    date_obj.year() < 1900
}

/// Returns `true` if `date_obj` is more than 3 days after `now`.
pub fn is_date_in_future(date_obj: &DateTime<FixedOffset>, now: DateTime<FixedOffset>) -> bool {
    let max_future = now + chrono::Duration::days(3);
    *date_obj > max_future
}

/// Returns `true` if the date is too old OR too far in the future.
pub fn check_date_issues(date_obj: &DateTime<FixedOffset>, now: DateTime<FixedOffset>) -> bool {
    is_date_too_old(date_obj) || is_date_in_future(date_obj, now)
}

/// Corrects dates where the year was accidentally stored modulo 100.
///
/// The original implementation served all encodings, but the rust chrono
/// crate handles this for rfc2822.
/// This function is here to handle unlikely other cases of this in other encodings.
///
/// rules from chrono:
/// - two-digit year < 50 should be interpreted by adding 2000.
///   two-digit year >= 50 or three-digit year should be interpreted
///   by adding 1900. note that four-or-more-digit years less than 1000
///   are *never* affected by this rule.
pub fn fix_millennium_date(
    date_obj: DateTime<FixedOffset>,
    now: DateTime<FixedOffset>,
) -> DateTime<FixedOffset> {
    let year = date_obj.year();
    let max_year = now.year();
    if year > 1000 {
        return date_obj;
    }

    let adjusted: i32 = if year < 50 { year + 2000 } else { year + 1900 };

    if adjusted <= max_year
        && let Some(new_date) = NaiveDate::from_ymd_opt(adjusted, date_obj.month(), date_obj.day())
    {
        let time = date_obj.time();
        let naive = new_date.and_time(time);
        let offset = date_obj.offset();
        if let Some(fixed) = offset.from_local_datetime(&naive).single() {
            return fixed;
        }
    }
    date_obj
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_date_in_string_matches() {
        let cases = vec![
            // === rfc2822 (strict, with timezone) ===
            (
                "Mon, 03 Jan 1978 18:27:37 +0000",
                Some("Mon, 03 Jan 1978 18:27:37 +0000"),
            ),
            ("03 Jan 1978 18:27:37 UT", Some("03 Jan 1978 18:27:37 UT")),
            (
                "Sat, 31 Dec 2022 23:59:59 -0500",
                Some("Sat, 31 Dec 2022 23:59:59 -0500"),
            ),
            (
                "Sun, 01 Jan 2023 00:00:00 +0530",
                Some("Sun, 01 Jan 2023 00:00:00 +0530"),
            ),
            // single-digit day
            (
                "Mon, 3 Jan 1978 18:27:37 +0000",
                Some("Mon, 3 Jan 1978 18:27:37 +0000"),
            ),
            ("3 Jan 1978 18:27:37 GMT", Some("3 Jan 1978 18:27:37 GMT")),
            // military timezone
            ("01 Jul 2021 12:00:00 A", Some("01 Jul 2021 12:00:00 A")),
            // no seconds
            (
                "Mon, 15 Mar 2021 10:30 UT",
                Some("Mon, 15 Mar 2021 10:30 UT"),
            ),
            // === rfc2822_loose (no timezone, 2-4 digit years) ===
            ("Mon, 3 Jan 78 18:27:37", Some("Mon, 3 Jan 78 18:27:37")),
            ("3 Jan 2000 18:27:37", Some("3 Jan 2000 18:27:37")),
            ("Mon, 3 Jan 0100 18:27:37", Some("Mon, 3 Jan 0100 18:27:37")),
            ("Mon, 3 Jan 100 18:27:37", Some("Mon, 3 Jan 100 18:27:37")),
            ("Tue, 15 Feb 99 20:15:00", Some("Tue, 15 Feb 99 20:15:00")),
            // no seconds
            ("Fri, 1 Jun 22 14:30", Some("Fri, 1 Jun 22 14:30")),
            // single-digit day, no weekday
            ("1 Dec 2022 09:15:42", Some("1 Dec 2022 09:15:42")),
            // === rfc1123 ===
            (
                "Sun, 06 Nov 1994 08:49:37 GMT",
                Some("Sun, 06 Nov 1994 08:49:37 GMT"),
            ),
            (
                "Wed, 21 Oct 2015 07:28:00 EST",
                Some("Wed, 21 Oct 2015 07:28:00 EST"),
            ),
            // === rfc1036 ===
            (
                "Sunday, 06-Nov-94 08:49:37 GMT",
                Some("Sunday, 06-Nov-94 08:49:37 GMT"),
            ),
            (
                "Wed, 01-Jan-20 12:00:00 PST",
                Some("Wed, 01-Jan-20 12:00:00 PST"),
            ),
            // === ctime ===
            ("Sun Nov  6 08:49:37 1994", Some("Sun Nov  6 08:49:37 1994")),
            ("Mon Jan 15 14:30:00 2023", Some("Mon Jan 15 14:30:00 2023")),
            ("Sat Mar  1 00:00:00 2020", Some("Sat Mar  1 00:00:00 2020")),
            ("Sun Jan 11 05:59:04 2004", Some("Sun Jan 11 05:59:04 2004")),
            (
                "Thu Oct 16 22:10:38 EST 2008",
                Some("Thu Oct 16 22:10:38 EST 2008"),
            ),
            (
                "Thu Oct 16 22:10:38 GMT 2008",
                Some("Thu Oct 16 22:10:38 GMT 2008"),
            ),
            // === dash date with TZ ===
            ("31-Oct-2005 11:20:23 MST", Some("31-Oct-2005 11:20:23 MST")),
            // === embedded in larger text ===
            (
                "Received: from mail.example.com; Mon, 03 Jan 1978 18:27:37 +0000",
                Some("Mon, 03 Jan 1978 18:27:37 +0000"),
            ),
            (
                "Date: Sun, 06 Nov 1994 08:49:37 GMT\nSubject: Hello",
                Some("Sun, 06 Nov 1994 08:49:37 GMT"),
            ),
            (
                "preceding text Sun Nov  6 08:49:37 1994 trailing",
                Some("Sun Nov  6 08:49:37 1994"),
            ),
            (
                "Blablabal 2025-12-05T11:13:54-06:00 date",
                Some("2025-12-05T11:13:54-06:00"),
            ),
            // === non-matching ===
            ("not a date at all", None),
            ("", None),
            ("12345", None),
        ];

        for (input, expected) in &cases {
            let result = find_date_in_string(input);
            let result_ref: Option<&str> = result.as_deref();
            assert_eq!(result_ref, *expected, "find_date_in_string({:?})", input);
        }
    }
}
