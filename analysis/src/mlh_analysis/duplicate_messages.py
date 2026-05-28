"""duplicate_messages.py"""

import os
from collections import Counter
from itertools import combinations

import matplotlib.pyplot as plt
import matplotlib.ticker as mticker
import numpy as np
import polars as pl
import seaborn as sns
from matplotlib.colors import LogNorm

sns.set_style("whitegrid")


def main(dataset_dir: str, output_dir: str) -> None:
    df = (
        pl.scan_parquet(f"{dataset_dir}/**/*.parquet", hive_partitioning=True)
        .group_by(["message_id", "body_sha1"])
        .agg(
            pl.col("date").min(),
            pl.col("list").count().alias("number_of_replicas"),
            pl.col("list").alias("lists_present"),
        )
        .with_columns(pl.col("lists_present").list.unique().list.sort())
        .filter(pl.col("number_of_replicas") >= 2)
        .sort(["number_of_replicas", "date"], descending=[True, False])
        .collect()
    )

    # -- Replica distribution -----------------------------------------------
    replica_distribution = (
        df.group_by("number_of_replicas")
        .agg(pl.len().alias("email_count"))
        .with_columns(
            (pl.col("email_count") / pl.col("email_count").sum() * 100)
            .round(2)
            .alias("pct_of_emails")
        )
        .sort("number_of_replicas")
    )

    # -- Save outputs ----------------------------------------------------------
    dataset_out = os.path.join(output_dir, "dataset")
    os.makedirs(dataset_out, exist_ok=True)

    df.write_parquet(os.path.join(dataset_out, "duplicate_messages.parquet"))
    df.with_columns(pl.col("lists_present").list.join(", ")).write_csv(
        os.path.join(output_dir, "duplicate_messages.csv")
    )

    output_path_analysis = os.path.join(output_dir, "replica_distribution.csv")
    replica_distribution.write_csv(output_path_analysis)

    # -- Plots ---------------------------------------------------------------------
    _plot_heatmap_overlap(df, output_dir)


# -- helper -------------------------------------------------------------------

def _fmt_k(x: float, _=None) -> str:
    if x >= 1_000_000:
        return f"{x / 1_000_000:.1f}M"
    if x >= 1_000:
        return f"{x / 1_000:.0f}k"
    return str(int(x))

# -- plots ---------------------------------------------------------------------

def _plot_heatmap_overlap(df: pl.DataFrame, output_dir: str) -> None:
    """Heatmap of duplicate message overlap between lists."""
    HEATMAP_N = 15

    pair_counter: Counter = Counter()
    for row in df.iter_rows(named=True):
        for a, b in combinations(sorted(set(row["lists_present"])), 2):
            pair_counter[(a, b)] += 1

    if not pair_counter:
        return

    list_score: Counter = Counter()
    for (a, b), cnt in pair_counter.items():
        list_score[a] += cnt
        list_score[b] += cnt
    top_names = [name for name, _ in list_score.most_common(HEATMAP_N)]

    n = len(top_names)
    matrix = np.zeros((n, n))
    idx = {name: i for i, name in enumerate(top_names)}
    for (a, b), count in pair_counter.items():
        if a in idx and b in idx:
            matrix[idx[a]][idx[b]] = count
            matrix[idx[b]][idx[a]] = count

    np.fill_diagonal(matrix, np.nan)
    masked = np.ma.masked_invalid(matrix)

    vmin = max(1, np.nanmin(matrix[matrix > 0])) if np.any(matrix > 0) else 1
    vmax = np.nanmax(matrix)
    norm = LogNorm(vmin=vmin, vmax=vmax)

    fig, ax = plt.subplots(figsize=(13, 11))
    im = ax.imshow(masked, cmap="YlOrRd", norm=norm, aspect="auto")
    ax.set_xticks(range(n))
    ax.set_yticks(range(n))
    ax.set_xticklabels(top_names, rotation=40, ha="right", fontsize=9)
    ax.set_yticklabels(top_names, fontsize=9)
    ax.set_facecolor("#F5F5F5")

    for i in range(n):
        for j in range(n):
            v = matrix[i, j]
            if np.isnan(v) or v == 0:
                continue
            ax.text(
                j, i, _fmt_k(v), ha="center", va="center",
                fontsize=8, fontweight="bold",
                color="white" if (v / vmax) > 0.4 else "#333",
            )

    cbar = fig.colorbar(im, ax=ax, fraction=0.03, pad=0.02)
    cbar.set_label("Messages in common (log scale)", fontsize=9)
    cbar.ax.yaxis.set_major_formatter(mticker.FuncFormatter(_fmt_k))
    ax.set_title(
        f"Duplicate Message Overlap Between Lists — Top {HEATMAP_N}\n"
        "(values = messages in common  |  log color scale)",
        fontsize=12,
    )
    fig.tight_layout()
    plt.savefig(os.path.join(output_dir, "duplicate_messages_heatmap_overlap.svg"), bbox_inches="tight")
    plt.close()