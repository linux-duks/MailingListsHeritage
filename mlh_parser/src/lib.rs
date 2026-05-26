//! MLH Parser — converts raw mailing list emails into a structured Parquet dataset.
//!
//! # Pipeline
//!
//! ```text
//! raw .eml / .parquet  →  parse_email()  →  ParsedEmail  →  build_record_batch()  →  .parquet
//! ```
//!
//! The main entry point is [`start`], which orchestrates thread-pool dispatch
//! across mailing lists. Individual emails are parsed via [`parse_email`]
//! and collected into batched Arrow record batches via [`flush_batch`].
//!
//! [`parse_email`]: crate::email_parser::parse_email
//! [`flush_batch`]: crate::dataset_writer::flush_batch

pub mod address_parser;
pub mod config;
pub mod constants;
pub mod dataset_writer;
pub mod date_parser;
pub mod email_file_reader;
pub mod email_parser;
pub mod email_reader;
pub mod entities;
pub mod errors;
pub mod extractors;
pub mod lineage_parser;

use crate::constants::{BATCH_MAX_RECORDS, PARQUET_FILE_NAME};
use crate::errors::ParseError;
pub use entities::{Attribution, ParsedEmail};

use chrono::FixedOffset;
use rayon::prelude::*;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Convenience result type used throughout the crate.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Starts the parsing routine according to the configs
pub fn start(cfg: &mut crate::config::AppConfig) -> Result<()> {
    let input_path = PathBuf::from(&cfg.input_dir_path);
    let output_path = PathBuf::from(&cfg.output_dir_path);

    // use the list of files folders in the config, or list all subfolders for the input folder
    let lists: Vec<String> = if let Some(ref specified_lists) = cfg.lists_to_parse {
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
        log::warn!("No items found to parse.");
        return Ok(());
    }

    if cfg.nthreads < 1 {
        cfg.nthreads = 1;
    }

    rayon::scope(|s| {
        for mail_l in lists {
            let input = input_path.clone();
            let output = output_path.clone();
            let fail_on_err = cfg.fail_on_parsing_error;

            s.spawn(move |_| {
                log::debug!("Processing: {mail_l}");

                if let Err(e) = process_mailing_list_wrap(&mail_l, &input, &output, fail_on_err) {
                    log::error!("Error on {}: {}", mail_l, e);
                    if fail_on_err {
                        panic!("Fail on error requested. Error: {e}");
                    }
                }
            });
        }
    });
    lineage_parser::parse_lineage(&input_path, &output_path)?;

    Ok(())
}

fn process_mailing_list_wrap(
    mail_l: &str,
    input_dir: &Path,
    output_dir: &Path,
    fail_on_error: bool,
) -> Result<()> {
    log::debug!("Processing: {}", mail_l);
    process_mailing_list(
        mail_l,
        input_dir,
        output_dir,
        fail_on_error,
        BATCH_MAX_RECORDS,
    )
    .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))
}

