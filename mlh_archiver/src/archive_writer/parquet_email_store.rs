use super::email_store::{EmailData, EmailStore};

use arrow::array::{RecordBatch, StringBuilder};
use arrow::datatypes::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::errors::ParquetError;
use parquet::file::properties::{WriterProperties, WriterVersion};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// ParquetEmailStore stores the emails in memory and write batches to parquet files.
/// The ArrowWriter is kept open across flushes, writing each batch as a separate
/// row group within a single parquet file. When max_emails_per_file is reached,
/// the current file is finalized and a new one is started.
pub struct ParquetEmailStore {
    output_path: PathBuf,
    schema: Arc<Schema>,
    buffer: Vec<EmailData>,
    /// Maximum number of emails per parquet file (creates new file when reached)
    max_emails_per_file: usize,
    /// Internal row group size: how many emails go into each Arrow RecordBatch.
    /// Capped at 2048 to keep bytes per RecordBatch safely under i32::MAX (~2.15 GB).
    row_group_size: usize,
    writer_index: usize,
    commited_files: Vec<String>,
    writer: Option<ArrowWriter<File>>,
    current_filename: Option<String>,
    emails_in_current_file: usize,
}

/// email_id: Utf8, content: Utf8
pub fn parquet_email_store_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("email_id", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
    ]))
}

impl ParquetEmailStore {
    /// Initializes the EmailStore.
    ///
    /// * `output_path` - directory or .parquet path for output files
    /// * `max_emails_per_file` - max emails per parquet file before rotating
    pub fn new(output_path: PathBuf, max_emails_per_file: usize) -> Self {
        let schema = parquet_email_store_schema();
        // Internal row group size: limits the bytes per Arrow RecordBatch to
        // stay under i32::MAX (~2.15 GB). At 0.5 MB per email, 2048 emails
        // produce ~1 GB per row group — safe margin.
        let row_group_size = max_emails_per_file.clamp(1, 2048);

        Self {
            output_path,
            schema,
            buffer: Vec::with_capacity(row_group_size),
            max_emails_per_file,
            row_group_size,
            writer_index: 0,
            commited_files: vec![],
            writer: None,
            current_filename: None,
            emails_in_current_file: 0,
        }
    }

    /// Internal method to lazily initialize the Parquet writer with specific properties.
    fn init_writer(&mut self) -> crate::Result<()> {
        let final_path = self.resolve_path()?;
        let file = File::create(&final_path)?;

        let filename = final_path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("Should have a file name")
            .to_string();

        let props = WriterProperties::builder()
            .set_compression(Compression::ZSTD(ZstdLevel::default()))
            .set_writer_version(WriterVersion::PARQUET_2_0)
            .build();

        let writer = ArrowWriter::try_new(file, self.schema.clone(), Some(props))?;
        self.writer = Some(writer);
        self.current_filename = Some(filename);
        Ok(())
    }

    /// Closes the current writer (finalizing the parquet file) and prepares
    /// for the next file by incrementing the writer index.
    fn rotate_file(&mut self) -> crate::Result<()> {
        if let Some(writer) = self.writer.take() {
            writer.close()?;
            if let Some(filename) = self.current_filename.take() {
                self.commited_files.push(filename);
            }
        }
        self.writer_index += 1;
        self.emails_in_current_file = 0;
        Ok(())
    }

    fn resolve_path(&mut self) -> Result<PathBuf, ParquetError> {
        let is_parquet = self
            .output_path
            .extension()
            .is_some_and(|ext| ext == "parquet");

        let (parent, stem) = if is_parquet {
            let p = self
                .output_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
            let s = self
                .output_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("data");
            (p, s.to_string())
        } else {
            (self.output_path.clone(), "data".to_string())
        };

        fs::create_dir_all(&parent)
            .map_err(|e| ParquetError::General(format!("Failed to create directories: {}", e)))?;

        if let Ok(entries) = fs::read_dir(&parent) {
            let mut max_index = None;

            for entry in entries.flatten() {
                if let Some(file_name) = entry.file_name().to_str()
                    && file_name.starts_with(&format!("{}_", stem))
                    && file_name.ends_with(".parquet")
                {
                    let index_part = &file_name[stem.len() + 1..file_name.len() - 8];
                    if let Ok(idx) = index_part.parse::<usize>() {
                        max_index = Some(max_index.map_or(idx, |m| std::cmp::max(m, idx)));
                    }
                }
            }

            self.writer_index = max_index.map(|m| m + 1).unwrap_or(0);
        }

        let final_filename = format!("{}_{:03}.parquet", stem, self.writer_index);
        Ok(parent.join(final_filename))
    }

