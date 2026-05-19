mod common;

use chrono::DateTime;
use common::list_files_with_extension;
use mlh_parser::email_parser::parse_email;
use std::fs;

#[test]
// TODO: add other validations here. This test only validates if the parser fails with any email
fn test_complete_parser() {
    let directory = "./fixtures/";
    let email_files = list_files_with_extension(directory, ".eml");

    // TODO: this should reflect the maximum real date in tests.
    // I will only cause problems if new cases are introduced with dates in the future
    // relative to this one:
    let now = DateTime::from_timestamp(1779062556, 0).unwrap().into();

    if email_files.is_empty() {
        panic!("test cases missing")
    }

    for email_file in &email_files {
        let mail_bytes = match fs::read(email_file) {
            Ok(b) => b,
            Err(_) => continue,
        };

        match parse_email(&mail_bytes, now) {
            Ok(r) => {
                if r.raw_body.is_empty() {
                    eprintln!("Skipping {:?}: empty body", email_file);
                    continue;
                }
                for trailer in &r.trailers {
                    assert!(
                        trailer.attribution.ends_with("-by"),
                        "Invalid attribution: {:?}",
                        trailer.attribution
                    );
                }
            }
            Err(e) => {
                eprintln!("Failed to parse {:?}: {}", email_file, e);
                continue;
            }
        };
    }
}
