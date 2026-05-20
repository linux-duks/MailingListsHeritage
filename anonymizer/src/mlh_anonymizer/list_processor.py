"""Per-list processing for anonymization."""

import itertools
import logging
import os

import polars as pl

from mlh_anonymizer.dataframe_processor import process_dataframe
from mlh_anonymizer.identity_map import IdentityMap
from mlh_anonymizer.constants import SPLIT_DATASET_COLUMNS, IDENTITY_MAP_FILENAME

logger = logging.getLogger(__name__)


def parse_mail_at(mailing_list: str, input_dir_path: str, output_dir_path: str) -> None:
    """Parse and anonymize emails from a single mailing list.

    Creates (or resumes) an :class:`~mlh_anonymizer.identity_map.IdentityMap`
    for the list, passes it through :func:`~mlh_anonymizer.dataframe_processor.process_dataframe`
    so that ``raw_body`` occurrences are counted, then persists the map to
    ``<output_dir>/__main_dataset/<mailing_list>/identity_map.json``.

    The map is only passed to the **main dataset** – split/id-map datasets do
    not contain a ``raw_body`` column, so passing it there would be a no-op
    and misleading.

    Args:
        mailing_list:    Name of the mailing list.
        input_dir_path:  Base input directory path.
        output_dir_path: Base output directory path.
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
        # ── Identity map: load existing or start fresh ────────────────
        identity_map_path = os.path.join(
            output_dir_path, "__main_dataset", mailing_list, IDENTITY_MAP_FILENAME
        )
        identity_map = IdentityMap.load(identity_map_path)
        logger.info(
            f"Loaded identity map for '{mailing_list}' from '{identity_map_path}'"
            if os.path.exists(identity_map_path)
            else f"Starting new identity map for '{mailing_list}'"
        )

        # ── Main dataset (carries raw_body) ───────────────────────────
        process_dataframe(
            read_dataset(),
            "__main_dataset",
            input_path,
            mailing_list,
            output_dir_path,
            identity_map=identity_map,
        )

        # Persist the map immediately after the main dataset is written.
        # Split datasets below do not touch raw_body, so the map is final
        # at this point.
        identity_map.save(identity_map_path)
        logger.info(f"Identity map saved to '{identity_map_path}'")

        # Create split datasets generator for ID mapping
        def dataset_generator() -> tuple[str, pl.DataFrame]:
            """Generate split datasets for columns that need ID mapping."""
            for split_column in SPLIT_DATASET_COLUMNS:
                df = read_dataset()
                if df is not None:
                    yield (
                        f"__id_map_{split_column}",
                        read_dataset().select(
                            pl.col(split_column).alias(f"__original_{split_column}"),
                            pl.col(split_column),
                        ),
                    )

        for dataset_name, df in itertools.chain(dataset_generator()):
            # identity_map intentionally not passed: these datasets contain
            # only already-hashed header columns, no raw_body.
            process_dataframe(
                df, dataset_name, input_path, mailing_list, output_dir_path
            )

    except Exception as e:
        raise e
