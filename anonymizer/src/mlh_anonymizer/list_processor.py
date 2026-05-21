"""Per-list processing for anonymization."""

import itertools
import logging

import polars as pl

from mlh_anonymizer.dataframe_processor import process_dataframe
from mlh_anonymizer.constants import SPLIT_DATASET_COLUMNS, ANONYMIZE_COLUMNS

logger = logging.getLogger(__name__)


def parse_mail_at(mailing_list: str, input_dir_path: str, output_dir_path: str) -> None:
    """Parse and anonymize emails from a single mailing list.

    Args:
        mailing_list: Name of the mailing list
        input_dir_path: Base input directory path
        output_dir_path: Base output directory path
    """
    input_path = f"{input_dir_path}/{mailing_list}"

    def read_dataset() -> pl.DataFrame | None:
        """Read parquet dataset from input path."""
        try:
            df = pl.read_parquet(input_path)
            if df.limit(1).is_empty():
                return None
            return df
        except Exception as e:
            logger.error(f"Failed to read dataset from {input_path} error: {e}")
            return None

    try:
        # Process main dataset first
        process_dataframe(
            read_dataset(),
            "__main_dataset",
            input_path,
            mailing_list,
            output_dir_path,
            ANONYMIZE_COLUMNS,
        )

        # Create split datasets generator for ID mapping
        def dataset_generator() -> tuple[str, pl.DataFrame]:
            """Generate split datasets for columns that need ID mapping."""
            for split_column_type, split_column in SPLIT_DATASET_COLUMNS.items():
                df = read_dataset()
                if df is not None:
                    # returns dataset name, the dataframe, and the map of columns
                    yield (
                        f"__id_map_{split_column}",
                        read_dataset().select(
                            pl.col(split_column).alias(
                                f"__original_{split_column}"
                            ),
                            pl.col(split_column),
                        ),
                        {split_column_type: [split_column]},
                    )

        # Process split datasets
        for dataset_name, df, columns in itertools.chain(dataset_generator()):
            process_dataframe(
                df, dataset_name, input_path, mailing_list, output_dir_path, columns
            )

    except Exception as e:
        raise e
