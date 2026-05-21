"""Constant definitions for the anonymizer.

This module contains column definitions and other constants
that define the anonymization schema.
"""

# Columns to anonymize with direct SHA-1 hashing
ANONYMIZE_COLUMNS = {
    # columns with string structure
    "str": [
        "from",
        "to",
        "cc",
    ],
    # Columns with nested structures to anonymize
    # Format: "parent.child" where child is the key to anonymize
    "map": [
        "trailers.identification",
    ],
}

# Generate a sub-dataset with a mapping of values for these columns
# for now, only one column per sub-dataset is supported
SPLIT_DATASET_COLUMNS = {"str": "from"}