    /// Converts the row-based buffer into columnar Arrow arrays and writes the batch
    /// as a new row group in the open parquet file.
    #[cfg_attr(feature = "otel", tracing::instrument(skip(self)))]
    fn flush(&mut self) -> crate::Result<Vec<String>> {
        let mut synced_items: Vec<String> = vec![];

        if self.buffer.is_empty() {
            return Ok(synced_items);
        }

        // Lazy-init the writer on first flush
        if self.writer.is_none() {
            self.init_writer()?;
        }

        let mut id_builder = StringBuilder::new();
        let mut content_builder = StringBuilder::new();

        for email in self.buffer.drain(..) {
            synced_items.push(email.email_id.clone());
            id_builder.append_value(email.email_id);
            content_builder.append_value(email.content);
        }

        let id_array = Arc::new(id_builder.finish());
        let content_array = Arc::new(content_builder.finish());

        let batch = RecordBatch::try_new(self.schema.clone(), vec![id_array, content_array])
            .map_err(|e| ParquetError::General(format!("Arrow error: {}", e)))?;

        // Write the RecordBatch as a new row group — writer stays open
        self.writer.as_mut().unwrap().write(&batch)?;

        Ok(synced_items)
    }
}

impl EmailStore for ParquetEmailStore {
    /// Appends data to the buffer. Flushes a row group when buffer reaches
    /// row_group_size. Creates a new file when max_emails_per_file is reached.
    fn add_email(&mut self, email: EmailData) -> crate::Result<Option<Vec<String>>> {
        self.buffer.push(email);
        self.emails_in_current_file += 1;

        let mut synced_items: Option<Vec<String>> = None;

        // Flush a row group when buffer reaches row_group_size
        if self.buffer.len() >= self.row_group_size {
            synced_items = Some(self.flush()?);
        }

        // Rotate to a new file when max_emails_per_file is reached
        if self.emails_in_current_file >= self.max_emails_per_file && self.max_emails_per_file > 0 {
            if !self.buffer.is_empty() {
                let flushed = self.flush()?;
                if let Some(ref mut items) = synced_items {
                    items.extend(flushed);
                } else {
                    synced_items = Some(flushed);
                }
            }
            self.rotate_file()?;
        }

        Ok(synced_items)
    }

