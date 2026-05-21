"""MLH Anonymizer - Pseudo-anonymize mailing list datasets.

This package provides tools for anonymizing personally identifiable
information (PII) in mailing list Parquet datasets using SHA-1 hashing.
"""

from mlh_anonymizer.hasher import generate_sha1_hash
from mlh_anonymizer.anonymizer import anonymize_string, anonymize_map
from mlh_anonymizer.dataframe_processor import process_dataframe
from mlh_anonymizer.list_processor import parse_mail_at
from mlh_anonymizer import constants
from mlh_anonymizer import configs

__all__ = [
    "generate_sha1_hash",
    "anonymize_string",
    "anonymize_map",
    "process_dataframe",
    "parse_mail_at",
    "constants",
    "configs",
]
