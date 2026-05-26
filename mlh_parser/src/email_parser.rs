//! Top-level email parsing: decodes raw bytes into a [`ParsedEmail`].

use crate::ParsedEmail;
use crate::address_parser::normalize_address;
use crate::address_parser::normalize_raw_address_header;
use crate::address_parser::{AddressScore, addr_to_string, score_email_address};
use crate::date_parser;
use crate::email_reader::{
    self, header_value_date, header_value_to_string, header_value_to_string_list,
};
use crate::errors::ParseError;
use crate::extractors::{self};

use chrono::{DateTime, FixedOffset, Utc};
use mail_parser::Message;
use parquet::errors::Result;
use sha1::{Digest, Sha1};

/// Parses a raw RFC 822 email byte slice into a [`ParsedEmail`].
///
/// Extracts headers, body text, trailers, and code patches. Dates are
/// normalized by [`process_date`](crate::date_parser::process_date). Missing
/// single-valued columns are populated with empty strings.
pub fn parse_email(
    email_data: &[u8],
    now: DateTime<FixedOffset>,
) -> Result<ParsedEmail, ParseError> {
    let msg = email_reader::decode_mail(email_data)
        .ok_or_else(|| ParseError::DecodeError("Failed to parse email bytes".to_string()))?;

    let mut email = ParsedEmail::default();
    collect_header_data(&msg, &mut email, now);

    email.subject_tags = extractors::extract_tags_from_subject(&email.subject);

    let raw_body = email_reader::get_body(&msg);

    email.trailers = extractors::extract_attributions(&raw_body);
    email.code = extractors::extract_patches(&raw_body);
    email.body_sha1 = generate_sha1_hash(&raw_body);
    email.raw_body = raw_body;
    Ok(email)
}

pub fn read_raw_offset(raw_content: &[u8], start_offset: u32, end_offset: u32) -> String {
    if start_offset >= end_offset {
        return String::new();
    }

    let start = (start_offset as usize).min(raw_content.len());
    let end = (end_offset as usize).min(raw_content.len());

    let sub_slice = &raw_content[start..end];
    String::from_utf8_lossy(sub_slice).into_owned()
}

/// Extracts all headers from a parsed message
///
/// Also evaluates headers using body information to better guide `From` selection.
/// The `From` header is chosen by scoring candidates (name presence, valid
/// email address). Obfuscated addresses are normalized.
fn collect_header_data(msg: &Message<'_>, email: &mut ParsedEmail, now: DateTime<FixedOffset>) {
    let mut from_candidates: Vec<String> = Vec::new();
    let mut date_options = vec![];
    let mut client_dates = vec![];

    for header in msg.headers() {
        let key = header.name().to_lowercase();

        if key == "message-id" {
            email.message_id = header_value_to_string(header.value()).unwrap_or_default();
        } else if key == "from" {
            if let Some(val_str) = header_value_to_string(header.value()) {
                from_candidates.push(val_str);
            }
        } else if key == "to" || key == "cc" {
            // Read raw header text to pre-normalize obfuscation, since
            // mail_parser strips (a) comments inside angle brackets.
            let raw = read_raw_offset(
                msg.raw_message(),
                header.offset_start(),
                header.offset_end(),
            );
            let has_obfuscation =
                raw.contains("(a)") || raw.contains("(A)") || raw.contains(" at ");
            let mut addrs = if has_obfuscation {
                normalize_raw_address_header(&raw)
            } else {
                header_value_to_string_list(header.value()).unwrap_or_default()
            };
            if key == "to" {
                email.to.append(&mut addrs);
            } else {
                email.cc.append(&mut addrs);
            }
        } else if key == "subject" {
            if let Some(val_str) = header_value_to_string(header.value()) {
                email.subject = val_str;
            }
        } else if key == "date" {
            // Date header is used in the client_date and possibly in the "date" column
            let raw_date = read_raw_offset(
                msg.raw_message(),
                header.offset_start(),
                header.offset_end(),
            )
            .trim()
            .to_owned();

            if let Some(val_date) = header_value_date(header.value()) {
                date_options.push(val_date);
            } else if let Some(dt) = date_parser::parse_date_string(&raw_date) {
                date_options.push(dt);
            }
            client_dates.push(raw_date);

            // depends on type
        } else if key == "received" || key == "x-received" {
            // these in the other hand are only eligible to the "date" column
            if let Some(val_date) = header_value_date(header.value()) {
                date_options.push(val_date);
            } else {
                let raw_date = read_raw_offset(
                    msg.raw_message(),
                    header.offset_start(),
                    header.offset_end(),
                )
                .trim()
                .to_owned();
                if let Some(dt) = date_parser::parse_date_string(&raw_date) {
                    date_options.push(dt);
                }
            }
        } else if key == "in-reply-to" {
            email.in_reply_to = header_value_to_string(header.value());
        } else if key == "references" {
            if let Some(mut val_vec) = header_value_to_string_list(header.value()) {
                email.references.append(&mut val_vec);
            }
        } else if key == "x-mailing-list" {
            email.x_mailing_list = header_value_to_string(header.value());
        }
    }

    // select date
    email.date = select_date(date_options, now);
    email.client_date = client_dates;

    if from_candidates.is_empty()
        && let Some(from) = msg.from()
    {
        for addr in from.iter() {
            from_candidates.push(addr_to_string(addr));
        }
    }

    // some malformed messages put their "FROM" header in the body.
    if from_candidates.is_empty()
        || from_candidates
            .iter()
            .all(|c| !score_email_address(c).has_name)
    {
        let body_from = email_reader::extract_all_from_from_body(&msg.raw_message);
        from_candidates.extend(body_from);
    }

    if !from_candidates.is_empty() {
        email.from = select_best_from_header(&from_candidates);
    };
}

