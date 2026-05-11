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
use std::sync::Arc;

use arrow::array::StringBuilder;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::file::properties::{WriterProperties, WriterVersion};

use crate::Result;

const ROW_GROUP_SIZE: usize = 10_000;

#[derive(Debug, serde::Deserialize)]
struct LineageRecord {
    email_index: String,
    list_name: String,
    source_type: String,
    write_mode: String,
    timestamp: String,
    archiver_build_info: String,
}

impl From<LineageRecord> for HashMap<String, String> {
    fn from(r: LineageRecord) -> Self {
        let mut m = HashMap::new();
        m.insert("email_index".to_string(), r.email_index);
        m.insert("list_name".to_string(), r.list_name);
        m.insert("source_type".to_string(), r.source_type);
        m.insert("write_mode".to_string(), r.write_mode);
        m.insert("timestamp".to_string(), r.timestamp);
        m.insert("archiver_build_info".to_string(), r.archiver_build_info);
        m
    }
}

fn lineage_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("email_index", DataType::Utf8, true),
        Field::new("list_name", DataType::Utf8, true),
        Field::new("source_type", DataType::Utf8, true),
        Field::new("write_mode", DataType::Utf8, true),
        Field::new("timestamp", DataType::Utf8, true),
        Field::new("archiver_build_info", DataType::Utf8, true),
    ]))
}

fn build_lineage_batch(
    entries: &[HashMap<String, String>],
    schema: &Arc<Schema>,
) -> Result<RecordBatch> {
    let mut email_builder = StringBuilder::new();
    let mut list_builder = StringBuilder::new();
    let mut source_builder = StringBuilder::new();
    let mut write_mode_builder = StringBuilder::new();
    let mut timestamp_builder = StringBuilder::new();
    let mut build_info_builder = StringBuilder::new();

    for entry in entries {
        email_builder.append_value(entry.get("email_index").map(|s| s.as_str()).unwrap_or(""));
        list_builder.append_value(entry.get("list_name").map(|s| s.as_str()).unwrap_or(""));
        source_builder.append_value(entry.get("source_type").map(|s| s.as_str()).unwrap_or(""));
        write_mode_builder.append_value(entry.get("write_mode").map(|s| s.as_str()).unwrap_or(""));
        timestamp_builder.append_value(entry.get("timestamp").map(|s| s.as_str()).unwrap_or(""));
        build_info_builder.append_value(
            entry
                .get("archiver_build_info")
                .map(|s| s.as_str())
                .unwrap_or(""),
        );
    }

    let batch = RecordBatch::try_new(
        Arc::clone(schema),
        vec![
            Arc::new(email_builder.finish()),
            Arc::new(list_builder.finish()),
            Arc::new(source_builder.finish()),
            Arc::new(write_mode_builder.finish()),
            Arc::new(timestamp_builder.finish()),
            Arc::new(build_info_builder.finish()),
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
            let record = match serde_yaml::from_str::<LineageRecord>(trimmed) {
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
