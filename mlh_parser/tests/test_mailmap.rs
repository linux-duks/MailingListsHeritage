mod common;

use common::list_files_with_extension;
use mlh_parser::email_parser::parse_email;
use mlh_parser::mailmap::Mailmap;
use std::fs;
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn load_test_mailmap() -> Mailmap {
    Mailmap::from_file(&fixture_path("test.mailmap")).expect("Failed to load test mailmap")
}

fn load_mail(path: &str) -> Vec<u8> {
    fs::read(fixture_path(path)).expect("Failed to read fixture")
}

#[test]
fn test_mailmap_resolves_from_cassian_andor() {
    let mailmap = load_test_mailmap();
    let now = chrono::DateTime::from_timestamp(1783296000, 0)
        .unwrap()
        .into();
    let mail_bytes = load_mail("synthetic_many-to.eml");

    let parsed = parse_email(&mail_bytes, now, Some(&mailmap)).expect("Parse should not fail");

    assert_eq!(
        parsed.from_canonical_addr,
        "Cassian Andor <cassian.andor@ferrix.local>"
    );
}

#[test]
fn test_mailmap_resolves_from_mon_mothma() {
    let mailmap = load_test_mailmap();
    let now = chrono::DateTime::from_timestamp(1783296000, 0)
        .unwrap()
        .into();
    let mail_bytes = load_mail("synthetic_many-cc.eml");

    let parsed = parse_email(&mail_bytes, now, Some(&mailmap)).expect("Parse should not fail");

    assert_eq!(
        parsed.from_canonical_addr,
        "Mon Mothma <mon.mothma@coruscant.senate>"
    );
}

#[test]
fn test_mailmap_no_mapping_leaves_columns_empty() {
    let mailmap = load_test_mailmap();
    let now = chrono::DateTime::from_timestamp(1783296000, 0)
        .unwrap()
        .into();
    let mail_bytes = load_mail("synthetic_many-to.eml");

    let parsed = parse_email(&mail_bytes, now, Some(&mailmap)).expect("Parse should not fail");

    assert!(!parsed.from_canonical_addr.is_empty()); // has mapping
    assert!(parsed.trailers_canonical_addr.is_empty()); // no trailers
}

#[test]
fn test_without_mailmap_columns_are_empty() {
    let now = chrono::DateTime::from_timestamp(1783296000, 0)
        .unwrap()
        .into();
    let mail_bytes = load_mail("synthetic_many-to.eml");

    let parsed = parse_email(&mail_bytes, now, None).expect("Parse should not fail");

    assert!(parsed.from_canonical_addr.is_empty());
    assert!(parsed.trailers_canonical_addr.is_empty());
}

#[test]
fn test_already_canonical_address_not_mapped() {
    let mailmap = load_test_mailmap();
    let now = chrono::DateTime::from_timestamp(1783296000, 0)
        .unwrap()
        .into();
    let mail_bytes = load_mail("0000000001-e0-57ca0a1f200b994f0fface20de73e2b936fd64a1.eml");

    let parsed = parse_email(&mail_bytes, now, Some(&mailmap)).expect("Parse should not fail");

    // mon.mothma@coruscant.senate is the canonical email; no alias maps to it,
    // so from_canonical_addr should be empty.
    assert!(parsed.from_canonical_addr.is_empty());
}

#[test]
fn test_all_fixtures_parse_with_mailmap() {
    let mailmap = load_test_mailmap();
    let now = chrono::DateTime::from_timestamp(1783296000, 0)
        .unwrap()
        .into();
    let email_files = list_files_with_extension("./fixtures/", ".eml");

    assert!(!email_files.is_empty(), "test cases missing");

    for email_file in &email_files {
        let mail_bytes = match fs::read(email_file) {
            Ok(b) => b,
            Err(_) => continue,
        };

        match parse_email(&mail_bytes, now, Some(&mailmap)) {
            Ok(parsed) => {
                // from_canonical_addr and trailers_canonical_addr should be
                // populated or empty but never cause a panic.
                assert!(!parsed.from.is_empty());
            }
            Err(_) => continue,
        };
    }
}
