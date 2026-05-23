from mlh_analysis import list_comparison
from mlh_analysis import list_sizes
from mlh_analysis import unique_authors
from mlh_analysis import date_analysis
from mlh_analysis import patch_missing
from mlh_analysis import author_distribution
from mlh_analysis import date_missing
from mlh_analysis import sql_querier
from mlh_analysis import duplicate_messages
from mlh_analysis.inputs import resolve_inputs

import os


def main():
    input_dirs = os.environ.get("INPUT_DIR", "").split(",")
    output_dir = os.environ.get("OUTPUT_DIR", "results")
    analysis_script = os.environ.get("ANALYSIS_SCRIPT", "")

    inputs = resolve_inputs(input_dirs)

    scripts = {
        "list_comparison": lambda: list_comparison.main(
            inputs["dataset"], output_dir
        ),
        "list_sizes": lambda: list_sizes.main(inputs["dataset"], output_dir),
        "unique_authors": lambda: unique_authors.main(inputs["id_map"], output_dir),
        "date_analysis": lambda: date_analysis.main(inputs["dataset"], output_dir),
        "patch_missing": lambda: patch_missing.main(inputs["dataset"], output_dir),
        "date_missing": lambda: date_missing.main(inputs["dataset"], output_dir),
        # these scripts below will not run by default
        "author_distribution": lambda: author_distribution.main(
            inputs["dataset"], output_dir
        ),
        "duplicate_messages": lambda: duplicate_messages.main(
            inputs["dataset"], output_dir
        ),
        "sql_querier": lambda: sql_querier.main(
            inputs, output_dir
        ),
    }

    non_default_scripts = ["author_distribution", "sql_querier"]

    if analysis_script:
        if analysis_script in scripts.keys():
            print(f"Starting {analysis_script}...\n")
            scripts[analysis_script]()
            print()
        else:
            print(f"Unknown analysis script: {analysis_script}")
            print(f"Available: {', '.join(scripts.keys())}")
        # if specific script selected, return early
        return

    for name in scripts.keys():
        if name in non_default_scripts:
            continue
        print(f"Starting {name}...\n")
        scripts[name]()
        print()


if __name__ == "__main__":
    main()
