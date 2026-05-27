//! Parquet schema definition, batch limits, and column constants.

use std::sync::{Arc, LazyLock};

use arrow::datatypes::{DataType, Field, Fields, Schema, TimeUnit};

/// Maximum number of emails to accumulate before flushing to a Parquet row group.
///
/// Batch limits keep row-group size well under Arrow's `i32` string-offset
/// ceiling (~2.1 GB). These values can be overridden in tests without needing
/// multi-gigabyte test fixtures.
pub const BATCH_MAX_RECORDS: usize = 50_000;

/// Oldest date allowed in the "date filter".
/// This is not a config option, but can be changed easily.
/// I originally set this to 1970, because
/// the first email in history was sent in 1971
///   <https://en.wikipedia.org/wiki/History_of1_email>
/// However, I found many emails where the date is
/// in 1970 because of unixtime 0
///
/// I couldnt find a reliable source for the first emails archived,
///  of first mailing list. One good candidate is 1986 for the LISTSERV
///  <https://en.wikipedia.org/wiki/LISTSERV>
///  and possibly the "MsgGroup" in 1975, for the ARPANET
///  <https://www.cs.kent.edu/~javed/internetbook/hobbestimeline/HIT.html>
///  <https://www.britannica.com/technology/Internet>
///  I chose 1986 as the default cutoff
pub const OLDEST_MAIL_DATE_CUTOFF: usize = 1986;

/// The fixed Arrow schema used for all Parquet output.
///
/// Column order: `from, to, cc, subject, date, client-date, message-id,
/// in-reply-to, references, x-mailing-list, trailers, code, raw_body, _source_reference`.
pub static PARQUET_SCHEMA: LazyLock<Schema> = LazyLock::new(|| {
    let trailer_fields = Fields::from(vec![
        Field::new("attribution", DataType::Utf8, false),
        Field::new("identification", DataType::Utf8, false),
    ]);

    Schema::new(vec![
        Field::new("message_id", DataType::Utf8, true),
        Field::new("from", DataType::Utf8, true),
        Field::new(
            "to",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            true,
        ),
        Field::new(
            "cc",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            true,
        ),
        Field::new("subject", DataType::Utf8, true),
        Field::new("has_patch_tag", DataType::Boolean, true),
        Field::new("has_rfc_tag", DataType::Boolean, true),
        Field::new("has_response_tag", DataType::Boolean, true),
        Field::new("has_forward_tag", DataType::Boolean, true),
        Field::new("patch_version", DataType::UInt16, true),
        Field::new("patchset_sequence_number", DataType::Utf8, true),
        Field::new(
            "subject_tags",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            true,
        ),
        Field::new("untagged_subject", DataType::Utf8, true),
        Field::new(
            "date",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
        Field::new(
            "client_date",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            true,
        ),
        Field::new("in_reply_to", DataType::Utf8, true),
        Field::new(
            "references",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            true,
        ),
        Field::new("x_mailing_list", DataType::Utf8, true),
        Field::new(
            "trailers",
            DataType::List(Arc::new(Field::new(
                "item",
                DataType::Struct(trailer_fields),
                true,
            ))),
            true,
        ),
        Field::new(
            "code",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            true,
        ),
        Field::new("raw_body", DataType::Utf8, true),
        Field::new("body_sha1", DataType::Utf8, true),
        Field::new("_source_reference", DataType::Utf8, true),
    ])
});

/// Output Parquet filename inside each list's Hive partition directory.
pub const PARQUET_FILE_NAME: &str = "list_data.parquet";
