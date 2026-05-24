//! MLH anonymizer — anonymizes personally identifiable information in Parquet datasets.
//!
//! # Pipeline
//!
//! ```text
//! input .parquet  →  read DataFrame  →  anonymize columns (batched)  →  write .parquet
//! ```

pub mod anonymizer;
pub mod config;
pub mod constants;
pub mod errors;
pub mod reader;
pub mod transform;
pub mod writer;

use crate::constants::BATCH_MAX_RECORDS;
use polars::io::parquet::write::BatchedWriter;
use std::fs;
use std::path::{Path, PathBuf};

/// Convenience result type used throughout the crate.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Starts the anonymization routine according to the config.
pub fn start(cfg: &mut crate::config::AppConfig) -> Result<()> {
    let input_path = PathBuf::from(&cfg.input_dir_path);
    let output_path = PathBuf::from(&cfg.output_dir_path);

    let lists: Vec<String> = if let Some(ref specified_lists) = cfg.lists_to_anonymize {
        specified_lists.clone()
    } else {
        fs::read_dir(input_path.as_path())?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if entry.file_type().ok()?.is_dir() {
                    entry.file_name().into_string().ok()
                } else {
                    None
                }
            })
            .collect()
    };

    if lists.is_empty() {
        log::warn!("No items found to anonymize.");
        return Ok(());
    }

    if cfg.nthreads < 1 {
        cfg.nthreads = 1;
    }

    let compression = cfg.compression_level.unwrap_or(12);
    rayon::scope(|s| {
        for mail_l in lists {
            let input = input_path.clone();
            let output = output_path.clone();

            s.spawn(move |_| {
                log::debug!("Processing: {mail_l}");

                if let Err(e) = process_mailing_list(&mail_l, &input, &output, compression) {
                    log::error!("Error on {}: {}", mail_l, e);
                }
            });
        }
    });

    Ok(())
}

/// Try to find the list directory, checking both bare name and `list=<name>` Hive format.
fn resolve_list_dir(input_dir: &Path, mailing_list: &str) -> PathBuf {
    let bare = input_dir.join(mailing_list);
    if bare.is_dir() {
        return bare;
    }
    let hive = input_dir.join(format!("list={}", mailing_list));
    if hive.is_dir() {
        return hive;
    }
    bare
}

/// Anonymizes all rows for a single mailing list: reads input Parquet in batches,
/// anonymizes each batch, and writes the output with row group control.
pub fn process_mailing_list(
    mailing_list: &str,
    input_dir: &Path,
    output_dir: &Path,
    compression: usize,
) -> Result<()> {
    let list_input_path = resolve_list_dir(input_dir, mailing_list);

    let main_output_dir = output_dir.join("dataset").join(mailing_list);
    let main_output_path = main_output_dir.join("list_data.parquet");

    let id_map_output_dir = output_dir
        .join(format!("id_map_{}", constants::SPLIT_DATASET_COLUMN))
        .join(mailing_list);
    let id_map_output_path = id_map_output_dir.join("list_data.parquet");

    log::info!(
        "Anonymizing list '{}': {} → {}",
        mailing_list,
        list_input_path.display(),
        main_output_path.display()
    );

    let mut main_writer: Option<BatchedWriter<fs::File>> = None;
    let mut id_map_writer: Option<BatchedWriter<fs::File>> = None;
    let mut total_rows = 0usize;
    let mut batch_count = 0usize;

    reader::read_parquet_dir_batched(&list_input_path, BATCH_MAX_RECORDS, |df| {
        batch_count += 1;
        let rows = df.height();
        total_rows += rows;

        log::info!(
            " {} batch {}: read {} rows ({} total)",
            mailing_list,
            batch_count,
            rows,
            total_rows,
        );

        let mut id_map = transform::build_id_map(&df)?;
        id_map.rechunk_mut();
        let mut anon_df = transform::anonymize_dataframe(df)?;
        anon_df.rechunk_mut();

        if main_writer.is_none() {
            main_writer = Some(writer::create_batched_writer(
                &main_output_path,
                compression,
                anon_df.schema(),
            )?);
        }
        main_writer.as_mut().unwrap().write_batch(&anon_df)?;

        if id_map_writer.is_none() {
            id_map_writer = Some(writer::create_batched_writer(
                &id_map_output_path,
                compression,
                id_map.schema(),
            )?);
        }
        id_map_writer.as_mut().unwrap().write_batch(&id_map)?;

        Ok(())
    })?;

    if batch_count == 0 {
        log::warn!("No data found for list '{}'", mailing_list);
        return Ok(());
    }

    if let Some(w) = main_writer {
        w.finish()?;
    }
    if let Some(w) = id_map_writer {
        w.finish()?;
    }

    log::info!(
        "Saved {} anonymized rows in {} batches for list '{}'",
        total_rows,
        batch_count,
        mailing_list,
    );

    Ok(())
}
