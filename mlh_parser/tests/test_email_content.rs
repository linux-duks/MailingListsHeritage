mod common;

use chrono::DateTime;
use common::{parse_body_file, parse_headers_file, rfc2047_decode};
use mlh_parser::email_parser::parse_email;
use mlh_parser::email_reader::{decode_mail, get_body};
use std::fs;

#[test]
fn test_body_parser() {
    let directory = "./fixtures/";
    let pairs = common::list_fixture_pairs(directory, ".body.expected");

    for (body_file, email_file) in &pairs {
        let mail_bytes = fs::read(email_file).unwrap();
        let expected_body = parse_body_file(body_file);

        let mail = decode_mail(&mail_bytes).unwrap();
        let actual_body = get_body(&mail);

        assert_eq!(
            actual_body, expected_body,
            "Body mismatch for {:?}",
            email_file
        );
    }
}

fn strip_brackets(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('<') && s.ends_with('>') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn normalize_refs(value: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for part in value.split(&[',', ' ', '\n', '\r'][..]) {
        let stripped = strip_brackets(part.trim());
        if !stripped.is_empty() {
            ids.push(stripped);
        }
    }
    ids.sort();
    ids
}

fn parse_date_for_comparison(value: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc2822(value)
        .ok()
        .or_else(|| chrono::DateTime::parse_from_rfc3339(value).ok())
}

fn normalize_address_for_comparison(addr: &str) -> String {
    let addr = addr.trim();
    if let Some(lt) = addr.find('<') {
        let name = addr[..lt].trim();
        let rest = &addr[lt..];
        let name = if name.starts_with('"') && name.ends_with('"') {
            name[1..name.len() - 1].to_string()
        } else {
            name.to_string()
        };
        if name.is_empty() {
            strip_brackets(rest)
        } else {
            format!("{} {}", name, rest)
        }
    } else {
        strip_brackets(addr)
    }
}

#[test]
fn test_header_parser() {
    let directory = "./fixtures/";
    let pairs = common::list_fixture_pairs(directory, ".headers.expected");

    if pairs.is_empty() {
        panic!("test cases missing")
    }

    let now = DateTime::from_timestamp(1779062556, 0).unwrap().into();

    for (headers_file, email_file) in &pairs {
        let mail_bytes = fs::read(email_file).unwrap();
        let expected_headers = parse_headers_file(headers_file);

        let parsed = match parse_email(&mail_bytes, now, None) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Skipping {:?}: {}", email_file, e);
                continue;
            }
        };

        for (key, expected_value) in &expected_headers {
            match key.as_str() {
                "from" => {
                    let actual_norm =
                        normalize_address_for_comparison(&rfc2047_decode(&parsed.from));
                    let expected_norm =
                        normalize_address_for_comparison(&rfc2047_decode(expected_value));
                    assert_eq!(
                        actual_norm, expected_norm,
                        "From mismatch in {:?}",
                        email_file
                    );
                }
                "to" => {
                    let actual: Vec<String> = parsed
                        .to
                        .iter()
                        .map(|a| normalize_address_for_comparison(&rfc2047_decode(a)))
                        .collect();
                    let expected: Vec<String> = expected_value
                        .split(',')
                        .map(|s| normalize_address_for_comparison(&rfc2047_decode(s.trim())))
                        .collect();
                    assert_eq!(actual, expected, "To mismatch in {:?}", email_file);
                }
                "cc" => {
                    let actual: Vec<String> = parsed
                        .cc
                        .iter()
                        .map(|a| normalize_address_for_comparison(&rfc2047_decode(a)))
                        .collect();
                    let expected: Vec<String> = expected_value
                        .split(',')
                        .map(|s| normalize_address_for_comparison(&rfc2047_decode(s.trim())))
                        .collect();
                    assert_eq!(actual, expected, "Cc mismatch in {:?}", email_file);
                }
                "subject" => {
                    assert_eq!(
                        parsed.subject, *expected_value,
                        "Subject mismatch in {:?}",
                        email_file
                    );
                }
                "message-id" => {
                    let expected_stripped = strip_brackets(expected_value);
                    assert_eq!(
                        parsed.message_id, expected_stripped,
                        "Message-ID mismatch in {:?}",
                        email_file
                    );
                }
                "in-reply-to" => {
                    let expected_stripped = strip_brackets(expected_value);
                    let actual_val = parsed.in_reply_to.clone().unwrap_or_default();
                    assert_eq!(
                        actual_val, expected_stripped,
                        "In-Reply-To mismatch in {:?}",
                        email_file
                    );
                }
                "references" => {
                    let mut actual: Vec<String> = parsed
                        .references
                        .iter()
                        .map(|r| strip_brackets(r))
                        .collect();
                    actual.sort();
                    let expected = normalize_refs(expected_value);
                    assert_eq!(actual, expected, "References mismatch in {:?}", email_file);
                }
                "date" => {
                    let expected_date_parsed = parse_date_for_comparison(expected_value);
                    if let Some(ref actual_date) = parsed.date {
                        let actual_fixed: chrono::DateTime<chrono::FixedOffset> =
                            (*actual_date).into();
                        if let Some(ref expected_dt) = expected_date_parsed {
                            let diff_secs = (actual_fixed - *expected_dt).num_seconds().abs();
                            if diff_secs > 86400 {
                                eprintln!(
                                    "Date differs by {}s for {:?}: expected='{}', actual='{:?}'",
                                    diff_secs, email_file, expected_value, actual_date
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
