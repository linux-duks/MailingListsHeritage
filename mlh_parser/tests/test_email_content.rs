mod common;

use common::{parse_body_file, parse_headers_file};
use mlh_parser::email_reader::{decode_mail, get_body, get_headers};
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

#[test]
fn test_header_parser() {
    let directory = "./fixtures/";
    let pairs = common::list_fixture_pairs(directory, ".headers.expected");

    if pairs.is_empty() {
        panic!("test cases missing")
    }

    for (headers_file, email_file) in &pairs {
        let mail_bytes = fs::read(email_file).unwrap();
        let expected_headers = parse_headers_file(headers_file);

        let mail = decode_mail(&mail_bytes).unwrap();
        let actual_headers = get_headers(&mail);

        for (key, expected_value) in &expected_headers {
            let actual_value = actual_headers.get(key).cloned().unwrap_or_default();
            assert_eq!(
                actual_value, *expected_value,
                "Header mismatch for '{}' in {:?}",
                key, email_file
            );
        }
    }
}
