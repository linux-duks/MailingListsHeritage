use arrow::array::{Array, Date64Array, StringArray};
use mlh_parser::lineage_parser::parse_lineage;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_parse_lineage_single_entry() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let list_dir = input_dir.path().join("test.list");
    fs::create_dir_all(&list_dir).unwrap();

    let lineage_yaml = concat!(
        "email_index: \"1\"\n",
        "list_name: \"test.list\"\n",
        "source_type: \"NNTP h=localhost\"\n",
        "write_mode: \"append\"\n",
        "archive_timestamp: \"2025-01-15T10:30:00Z\"\n",
        "archiver_build_info: \"Archiver v=0.1.0 commit=abc123\"\n",
    );
    fs::write(list_dir.join("__lineage.yaml"), lineage_yaml).unwrap();

    parse_lineage(input_dir.path(), output_dir.path()).unwrap();

    let parquet_path = output_dir.path().join("lineage").join("lineage.parquet");
    assert!(parquet_path.exists());

    let file = fs::File::open(&parquet_path).unwrap();
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
    let reader = builder.build().unwrap();

    let mut row_count = 0;
    for batch in reader {
        let batch = batch.unwrap();
        row_count += batch.num_rows();

        let email_index = batch
            .column_by_name("email_index")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let list_name = batch
            .column_by_name("list_name")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let source_type = batch
            .column_by_name("source_type")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let write_mode = batch
            .column_by_name("write_mode")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let archive_timestamp = batch
            .column_by_name("archive_timestamp")
            .unwrap()
            .as_any()
            .downcast_ref::<Date64Array>()
            .unwrap();
        let archiver_build_info = batch
            .column_by_name("archiver_build_info")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let parse_timestamp = batch
            .column_by_name("parse_timestamp")
            .unwrap()
            .as_any()
            .downcast_ref::<Date64Array>()
            .unwrap();
        let parser_build_info = batch
            .column_by_name("parser_build_info")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();

        assert_eq!(email_index.value(0), "1");
        assert_eq!(list_name.value(0), "test.list");
        assert_eq!(source_type.value(0), "NNTP h=localhost");
        assert_eq!(write_mode.value(0), "append");
        assert_eq!(archive_timestamp.value(0), 1736937000);
        assert_eq!(
            archiver_build_info.value(0),
            "Archiver v=0.1.0 commit=abc123"
        );
        assert!(parse_timestamp.value(0) > 0);
        assert!(!parser_build_info.value(0).is_empty());
    }

    assert_eq!(row_count, 1);
}

#[test]
fn test_parse_lineage_multiple_entries() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let list_dir = input_dir.path().join("multi.list");
    fs::create_dir_all(&list_dir).unwrap();

    let lineage_yaml = concat!(
        "email_index: \"1\"\n",
        "list_name: \"multi.list\"\n",
        "source_type: \"NNTP\"\n",
        "write_mode: \"append\"\n",
        "archive_timestamp: \"2025-01-15T10:30:00Z\"\n",
        "archiver_build_info: \"Archiver v=0.1.0\"\n",
        "---\n",
        "email_index: \"2\"\n",
        "list_name: \"multi.list\"\n",
        "source_type: \"NNTP\"\n",
        "write_mode: \"append\"\n",
        "archive_timestamp: \"2025-01-15T10:30:05Z\"\n",
        "archiver_build_info: \"Archiver v=0.1.0\"\n",
        "---\n",
        "email_index: \"3\"\n",
        "list_name: \"multi.list\"\n",
        "source_type: \"NNTP\"\n",
        "write_mode: \"append\"\n",
        "archive_timestamp: \"2025-01-15T10:30:10Z\"\n",
        "archiver_build_info: \"Archiver v=0.1.0\"\n",
    );
    fs::write(list_dir.join("__lineage.yaml"), lineage_yaml).unwrap();

    parse_lineage(input_dir.path(), output_dir.path()).unwrap();

    let parquet_path = output_dir.path().join("lineage").join("lineage.parquet");
    let file = fs::File::open(&parquet_path).unwrap();
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
    let reader = builder.build().unwrap();

    let mut row_count = 0;
    for batch in reader {
        let batch = batch.unwrap();
        row_count += batch.num_rows();

        let email_index = batch
            .column_by_name("email_index")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let archive_timestamp = batch
            .column_by_name("archive_timestamp")
            .unwrap()
            .as_any()
            .downcast_ref::<Date64Array>()
            .unwrap();

        assert_eq!(email_index.value(0), "1");
        assert_eq!(email_index.value(1), "2");
        assert_eq!(email_index.value(2), "3");
        assert_eq!(archive_timestamp.value(0), 1736937000);
        assert_eq!(archive_timestamp.value(1), 1736937005);
        assert_eq!(archive_timestamp.value(2), 1736937010);
    }

    assert_eq!(row_count, 3);
}

