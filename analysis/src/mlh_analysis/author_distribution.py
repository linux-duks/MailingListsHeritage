import polars as pl
import os
import glob


def main(dataset_dir, output_dir):
    # pass list names split by ","
    # read all lists by default
    AUTHOR_EMAIL = os.environ.get("AUTHOR_EMAIL", "")
    if not AUTHOR_EMAIL:
        print("AUTHOR_EMAIL not defined")
        AUTHOR_EMAIL = input("Enter AUTHOR_EMAIL: ")

    LISTS_OF_INTEREST = os.environ.get("LISTS_OF_INTEREST", "").split(",")
    LISTS_OF_INTEREST = [li for li in LISTS_OF_INTEREST if li]

    if not LISTS_OF_INTEREST:
        raw_dirs = glob.glob(f"{dataset_dir}/list=*")
        LISTS_OF_INTEREST = sorted(
            [os.path.basename(d).removeprefix("list=") for d in raw_dirs]
        )
        print(f"Using all available lists: {LISTS_OF_INTEREST}")

    out_path = f"{output_dir}/author_distribution.csv"
    if os.path.exists(out_path):
        os.remove(out_path)
    header_written = False

    for mailing_list in LISTS_OF_INTEREST:
        df = pl.read_parquet(f"{dataset_dir}/list={mailing_list}/*.parquet")
        df = (
            df.filter(pl.col("from").str.contains(AUTHOR_EMAIL))
            .with_columns(pl.col("date").dt.year().alias("year"))
            .group_by("year")
            .agg(pl.len().alias("emails"))
            .with_columns(pl.lit(mailing_list).alias("list"))
            .select(["list", "year", "emails"])
            .sort("year")
        )

        if df.is_empty():
            print(f"  {mailing_list}: no matching emails")
            continue

        if not header_written:
            df.write_csv(out_path, include_header=True)
            header_written = True
        else:
            with open(out_path, "ab") as f:
                df.write_csv(f, include_header=False)

        print(f"  {mailing_list}: {len(df)} years of activity")
