/// A trailer line extracted from an email signature block.
///
/// Common examples: `Signed-off-by`, `Reviewed-by`, `Tested-by`, `Acked-by`.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribution {
    /// The trailer tag (e.g. `Signed-off-by`, `Reviewed-by`)
    pub attribution: String,
    /// The person identifiation in `Name <email>` form
    pub identification: String,
}

/// A fully parsed email with headers, body, trailers, and code patches.
///
/// Produced by [`parse_email`](crate::email_parser::parse_email) and consumed
/// by [`build_record_batch`](crate::dataset_writer::build_record_batch).
#[derive(Debug, Clone, Default)]
pub struct ParsedEmail {
    /// RFC 822 headers plus computed fields (`date`, `client-date`, etc.).
    /// Multi-value headers (to, cc, references) are delimited with `||`.
    pub message_id: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    /// Tags read from the Subject message
    pub subject_tags: SubjectTags,
    /// date, according to our best attempts at correcting it
    pub date: Option<chrono::DateTime<chrono::Utc>>,
    /// the strings in the "Date" header, with incorrect data
    pub client_date: Vec<String>,
    /// reference to the previous message-id in case this is a reply
    pub in_reply_to: Option<String>,
    /// reference to other emssage-ids. Usually in threads or patchsets
    pub references: Vec<String>,
    /// list-id header. Can be different than the "list" a message was read from.
    pub x_mailing_list: Option<String>,
    /// Trailers extracted from the signature block.
    pub trailers: Vec<Attribution>,
    /// Code patches extracted from the email body.
    pub code: Vec<String>,
    /// Full email body text, CRLF-normalized to LF.
    pub raw_body: String,
    /// pre-calculated SHA1 sum of the raw body
    pub body_sha1: String,
    /// source reference is a information to trace back to the original source
    /// it will be different for each kind of email source.
    pub source_reference: String,
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct SubjectTags {
    /// `[PATCH]`
    pub has_patch_tag: bool,
    /// `[RFC]`
    pub has_rfc_tag: bool,
    /// `Re:`
    pub has_response_tag: bool,
    /// `Fwd:`
    pub has_forward_tag: bool,
    /// `[v3]`
    pub patch_version: Option<u16>,
    /// `[0/3]`
    pub patchset_sequence_number: Option<String>,
    /// list of tags considered, in order as they appear
    pub subject_tags: Vec<String>,
    /// the subject message, cut after the last tag
    pub untegged_subject: String,
}
