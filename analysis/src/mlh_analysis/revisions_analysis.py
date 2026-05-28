import polars as pl
import seaborn as sns
from datetime import date
import matplotlib.pyplot as plt
import os
import glob

sns.set_style("whitegrid")


def main(working_dir, output_dir):
    if not working_dir:
        print("Error: no input directory available.")
        return

    default_lists = "netdev,bpf,rust-for-linux"
    selected_lists = (os.environ.get("LISTS_OF_INTEREST") or default_lists).split(",")
    selected_lists = [li for li in selected_lists if li]

    if not selected_lists:
        raw_dirs = glob.glob(f"{working_dir}/list=*")
        selected_lists = sorted(
            [os.path.basename(d).removeprefix("list=") for d in raw_dirs]
        )
        print(f"Using all available lists: {selected_lists}")

    # Generate merged DataFrame os lists
    df_array = []
    for m_list in selected_lists:
        new_list_df = pl.read_parquet(f"{working_dir}/list={m_list}/*.parquet")
        new_list_df = new_list_df.with_columns(pl.lit(m_list).alias("list"))
        df_array.append(new_list_df)
    df = pl.concat(df_array)

    # Filter out non-patches
    df = df.filter(
        pl.col("date").is_between(date(2016, 5, 1), date(2026, 5, 1))
        & (pl.col("has_patch_tag") | pl.col("has_rfc_tag"))
        & (~pl.col("has_response_tag"))
        & (~pl.col("has_forward_tag"))
        & (
            pl.col("untagged_subject").is_not_null()
            & (pl.col("untagged_subject") != "")
        )
    )

    # Group by mailing list (plot unit) and untagged_subject (data point unit)
    df = (
        df.group_by(["list", "untagged_subject"])
        .agg(
            pl.col("date").min().alias("min_date"),
            pl.col("date").max().alias("max_date"),
            pl.len().alias("rev_count"),
        )
        .sort("list")
    )

    # Calculate time difference between last and first version
    df = df.with_columns(
        (pl.col("max_date") - pl.col("min_date"))
        .dt.total_days()
        .alias("time_diff_days")
    )

    # Plot

    ## Time difference between first and last versions
    df_time_diff = df.filter(pl.col("time_diff_days") > 0)

    counts_time_diff = df_time_diff.group_by("list").len().sort("list")
    label_map_time_diff = {
        row["list"]: f"{row['list']} (n={row['len']})"
        for row in counts_time_diff.iter_rows(named=True)
    }
    df_time_diff_plot = df_time_diff.with_columns(
        pl.col("list").replace_strict(label_map_time_diff).alias("list_label")
    )

    plt.figure(figsize=(9, (2 * len(selected_lists)) - 1))
    sns.violinplot(
        df_time_diff_plot,
        y="list_label",
        x="time_diff_days",
        log_scale=True,
        inner="quartile",
        hue="list_label",
        # split=True, # good option if not using hue
    )
    plt.ylabel("Mailing List")
    plt.xlabel("Time Difference (Days)")
    plt.title("Time Difference Between First and Last Patch Versions")
    plt.tight_layout()

    if output_dir:
        os.makedirs(output_dir, exist_ok=True)
        plt.savefig(f"{output_dir}/revisions_latencies.svg")

    # BOXEN PLOT
    # ------------------------

    counts_versions = df.group_by("list").len().sort("list")
    label_map_versions = {
        row["list"]: f"{row['list']} (n={row['len']})"
        for row in counts_versions.iter_rows(named=True)
    }
    df_versions_full = df.with_columns(
        pl.col("list").replace_strict(label_map_versions).alias("list_label")
    )

    plt.figure(figsize=(9, len(selected_lists)))
    sns.boxenplot(df_versions_full, y="list_label", x="rev_count", hue="list_label")
    plt.ylabel("Mailing List")
    plt.xlabel("Maximum Patch Version")
    plt.xlim(1, 8)
    plt.title("Distribution of Maximum Patch Versions")
    plt.tight_layout()

    if output_dir:
        os.makedirs(output_dir, exist_ok=True)
        plt.savefig(f"{output_dir}/revisions_versions_boxen.svg")

    # BOX PLOT
    # ------------------------

    ## revcount 95th percentile
    p95 = df["rev_count"].quantile(0.95)
    df_versions = df.filter(pl.col("rev_count") <= p95)

    counts_versions = df_versions.group_by("list").len().sort("list")
    label_map_versions = {
        row["list"]: f"{row['list']} (n={row['len']})"
        for row in counts_versions.iter_rows(named=True)
    }
    df_versions_plot = df_versions.with_columns(
        pl.col("list").replace_strict(label_map_versions).alias("list_label")
    )

    plt.figure(figsize=(9, len(selected_lists) - 1))
    sns.boxplot(df_versions_plot, y="list_label", x="rev_count", hue="list_label")
    plt.ylabel("Mailing List")
    plt.xlabel("Maximum Patch Version")
    plt.xlim(1, 8)
    plt.title("Distribution of Maximum Patch Versions (P95)")
    plt.tight_layout()

    if output_dir:
        os.makedirs(output_dir, exist_ok=True)
        plt.savefig(f"{output_dir}/revisions_versions_box.svg")
