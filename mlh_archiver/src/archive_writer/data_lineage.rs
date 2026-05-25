//! Data lineage tracking for the archive writer.
//!
//! Every time an article is fetched and stored, a `DataLineageRecord` record is
//! appended to the `__progress.yaml` file. This creates an append-only audit
//! trail that captures:
//!
//! - **What** was fetched (article ID, list name)
//! - **Where** it came from (source type / run mode configuration)
//! - **When** it was fetched (UTC timestamp)
//! - **With which version** of the archiver (build info including commit,
//!   target platform, Rust version, build time)
//!
//! The `__progress.yaml` file is a multi-document YAML stream where each
//! document is a `DataLineageRecord` entry, separated by `---`.
//!
//! # Example file content
//!
//! ```yaml
//! email_index: 1
//! list_name: test.groups.foo
//! source_type: "NNTP h=localhost"
//! timestamp: 2025-01-15T10:30:00Z
//! archiver_build_info: "Archiver v=0.1.0 commit=abc123 ..."
//! ---
//! email_index: 2
//! list_name: test.groups.foo
//! source_type: "NNTP h=localhost"
//! timestamp: 2025-01-15T10:30:05Z
//! archiver_build_info: "Archiver v=0.1.0 commit=abc123 ..."
//! ```

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use crate::archive_writer::WriteMode;
use crate::config::{RunModeConfig, built_info};

/// Shared build info string — computed once, cloned cheaply via `Arc`.
static BUILD_INFO: LazyLock<Arc<str>> = LazyLock::new(|| {
    format!(
        "\"Archiver v='{}' commit='{}' build_time_utc='{}' target='{}' rustc='{}'\"",
        built_info::PKG_VERSION,
        built_info::GIT_VERSION.unwrap_or("unknown"),
        built_info::BUILT_TIME_UTC,
        built_info::TARGET,
        built_info::RUSTC_VERSION,
    )
    .into()
});

/// Progress state for a mailing list.
#[derive(Serialize, Deserialize, Debug)]
pub struct DataLineageRecord {
    /// email id/file_name
    pub email_index: String,
    /// mailing list name
    pub list_name: String,
    /// name of the RunMode
    pub source_type: String,
    /// writer module used
    pub write_mode: String,
    /// UTC date when the read was performed
    /// encoded using RFC3339
    pub archive_timestamp: String,
    /// build information about the archiver software
    pub archiver_build_info: String,
}

impl From<DataLineageRecord> for HashMap<String, String> {
    fn from(r: DataLineageRecord) -> Self {
        let mut m = HashMap::new();
        m.insert("email_index".to_string(), r.email_index);
        m.insert("list_name".to_string(), r.list_name);
        m.insert("source_type".to_string(), r.source_type);
        m.insert("write_mode".to_string(), r.write_mode);
        m.insert("archive_timestamp".to_string(), r.archive_timestamp);
        m.insert("archiver_build_info".to_string(), r.archiver_build_info);
        m
    }
}

#[derive(std::fmt::Debug)]
pub struct DataLineageWriter {
    output_path: PathBuf,
    list_name: String,
    build_info: Arc<str>,
    // save as string, ready to format
    run_mode: String,
    write_mode: String,
}

impl DataLineageWriter {
    /// # Arguments
    ///
    /// * `base_path` - Root output directory (e.g., `./output`)
    /// * `list_name` - Mailing list name (becomes subdirectory)
    pub fn new(
        base_path: &Path,
        list_name: &str,
        run_mode: RunModeConfig,
        write_mode: WriteMode,
    ) -> Self {
        Self {
            output_path: base_path.join(list_name).join("__lineage.yaml"),
            list_name: list_name.to_string(),
            build_info: BUILD_INFO.clone(),
            run_mode: run_mode.to_string(),
            write_mode: write_mode.to_string(),
        }
    }

    /// Persists the last successfully processed email ID.
    ///
    /// # Arguments
    ///
    /// * `id` - email ID that was just processed
    pub fn update(&self, id: &str) -> crate::Result<()> {
        crate::file_utils::append_yaml_to_file(
            self.output_path.to_str().unwrap(),
            &DataLineageRecord {
                email_index: id.to_string(),
                list_name: self.list_name.clone(),
                source_type: self.run_mode.clone(),
                archiver_build_info: (*self.build_info).to_string(),
                write_mode: self.write_mode.to_string(),
                archive_timestamp: Utc::now().to_rfc3339(),
            },
        )
        .map_err(crate::errors::Error::Io)
    }
}
