import polars as pl
import os
import glob


def main(dataset_dir, output_dir):
    if not dataset_dir:
        print("Expected input dataset missing")
        return

    # pass list names split by ","
    # read all lists by default
    LISTS_OF_INTEREST = os.environ.get("LISTS_OF_INTEREST", "").split(",")
    LISTS_OF_INTEREST = [li for li in LISTS_OF_INTEREST if li]

    if not LISTS_OF_INTEREST:
        raw_dirs = glob.glob(f"{dataset_dir}/list=*")
        LISTS_OF_INTEREST = sorted(
            [os.path.basename(d).removeprefix("list=") for d in raw_dirs]
        )
        print(f"Using all available lists: {LISTS_OF_INTEREST}")

    out_path = f"{output_dir}/patch-missing-date.csv"
    if os.path.exists(out_path):
        os.remove(out_path)
    header_written = False

    for mailing_list in LISTS_OF_INTEREST:
        df = pl.read_parquet(f"{dataset_dir}/list={mailing_list}/*.parquet")
        df = df.filter(
            pl.col("date").is_null()
            & pl.col("client-date").is_not_null()
            & (pl.col("client-date").list.len() > 0)
        )
        df = df.with_columns(pl.lit(mailing_list).alias("list"))
        df = df.with_columns(pl.col("client-date").list.join("||").alias("client-date"))
        df = df.select(["list", "message-id", "subject", "date", "__file_name"])

        if not header_written:
            df.write_csv(out_path, include_header=True)
            header_written = True
        else:
            with open(out_path, "ab") as f:
                df.write_csv(f, include_header=False)

        print(f"  {mailing_list}: {len(df)} matching emails")