#[test]
fn test_parse_lineage_multiple_lists() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let list1 = input_dir.path().join("list.one");
    let list2 = input_dir.path().join("list.two");
    fs::create_dir_all(&list1).unwrap();
    fs::create_dir_all(&list2).unwrap();

    fs::write(
        list1.join("__lineage.yaml"),
        concat!(
            "email_index: \"a\"\n",
            "list_name: \"list.one\"\n",
            "source_type: \"NNTP\"\n",
            "write_mode: \"append\"\n",
            "archive_timestamp: \"2025-01-15T10:30:00Z\"\n",
            "archiver_build_info: \"A v=1\"\n",
        ),
    )
    .unwrap();
    fs::write(
        list2.join("__lineage.yaml"),
        concat!(
            "email_index: \"b\"\n",
            "list_name: \"list.two\"\n",
            "source_type: \"IMAP\"\n",
            "write_mode: \"overwrite\"\n",
            "archive_timestamp: \"2025-01-15T10:30:05Z\"\n",
            "archiver_build_info: \"A v=2\"\n",
        ),
    )
    .unwrap();

    parse_lineage(input_dir.path(), output_dir.path()).unwrap();

    let parquet_path = output_dir.path().join("lineage").join("lineage.parquet");
    let file = fs::File::open(&parquet_path).unwrap();
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
    let reader = builder.build().unwrap();

    let mut list_names = Vec::new();
    for batch in reader {
        let batch = batch.unwrap();
        let list_col = batch
            .column_by_name("list_name")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for r in 0..batch.num_rows() {
            list_names.push(list_col.value(r).to_string());
        }
    }

    assert_eq!(list_names.len(), 2);
    assert!(list_names.contains(&"list.one".to_string()));
    assert!(list_names.contains(&"list.two".to_string()));
}

#[test]
fn test_parse_lineage_static_columns_same_value() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let list_dir = input_dir.path().join("s.list");
    fs::create_dir_all(&list_dir).unwrap();

    let mut yaml = String::new();
    for i in 0..10 {
        if i > 0 {
            yaml.push_str("---\n");
        }
        yaml.push_str(&format!(
            "email_index: \"{}\"\n\
             list_name: \"s.list\"\n\
             source_type: \"SRC\"\n\
             write_mode: \"append\"\n\
             archive_timestamp: \"2025-01-15T10:30:00Z\"\n\
             archiver_build_info: \"B ld=1\"\n",
            i
        ));
    }
    fs::write(list_dir.join("__lineage.yaml"), yaml).unwrap();

    parse_lineage(input_dir.path(), output_dir.path()).unwrap();

    let parquet_path = output_dir.path().join("lineage").join("lineage.parquet");
    let file = fs::File::open(&parquet_path).unwrap();
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
    let reader = builder.build().unwrap();

    let mut parser_build_infos = Vec::new();
    for batch in reader {
        let batch = batch.unwrap();
        let pbi = batch
            .column_by_name("parser_build_info")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for r in 0..batch.num_rows() {
            parser_build_infos.push(pbi.value(r).to_string());
        }
    }

    assert_eq!(parser_build_infos.len(), 10);

    let first = &parser_build_infos[0];
    assert!(!first.is_empty());

    for info in &parser_build_infos {
        assert_eq!(
            info, first,
            "all rows must have same parser build info"
        );
    }
}

#[test]
fn test_parse_lineage_no_lineage_files() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let list_dir = input_dir.path().join("nolist");
    fs::create_dir_all(&list_dir).unwrap();

    parse_lineage(input_dir.path(), output_dir.path()).unwrap();

    let parquet_path = output_dir.path().join("lineage").join("lineage.parquet");
    assert!(!parquet_path.exists());
}

#[test]
fn test_parse_lineage_empty_yaml() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let list_dir = input_dir.path().join("emptylist");
    fs::create_dir_all(&list_dir).unwrap();
    fs::write(list_dir.join("__lineage.yaml"), "").unwrap();

    parse_lineage(input_dir.path(), output_dir.path()).unwrap();

    let parquet_path = output_dir.path().join("lineage").join("lineage.parquet");
    assert!(!parquet_path.exists());
}

#[test]
fn test_parse_lineage_no_subdirectories() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    parse_lineage(input_dir.path(), output_dir.path()).unwrap();

    let parquet_path = output_dir.path().join("lineage").join("lineage.parquet");
    assert!(!parquet_path.exists());
}

#[test]
fn test_parse_lineage_parse_timestamp_consistent_type() {
    let input_dir = TempDir::new().unwrap();
    let output_dir = TempDir::new().unwrap();

    let list_dir = input_dir.path().join("ts.list");
    fs::create_dir_all(&list_dir).unwrap();

    let lineage_yaml = concat!(
        "email_index: \"1\"\n",
        "list_name: \"ts.list\"\n",
        "source_type: \"SRC\"\n",
        "write_mode: \"append\"\n",
        "archive_timestamp: \"2025-01-15T10:30:00Z\"\n",
        "archiver_build_info: \"A v=1\"\n",
    );
    fs::write(list_dir.join("__lineage.yaml"), lineage_yaml).unwrap();

    parse_lineage(input_dir.path(), output_dir.path()).unwrap();

    let parquet_path = output_dir.path().join("lineage").join("lineage.parquet");
    let file = fs::File::open(&parquet_path).unwrap();
    let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
    let reader = builder.build().unwrap();

    for batch in reader {
        let batch = batch.unwrap();
        let parse_ts = batch
            .column_by_name("parse_timestamp")
            .unwrap()
            .as_any()
            .downcast_ref::<Date64Array>()
            .unwrap();
        let archive_ts = batch
            .column_by_name("archive_timestamp")
            .unwrap()
            .as_any()
            .downcast_ref::<Date64Array>()
            .unwrap();

        assert!(!parse_ts.is_null(0));
        assert!(!archive_ts.is_null(0));
    }
}
