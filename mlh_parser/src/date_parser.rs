//! Date parsing, validation, and correction for email headers.
//!
//! Handles RFC 2822, RFC 3339, and various malformed date formats found in
//! mailing list archives. Includes millennium-year correction and fallback
//! date discovery from `Received` headers.

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use std::sync::LazyLock;

static DATE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    let rfc2822 = r"(?:(Sun|Mon|Tue|Wed|Thu|Fri|Sat),\s+)?(0[1-9]|[1-2]?[0-9]|3[01])\s+(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+(19[0-9]{2}|[2-9][0-9]{3}|0[0-9]{3})\s+(2[0-3]|[0-1][0-9]):([0-5][0-9])(?::(60|[0-5][0-9]))?\s+([-\+][0-9]{2}[0-5][0-9]|(?:UT|GMT|(?:E|C|M|P)(?:ST|DT)|[A-IK-Z]))";
    let rfc2822_loose = r"(?:(Sun|Mon|Tue|Wed|Thu|Fri|Sat),\s+)?(0[1-9]|[1-2]?[0-9]|3[01])\s+(Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\s+([0-9]{2,4})\s+([0-9]{2}:[0-9]{2}(?::[0-9]{2})?)";
    let rfc1123 = r"\w{3}, \d{2} \w{3} \d{4} \d{2}:\d{2}:\d{2} \w{3}";
    let rfc1036 = r"\w+?, \d{2}-\w{3}-\d{2} \d{2}:\d{2}:\d{2} \w{3}";
    let ctime = r"\w{3}\s+\w{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}\s+\d{4}";
    let ctime_tz = r"\w{3}\s+\w{3}\s+\d{1,2}\s+\d{2}:\d{2}:\d{2}\s+\w{3,5}\s+\d{4}";
    let dash_date_tz = r"\d{1,2}-\w{3}-\d{4}\s+\d{2}:\d{2}:\d{2}\s+\w{3,5}";
    // rfc3339_loose allows for 2,3,4 digit years and optional non-colon timezone
    let rfc3339_loose =
        r"((?:\d{2,4}-\d{2}-\d{2})[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+\-]\d{2}:?\d{2})?)";
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
        if let Ok(dt) = DateTime::parse_from_rfc2822(&cleaned)
            && has_valid_utc_offset(&dt)
        {
            return Some(dt);
        }
        // fall through to last_effort_date_finder on invalid offset

        // Try RFC 3339
        if let Ok(dt) = DateTime::parse_from_rfc3339(&cleaned)
            && has_valid_utc_offset(&dt)
        {
            return Some(dt);
        }
        // fall through to last_effort_date_finder on invalid offset

        // Try RFC 3339 with zero-padded year (handles 2-3 digit millennium years)
        if let Some(first_dash) = cleaned.find('-') {
            let year_part = &cleaned[..first_dash];
            if !year_part.is_empty()
                && year_part.len() < 4
                && year_part.chars().all(|c| c.is_ascii_digit())
            {
                let padded = format!("{:0>4}{}", year_part, &cleaned[first_dash..]);
                if let Ok(dt) = DateTime::parse_from_rfc3339(&padded)
                    && has_valid_utc_offset(&dt)
                {
                    return Some(dt);
                }
                // fall through to last_effort_date_finder on invalid offset
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

static NUMERIC_TZ_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\s|^)([+\-]\d{2}):?(\d{2})(?:\s|$)").expect("NUMERIC_TZ_REGEX must compile")
});

fn extract_tz_offset(text: &str) -> Option<i32> {
    NUMERIC_TZ_REGEX.captures(text).and_then(|caps| {
        let sign = if &caps[1][..1] == "-" { -1 } else { 1 };
        let hours: i32 = caps[1][1..].parse().ok()?;
        let minutes: i32 = caps[2].parse().ok()?;
        Some(sign * (hours * 3600 + minutes * 60))
    })
}

