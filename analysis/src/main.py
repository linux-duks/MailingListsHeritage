from mlh_analysis import list_comparison
from mlh_analysis import list_sizes
from mlh_analysis import unique_authors
from mlh_analysis import date_analysis
from mlh_analysis import patch_missing
from mlh_analysis import author_distribution
from mlh_analysis import date_missing
from mlh_analysis import sql_querier
from mlh_analysis import revisions_analysis
from mlh_analysis import duplicate_messages

import logging
import os

logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")
logger = logging.getLogger(__name__)


def main():
    input_dirs = os.environ.get("INPUT_DIR", "").split(",")
    output_dir = os.environ.get("OUTPUT_DIR", "results")
    analysis_script = os.environ.get("ANALYSIS_SCRIPT", "")

    inputs = resolve_inputs(input_dirs)

    # for analysis that work with multiple inputs
    # this function selects one in order
    def pick(*keys):
        for k in keys:
            if inputs.get(k):
                return inputs[k]
        return None

    scripts = {
        "list_sizes": lambda: list_sizes.main(
            pick("dataset", "anon_dataset"), output_dir
        ),
        "unique_authors": lambda: unique_authors.main(
            pick("id_map", "dataset", "anon_dataset"), output_dir
        ),
        "date_analysis": lambda: date_analysis.main(
            pick("dataset", "anon_dataset"), output_dir
        ),
        "patch_missing": lambda: patch_missing.main(
            pick("dataset", "anon_dataset"), output_dir
        ),
        "date_missing": lambda: date_missing.main(
            pick("dataset", "anon_dataset"), output_dir
        ),
        "revisions_analysis": lambda: revisions_analysis.main(
            pick("dataset", "anon_dataset"), output_dir
        ),
        # these scripts below will not run by default
        "list_comparison": lambda: list_comparison.main(
            pick("dataset", "anon_dataset"), output_dir
        ),
        "duplicate_messages": lambda: duplicate_messages.main(
            pick("dataset", "anon_dataset"), output_dir
        ),
        "author_distribution": lambda: author_distribution.main(
            pick("dataset", "anon_dataset"), output_dir
        ),
        "sql_querier": lambda: sql_querier.main(inputs, output_dir),
    }

    non_default_scripts = [
        "list_comparison",
        "author_distribution",
        "sql_querier",
        "revisions_analysis",
        "duplicate_messages",
    ]

    run_all_scripts = False

    if analysis_script:
        if analysis_script.lower() == "all":
            run_all_scripts = True

        else:
            # run only default scripts
            if analysis_script in scripts.keys():
                logger.info("Starting %s...", analysis_script)
                scripts[analysis_script]()
            else:
                logger.warning("Unknown analysis script: %s", analysis_script)
                logger.warning("Available: %s", ", ".join(scripts.keys()))
            return

    for name in scripts.keys():
        if not run_all_scripts and name in non_default_scripts:
            continue
        logger.info("Starting %s...", name)
        try:
            scripts[name]()
        except Exception:
            logger.exception("Failed to run %s analysis", name)


def resolve_inputs(input_dirs):
    """Resolve dataset, lineage, and id_map directories from a list of input directories.

    Returns a dict with keys 'dataset', 'anon_dataset', 'lineage', and 'id_map'.
    """
    dataset_dir = None
    anon_dataset_dir = None
    lineage_dir = None
    id_map_dir = None

    for d in input_dirs:
        d = d.strip()
        if not d or not os.path.isdir(d):
            continue

        entries = os.listdir(d)

        if "id_map_from" in entries and id_map_dir is None:
            id_map_dir = os.path.join(d, "id_map_from")

        if lineage_dir is None:
            if os.path.isfile(os.path.join(d, "lineage.parquet")):
                lineage_dir = d

        has_list_dirs = any(e.startswith("list=") for e in entries)
        if dataset_dir is None and has_list_dirs:
            dataset_dir = d

        if (
            ("anonymizer" in d or "anonymized" in d)
            and anon_dataset_dir is None
            and "dataset" in entries
        ):
            anon_dataset_dir = os.path.join(d, "dataset")

        # output/parser/dataset
        # if missing, use the anonimyzed in its place
        if "parser" in d and dataset_dir is None and not has_list_dirs:
            if "dataset" in entries:
                dataset_dir = os.path.join(d, "dataset")

    if dataset_dir is None and anon_dataset_dir is None:
        raise FileNotFoundError(
            f"No dataset directory found in: {input_dirs}. "
            "Expected 'list=*/' subdirectories, 'dataset/'"
        )

    return {
        "dataset": dataset_dir or anon_dataset_dir or "",
        "anon_dataset": anon_dataset_dir or "",
        "lineage": lineage_dir or "",
        "id_map": id_map_dir or "",
    }


if __name__ == "__main__":
    main()