fn select_best_from_header(values: &[String]) -> String {
    if values.is_empty() {
        return String::new();
    }
    if values.len() == 1 {
        return normalize_address(&values[0]);
    }

    let mut scored: Vec<(AddressScore, &String)> =
        values.iter().map(|v| (score_email_address(v), v)).collect();
    scored.sort_by_key(|b| std::cmp::Reverse(b.0));
    normalize_address(scored[0].1)
}

/// Selects the best date from the available options, applying millennium
/// correction and filtering dates too old and in the future.
/// When more than 2 safe dates remain, usesmedian absolute deviation
/// to detect if dates are spread widely; if so, eliminates outliers
pub fn select_date(
    date_options: Vec<DateTime<FixedOffset>>,
    now: DateTime<FixedOffset>,
) -> Option<DateTime<Utc>> {
    let date_options_len = date_options.len();

    let mut safe: Vec<DateTime<FixedOffset>> = date_options
        .into_iter()
        .map(|d| date_parser::fix_millennium_date(d, now))
        .filter(|d| !date_parser::check_date_issues(d, now))
        .collect();

    log::debug!(
        "date selection received {} dates, and evaluated {} to be safe",
        date_options_len,
        safe.len()
    );

    if safe.is_empty() {
        return None;
    }

    safe.sort();

    // For 2 or fewer dates, just pick the earliest
    if safe.len() <= 2 {
        return Some(safe[0].into());
    }

    // With 3+ dates, check if the spread is implausibly large.
    // If dates are within ~1 hour of each other, no filtering needed.
    let first_ts = safe[0].timestamp_micros();
    let last_ts = safe[safe.len() - 1].timestamp_micros();
    let range_us = (last_ts - first_ts) as f64;
    let one_day_us = 24.0 * 3600.0 * 1_000_000.0;

    if range_us <= one_day_us {
        log::debug!(
            "date selection range={:.0}s, all dates close enough, picking earliest",
            range_us / 1_000_000.0
        );
        return Some(safe[0].into());
    }

    // Spread is large: use median absolute deviation to find the core cluster.
    // Dates farther than 2 days from the median are considered outliers.
    let timestamps: Vec<i64> = safe.iter().map(|d| d.timestamp_micros()).collect();

    let median = if timestamps.len() % 2 == 1 {
        timestamps[timestamps.len() / 2] as f64
    } else {
        let mid = timestamps.len() / 2;
        (timestamps[mid - 1] as f64 + timestamps[mid] as f64) / 2.0
    };

    let mut deviations: Vec<f64> = timestamps
        .iter()
        .map(|&ts| ((ts as f64) - median).abs())
        .collect();
    deviations.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());

    let mad = if deviations.len() % 2 == 1 {
        deviations[deviations.len() / 2]
    } else {
        let mid = deviations.len() / 2;
        (deviations[mid - 1] + deviations[mid]) / 2.0
    };

    log::debug!(
        "date selection range={:.0}s median={:.0}, using MAD outlier filter mad={}",
        range_us / 1_000_000.0,
        median / 1_000_000.0,
        mad
    );

    let original_safe = safe.clone();
    if mad > 0.0 {
        safe.retain(|d| {
            let ts = d.timestamp_micros() as f64;
            let modified_z_score = (0.6745 * (ts - median).abs()) / mad;
            modified_z_score <= 3.5 // 3.5 is the standard threshold for outlier detection
        });
    }

    // If outlier removal was too aggressive (fewer than 2 remain),
    // fall back to the original set.
    if safe.len() < 2 {
        safe = original_safe;
    }

    Some(safe[0].into())
}

