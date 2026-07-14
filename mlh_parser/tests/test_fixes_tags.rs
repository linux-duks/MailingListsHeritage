mod common;

use common::parse_fixes_body_tags_file;
use mlh_parser::email_reader::{decode_mail, get_body};
use mlh_parser::extractors::extract_fixes_body_tags_from_body;
use std::fs;

#[test]
fn test_email_fixes_body_tags() {
    let directory = "./fixtures/";
    let pairs = common::list_fixture_pairs(directory, ".fixes_body_tags.expected");

    if pairs.is_empty() {
        panic!("test cases missing")
    }

    for (expected_file, email_file) in &pairs {
        let mail_bytes = fs::read(email_file).unwrap();
        let expected_tags = parse_fixes_body_tags_file(expected_file);

        let mail = decode_mail(&mail_bytes).unwrap();
        let actual_body = get_body(&mail);

        let tags = extract_fixes_body_tags_from_body(&actual_body);

        assert_eq!(
            tags.len(),
            expected_tags.len(),
            "Fixes tags count mismatch for {:?}",
            email_file
        );

        for (i, tag) in tags.iter().enumerate() {
            assert_eq!(
                tag, &expected_tags[i],
                "Fixes tag mismatch at index {} for {:?}",
                i, email_file
            );
        }
    }
}
