import os


def resolve_inputs(input_dirs):
    """Resolve dataset, lineage, and id_map directories from a list of input directories.

    Returns a dict with keys 'dataset', 'lineage', and 'id_map'.
    """
    dataset_dir = None
    lineage_dir = None
    id_map_dir = None

    for d in input_dirs:
        d = d.strip()
        if not d or not os.path.isdir(d):
            continue

        entries = os.listdir(d)

        if "__id_map_from" in entries and id_map_dir is None:
            id_map_dir = os.path.join(d, "__id_map_from")

        if lineage_dir is None:
            if os.path.isfile(os.path.join(d, "lineage.parquet")):
                lineage_dir = d

        if dataset_dir is None:
            has_list_dirs = any(e.startswith("list=") for e in entries)
            if has_list_dirs:
                dataset_dir = d
            elif "__main_dataset" in entries:
                dataset_dir = os.path.join(d, "__main_dataset")
            elif "dataset" in entries:
                dataset_dir = os.path.join(d, "dataset")

    if dataset_dir is None:
        raise FileNotFoundError(
            f"No dataset directory found in: {input_dirs}. "
            "Expected 'list=*/' subdirectories, '__main_dataset/', or 'dataset/'."
        )

    return {
        "dataset": dataset_dir,
        "lineage": lineage_dir or "",
        "id_map": id_map_dir or "",
    }
