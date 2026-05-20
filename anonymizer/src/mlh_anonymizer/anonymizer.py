"""Anonymization functions for applying SHA-1 hashing to various data types."""

import logging
from typing import Any, Union

from mlh_anonymizer.hasher import generate_sha1_hash
from mlh_anonymizer.body_anonymizer import anonymize_body
from mlh_anonymizer.identity_map import IdentityMap

logger = logging.getLogger(__name__)


def mlh_anonymizer(col: Any) -> Union[str, list[str]]:
    """Apply SHA-1 anonymization to a column value.

    Handles strings and lists of strings.

    Args:
        col: Value to anonymize (str or list[str])

    Returns:
        Anonymized value (SHA-1 hash or list of hashes)

    Raises:
        Exception: If type is not supported
    """
    if isinstance(col, str):
        return generate_sha1_hash(col)
    if hasattr(col, "__iter__"):
        return [generate_sha1_hash(val) for val in col]
    raise Exception(f"Unmapped type for {type(col)}")


def anonymize_map(col: Any, map_key: str) -> Union[list[dict], dict]:
    """Anonymize a specific key within map/list structures.

    Used for nested structures like trailers.identification.

    Args:
        col: Column value (list[dict] or dict)
        map_key: Key within the dict to anonymize

    Returns:
        Column with specified key anonymized

    Raises:
        Exception: If type is not supported
    """
    if hasattr(col, "__iter__") and not isinstance(col, dict):
        parts = len(col)
        newcol = [{}] * parts
        for part_i in range(parts):
            part = col[part_i]
            # Anonymize the specified key
            part[map_key] = mlh_anonymizer(part[map_key])
            newcol[part_i] = part
        return newcol
    elif isinstance(col, dict):
        newcol = {}
        newcol[map_key] = mlh_anonymizer(col[map_key])
        return newcol
    else:
        raise Exception(f"Unsupported type for anonymize_map: {type(col)}")

def anonymize_identification(col: Any, identity_map: IdentityMap) -> list[dict] | dict:
    """Anonimiza o campo 'identification' dos trailers usando anonymize_body,
    para tratar corretamente o formato 'Name <email>'."""
    if hasattr(col, "__iter__") and not isinstance(col, dict):
        result = []
        for part in col:
            part = dict(part)  # cópia para não mutar o original
            if "identification" in part and part["identification"]:
                part["identification"], _ = anonymize_body(
                    part["identification"], identity_map
                )
            result.append(part)
        return result
    raise Exception(f"Unsupported type for anonymize_identification: {type(col)}")