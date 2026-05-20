"""Constant definitions for the anonymizer.

This module contains column definitions and other constants
that define the anonymization schema.
"""

# Default maximum number of processes (used as a ceiling by configs.py)
N_PROC_DEFAULT_MAX = 8

# Columns to anonymize with direct SHA-1 hashing
ANONYMIZE_COLUMNS = [
    "from",
    "to",
    "cc",
]

# Generate a sub-dataset with a mapping of values for these columns
SPLIT_DATASET_COLUMNS = ["from"]

# Columns with nested structures to anonymize (dot notation)
# Format: "parent.child" where child is the key to anonymize
ANONYMIZE_MAP = [
    "trailers.identification",
]

# Column whose free-form text content is scanned for emails (issue #17)
RAW_BODY_COLUMN = "raw_body"

# Filename written inside each mailing-list output directory.
# Stores the per-(email, name) occurrence counts split by location.
IDENTITY_MAP_FILENAME = "identity_map.json"

# Campos de string livre que contêm emails embutidos
FREE_TEXT_COLUMNS = [
    "message-id",
    "in-reply-to",
]