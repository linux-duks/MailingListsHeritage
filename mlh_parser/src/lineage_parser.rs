use chrono::Utc;
/// Lineage
///
/// folder structure:
///
/// ├── {list_name}
/// │   ├── data_000.parquet (one or more)
/// │   ├── __lineage.yaml
/// │   └── __progress.yaml
///
/// Each __lineage.yaml is a multi-document YAML stream (separated by ---).
/// All entries share the same columns:
///   email_index, list_name, source_type, write_mode, timestamp, archiver_build_info
///
/// This module will stream content into parquet row group batches
///
use std::collections::HashMap;
use std::fs;
use std::io::BufWriter;
use std::path::Path;
use std::sync::{Arc, LazyLock};

use crate::config::built_info;

/// Shared build info string — computed once, cloned cheaply via `Arc`.
static BUILD_INFO: LazyLock<Arc<str>> = LazyLock::new(|| {
    format!(
        "\"Parser v='{}' commit='{}' build_time_utc='{}' target='{}' rustc='{}'\"",
        built_info::PKG_VERSION,
        built_info::GIT_VERSION.unwrap_or("unknown"),
        built_info::BUILT_TIME_UTC,
        built_info::TARGET,
        built_info::RUSTC_VERSION,
    )
    .into()
});

use arrow::array::{Date64Builder, StringBuilder};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::{WriterProperties, WriterVersion};

use crate::Result;

const ROW_GROUP_SIZE: usize = 50_000;

use mlh_archiver::DataLineageRecord;

fn lineage_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("email_index", DataType::Utf8, true),
        Field::new("list_name", DataType::Utf8, true),
        Field::new("source_type", DataType::Utf8, true),
        Field::new("write_mode", DataType::Utf8, true),
        Field::new("archive_timestamp", DataType::Date64, true),
        Field::new("archiver_build_info", DataType::Utf8, true),
        Field::new("parse_timestamp", DataType::Date64, true),
        Field::new("parser_build_info", DataType::Utf8, true),
    ]))
}

fn build_lineage_batch(
    entries: &[HashMap<String, String>],
    schema: &Arc<Schema>,
) -> Result<RecordBatch> {
    // columns from archiver
    let mut email_builder = StringBuilder::new();
    let mut list_builder = StringBuilder::new();
    let mut source_builder = StringBuilder::new();
    let mut write_mode_builder = StringBuilder::new();
    let mut archiver_timestamp_builder = Date64Builder::new();
    let mut archiver_build_info_builder = StringBuilder::new();
    // columns from parser
    let mut parser_timestamp_builder = Date64Builder::new();
    let mut parser_build_info_builder = StringBuilder::new();

    // single timestamp, because this is not done at the same time for each line.
    let parse_ts = Utc::now().timestamp();

    for entry in entries {
        // read entries from archiver
        email_builder.append_value(entry.get("email_index").map(|s| s.as_str()).unwrap_or(""));
        list_builder.append_value(entry.get("list_name").map(|s| s.as_str()).unwrap_or(""));
        source_builder.append_value(entry.get("source_type").map(|s| s.as_str()).unwrap_or(""));
        write_mode_builder.append_value(entry.get("write_mode").map(|s| s.as_str()).unwrap_or(""));

        // this failure should not happen, but try to parse from the RFC3339 expected from Archiver
        let archiver_timestamp = entry
            .get("archive_timestamp")
            .map(|s| match chrono::DateTime::parse_from_rfc3339(s) {
                Ok(d) => Some(d.timestamp()),
                Err(_) => None,
            })
            .unwrap_or(None);

        if let Some(archive_ts) = archiver_timestamp {
            archiver_timestamp_builder.append_value(archive_ts);
        } else {
            archiver_timestamp_builder.append_null();
        }

        archiver_build_info_builder.append_value(
            entry
                .get("archiver_build_info")
                .map(|s| s.as_str())
                .unwrap_or(""),
        );

        // fill entries from parser
        parser_timestamp_builder.append_value(parse_ts);
        parser_build_info_builder.append_value(BUILD_INFO.clone());
    }

    let batch = RecordBatch::try_new(
        Arc::clone(schema),
        vec![
            Arc::new(email_builder.finish()),
            Arc::new(list_builder.finish()),
            Arc::new(source_builder.finish()),
            Arc::new(write_mode_builder.finish()),
            Arc::new(archiver_timestamp_builder.finish()),
            Arc::new(archiver_build_info_builder.finish()),
            Arc::new(parser_timestamp_builder.finish()),
            Arc::new(parser_build_info_builder.finish()),
        ],
    )?;

    Ok(batch)
}

pub fn parse_lineage(input_dir: &Path, output_dir: &Path) -> Result<()> {
    let mut total_entries: usize = 0;
    let mut writer: Option<ArrowWriter<BufWriter<fs::File>>> = None;
    let schema = lineage_schema();
    let mut batch: Vec<HashMap<String, String>> = Vec::with_capacity(ROW_GROUP_SIZE);

    for entry in fs::read_dir(input_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let lineage_path = entry.path().join("__lineage.yaml");
        if !lineage_path.exists() {
            continue;
        }

        log::debug!("Reading lineage from: {}", lineage_path.display());

        let content = match fs::read_to_string(&lineage_path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Error reading {}: {}", lineage_path.display(), e);
                continue;
            }
        };

        if content.is_empty() {
            continue;
        }

        for (i, part) in content.split("\n---\n").enumerate() {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            let record = match serde_yaml::from_str::<DataLineageRecord>(trimmed) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!(
                        "Failed to parse document {} in {}: {}",
                        i,
                        lineage_path.display(),
                        e
                    );
                    continue;
                }
            };

            let entry_map: HashMap<String, String> = record.into();

            if writer.is_none() {
                let lineage_dir = output_dir.join("lineage");
                fs::create_dir_all(&lineage_dir)?;
                let parquet_path = lineage_dir.join("lineage.parquet");

                let file = fs::File::create(&parquet_path)?;
                let level = ZstdLevel::try_new(22)?;
                let props = WriterProperties::builder()
                    .set_compression(Compression::ZSTD(level))
                    .set_writer_version(WriterVersion::PARQUET_2_0)
                    .build();

                writer = Some(ArrowWriter::try_new(
                    BufWriter::new(file),
                    Arc::clone(&schema),
                    Some(props),
                )?);
                log::info!("Writing lineage to: {}", parquet_path.display());
            }

            batch.push(entry_map);

            if batch.len() >= ROW_GROUP_SIZE {
                let record_batch = build_lineage_batch(&batch, &schema)?;
                writer.as_mut().unwrap().write(&record_batch)?;
                total_entries += batch.len();
                batch.clear();
            }
        }
    }

    if !batch.is_empty()
        && let Some(ref mut w) = writer
    {
        let record_batch = build_lineage_batch(&batch, &schema)?;
        w.write(&record_batch)?;
        total_entries += batch.len();
    }

    if let Some(w) = writer {
        w.close()?;
        log::info!(
            "Wrote {} total lineage entries across all row groups",
            total_entries
        );
    } else {
        log::warn!("No lineage entries found to write.");
    }

    Ok(())
}
