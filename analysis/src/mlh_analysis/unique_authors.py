import os

import polars as pl


def main(working_dir, output_dir):
    if not working_dir:
        print("Error: no input directory available.")
        return

    entries = os.listdir(working_dir)
    if any(e.startswith("list=") for e in entries):
        df = (
            pl.scan_parquet(f"{working_dir}")
            .group_by("from")
            .agg(pl.col("list"))
            .collect()
        )
    else:
        df = pl.scan_parquet(working_dir)
        df = df.group_by(["__original_from", "from"]).agg(pl.col("list")).collect()

    df.write_parquet(f"{output_dir}/unique_linux_authors.parquet")