fn last_effort_date_finder(date_text: &str) -> Option<DateTime<FixedOffset>> {
    let cleaned = if let Some(pos) = date_text.find('(') {
        date_text[..pos].trim().to_string()
    } else {
        date_text.to_string()
    };

    let tz_offset = extract_tz_offset(&cleaned);
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
        // ISO 8601 formats with timezone (deterministic, no local timezone dependency)
        for fmt in &[
            "%Y-%m-%d %H:%M:%S %:z",
            "%Y-%m-%d %H:%M:%S %z",
            "%Y-%m-%dT%H:%M:%S%:z",
            "%Y-%m-%dT%H:%M:%S%z",
        ] {
            if let Ok(dt) = DateTime::parse_from_str(attempt, fmt) {
                return Some(dt);
            }
        }
        // ISO 8601 without timezone — apply extracted offset
        for fmt in &["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(attempt, fmt) {
                let offset = FixedOffset::east_opt(tz_offset.unwrap_or(0))?;
                let dt = offset.from_local_datetime(&naive).single()?;
                return Some(dt);
            }
        }
        // Dash date format: "31-Oct-2005 11:20:23"
        for fmt in &["%d-%b-%Y %H:%M:%S", "%d-%b-%y %H:%M:%S"] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(attempt, fmt) {
                let offset = FixedOffset::east_opt(tz_offset.unwrap_or(0))?;
                let dt = offset.from_local_datetime(&naive).single()?;
                return Some(dt);
            }
        }
        // Try with weekday prefix (works for 2-digit years)
        for fmt in &["%a, %d %b %Y %H:%M:%S", "%a, %d %b %y %H:%M:%S"] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(attempt, fmt) {
                let offset = FixedOffset::east_opt(tz_offset.unwrap_or(0))?;
                let dt = offset.from_local_datetime(&naive).single()?;
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
                let offset = FixedOffset::east_opt(tz_offset.unwrap_or(0))?;
                let dt = offset.from_local_datetime(&naive).single()?;
                return Some(dt);
            }
        }
        // Try ctime format: "%a %b %d %H:%M:%S %Y"
        for fmt in &["%a %b %d %H:%M:%S %Y", "%a %b %d %H:%M:%S %y"] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(attempt, fmt) {
                let offset = FixedOffset::east_opt(tz_offset.unwrap_or(0))?;
                let dt = offset.from_local_datetime(&naive).single()?;
                return Some(dt);
            }
        }
        let without_weekday_ctime = attempt
            .find(' ')
            .map(|pos| &attempt[pos + 1..])
            .unwrap_or(attempt);
        for fmt in &["%b %d %H:%M:%S %Y", "%b %d %H:%M:%S %y"] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(without_weekday_ctime, fmt) {
                let offset = FixedOffset::east_opt(tz_offset.unwrap_or(0))?;
                let dt = offset.from_local_datetime(&naive).single()?;
                return Some(dt);
            }
        }
    }

    // last effort lib
    if let Ok(date) = dateparser::parse(date_text.trim()) {
        // Apply extracted timezone to dateparser result (which always returns UTC)
        let offset_opt = FixedOffset::east_opt(tz_offset.unwrap_or(0))?;
        let utc_dt: DateTime<Utc> = date;
        Some(utc_dt.with_timezone(&offset_opt))
    } else {
        None
    }
}

/// Returns `true` if the date's year is before 1970 (likely malformed).
pub fn is_date_too_old(date_obj: &DateTime<FixedOffset>) -> bool {
    // the first email in history was sent in 1971
    // https://en.wikipedia.org/wiki/History_of_email
    date_obj.year() < 1970
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

pub fn mail_datetime_to_chrono(dt: &mail_parser::DateTime) -> Option<DateTime<FixedOffset>> {
    use chrono::NaiveDate;
    use chrono::NaiveTime;

    let naive_date = NaiveDate::from_ymd_opt(dt.year as i32, dt.month as u32, dt.day as u32)?;
    let naive_time = NaiveTime::from_hms_opt(dt.hour as u32, dt.minute as u32, dt.second as u32)?;
    let naive = naive_date.and_time(naive_time);

    let tz_offset = if dt.tz_before_gmt {
        -(dt.tz_hour as i32 * 3600 + dt.tz_minute as i32 * 60)
    } else {
        dt.tz_hour as i32 * 3600 + dt.tz_minute as i32 * 60
    };

    let offset = FixedOffset::east_opt(tz_offset)?;
    offset.from_local_datetime(&naive).single()
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
