"""DataFrame processing for anonymization."""

import os
import logging
from typing import TYPE_CHECKING

import polars as pl

from mlh_anonymizer.anonymizer import mlh_anonymizer
from mlh_anonymizer.body_anonymizer import anonymize_body
from mlh_anonymizer.constants import ANONYMIZE_COLUMNS, RAW_BODY_COLUMN

if TYPE_CHECKING:
    from mlh_anonymizer.identity_map import IdentityMap

logger = logging.getLogger(__name__)


def process_dataframe(
    df: pl.DataFrame,
    dataset_name: str,
    input_path: str,
    mailing_list: str,
    output_dir_path: str,
    identity_map: "IdentityMap | None" = None,
) -> None:
    """Process a DataFrame by anonymizing configured columns and writing to parquet.

    Anonymization order:
    1. Standard columns (``from``, ``to``, ``cc``) – SHA-1 via :func:`mlh_anonymizer`.
    2. ``message-id`` and ``in-reply-to`` – free-text strings that may contain
       an email address embedded in them (e.g. ``154267-4-user@host``).
       Processed via :func:`anonymize_body` so only the email portion is replaced.
    3. ``references`` – same as above but stored as a List(String); each element
       is processed individually.
    4. ``trailers`` – List of structs with an ``identification`` field that contains
       ``"Name <email>"`` strings.  Processed via :func:`anonymize_body` so both
       the name and the email are replaced with hashes.
    5. ``raw_body`` – full free-text body, processed via :func:`anonymize_body`.

    Args:
        df:             Polars DataFrame to process.
        dataset_name:   Name of the dataset (e.g. ``"__main_dataset"``).
        input_path:     Input directory path (used in log messages only).
        mailing_list:   Mailing list name.
        output_dir_path: Base output directory path.
        identity_map:   Shared :class:`~mlh_anonymizer.identity_map.IdentityMap`
                        instance.  When provided, body-style columns are processed
                        and ``body_count`` is incremented for every email found.
                        Pass ``None`` to skip (e.g. for split/id-map datasets).

    Returns:
        None
    """
    if df is None:
        logger.warning(f"Dataset '{dataset_name}'.'{input_path}' did not produce data")
        return

    df_columns = df.collect_schema().names()

    # ── Step 1: standard columns (from, to, cc) ──────────────────────────────
    for col in ANONYMIZE_COLUMNS:
        if col not in df_columns:
            logger.warning(f"Column {col} not available in dataset {dataset_name}")
            continue
        logger.info(f"Running '{col}'.'{dataset_name}'.'{input_path}'")
        df = df.with_columns(
            pl.col(col)
            .map_elements(lambda x: mlh_anonymizer(x), return_dtype=pl.self_dtype())
            .alias(col),
        )

    if identity_map is not None:
        # ── Step 2: message-id and in-reply-to (String columns) ──────────────
        for col in ("message-id", "in-reply-to"):
            if col not in df_columns:
                continue
            logger.info(f"Running '{col}'.'{dataset_name}'.'{input_path}'")
            df = df.with_columns(
                pl.col(col)
                .map_elements(
                    lambda x: anonymize_body(x, identity_map)[0] if x else x,
                    return_dtype=pl.String,
                )
                .alias(col),
            )

        # ── Step 3: references (List(String) column) ──────────────────────────
        # Each element of the list is a message-id string that may embed an email.
        # We call anonymize_body on every element individually and return the
        # anonymized list.
        if "references" in df_columns:
            logger.info(f"Running 'references'.'{dataset_name}'.'{input_path}'")

            def _anonymize_references(lst):
                if lst is None:
                    return None
                # lst is a polars Series when coming from a List column cell;
                # convert to plain Python list first, process, return as list.
                return [
                    anonymize_body(item, identity_map)[0] if item else item
                    for item in lst
                ]

            df = df.with_columns(
                pl.col("references")
                .map_elements(_anonymize_references, return_dtype=pl.List(pl.String))
                .alias("references"),
            )

        # ── Step 4: trailers (List(Struct) column) ────────────────────────────
        # Each struct has shape {"attribution": str, "identification": str}.
        # The "identification" field contains "Name <email>" free text – exactly
        # the format anonymize_body already handles.  We leave "attribution"
        # (e.g. "Signed-off-by") untouched since it carries no personal data.
        if "trailers" in df_columns:
            logger.info(f"Running 'trailers'.'{dataset_name}'.'{input_path}'")

            def _anonymize_trailers(trailer_list):
                if trailer_list is None:
                    return None
                result = []
                for trailer in trailer_list:
                    # trailer is a dict-like struct: {"attribution": ..., "identification": ...}
                    identification = trailer.get("identification") if isinstance(trailer, dict) else None
                    if identification:
                        anonymized, _ = anonymize_body(identification, identity_map)
                    else:
                        anonymized = identification
                    result.append({
                        "attribution": trailer["attribution"] if isinstance(trailer, dict) else trailer[0],
                        "identification": anonymized,
                    })
                return result

            df = df.with_columns(
                pl.col("trailers")
                .map_elements(
                    _anonymize_trailers,
                    return_dtype=pl.List(
                        pl.Struct({"attribution": pl.String, "identification": pl.String})
                    ),
                )
                .alias("trailers"),
            )

        # ── Step 5: raw_body ──────────────────────────────────────────────────
        if RAW_BODY_COLUMN in df_columns:
            logger.info(f"Running '{RAW_BODY_COLUMN}'.'{dataset_name}'.'{input_path}'")
            df = _anonymize_raw_body_column(df, identity_map)

    output_path = f"{output_dir_path}/{dataset_name}/{mailing_list}"
    logger.info(f"Writing {output_path}")

    os.makedirs(output_path, exist_ok=True)
    df.write_parquet(
        output_path + "/data.parquet",
        compression="zstd",
        row_group_size=1024**2,  # double the default
        data_page_size=(1024 * 2) ** 2,
        compression_level=22,  # maximum compression for Zenodo
    )


def _anonymize_raw_body_column(
    df: pl.DataFrame,
    identity_map: "IdentityMap",
) -> pl.DataFrame:
    """Replace email addresses in the ``raw_body`` column with SHA-1 hash tokens.

    Uses :func:`~mlh_anonymizer.body_anonymizer.anonymize_body` row-by-row.
    The *identity_map* is updated in-place as a side effect; it is
    **not** persisted here — that is the caller's responsibility.

    Null values in ``raw_body`` are passed through unchanged.

    Args:
        df:           DataFrame that contains a ``raw_body`` column.
        identity_map: Shared identity map; mutated in-place.

    Returns:
        DataFrame with ``raw_body`` anonymized.
    """

    def _process_cell(raw: str | None) -> str | None:
        if raw is None:
            return None
        anonymized, _ = anonymize_body(raw, identity_map)
        return anonymized

    return df.with_columns(
        pl.col(RAW_BODY_COLUMN)
        .map_elements(_process_cell, return_dtype=pl.String)
        .alias(RAW_BODY_COLUMN),
    )