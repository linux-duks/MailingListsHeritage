use arrow::array::{Array, ListArray, StringArray};
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

/// Creates a single EML with five To addresses, runs the pipeline,
/// then reads the Parquet output and verifies that the row count is
/// 1 (not expanded per address) and the `to` list column contains
/// all five addresses in order.
#[test]
fn test_multiple_to_addresses_in_parquet() {
    let temp_dir = TempDir::new().unwrap();
    let input_base = temp_dir.path().to_path_buf();
    let list_dir = input_base.join("manyto");
    fs::create_dir_all(&list_dir).unwrap();
    let output_base = temp_dir.path().join("output");

    let eml_content = concat!(
        "From: Cass Andor <cass@ferrix.local>\r\n",
        "To: one@test.local, two@test.local, three@test.local, four@test.local, five@test.local\r\n",
        "Subject: Many To\r\n",
        "Date: Mon, 10 Feb 2025 12:00:00 +0000\r\n",
        "Message-ID: <many-to@ferrix.local>\r\n",
        "\r\n",
        "Body text.\r\n"
    );
    fs::write(list_dir.join("many_to.eml"), eml_content).unwrap();

    let result = process_mailing_list(
        "manyto",
        &input_base,
        &output_base,
        false,
        BATCH_MAX_RECORDS,
    );
    assert!(result.is_ok());

    let parquet_path = output_base
        .join("dataset")
        .join("list=manyto")
        .join("list_data.parquet");
    assert!(parquet_path.exists());

    let file = fs::File::open(&parquet_path).unwrap();
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
    let reader = builder.build().unwrap();

    let mut row_count = 0;
    for batch in reader {
        let batch = batch.unwrap();
        row_count += batch.num_rows();

        let to_col = batch
            .column_by_name("to")
            .unwrap()
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();

        for r in 0..batch.num_rows() {
            let offsets = to_col.value_offsets();
            let start = offsets[r] as usize;
            let end = offsets[r + 1] as usize;
            let list_len = end - start;
            assert_eq!(list_len, 5, "`to` list should have 5 entries");

            let values = to_col
                .values()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();

            let expected = [
                "one@test.local",
                "two@test.local",
                "three@test.local",
                "four@test.local",
                "five@test.local",
            ];
            for (i, expected_addr) in expected.iter().enumerate() {
                assert_eq!(values.value(start + i), *expected_addr);
            }
        }
    }

    assert_eq!(row_count, 1, "one input email should produce exactly one row");
}

/// Creates a single EML with four CC addresses and one To address,
/// then verifies the Parquet output has one row with the correct
/// `cc` list length and content.
#[test]
fn test_multiple_cc_addresses_in_parquet() {
    let temp_dir = TempDir::new().unwrap();
    let input_base = temp_dir.path().to_path_buf();
    let list_dir = input_base.join("manycc");
    fs::create_dir_all(&list_dir).unwrap();
    let output_base = temp_dir.path().join("output");

    let eml_content = concat!(
        "From: Mon Mothma <mon@chandrila.gov>\r\n",
        "To: recipient@test.local\r\n",
        "CC: cc-one@test.local, cc-two@test.local, cc-three@test.local, cc-four@test.local\r\n",
        "Subject: Many CC\r\n",
        "Date: Tue, 11 Feb 2025 14:30:00 +0000\r\n",
        "Message-ID: <many-cc@chandrila.gov>\r\n",
        "\r\n",
        "Body text.\r\n"
    );
    fs::write(list_dir.join("many_cc.eml"), eml_content).unwrap();

    let result = process_mailing_list(
        "manycc",
        &input_base,
        &output_base,
        false,
        BATCH_MAX_RECORDS,
    );
    assert!(result.is_ok());

    let parquet_path = output_base
        .join("dataset")
        .join("list=manycc")
        .join("list_data.parquet");
    assert!(parquet_path.exists());

    let file = fs::File::open(&parquet_path).unwrap();
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
    let reader = builder.build().unwrap();

    let mut row_count = 0;
    for batch in reader {
        let batch = batch.unwrap();
        row_count += batch.num_rows();

        // Check `to` list: single entry
        let to_col = batch
            .column_by_name("to")
            .unwrap()
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();

        for r in 0..batch.num_rows() {
            let to_offsets = to_col.value_offsets();
            let to_start = to_offsets[r] as usize;
            let to_end = to_offsets[r + 1] as usize;
            let to_len = to_end - to_start;
            assert_eq!(to_len, 1, "`to` list should have 1 entry");

            let to_values = to_col
                .values()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            assert_eq!(to_values.value(to_start), "recipient@test.local");
        }

        // Check `cc` list: four entries
        let cc_col = batch
            .column_by_name("cc")
            .unwrap()
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();

        for r in 0..batch.num_rows() {
            let cc_offsets = cc_col.value_offsets();
            let cc_start = cc_offsets[r] as usize;
            let cc_end = cc_offsets[r + 1] as usize;
            let cc_len = cc_end - cc_start;
            assert_eq!(cc_len, 4, "`cc` list should have 4 entries");

            let cc_values = cc_col
                .values()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();

            let expected_cc = [
                "cc-one@test.local",
                "cc-two@test.local",
                "cc-three@test.local",
                "cc-four@test.local",
            ];
            for (i, expected_addr) in expected_cc.iter().enumerate() {
                assert_eq!(cc_values.value(cc_start + i), *expected_addr);
            }
        }
    }

    assert_eq!(row_count, 1, "one input email should produce exactly one row");
}

