from mlh_analysis import list_comparison
from mlh_analysis import list_sizes
from mlh_analysis import unique_authors
from mlh_analysis import date_analysis
from mlh_analysis import patch_missing
from mlh_analysis import date_missing
from mlh_analysis.inputs import resolve_inputs

import os


def main():
    input_dirs = os.environ.get("INPUT_DIR", "").split(",")
    output_dir = os.environ.get("OUTPUT_DIR", "results")

    run_validation_scripts = os.environ.get("RUN_VALIDATION_SCRIPTS", False)

    inputs = resolve_inputs(input_dirs)

    print("Starting list_comparison...\n")
    list_comparison.main(inputs["dataset_dir"], output_dir)
    print()

    print("Starting list_sizes...\n")
    list_sizes.main(inputs["dataset_dir"], output_dir)
    print()

    print("Starting unique_authors...\n")
    unique_authors.main(inputs["id_map_dir"], output_dir)
    print()

    print("Starting date_analysis...\n")
    date_analysis.main(inputs["dataset_dir"], output_dir)
    print()

    if not run_validation_scripts:
        return

    print("Starting scripts used for manual validation of the dataset...\n")

    print("Starting missing patches...\n")
    patch_missing.main(inputs["dataset_dir"], output_dir)
    print()

    print("Starting missing dates...\n")
    date_missing.main(inputs["dataset_dir"], output_dir)
    print()


if __name__ == "__main__":
    main()
