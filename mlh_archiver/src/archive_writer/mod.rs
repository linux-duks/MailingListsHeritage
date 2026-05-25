//! Archive writer module — reusable facade for storing fetched emails,
//! tracking progress, and logging errors for a single mailing list.
//!
//! # Design
//!
//! `ArchiveWriter` provides a consistent storage interface that **all worker
//! implementations MUST use**. This ensures:
//!
//! 1. **Uniform progress tracking** — `__progress.yaml` YAML files are
//!    created and updated the same way across all sources (NNTP, IMAP, etc.)
//! 2. **Resume support** — workers can resume from the last processed position
//!    regardless of the source type
//! 3. **Consistent file layout** — identical directory structure and file names
//!    for all implementations
//! 4. **Data lineage** — every fetched article is logged with metadata (source,
//!    timestamp, build info) to `__lineage.yaml`, creating an append-only audit trail
//!
//! # Architecture
//!
//! `ArchiveWriter` is a facade over four specialized components:
//!
//! | Component | Purpose |
//! |-----------|---------|
//! | [`ProgressTracker`] | Reads/writes `__progress.yaml` for resume support |
//! | [`DataLineageWriter`] | Appends lineage records to `__lineage.yaml` |
//! | [`RawEmailStore`] | Writes `{id}.eml` files |
//! | [`ErrorLogger`] | Appends `{id},{error}` to `__errors.csv` CSV |
//!
//! # Concurrency
//!
//! Each worker creates its own `ArchiveWriter` instance per list. Since workers
//! write to distinct output paths (one subdirectory per list), **no concurrency
//! control is needed**.
//!
//! # Usage
//!
//! ```rust
//! use std::path::Path;
//! use mlh_archiver::archive_writer::ArchiveWriter;
//! use mlh_archiver::config::RunModeConfig;
//!
//! // In real code, run_mode comes from AppConfig::get_run_mode_config(),
//! //  or by manually creating AppConfig::Variant(config)
//! # // For doctest we just show the pattern
//! ```
//!
//! ```ignore
//! let writer = ArchiveWriter::new(Path::new("./output"), "test.list", run_mode);
//!
//! // Resume: get last processed email ID
//! let last_id = writer.last_processed_id();
//!
//! // Archive a fetched email (writes .eml, updates progress, saves lineage)
//! writer.archive_email("42", &["From: user@example.com".to_string()]).unwrap();
//!
//! // Log unavailable emails (non-fatal)
//! writer.log_error("43", "email not available");
//! ```
//!
//! # File Layout
//!
//! ```text
//! output/
//! ├── list.name/
//! │   ├── 1.eml                    # Fetched email
//! │   ├── 2.eml
//! │   ├── __progress.yaml          # YAML: last processed ID (resume)
//! │   ├── __lineage.yaml           # YAML stream: DataLineage entries
//! │   └── __errors.csv             # CSV: id,error_message
//! ```

pub mod data_lineage;
mod email_store;
mod error_log;
pub mod parquet_email_store;
mod progress;
mod raw_email_store;

use crate::config::RunModeConfig;

pub use data_lineage::DataLineageWriter;
pub use email_store::{EmailData, EmailStore, WriteMode};
pub use error_log::ErrorLogger;
pub use parquet_email_store::ParquetEmailStore;
pub use parquet_email_store::parquet_email_store_schema;
pub use progress::ProgressTracker;
pub use raw_email_store::RawEmailStore;

use std::path::Path;

/// Facade combining progress tracking, error logging, email storage,
/// and data lineage for a single mailing list.
///
/// Created once per list by a worker. The email store requires `&mut self`
/// access for writes, so callers pass `&mut ArchiveWriter`.
///
/// # Why a Facade?
///
/// Instead of workers managing their own file I/O, `ArchiveWriter` provides
/// a single interface that all workers use. This ensures consistent behavior
/// across different source implementations (NNTP, IMAP, mbox, etc.).
pub struct ArchiveWriter {
    progress: ProgressTracker,
    error_log: ErrorLogger,
    email_store: Box<dyn EmailStore>,
    data_lineage: DataLineageWriter,
}

impl ArchiveWriter {
    /// Creates a new archive writer for the given list.
    ///
    /// # Arguments
    ///
    /// * `base_output_path` - Root output directory (e.g., `./output`)
    /// * `list_name` - Mailing list name (becomes subdirectory)
    /// * `run_mode` - Run mode configuration (used for lineage source type)
    pub fn new(
        base_output_path: &Path,
        list_name: &str,
        run_mode: RunModeConfig,
        write_mode: email_store::WriteMode,
    ) -> Self {
        let list_path = base_output_path.join(list_name);

        let storage: Box<dyn EmailStore> = match write_mode {
            WriteMode::RawEmails => Box::new(RawEmailStore::new(list_path)),
            WriteMode::Parquet { buffer_size } => {
                Box::new(ParquetEmailStore::new(list_path, buffer_size))
            }
        };

        Self {
            progress: ProgressTracker::new(base_output_path, list_name),
            error_log: ErrorLogger::new(base_output_path, list_name),
            email_store: storage,
            data_lineage: DataLineageWriter::new(base_output_path, list_name, run_mode, write_mode),
        }
    }

    /// Returns the last processed email ID from persisted state.
    ///
    /// This is the primary entry point for resume support. Workers should
    /// call this before starting to fetch emails, then start from the
    /// returned ID + 1.
    ///
    /// If no progress file exists, returns None, and each implementations
    /// should determine what is the initial ID
    pub fn last_processed_id(&self) -> Option<String> {
        self.progress.last_processed_id()
    }

    /// Archives a fetched email: writes to disk, updates progress, and saves
    /// lineage information.
    ///
    /// This is the primary method for storing a successfully fetched email.
    /// It performs three operations atomically:
    /// 1. Writes the email content to parquet inside `{list_name}/`
    /// 2. Updates `__progress.yaml` with the new last-processed ID
    /// 3. Appends a `DataLineage` record to `__lineage.yaml`
    ///
    /// # Arguments
    ///
    /// * `email_id` - Email/article number
    /// * `lines` - Raw email lines (can be any iterable collection of strings)
    pub fn archive_email<I, L>(&mut self, email_id: &str, lines: I) -> crate::Result<()>
    where
        I: IntoIterator<Item = L>,
        L: AsRef<str>,
    {
        let content: String = lines.into_iter().map(|l| l.as_ref().to_string()).collect();

        let committed_emails = self.email_store.add_email(email_store::EmailData {
            email_id: email_id.to_string(),
            content,
        })?;

        if let Some(committed_emails) = committed_emails {
            for email_id in committed_emails {
                self.progress.update(&email_id)?;
            }
        }
        self.data_lineage.update(email_id)
    }

    /// Logs an error for an unavailable email (non-fatal).
    ///
    /// Appends `{email_id},{error}` to the `__errors.csv` file.
    /// Failures to write the error log are logged as warnings but
    /// do not propagate as errors.
    pub fn log_error(&self, email_id: &str, error: &str) {
        self.error_log.log(email_id, error);
    }
}

impl Drop for ArchiveWriter {
    fn drop(&mut self) {
        if let Ok(Some(committed_emails)) = self.email_store.close()
            && let Some(last_email) = committed_emails.last()
        {
            log::debug!(
                "Saving progress on drop of ArchiveWriter with: {}",
                last_email
            );
            let _ = self.progress.update(last_email);
        }
    }
}