    /// Flushes any remaining data in the buffer, then closes the writer
    /// to write the Parquet footer.
    fn close(&mut self) -> crate::Result<Option<Vec<String>>> {
        let mut synced_items: Option<Vec<String>> = None;

        let flushed = self.flush()?;
        if !flushed.is_empty() {
            synced_items = Some(flushed)
        }

        // Close the writer — writes the metadata footer and finalizes the file
        if let Some(writer) = self.writer.take() {
            writer.close()?;
            if let Some(filename) = self.current_filename.take() {
                self.commited_files.push(filename);
            }
        }

        Ok(synced_items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::StringArray;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    struct TestDirGuard {
        path: PathBuf,
    }

    impl TestDirGuard {
        fn new(path: PathBuf) -> Self {
            let _ = fs::remove_dir_all(&path);
            TestDirGuard { path }
        }
    }

    impl Drop for TestDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn create_dummy_email(id: &str, line_count: usize) -> EmailData {
        let content = (0..line_count)
            .map(|i| format!("This is line {} of email {}", i, id))
            .collect::<Vec<_>>()
            .join("");
        EmailData {
            email_id: id.to_string(),
            content,
        }
    }

    /// Reads all parquet files in a directory into a vec of (email_id, content) pairs.
    fn read_parquet_dir(dir: &Path) -> Vec<(String, String)> {
        let mut results = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            let mut parquet_paths: Vec<PathBuf> = entries
                .flatten()
                .filter_map(|e| {
                    let p = e.path();
                    if p.extension().is_some_and(|ext| ext == "parquet") {
                        Some(p)
                    } else {
                        None
                    }
                })
                .collect();
            parquet_paths.sort();
            for path in &parquet_paths {
                if let Ok(file) = File::open(path)
                    && let Ok(builder) = ParquetRecordBatchReaderBuilder::try_new(file)
                    && let Ok(reader) = builder.build()
                {
                    for batch in reader.flatten() {
                        let ids = batch
                            .column(0)
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .unwrap();
                        let contents = batch
                            .column(1)
                            .as_any()
                            .downcast_ref::<StringArray>()
                            .unwrap();
                        for row in 0..batch.num_rows() {
                            results.push((
                                ids.value(row).to_string(),
                                contents.value(row).to_string(),
                            ));
                        }
                    }
                }
            }
        }
        results
    }

    #[test]
    fn test_lazy_initialization() {
        let dir = PathBuf::from("./parquet_email_store.test_lazy_initialization/");
        let _guard = TestDirGuard::new(dir.clone());
        let base_path = dir.join("lazy_test.parquet");
        let expected_path = dir.join("lazy_test_000.parquet");

        let mut store = ParquetEmailStore::new(base_path.clone(), 10);

        assert!(
            !expected_path.exists(),
            "File should not be created until data is written"
        );

        store.add_email(create_dummy_email("email_1", 2)).unwrap();
        assert!(
            !expected_path.exists(),
            "File should still not exist, buffer not full"
        );

        store.flush().unwrap();
        assert!(expected_path.exists(), "File should be created after flush");
    }

    #[test]
    fn test_batch_flush_trigger() {
        let dir = PathBuf::from("./parquet_email_store.test_batch_flush_trigger/");
        let _guard = TestDirGuard::new(dir.clone());
        let base_path = dir.join("batch_test.parquet");
        let expected_path = dir.join("batch_test_000.parquet");

        // max_emails_per_file=2 → row_group_size = min(2048,2) = 2
        let mut store = ParquetEmailStore::new(base_path.clone(), 2);

        store.add_email(create_dummy_email("email_1", 1)).unwrap();
        assert!(!expected_path.exists());
        assert_eq!(store.buffer.len(), 1);

        // Second email: buffer hits 2 → flush (file created) → max reached → rotate
        store.add_email(create_dummy_email("email_2", 1)).unwrap();
        assert!(expected_path.exists());
        assert_eq!(store.buffer.len(), 0);
    }

    #[test]
    fn test_data_integrity_readback() {
        let dir = PathBuf::from("./parquet_email_store.test_data_integrity_readback/");
        let _guard = TestDirGuard::new(dir.clone());
        let base_path = dir.join("integrity_test.parquet");
        let _expected_path = dir.join("integrity_test_000.parquet");

        // 2 emails with max=5: both fit in one row group, close flushes
        let mut store = ParquetEmailStore::new(base_path.clone(), 5);
        store.add_email(create_dummy_email("email_1", 2)).unwrap();
        store.add_email(create_dummy_email("email_2", 3)).unwrap();

        store.close().unwrap();

        let data = read_parquet_dir(&dir);
        assert_eq!(data.len(), 2);

        assert_eq!(data[0].0, "email_1");
        assert_eq!(data[1].0, "email_2");
        assert_eq!(
            data[0].1,
            "This is line 0 of email email_1This is line 1 of email email_1"
        );
        assert_eq!(
            data[1].1,
            "This is line 0 of email email_2This is line 1 of email email_2This is line 2 of email email_2"
        );
    }

    #[test]
    fn test_large_email_stress() {
        // Tunable parameters via env vars:
        //   STRESS_NUM_EMAILS  — total emails to write
        //   STRESS_BATCH_SIZE  — max_emails_per_file (creates new file when reached)
        let number_of_emails: usize = std::env::var("STRESS_NUM_EMAILS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500);
        let max_per_file: usize = std::env::var("STRESS_BATCH_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2048); // max_emails_per_file (also caps row_group_size)
        let body_size_bytes: usize = 512 * 1024; // 0.5MB

        let dir = PathBuf::from(format!(
            "./parquet_email_store.test_large_email_stress_{}/",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let _guard = TestDirGuard::new(dir.clone());
        let base_path = dir.join("stress_test.parquet");

        let mut store = ParquetEmailStore::new(base_path.clone(), max_per_file);

        for i in 0..number_of_emails {
            let prefix = format!("email_{i}_");
            let padding = "X".repeat(body_size_bytes - prefix.len());
            let content = prefix + &padding;
            assert_eq!(content.len(), body_size_bytes);
            let email = EmailData {
                email_id: format!("email_{i}"),
                content,
            };
            store.add_email(email).unwrap();
        }

        store.close().unwrap();

        // Verify files exist and have data
        assert!(
            dir.read_dir().unwrap().count() > 0,
            "Output dir should not be empty"
        );

        // Read back all parquet files and verify total
        let all_data = read_parquet_dir(&dir);
        assert_eq!(all_data.len(), number_of_emails);

        // Each content should have the expected byte length
        for (_id, content) in &all_data {
            assert_eq!(content.len(), body_size_bytes);
        }
    }

    #[test]
    fn test_path_collision_logic() {
        let dir = PathBuf::from("./parquet_email_store.test_path_collision_logic/");
        let _guard = TestDirGuard::new(dir.clone());
        let base_path = dir.join("collision.parquet");

        {
            // max=1 → each email creates its own file
            let mut store = ParquetEmailStore::new(base_path.clone(), 1);
            store
                .add_email(EmailData {
                    email_id: "1".into(),
                    content: String::new(),
                })
                .unwrap();
            store.close().unwrap();
            assert!(dir.join("collision_000.parquet").exists());
        }

        {
            let mut store = ParquetEmailStore::new(base_path.clone(), 1);
            store
                .add_email(EmailData {
                    email_id: "2".into(),
                    content: String::new(),
                })
                .unwrap();
            store.close().unwrap();
            assert!(dir.join("collision_001.parquet").exists());
        }

        {
            let mut store = ParquetEmailStore::new(base_path.clone(), 1);
            store
                .add_email(EmailData {
                    email_id: "3".into(),
                    content: String::new(),
                })
                .unwrap();
            store.close().unwrap();
            assert!(dir.join("collision_002.parquet").exists());
        }
    }
}
