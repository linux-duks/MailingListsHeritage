//! Column constants for the anonymizer.

/// Maximum number of rows to process per batch / parquet row group.
pub const BATCH_MAX_RECORDS: usize = 50_000;

/// String columns to anonymize with SHA-1 hashing.
pub const ANONYMIZE_STR_COLUMNS: &[&str] = &[
    "from",
    "to",
    "cc",
    "raw_body",
    "from_canonical_addr",
    "trailers_canonical_addr",
];
pub const ANONYMIZE_MAP_COLUMNS: &[(&str, &str)] = &[("trailers", "identification")];

/// Column used to generate the split ID-map dataset.
// TODO: make this a list to add more dataset maps
pub const SPLIT_DATASET_COLUMN: &str = "from";
