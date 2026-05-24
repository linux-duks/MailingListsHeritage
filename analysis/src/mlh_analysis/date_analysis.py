import polars as pl


def main(dataset_dir, output_dir):
    if not dataset_dir:
        print("Expected input dataset missing")
        return

    lazy_df = pl.scan_parquet(f"{dataset_dir}/")
    email_dates = lazy_df.describe()["date"]
    min_date = email_dates[4]
    quartile_dates = email_dates[5]

    with open(f"{output_dir}/date_stats.txt", "w") as date_file:
        date_file.write(min_date + "\n" + quartile_dates)
