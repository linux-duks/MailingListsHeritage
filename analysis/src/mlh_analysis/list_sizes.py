import os
import polars as pl
import numpy as np


def main(dataset_dir, output_dir):
    if not dataset_dir:
        print("Expected input dataset missing")
        return

    list_sizes = []
    biggest_list = (-1, "none")
    smallest_list = (999999999999999, "none")

    for subdir_name in os.listdir(dataset_dir):
        list_name = subdir_name.split("=")[1]

        sub_df = pl.scan_parquet(os.path.join(dataset_dir, subdir_name))
        list_size = int(sub_df.describe()["from"][0])

        list_sizes.append(list_size)

        if list_size > biggest_list[0]:
            biggest_list = (list_size, list_name)

        if list_size < smallest_list[0]:
            smallest_list = (list_size, list_name)

    with open(f"{output_dir}/list_sizes.txt", "w") as list_data_file:
        list_data_file.write("Min:" + str(smallest_list))
        list_data_file.write("\nQ1:" + str(np.percentile(list_sizes, 25)))
        list_data_file.write("\nQ2:" + str(np.percentile(list_sizes, 50)))
        list_data_file.write("\nQ3:" + str(np.percentile(list_sizes, 75)))
        list_data_file.write("\nMax:" + str(biggest_list))