/// Parses all emails in a single mailing list directory into a Parquet file.
///
/// Reads `.eml` and `.parquet` files from `input_dir/<mailing_list>/`, writes
/// the output to `output_dir/dataset/list=<mailing_list>/list_data.parquet`.
///
/// Emails are accumulated into batches and flushed when `max_records_per_batch`
/// is reached, keeping Arrow row groups under the `i32` offset ceiling.
pub fn process_mailing_list(
    mailing_list: &str,
    input_dir: &Path,
    output_dir: &Path,
    fail_on_error: bool,
    max_records_per_batch: usize,
) -> Result<()> {
    let list_input_path = input_dir.join(mailing_list);

    let success_output_path = output_dir
        .join("dataset")
        .join(format!("list={}", mailing_list));
    let parquet_path = success_output_path.join(PARQUET_FILE_NAME);

    let lineage_dir_path = output_dir.join("lineage");
    let error_output_path = output_dir.join(format!("errors/list={}", mailing_list));

    fs::create_dir_all(&success_output_path)?;
    fs::create_dir_all(&lineage_dir_path)?;

    let files = collect_email_files(&list_input_path);

    log::debug!("Collected a list of {} files. First 5:", files.len());
    if log::log_enabled!(log::Level::Debug) {
        for val in files.iter().take(5) {
            println!(" {}", val.clone().display());
        }
    }

    let now = FixedOffset::east_opt(0)
        .map(|tz| chrono::Utc::now().with_timezone(&tz))
        .unwrap_or_else(|| chrono::Utc::now().with_timezone(&FixedOffset::east_opt(0).unwrap()));

    let mut total_parsed: usize = 0;
    let mut arrow_writer: Option<dataset_writer::DatasetWriter> = None;
    let mut error_writer: Option<BufWriter<fs::File>> = None;

    // created the email iterator
    let mut emails = email_file_reader::file_iterator(files);

    loop {
        // Take the next batch of emails from the iterator
        let batch: Vec<_> = emails.by_ref().take(max_records_per_batch).collect();

        if batch.is_empty() {
            break;
        }

        // Process this batch — IndexedParallelIterator preserves input order
        let mut batch_emails: Vec<(ParsedEmail, String)> = Vec::new();

        let mut rows = Vec::new();
        for r in batch {
            rows.push(r?);
        }

        // into_par_iter().map().collect() preserves order (IndexedParallelIterator)
        let results: Vec<_> = rows
            .into_par_iter()
            .map(|row| {
                let email_id = row.email_id;
                let content = row.content;
                match email_parser::parse_email(content.as_bytes(), now) {
                    Ok(email) => Ok((email, email_id)),
                    Err(e) => Err((e, email_id)),
                }
            })
            .collect();

        for result in results {
            match result {
                Ok((email, email_id)) => {
                    batch_emails.push((email, email_id));
                }
                Err((e, email_id)) => {
                    log::error!("Failed to parse email {}: {}", email_id, e);
                    if fail_on_error {
                        return Err(Box::new(e));
                    }
                    if error_writer.is_none() {
                        fs::create_dir_all(&error_output_path)?;
                        let csv_path = error_output_path.join("errors.csv");
                        let file = fs::File::create(&csv_path)?;
                        error_writer = Some(BufWriter::new(file));
                    }
                    let msg_flat = e.to_string().replace('\n', "\\n");
                    let line = csv_escape(&email_id, &msg_flat);
                    writeln!(error_writer.as_mut().unwrap(), "{line}")?;
                }
            }
        }

        // Flush the batch
        if !batch_emails.is_empty() {
            dataset_writer::flush_batch(
                mailing_list,
                &parquet_path,
                &mut batch_emails,
                &mut total_parsed,
                &mut arrow_writer,
            )?;
        }
    }

    if let Some(writer) = arrow_writer {
        writer.close()?;
    } else {
        log::warn!("No emails parsed successfully for list '{}'", mailing_list);
        return Ok(());
    }

    log::info!(
        "Saved {} parsed emails for list '{}'",
        total_parsed,
        mailing_list
    );

    Ok(())
}

/// Extracts the `Message-ID` header value from raw email content.
///
/// Returns [`ParseError::NoMessageId`] if the header is missing.
pub fn get_email_id(email_content: &str) -> std::result::Result<String, ParseError> {
    for line in email_content.lines() {
        if line.to_lowercase().starts_with("message-id:") {
            let message_id = line["message-id:".len()..].trim();
            return Ok(message_id.to_string());
        }
    }
    Err(ParseError::NoMessageId)
}

fn csv_escape(email_id: &str, error_msg: &str) -> String {
    let esc = |s: &str| -> String {
        if s.contains(',') || s.contains('"') || s.contains('\n') {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    format!("{},{}", esc(email_id), esc(error_msg))
}

fn collect_email_files(input_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(input_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let ext = path
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                if ext == "eml" || ext == "parquet" {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    files
}
