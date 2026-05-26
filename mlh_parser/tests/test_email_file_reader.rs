use std::collections::HashSet;
use std::fs;
use std::fs::File;

use arrow::array::StringArray;
use mlh_archiver::archive_writer::{EmailData, EmailStore, ParquetEmailStore};
use mlh_parser::{
    email_file_reader::{file_iterator, read_eml_email, read_parquet_emails},
    process_mailing_list,
};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

#[test]
fn test_read_single_eml_file() {
    let tmp = tempfile::tempdir().unwrap();
    let eml_path = tmp.path().join("14.eml");
    fs::write(
        &eml_path,
        b"From: test@example.com\r\nSubject: Hello\r\n\r\nBody text here",
    )
    .unwrap();

    let result = read_eml_email(&eml_path).unwrap();
    assert_eq!(result.email_id, "14");
    assert_eq!(
        result.content,
        "From: test@example.com\r\nSubject: Hello\r\n\r\nBody text here"
    );
}

#[test]
fn test_read_single_parquet_file() {
    let tmp = tempfile::tempdir().unwrap();
    let pq_path = tmp.path().join("single.parquet");

    let mut store = ParquetEmailStore::new(pq_path.clone(), 10);
    store
        .add_email(EmailData {
            email_id: "a".into(),
            content: "content_a".into(),
        })
        .unwrap();
    store
        .add_email(EmailData {
            email_id: "b".into(),
            content: "content_b".into(),
        })
        .unwrap();
    store.close().unwrap();

    let real_path = tmp.path().join("single_000.parquet");
    let reader = read_parquet_emails(&real_path).unwrap();
    let results: Vec<_> = reader.map(|r| r.unwrap()).collect();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].email_id, "a");
    assert_eq!(results[0].content, "content_a");
    assert_eq!(results[1].email_id, "b");
    assert_eq!(results[1].content, "content_b");
}

#[test]
fn test_file_iterator_mixed_types() {
    let tmp = tempfile::tempdir().unwrap();
    let tp = tmp.path();

    // Parquet file with 3 emails
    let pq_path = tp.join("batch1.parquet");
    let mut store = ParquetEmailStore::new(pq_path.clone(), 10);
    for i in 1..=3 {
        store
            .add_email(EmailData {
                email_id: format!("pq_{i}"),
                content: format!("parquet body {i}"),
            })
            .unwrap();
    }
    store.close().unwrap();

    // EML files
    let eml1 = tp.join("14.eml");
    fs::write(&eml1, b"From: a@b.com\r\n\r\neml body 1").unwrap();
    let eml2 = tp.join("26435.eml");
    fs::write(&eml2, b"From: c@d.com\r\n\r\neml body 2").unwrap();

    // Second parquet file with 2 emails
    let pq_path2 = tp.join("batch2.parquet");
    let mut store2 = ParquetEmailStore::new(pq_path2.clone(), 10);
    for i in 4..=5 {
        store2
            .add_email(EmailData {
                email_id: format!("pq_{i}"),
                content: format!("parquet body {i}"),
            })
            .unwrap();
    }
    store2.close().unwrap();

    let emails = file_iterator(vec![
        tp.join("batch1_000.parquet"),
        eml1,
        tp.join("batch2_000.parquet"),
        eml2,
    ]);

    let results: Vec<EmailData> = emails.map(|r| r.unwrap()).collect();

    assert_eq!(results.len(), 7);
    // pq_1, pq_2, pq_3 from first parquet
    assert_eq!(results[0].email_id, "pq_1");
    assert_eq!(results[1].email_id, "pq_2");
    assert_eq!(results[2].email_id, "pq_3");
    // eml file 14
    assert_eq!(results[3].email_id, "14");
    assert_eq!(results[3].content, "From: a@b.com\r\n\r\neml body 1");
    // pq_4, pq_5 from second parquet
    assert_eq!(results[4].email_id, "pq_4");
    assert_eq!(results[5].email_id, "pq_5");
    // eml file 26435
    assert_eq!(results[6].email_id, "26435");
    assert_eq!(results[6].content, "From: c@d.com\r\n\r\neml body 2");
}

