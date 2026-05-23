import os
import polars as pl


def main(dataset_dir, output_dir):
    df = pl.scan_parquet(f"{dataset_dir}/**/*.parquet", hive_partitioning=True)

    df = (
        df.rename({"list": "x_mailing_list"})
        .group_by(["message-id", "raw_body"])
        .agg(
            pl.col("date").min().alias("date"),
            pl.col("x_mailing_list").count().alias("number_of_replicas"),
            pl.col("x_mailing_list").alias("lists_present"),
        )
        .with_columns(pl.col("lists_present").list.unique().list.sort())
        .filter(pl.col("number_of_replicas") >= 2)
        .rename({"message-id": "message_id"})
        .sort(["number_of_replicas", "date"], descending=[True, False])
        .collect()
    )

    dataset_out = os.path.join(output_dir, "dataset")
    os.makedirs(dataset_out, exist_ok=True)

    df.write_parquet(os.path.join(dataset_out, "duplicate_messages.parquet"))
    df.with_columns(pl.col("lists_present").list.join(", ")).write_csv(
        os.path.join(output_dir, "duplicate_messages.csv")
    )