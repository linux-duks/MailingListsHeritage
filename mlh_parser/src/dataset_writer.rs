//! Parquet output — Arrow record batch construction and batched row-group writes.

use crate::ParsedEmail;
use crate::constants;

use arrow::array::*;
use arrow::datatypes::*;
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::errors::Result;
use parquet::file::properties::WriterProperties;
use std::fs;
use std::io::BufWriter;
use std::path::Path;
use std::sync::Arc;

/// Type alias for a buffered Arrow Parquet writer.
pub type DatasetWriter = ArrowWriter<BufWriter<fs::File>>;

/// Create a new dataset writer (not yet implemented).
pub fn create_writer() {}

/// Flushes accumulated parsed emails into a Parquet row group.
///
/// If the writer hasn't been created yet, it opens (or creates) the output
/// file at `parquet_path`. Subsequent calls write additional row groups into
/// the same file via the reused writer.
///
/// After flushing, `batch_emails` and `batch_raw_body_bytes` are cleared,
/// and `total_parsed` is incremented.
pub fn flush_batch(
    mailing_list: &str,
    parquet_path: &Path,
    batch_emails: &mut Vec<(ParsedEmail, String)>,
    total_parsed: &mut usize,
    arrow_writer: &mut Option<DatasetWriter>,
) -> Result<(), Box<dyn std::error::Error>> {
    if batch_emails.is_empty() {
        return Ok(());
    }

    let count = batch_emails.len();
    log::debug!("parse_mail_at[{mailing_list}]: flushing batch of {count} emails",);

    let batch = build_record_batch(batch_emails)?;

    if arrow_writer.is_none() {
        let file = fs::File::create(parquet_path)?;
        let writer = BufWriter::new(file);
        let props = WriterProperties::builder().build();
        *arrow_writer = Some(ArrowWriter::try_new(writer, batch.schema(), Some(props))?);
    }

    arrow_writer.as_mut().unwrap().write(&batch)?;

    *total_parsed += count;
    batch_emails.clear();

    Ok(())
}