#[test]
fn test_unknown_extension_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let tp = tmp.path();

    // A .txt file that should be skipped
    fs::write(tp.join("skipme.txt"), b"should be ignored").unwrap();

    // A valid .eml file
    let eml = tp.join("42.eml");
    fs::write(&eml, b"From: x@y.com\r\n\r\nonly this").unwrap();

    let emails = file_iterator(vec![tp.join("skipme.txt"), eml]);
    let results: Vec<EmailData> = emails.map(|r| r.unwrap()).collect();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].email_id, "42");
}

#[test]
fn test_missing_file_error() {
    let tmp = tempfile::tempdir().unwrap();

    // Valid eml first
    let eml = tmp.path().join("1.eml");
    fs::write(&eml, b"From: ok@ok.com\r\n\r\nok body").unwrap();

    // Then a non-existent parquet file
    let nonexistent = tmp.path().join("missing.parquet");

    let mut emails = file_iterator(vec![eml, nonexistent]);

    // First should succeed
    let row = emails.next().unwrap().unwrap();
    assert_eq!(row.email_id, "1");

    // Second should be an error
    let err = emails.next().unwrap();
    assert!(err.is_err(), "expected error for missing file");
}

#[test]
fn test_file_iterator_ordering_is_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let tp = tmp.path();

    let mut expected = Vec::new();

    // Create 3 eml files
    for (id, body) in [("1", "first"), ("2", "second"), ("3", "third")] {
        let eml = tp.join(format!("{id}.eml"));
        fs::write(&eml, format!("From: x@x\r\n\r\n{body}")).unwrap();
        expected.push(EmailData {
            email_id: id.to_string(),
            content: format!("From: x@x\r\n\r\n{body}"),
        });
    }

    let paths: Vec<_> = expected
        .iter()
        .map(|e| tp.join(format!("{}.eml", e.email_id)))
        .collect();

    let results: Vec<EmailData> = file_iterator(paths).map(|r| r.unwrap()).collect();

    for (i, exp) in expected.iter().enumerate() {
        assert_eq!(results[i].email_id, exp.email_id);
        assert_eq!(results[i].content, exp.content);
    }
}

#[test]
fn test_large_parquet_across_batches() {
    let tmp = tempfile::tempdir().unwrap();
    let pq_path = tmp.path().join("large.parquet");

    // small batch_size forces multiple parquet files, testing cross-file iteration
    let mut store = ParquetEmailStore::new(pq_path.clone(), 3);
    for i in 0..10 {
        store
            .add_email(EmailData {
                email_id: format!("id_{i}"),
                content: format!("content number {i}"),
            })
            .unwrap();
    }
    store.close().unwrap();

    // Collect all generated parquet files (large_000, large_001, ...)
    let mut paths: Vec<_> = (0..10)
        .map(|idx| tmp.path().join(format!("large_{idx:03}.parquet")))
        .filter(|p| p.exists())
        .collect();
    paths.sort();

    let results: Vec<EmailData> = file_iterator(paths).map(|r| r.unwrap()).collect();

    assert_eq!(results.len(), 10);
    for (i, row) in results.iter().enumerate() {
        assert_eq!(row.email_id, format!("id_{i}"));
        assert_eq!(row.content, format!("content number {i}"));
    }
}

#[test]
fn test_parquet_reader_is_exhausted() {
    let tmp = tempfile::tempdir().unwrap();
    let pq_path = tmp.path().join("exhaust.parquet");

    let mut store = ParquetEmailStore::new(pq_path.clone(), 5);
    store
        .add_email(EmailData {
            email_id: "only".into(),
            content: "only_content".into(),
        })
        .unwrap();
    store.close().unwrap();

    let real_path = tmp.path().join("exhaust_000.parquet");
    let mut reader = read_parquet_emails(&real_path).unwrap();
    assert!(reader.next().is_some()); // one row
    assert!(reader.next().is_none()); // exhausted
    assert!(reader.next().is_none()); // still none
}

