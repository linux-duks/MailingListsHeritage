"""DataFrame processing for anonymization."""

import os
import logging

import polars as pl

from mlh_anonymizer.anonymizer import anonymize_string, anonymize_map
from mlh_anonymizer.configs import COMPRESSION_LEVEL

logger = logging.getLogger(__name__)


def process_dataframe(
    df: pl.DataFrame,
    dataset_name: str,
    input_path: str,
    mailing_list: str,
    output_dir_path: str,
    columns_to_anomimyze: dict[str : list[str]],
) -> None:
    """Process a DataFrame by anonymizing configured columns and writing to parquet.

    Args:
        df: Polars DataFrame to process
        dataset_name: Name of the dataset (e.g., "__main_dataset", "__id_map_from")
        input_path: Input directory path for logging
        mailing_list: Mailing list name
        output_dir_path: Base output directory path

    Returns:
        None
    """
    if df is None:
        logger.warning(f"Dataset '{dataset_name}'.'{input_path}' did not produce data")
        return

    df_columns = df.collect_schema().names()

    # Anonymize string columns
    string_columns = columns_to_anomimyze.get("str", [])
    for col in string_columns:
        if col not in df_columns:
            logger.warning(f"Column {col} not available in dataset {dataset_name}")
            continue
        logger.info(f"Running '{col}'.'{dataset_name}'.'{input_path}'")
        df = df.with_columns(
            pl.col(col)
            .map_elements(lambda x: anonymize_string(x), return_dtype=pl.self_dtype())
            .alias(col),
        )

    # Anonymize mapped columns (nested structures)
    map_columns = columns_to_anomimyze.get("map", [])
    for col in map_columns:
        col_parts = col.split(".")
        if col_parts[0] not in df_columns:
            logger.warning(f"Column {col} not available in dataset {dataset_name}")
            continue
        logger.info(f"Running '{col}'.'{dataset_name}'.'{input_path}'")
        logger.info(
            f"Running map {col}. Will write '{col_parts[0]}' with '{col_parts[1]}' anonymized"
        )
        df = df.with_columns(
            pl.col(col_parts[0])
            .map_elements(
                lambda x: anonymize_map(x, col_parts[1]),
                return_dtype=pl.self_dtype(),
            )
            .alias(col_parts[0]),
        )

    output_path = f"{output_dir_path}/{dataset_name}/{mailing_list}"
    logger.info(f"Writing {output_path}")

    os.makedirs(output_path, exist_ok=True)
    df.write_parquet(
        output_path + "/data.parquet",
        compression="zstd",
        row_group_size=1024**2,  # double the default
        data_page_size=(1024 * 2) ** 2,
        compression_level=COMPRESSION_LEVEL,
    )