/// Builds an Arrow [`RecordBatch`] from a slice of `(ParsedEmail, source_reference)` pairs.
///
/// Uses the fixed schema defined in [`PARQUET_SCHEMA`](crate::constants::PARQUET_SCHEMA).
/// Each parsed email becomes one row; list-valued columns (to, cc, references,
/// client-date, trailers, code) are delimited, and the `date` column is
/// parsed from RFC 3339.
pub fn build_record_batch(
    emails: &[(ParsedEmail, String)],
) -> Result<RecordBatch, Box<dyn std::error::Error>> {
    let schema = constants::PARQUET_SCHEMA.clone();

    let mut from_arr = StringBuilder::new();
    let mut to_arr = ListBuilder::new(StringBuilder::new());
    let mut cc_arr = ListBuilder::new(StringBuilder::new());
    let mut subject_arr = StringBuilder::new();
    let mut has_patch_tag_arr = BooleanBuilder::new();
    let mut has_rfc_tag_arr = BooleanBuilder::new();
    let mut has_response_tag_arr = BooleanBuilder::new();
    let mut has_forward_tag_arr = BooleanBuilder::new();
    let mut patch_version_arr = UInt16Builder::new();
    let mut patchset_sequence_number_arr = StringBuilder::new();
    let mut untegged_subject_arr = StringBuilder::new();
    let mut subject_tags_arr = ListBuilder::new(StringBuilder::new());
    let mut date_arr = TimestampMicrosecondBuilder::new();
    let mut client_date_arr = ListBuilder::new(StringBuilder::new());
    let mut message_id_arr = StringBuilder::new();
    let mut in_reply_to_arr = StringBuilder::new();
    let mut references_arr = ListBuilder::new(StringBuilder::new());
    let mut x_mailing_list_arr = StringBuilder::new();

    let trailer_fields = Fields::from(vec![
        Field::new("attribution", DataType::Utf8, false),
        Field::new("identification", DataType::Utf8, false),
    ]);
    let mut trailers_arr = ListBuilder::new(StructBuilder::new(
        trailer_fields,
        vec![
            Box::new(StringBuilder::new()) as Box<dyn ArrayBuilder>,
            Box::new(StringBuilder::new()),
        ],
    ));

    let mut code_arr = ListBuilder::new(StringBuilder::new());
    let mut raw_body_arr = StringBuilder::new();
    let mut body_sha1_arr = StringBuilder::new();
    let mut source_reference_arr = StringBuilder::new();

    for (idx, (email, source_reference)) in emails.iter().enumerate() {
        // message_id
        {
            message_id_arr.append_value(&email.message_id);
        }
        // from
        {
            from_arr.append_value(&email.from);
        }

        // to
        {
            for item in &email.to {
                to_arr.values().append_value(item);
            }
            to_arr.append(!email.to.is_empty());
        }

        // cc
        {
            for item in &email.cc {
                cc_arr.values().append_value(item);
            }
            cc_arr.append(!email.cc.is_empty());
        }

        // subject
        {
            subject_arr.append_value(&email.subject);
        }

        // subject_tags fields
        {
            let st = &email.subject_tags;
            has_patch_tag_arr.append_value(st.has_patch_tag);
            has_rfc_tag_arr.append_value(st.has_rfc_tag);
            has_response_tag_arr.append_value(st.has_response_tag);
            has_forward_tag_arr.append_value(st.has_forward_tag);

            if let Some(v) = st.patch_version {
                patch_version_arr.append_value(v);
            } else {
                patch_version_arr.append_null();
            }

            if let Some(ref seq) = st.patchset_sequence_number {
                patchset_sequence_number_arr.append_value(seq);
            } else {
                patchset_sequence_number_arr.append_null();
            }

            untegged_subject_arr.append_value(&st.untegged_subject);

            for tag in &st.subject_tags {
                subject_tags_arr.values().append_value(tag);
            }
            subject_tags_arr.append(!st.subject_tags.is_empty());
        }

        // date
        {
            if let Some(dt) = email.date {
                // TODO : enforce it must be UTC
                date_arr.append_value(dt.timestamp_micros());
            } else {
                date_arr.append_null();
            }
        }

        // client-date
        {
            for client_date in &email.client_date {
                client_date_arr.values().append_value(client_date);
            }
            client_date_arr.append(!email.client_date.is_empty());
        }

        // in_reply_to
        {
            if let Some(irt) = &email.in_reply_to {
                in_reply_to_arr.append_value(irt);
            } else {
                in_reply_to_arr.append_null();
            }
        }
        // references
        {
            for item in &email.references {
                references_arr.values().append_value(item);
            }
            references_arr.append(!email.references.is_empty());
        }

        // x-mailing-list
        {
            if let Some(xml) = &email.x_mailing_list {
                x_mailing_list_arr.append_value(xml);
            } else {
                x_mailing_list_arr.append_null();
            }
        }

        // trailers - struct list
        {
            let struct_builder = trailers_arr.values();
            if email.trailers.is_empty() {
                trailers_arr.append_null();
            } else {
                for attr in &email.trailers {
                    struct_builder
                        .field_builder::<StringBuilder>(0)
                        .unwrap()
                        .append_value(&attr.attribution);
                    struct_builder
                        .field_builder::<StringBuilder>(1)
                        .unwrap()
                        .append_value(&attr.identification);
                    struct_builder.append(true);
                }
                trailers_arr.append(true);
            }
        }

        // code
        {
            for patch in &email.code {
                code_arr.values().append_value(patch.as_str());
            }
            code_arr.append(!email.code.is_empty());
        }

        log::debug!("build_record_batch[{idx}] email_id={source_reference}",);
        raw_body_arr.append_value(email.raw_body.as_str());
        body_sha1_arr.append_value(email.body_sha1.as_str());

        source_reference_arr.append_value(source_reference.as_str());
    }

    log::debug!(
        "build_record_batch: finished building {} columns for {} emails",
        schema.fields().len(),
        emails.len()
    );

    let batch = RecordBatch::try_new(
        Arc::new(schema),
        vec![
            Arc::new(message_id_arr.finish()),
            Arc::new(from_arr.finish()),
            Arc::new(to_arr.finish()),
            Arc::new(cc_arr.finish()),
            Arc::new(subject_arr.finish()),
            Arc::new(has_patch_tag_arr.finish()),
            Arc::new(has_rfc_tag_arr.finish()),
            Arc::new(has_response_tag_arr.finish()),
            Arc::new(has_forward_tag_arr.finish()),
            Arc::new(patch_version_arr.finish()),
            Arc::new(patchset_sequence_number_arr.finish()),
            Arc::new(subject_tags_arr.finish()),
            Arc::new(untegged_subject_arr.finish()),
            Arc::new(date_arr.finish()),
            Arc::new(client_date_arr.finish()),
            Arc::new(in_reply_to_arr.finish()),
            Arc::new(references_arr.finish()),
            Arc::new(x_mailing_list_arr.finish()),
            Arc::new(trailers_arr.finish()),
            Arc::new(code_arr.finish()),
            Arc::new(raw_body_arr.finish()),
            Arc::new(body_sha1_arr.finish()),
            Arc::new(source_reference_arr.finish()),
        ],
    )?;

    Ok(batch)
}
