import polars as pl
from datetime import datetime, timedelta

import seaborn as sns
import matplotlib.pyplot as plt
import re
import os
import glob

sns.set_style("whitegrid")


def main(dataset_dir, output_dir):

    # pass list names split by ","
    default_lists = "amd-gfx,intel-gfx,linux-iio,rust-for-linux"
    LISTS_OF_INTEREST = os.environ.get("LISTS_OF_INTEREST", default_lists).split(",")
    LISTS_OF_INTEREST = [li for li in LISTS_OF_INTEREST if li]

    if not LISTS_OF_INTEREST:
        raw_dirs = glob.glob(f"{dataset_dir}/list=*")
        LISTS_OF_INTEREST = sorted(
            [os.path.basename(d).removeprefix("list=") for d in raw_dirs]
        )
        print(f"Using all available lists: {LISTS_OF_INTEREST}")

    df = None

    for m_list in LISTS_OF_INTEREST:
        new_list_df = pl.read_parquet(f"{dataset_dir}/list={m_list}/*.parquet")
        new_list_df = new_list_df.with_columns(pl.lit(m_list).alias("list"))
        if df is None:
            df = new_list_df
        else:
            df.vstack(new_list_df)

    df = df.filter(pl.col("date") > datetime(2020, 1, 1))
    df = df.sort("date")

    WINDOW_SIZE = 60
    DATESAMPLINGINTERVAL = 5

    def retrieve_reviewers_and_testers(sorted_df):
        FIRSTCOMMITDATE = sorted_df[0]["date"][0] + timedelta(days=WINDOW_SIZE)
        LASTCOMMITDATE = sorted_df[-1]["date"][0]

        results = {}

        for mail_list in LISTS_OF_INTEREST:
            results[mail_list] = {
                "running_reviewed": 0,
                "running_tested": 0,
                "reviewed_points": [],
                "tested_points": [],
                "any_points": [],
            }

        window_begin = 0
        window_end = -1

        thisDate = FIRSTCOMMITDATE
        all_dates = []
        while thisDate < LASTCOMMITDATE:
            maxDate = thisDate + timedelta(days=1)
            minDate = thisDate + timedelta(days=-WINDOW_SIZE)

            # First, update the datetime window. Starting with the last commit of the window
            while (
                window_end < len(sorted_df) - 1
                and maxDate >= sorted_df[window_end + 1]["date"][0]
            ):
                window_end += 1

                this_email = sorted_df[window_end]

                this_list = this_email["list"][0]
                trailers = this_email["trailers"][0]

                if len(trailers) == 0:
                    continue

                for signature in trailers:
                    attr = signature["attribution"]

                    if re.match(r"reviewed-by", attr, re.IGNORECASE):
                        results[this_list]["running_reviewed"] += 1
                    elif re.match(r"tested-by", attr, re.IGNORECASE):
                        results[this_list]["running_tested"] += 1

            # Update window beginning
            while (
                window_begin < len(sorted_df) - 1
                and minDate > sorted_df[window_begin]["date"][0]
            ):
                window_begin += 1

                this_email = sorted_df[window_begin]
                this_list = this_email["list"][0]
                trailers = this_email["trailers"][0]

                if len(trailers) == 0:
                    continue

                for signature in trailers:
                    attr = signature["attribution"]

                    if re.match(r"reviewed-by", attr, re.IGNORECASE):
                        results[this_list]["running_reviewed"] -= 1
                    elif re.match(r"tested-by", attr, re.IGNORECASE):
                        results[this_list]["running_tested"] -= 1

            for mailing_list in results:
                results[mailing_list]["reviewed_points"].append(
                    results[mailing_list]["running_reviewed"]
                )
                results[mailing_list]["tested_points"].append(
                    results[mailing_list]["running_tested"]
                )
                results[mailing_list]["any_points"].append(
                    results[mailing_list]["running_reviewed"]
                    + results[mailing_list]["running_tested"]
                )
            all_dates.append(thisDate)
            thisDate = thisDate + timedelta(days=DATESAMPLINGINTERVAL)

        for mailing_list in results:
            results[mailing_list] = results[mailing_list]["any_points"]
        return all_dates, results

    dates, list_results = retrieve_reviewers_and_testers(df)

    # Plot each line
    for lista in LISTS_OF_INTEREST:
        plt.plot(dates, list_results[lista], label=lista)

    # Add labels and title
    plt.xlabel("Patch Date")
    plt.ylabel("Contributors")
    plt.title("Auxiliary Contributors")

    # Add a legend to distinguish the lines
    plt.legend()

    # Display the plot
    plt.show
    plt.savefig(f"{output_dir}/auxContribs.svg")
