import polars as pl
import os
import glob


def main(working_dir, output_dir):
    if not working_dir:
        print("Error: no input directory available.")
        return

    default_lists = "netdev,bpf,rust-for-linux"
    LISTS_OF_INTEREST = (os.environ.get("LISTS_OF_INTEREST") or default_lists).split(
        ","
    )
    LISTS_OF_INTEREST = [li for li in LISTS_OF_INTEREST if li]

    if not LISTS_OF_INTEREST:
        raw_dirs = glob.glob(f"{working_dir}/list=*")
        LISTS_OF_INTEREST = sorted(
            [os.path.basename(d).removeprefix("list=") for d in raw_dirs]
        )
        print(f"Using all available lists: {LISTS_OF_INTEREST}")

    df = None
    for m_list in LISTS_OF_INTEREST:
        new_df = pl.read_parquet(f"{working_dir}/list={m_list}/*.parquet")
        new_df = new_df.with_columns(pl.lit(m_list).alias("list"))
        df = new_df if df is None else df.vstack(new_df)

    duplicates = (
        df.group_by(["message_id", "body_sha1"])
        .agg(
            [
                pl.min("date").alias("date"),
                pl.count().alias("number_of_replicas"),
                pl.col("list").unique().alias("lists_present"),
            ]
        )
        .filter(pl.col("number_of_replicas") > 1)
        .sort("number_of_replicas", descending=True)
    )

    total = len(duplicates)
    print(f"Found {total} duplicated messages across all lists")

    if total > 0:
        print(duplicates)

    lists_key = (
        LISTS_OF_INTEREST[0]
        if len(LISTS_OF_INTEREST) == 1
        else "_".join([m_list[:4] for m_list in LISTS_OF_INTEREST])
    )
    output_path = os.path.join(output_dir, f"in_list_duplications_{lists_key}.csv")

    duplicates_csv = duplicates.with_columns(
        pl.col("lists_present").list.join(", ").alias("lists_present")
    )
    duplicates_csv.write_csv(output_path)
    print(f"Saved to {output_path}")
