mod common;

use chrono::DateTime;
use common::parse_date_file;
use mlh_parser::date_parser::{parse_date_tentative_raw, process_date};
use mlh_parser::email_reader::{decode_mail, get_headers};
use std::fs;

#[test]
fn test_millennium_dates() {
    let millennium_cases = vec![
        // These rfc2822 style dates are handled by chrono
        // 2 digit year
        ("Mon, 3 Jan 78 18:27:37", "Mon, 3 Jan 1978 18:27:37"),
        ("Mon, 3 Jan 99 18:27:37", "Mon, 3 Jan 1999 18:27:37"),
        // 2 digit year became a 3 digit when the year 2000 started
        ("Mon, 3 Jan 100 18:27:37", "Mon, 3 Jan 2000 18:27:37"),
        ("Mon, 3 Jan 101 18:27:37", "Mon, 3 Jan 2001 18:27:37"),
        // same issue but padded with a zero
        ("Mon, 3 Jan 0100 18:27:37", "Mon, 3 Jan 2000 18:27:37"),
        ("Mon, 3 Jan 0120 18:27:37", "Mon, 3 Jan 2020 18:27:37"),
        (
            "Mon, 3 Jan 0120 18:27:37 -0400",
            "Mon, 3 Jan 2020 18:27:37 -0400",
        ),
        (
            "Tue,  4 Nov 101 22:14:47 +0000 (UTC)",
            "Tue,  4 Nov 2001 22:14:47 +0000 (UTC)",
        ),
        // ISO 8601 / RFC 3339 format with millennium dates
        // 4-digit zero-padded year
        ("0103-09-29T10:34:51-04:00", "2003-09-29T10:34:51-04:00"),
        ("0121-01-15T08:30:00Z", "2021-01-15T08:30:00Z"),
        ("0105-03-14 14:30:00-05:00", "2005-03-14 14:30:00-05:00"),
        // 2-digit year
        ("99-12-31T23:59:59Z", "1999-12-31T23:59:59Z"),
        ("78-06-01T12:00:00+01:00", "1978-06-01T12:00:00+01:00"),
        // 3-digit year
        ("101-06-15T16:30:00-07:00", "2001-06-15T16:30:00-07:00"),
        // space separator (not T)
        ("0102-08-22 10:00:00Z", "2002-08-22 10:00:00Z"),
        ("0103-09-29T10:34:51-04:00", "2003-09-29T10:34:51-04:00"),
        ("0121-01-15T08:30:00Z", "2021-01-15T08:30:00Z"),
        ("0105-03-14 14:30:00-05:00", "2005-03-14 14:30:00-05:00"),
    ];

    let now = DateTime::from_timestamp(1734748800, 0)
        .expect("Should be able to read time")
        .into();

    for (found_str, expected_str) in millennium_cases {
        let found_date = parse_date_tentative_raw(found_str).expect("Parse should not be None");
        let expected_date =
            parse_date_tentative_raw(expected_str).expect("Parse should not be None");
        let fixed = mlh_parser::date_parser::fix_millennium_date(found_date, now);
        assert_eq!(fixed, expected_date, "Failed for {}", found_str);
    }
}

#[test]
fn test_email_dates() {
    let directory = "./fixtures/";
    let pairs = common::list_fixture_pairs(directory, ".date.expected");

    if pairs.is_empty() {
        panic!("test cases missing")
    }

    // TODO: this should reflect the maximum real date in tests.
    // I will only cause problems if new cases are introduced with dates in the future
    // relative to this one:
    // Mon May 18 2026 00:02:36 GMT+0000
    let now = DateTime::from_timestamp(1779062556, 0).unwrap().into();

    for (date_file, email_file) in &pairs {
        let mail_bytes = fs::read(email_file).unwrap();

        let expected_date_str = parse_date_file(date_file);
        if expected_date_str.is_empty() {
            continue;
        }
        let expected_date = parse_date_tentative_raw(&expected_date_str);

        let msg = decode_mail(&mail_bytes).unwrap();
        let mut headers = get_headers(&msg);

        process_date(&mut headers, now);

        if let (Some(expected), Some(actual_str)) = (expected_date, headers.get("date"))
            && let Ok(actual) = DateTime::parse_from_rfc3339(actual_str)
        {
            assert_eq!(actual, expected, "Date mismatch for {:?}", email_file);
        }
    }
}