/// Creates three EML files with varying To/CC counts, processes
/// them, and asserts that the Parquet output has exactly three rows
/// and that the list column lengths match their input.
#[test]
fn test_multiple_emails_varied_to_cc_row_count() {
    let temp_dir = TempDir::new().unwrap();
    let input_base = temp_dir.path().to_path_buf();
    let list_dir = input_base.join("varied");
    fs::create_dir_all(&list_dir).unwrap();
    let output_base = temp_dir.path().join("output");

    // Email 1: 3 To, no CC
    let eml1 = concat!(
        "From: a@test.local\r\n",
        "To: alpha@test.local, beta@test.local, gamma@test.local\r\n",
        "Subject: Three To\r\n",
        "Date: Mon, 01 Jan 2025 12:00:00 +0000\r\n",
        "Message-ID: <three-to@test.local>\r\n",
        "\r\n",
        "Three To addresses.\r\n"
    );

    // Email 2: 1 To, 2 CC
    let eml2 = concat!(
        "From: b@test.local\r\n",
        "To: delta@test.local\r\n",
        "CC: epsilon@test.local, zeta@test.local\r\n",
        "Subject: Two CC\r\n",
        "Date: Tue, 02 Jan 2025 12:00:00 +0000\r\n",
        "Message-ID: <two-cc@test.local>\r\n",
        "\r\n",
        "Two CC addresses.\r\n"
    );

    // Email 3: 2 To, 3 CC
    let eml3 = concat!(
        "From: c@test.local\r\n",
        "To: iota@test.local, kappa@test.local\r\n",
        "CC: lambda@test.local, mu@test.local, nu@test.local\r\n",
        "Subject: Mixed\r\n",
        "Date: Wed, 03 Jan 2025 12:00:00 +0000\r\n",
        "Message-ID: <mixed@test.local>\r\n",
        "\r\n",
        "Mixed addresses.\r\n"
    );

    fs::write(list_dir.join("email_01.eml"), eml1).unwrap();
    fs::write(list_dir.join("email_02.eml"), eml2).unwrap();
    fs::write(list_dir.join("email_03.eml"), eml3).unwrap();

    let result = process_mailing_list(
        "varied",
        &input_base,
        &output_base,
        false,
        BATCH_MAX_RECORDS,
    );
    assert!(result.is_ok());

    let parquet_path = output_base
        .join("dataset")
        .join("list=varied")
        .join("list_data.parquet");
    assert!(parquet_path.exists());

    let file = fs::File::open(&parquet_path).unwrap();
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
    let reader = builder.build().unwrap();

    let mut row_count = 0;
    let mut to_lengths: Vec<usize> = Vec::new();
    let mut cc_lengths: Vec<usize> = Vec::new();

    for batch in reader {
        let batch = batch.unwrap();
        row_count += batch.num_rows();

        let to_col = batch
            .column_by_name("to")
            .unwrap()
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();

        let cc_col = batch
            .column_by_name("cc")
            .unwrap()
            .as_any()
            .downcast_ref::<ListArray>()
            .unwrap();

        for r in 0..batch.num_rows() {
            {
                let to_offsets = to_col.value_offsets();
                to_lengths.push((to_offsets[r + 1] - to_offsets[r]) as usize);
            }
            {
                let cc_offsets = cc_col.value_offsets();
                cc_lengths.push((cc_offsets[r + 1] - cc_offsets[r]) as usize);
            }
        }
    }

    assert_eq!(row_count, 3, "three input emails should produce three rows");

    // Email 1: 3 To, 0 CC
    // Email 2: 1 To, 2 CC
    // Email 3: 2 To, 3 CC
    assert_eq!(to_lengths, vec![3, 1, 2]);
    assert_eq!(cc_lengths, vec![0, 2, 3]);
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
