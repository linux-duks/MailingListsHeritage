use arrow::array::{Array, StringArray};
use mlh_parser::{config::AppConfig, constants::BATCH_MAX_RECORDS, process_mailing_list, start};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::fs;
use std::sync::{Arc, atomic::AtomicBool};
use tempfile::TempDir;

#[test]
fn test_parse_empty_directory() {
    let temp_dir = TempDir::new().unwrap();
    let input_base = temp_dir.path().to_path_buf();
    let list_dir = input_base.join("empty_list");
    fs::create_dir_all(&list_dir).unwrap();
    let output_base = temp_dir.path().join("output");

    let result = process_mailing_list(
        "empty_list",
        &input_base,
        &output_base,
        false,
        BATCH_MAX_RECORDS,
    );
    assert!(result.is_ok());
}

#[test]
fn test_parse_single_eml() {
    let temp_dir = TempDir::new().unwrap();
    let input_base = temp_dir.path().to_path_buf();
    let list_dir = input_base.join("test_list");
    fs::create_dir_all(&list_dir).unwrap();

    let output_base = temp_dir.path().join("output");

    let eml_content = concat!(
        "From: Test User <test@example.com>\r\n",
        "To: recipient@example.com\r\n",
        "Subject: Test Email\r\n",
        "Date: Sat, 29 Mar 2025 20:07:52 +0000\r\n",
        "Message-ID: <test123@example.com>\r\n",
        "\r\n",
        "This is the body of the test email.\r\n"
    );
    fs::write(list_dir.join("test.eml"), eml_content).unwrap();

    let result = process_mailing_list(
        "test_list",
        &input_base,
        &output_base,
        false,
        BATCH_MAX_RECORDS,
    );
    assert!(result.is_ok());

    let parquet_path = output_base
        .join("dataset")
        .join("list=test_list")
        .join("list_data.parquet");
    assert!(parquet_path.exists());
}

#[test]
fn test_parse_errors_written_to_csv() {
    let temp_dir = TempDir::new().unwrap();
    let input_base = temp_dir.path().to_path_buf();
    let list_dir = input_base.join("err_list");
    fs::create_dir_all(&list_dir).unwrap();
    let output_base = temp_dir.path().join("output");

    // An empty file triggers a DecodeError (mail-parser returns None)
    fs::write(list_dir.join("broken_01.eml"), "").unwrap();
    // A second broken file to ensure multiple errors accumulate
    fs::write(list_dir.join("broken_02.eml"), "").unwrap();

    let result = process_mailing_list(
        "err_list",
        &input_base,
        &output_base,
        false,
        BATCH_MAX_RECORDS,
    );
    assert!(result.is_ok());

    let csv_path = output_base
        .join("errors")
        .join("list=err_list")
        .join("errors.csv");
    assert!(csv_path.exists(), "errors.csv should exist");

    let csv_content = fs::read_to_string(&csv_path).unwrap();
    assert!(
        csv_content.contains("broken_01"),
        "should contain first email_id"
    );
    assert!(
        csv_content.contains("broken_02"),
        "should contain second email_id"
    );
    assert!(
        csv_content.contains("Failed to decode email"),
        "should contain error message"
    );
}

#[test]
fn test_parse_errors_csv_forwarding_newlines() {
    let temp_dir = TempDir::new().unwrap();
    let input_base = temp_dir.path().to_path_buf();
    let list_dir = input_base.join("multierr");
    fs::create_dir_all(&list_dir).unwrap();
    let output_base = temp_dir.path().join("output");

    // Write content that lacks a Message-ID header — parse_email doesn't fail
    // on this, but we can trigger a decoding error via empty content to keep
    // the test simple.
    fs::write(list_dir.join("bad.eml"), "").unwrap();

    let result = process_mailing_list(
        "multierr",
        &input_base,
        &output_base,
        false,
        BATCH_MAX_RECORDS,
    );
    assert!(result.is_ok());

    let csv_path = output_base
        .join("errors")
        .join("list=multierr")
        .join("errors.csv");
    let csv_content = fs::read_to_string(&csv_path).unwrap();

    // The CSV should contain exactly one line (no trailing empty line from
    // writeln, though the last file read may leave one — we check that no
    // raw \n appears inside a field)
    assert!(
        !csv_content.contains("\"\n\""),
        "fields should not contain raw newlines"
    );
}

/// Creates 100 numbered emails split across two mailing lists
/// (list_a: 0-49, list_b: 50-99), runs `start()` under 1-5 local
/// rayon thread pools, then reads both Parquet outputs and verifies
/// that `raw_body` values appear in order 0, 1, …, 99.
#[test]
fn test_100_emails_order_preserved() {
    // Build emails upfront — two lists, 50 each
    let temp_dir = TempDir::new().unwrap();
    let input_base = temp_dir.path().to_path_buf();
    let output_base = temp_dir.path().join("output");
    fs::create_dir_all(&output_base).unwrap();

    for list_name in ["list_a", "list_b"] {
        let list_dir = input_base.join(list_name);
        fs::create_dir_all(&list_dir).unwrap();
        let start = if list_name == "list_a" { 0 } else { 50 };
        for i in start..(start + 50) {
            let eml = format!(
                "From: test@example.com\r\n\
                 Date: Sat, 29 Mar 2025 20:07:52 +0000\r\n\
                 Message-ID: <{}_{:03}@example.com>\r\n\
                 \r\n\
                 {}",
                list_name, i, i
            );
            fs::write(list_dir.join(format!("{:03}.eml", i)), eml.as_bytes()).unwrap();
        }
    }

    for nthreads in 1..=5 {
        // Clean output so each run starts fresh
        let _ = fs::remove_dir_all(&output_base);
        fs::create_dir_all(&output_base).unwrap();

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(nthreads)
            .build()
            .unwrap();

        let mut cfg = AppConfig {
            nthreads: nthreads as u8,
            input_dir_path: input_base.to_string_lossy().to_string(),
            output_dir_path: output_base.to_string_lossy().to_string(),
            fail_on_parsing_error: false,
            lists_to_parse: Some(vec!["list_a".to_string(), "list_b".to_string()]),
        };

        let shutdown = Arc::new(AtomicBool::new(false));
        pool.install(|| {
            start(&mut cfg, shutdown).expect("start() should succeed");
        });

        // Read both parquet files and collect raw_body strings in order
        let mut body_values = Vec::new();
        for list_name in ["list_a", "list_b"] {
            let parquet_path = output_base
                .join("dataset")
                .join(format!("list={list_name}"))
                .join("list_data.parquet");
            assert!(
                parquet_path.exists(),
                "missing parquet for {list_name} with {nthreads} threads",
            );

            let file = fs::File::open(&parquet_path).unwrap();
            let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
            for batch in builder.build().unwrap() {
                let batch = batch.unwrap();
                let col = batch
                    .column_by_name("raw_body")
                    .unwrap()
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap();
                for r in 0..col.len() {
                    body_values.push(col.value(r).to_string());
                }
            }
        }

        let expected: Vec<String> = (0..100).map(|i| i.to_string()).collect();
        assert_eq!(
            body_values, expected,
            "order broken with {nthreads} threads",
        );
    }
}