#[test]
fn test_file_iterator_multiple_parquet_files() {
    let tmp = tempfile::tempdir().unwrap();
    let tp = tmp.path();

    // parquet file 1: 2 emails
    let pq1 = tp.join("multi1.parquet");
    let mut s1 = ParquetEmailStore::new(pq1, 10);
    s1.add_email(EmailData {
        email_id: "a1".into(),
        content: "body_a1".into(),
    })
    .unwrap();
    s1.add_email(EmailData {
        email_id: "a2".into(),
        content: "body_a2".into(),
    })
    .unwrap();
    s1.close().unwrap();

    // parquet file 2: 1 email
    let pq2 = tp.join("multi2.parquet");
    let mut s2 = ParquetEmailStore::new(pq2, 10);
    s2.add_email(EmailData {
        email_id: "b1".into(),
        content: "body_b1".into(),
    })
    .unwrap();
    s2.close().unwrap();

    // parquet file 3: 3 emails
    let pq3 = tp.join("multi3.parquet");
    let mut s3 = ParquetEmailStore::new(pq3, 10);
    for i in 1..=3 {
        s3.add_email(EmailData {
            email_id: format!("c{i}"),
            content: format!("body_c{i}"),
        })
        .unwrap();
    }
    s3.close().unwrap();

    let emails = file_iterator(vec![
        tp.join("multi1_000.parquet"),
        tp.join("multi2_000.parquet"),
        tp.join("multi3_000.parquet"),
    ]);

    let results: Vec<EmailData> = emails.map(|r| r.unwrap()).collect();
    let ids: HashSet<String> = results.iter().map(|r| r.email_id.clone()).collect();

    assert_eq!(results.len(), 6);
    assert_eq!(
        ids,
        HashSet::from_iter(["a1", "a2", "b1", "c1", "c2", "c3"].map(String::from))
    );
}

#[test]
fn test_parse_mail_batched() {
    let tmp = tempfile::tempdir().unwrap();
    let input_dir = tmp.path().join("input");
    let output_dir = tmp.path().join("output");
    let mailing_list = "test-list";

    let input_list_dir = input_dir.join(mailing_list);
    fs::create_dir_all(&input_list_dir).unwrap();

    // Create 30 .eml files
    for i in 0..30 {
        let content = format!(
            "From: sender{i}@test.com\r\n\
             To: rcpt{i}@test.com\r\n\
             Date: Mon, 05 May 2025 10:00:00 +0000\r\n\
             Subject: Test email #{i}\r\n\
             Message-ID: <msg_{i}@test.com>\r\n\
             \r\n\
             body content number {i}\r\n"
        );
        fs::write(input_list_dir.join(format!("{:03}.eml", i)), content).unwrap();
    }

    // Parse with max_records_per_batch = 10 (forces 3+ batches over 30 emails)
    process_mailing_list(
        mailing_list,
        &input_dir,
        &output_dir,
        true, // fail_on_error
        10,   // max_records_per_batch
    )
    .unwrap();

    let output_parquet = output_dir
        .join("dataset")
        .join(format!("list={}", mailing_list))
        .join("list_data.parquet");

    assert!(output_parquet.is_file());

    // Read back and verify all 30 message-ids are present
    let file = File::open(&output_parquet).unwrap();
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)
        .unwrap()
        .with_batch_size(1024)
        .build()
        .unwrap();

    let mut message_ids: Vec<String> = Vec::new();
    for batch in reader {
        let batch = batch.unwrap();
        let col = batch.column(0);
        let arr = col.as_any().downcast_ref::<StringArray>().unwrap();
        for v in arr.iter().flatten() {
            message_ids.push(v.to_string());
        }
    }

    assert_eq!(message_ids.len(), 30);
    for i in 0..30 {
        assert!(
            message_ids.contains(&format!("msg_{i}@test.com")),
            "missing msg_{i}"
        );
    }
}