/// Compute the SHA-1 hex digest of a string.
pub fn generate_sha1_hash(input: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, FixedOffset, TimeZone, Utc};

    fn now_far_future() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2027, 1, 1, 0, 0, 0)
            .unwrap()
    }

    fn make_date(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        min: u32,
        sec: u32,
        offset_secs: i32,
    ) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(offset_secs)
            .unwrap()
            .with_ymd_and_hms(year, month, day, hour, min, sec)
            .unwrap()
    }

    #[test]
    fn test_select_date_earliest_by_timezone() {
        // Sun May 17 2026 16:52:41 GMT+0000 (16:52:41 UTC)
        // vs Sun May 17 2026 13:55:41 GMT-0300 (16:55:41 UTC)
        // GMT+0000 is earlier
        let dates = vec![
            make_date(2026, 5, 17, 16, 52, 41, 0),         // GMT+0000
            make_date(2026, 5, 17, 13, 55, 41, -3 * 3600), // GMT-0300
        ];
        let now = now_far_future();
        let result = select_date(dates, now);
        assert!(result.is_some());
        let expected = Utc.with_ymd_and_hms(2026, 5, 17, 16, 52, 41).unwrap();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_select_date_earliest_across_days() {
        let dates = vec![
            make_date(2026, 5, 17, 22, 0, 0, -4 * 3600), // 02:00 UTC on May 18
            make_date(2026, 5, 18, 1, 0, 0, 0),          // 01:00 UTC on May 18
            make_date(2026, 5, 17, 23, 0, 0, 0),         // 23:00 UTC on May 17 <- earliest
        ];
        let now = now_far_future();
        let result = select_date(dates, now);
        assert!(result.is_some());
        let expected = Utc.with_ymd_and_hms(2026, 5, 17, 23, 0, 0).unwrap();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_select_date_filters_too_old() {
        let dates = vec![
            make_date(1800, 1, 1, 0, 0, 0, 0),
            make_date(2026, 5, 17, 12, 0, 0, 0),
        ];
        let now = now_far_future();
        let result = select_date(dates, now);
        assert!(result.is_some());
        let expected = Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_select_date_filters_future() {
        let dates = vec![
            make_date(2030, 1, 1, 0, 0, 0, 0),
            make_date(2026, 5, 17, 12, 0, 0, 0),
        ];
        let now = make_date(2027, 1, 1, 0, 0, 0, 0);
        let result = select_date(dates, now);
        assert!(result.is_some());
        let expected = Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_select_date_empty_returns_none() {
        let now = now_far_future();
        let result = select_date(vec![], now);
        assert!(result.is_none());
    }

    #[test]
    fn test_select_date_all_filtered_returns_none() {
        let dates = vec![
            make_date(1800, 1, 1, 0, 0, 0, 0),
            make_date(1700, 1, 1, 0, 0, 0, 0),
        ];
        let now = now_far_future();
        let result = select_date(dates, now);
        assert!(result.is_none());
    }

    #[test]
    fn test_select_date_single_date() {
        let dates = vec![make_date(2026, 5, 17, 12, 0, 0, 0)];
        let now = now_far_future();
        let result = select_date(dates, now);
        assert!(result.is_some());
        let expected = Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_select_date_millennium_correction_applied() {
        let dates = vec![
            make_date(26, 5, 17, 10, 0, 0, 0),
            make_date(98, 5, 17, 10, 0, 0, 0),
        ];
        let now = now_far_future();
        let result = select_date(dates, now);
        assert!(result.is_some());
        let expected = Utc.with_ymd_and_hms(1998, 5, 17, 10, 0, 0).unwrap();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_select_date_mad_removes_outlier() {
        // Three dates: two are clustered around 2026-05-17, one is a far outlier
        let dates = vec![
            make_date(2026, 5, 17, 10, 0, 0, 0), // cluster
            make_date(2026, 5, 17, 12, 0, 0, 0), // cluster
            make_date(2020, 1, 1, 0, 0, 0, 0),   // outlier (far)
        ];
        let now = now_far_future();
        let result = select_date(dates, now);
        assert!(result.is_some());
        // Should pick earliest of the cluster (2026-05-17T10:00:00Z)
        let expected = Utc.with_ymd_and_hms(2026, 5, 17, 10, 0, 0).unwrap();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_select_date_mad_keeps_close_dates() {
        // Three dates all within a few hours
        let dates = vec![
            make_date(2026, 5, 17, 10, 0, 0, 0),
            make_date(2026, 5, 17, 12, 0, 0, 0),
            make_date(2026, 5, 17, 8, 0, 0, 0),
        ];
        let now = now_far_future();
        let result = select_date(dates, now);
        assert!(result.is_some());
        // Should pick earliest of all three: 2026-05-17T08:00:00Z
        let expected = Utc.with_ymd_and_hms(2026, 5, 17, 8, 0, 0).unwrap();
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn test_select_date_mad_all_outliers_returns_none() {
        // All dates are spread very far apart (>2 days cap).
        // With the fallback, when outlier filtering removes all but <2 dates,
        // it picks the earliest from the original set.
        let dates = vec![
            make_date(2020, 1, 1, 0, 0, 0, 0),
            make_date(2021, 1, 1, 0, 0, 0, 0),
            make_date(2022, 1, 1, 0, 0, 0, 0),
            make_date(2023, 1, 1, 0, 0, 0, 0),
            make_date(2024, 1, 1, 0, 0, 0, 0),
        ];
        let now = now_far_future();
        let result = select_date(dates, now);
        assert!(result.is_some());
        let expected = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(result.unwrap(), expected);
    }
}
