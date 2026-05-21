"""Anonymization functions for applying SHA-1 hashing to various data types."""

import logging
from typing import Any, Union

from mlh_anonymizer.hasher import generate_sha1_hash

logger = logging.getLogger(__name__)


def anonymize_string(row_val: Any) -> Union[str, list[str]]:
    """Apply SHA-1 anonymization to a row value.

    Handles strings and lists of strings.

    Args:
        row_val: Value to anonymize (str or list[str])

    Returns:
        Anonymized value (SHA-1 hash or list of hashes)

    Raises:
        Exception: If type is not supported
    """
    if isinstance(row_val, str):
        return generate_sha1_hash(row_val)
    if hasattr(row_val, "__iter__"):
        return [generate_sha1_hash(val) for val in row_val]
    raise Exception(f"Unmapped type for {type(row_val)}")


def anonymize_map(row_val: Any, map_key: str) -> Union[list[dict], dict]:
    """Anonymize a specific key within map/list structures.

    Used for nested structures like trailers.identification.

    Args:
        row_val: row value (list[dict] or dict)
        map_key: Key within the dict to anonymize

    Returns:
        row value with specified key anonymized

    Raises:
        Exception: If type is not supported
    """
    if hasattr(row_val, "__iter__") and not isinstance(row_val, dict):
        parts = len(row_val)
        newrow_val = [{}] * parts
        for part_i in range(parts):
            part = row_val[part_i]
            # Anonymize the specified key
            part[map_key] = anonymize_string(part[map_key])
            newrow_val[part_i] = part
        return newrow_val
    elif isinstance(row_val, dict):
        newrow_val = {}
        newrow_val[map_key] = anonymize_string(row_val[map_key])
        return newrow_val
    else:
        raise Exception(f"Unsupported type for anonymize_map: {type(row_val)}")
