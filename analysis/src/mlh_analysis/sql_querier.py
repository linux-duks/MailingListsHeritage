import os

from datafusion import SessionContext

try:
    import readline  # noqa: F401
except:  # noqa: E722
    # will use default pytyhon input othwise
    pass


def _detect_partition_cols(data_path):
    entries = os.listdir(data_path)
    for e in entries:
        if "=" in e and os.path.isdir(os.path.join(data_path, e)):
            col, _ = e.split("=", 1)
            return [(col, "string")]
    return None


def main(input_map, output_dir):
    if not input_map:
        print("Expected input dataset map")
        return

    ctx = SessionContext()

    for name, data_path in input_map.items():
        if not data_path:
            continue
        partition_cols = _detect_partition_cols(data_path)
        if partition_cols:
            ctx.register_parquet(name, data_path, table_partition_cols=partition_cols)
        else:
            ctx.register_parquet(name, data_path)

    df = ctx.sql("show tables")
    df.show()

    for name in input_map.keys():
        if not input_map[name]:
            continue
        table_name = name.removesuffix("_dir")
        print(f"\nSchema for {table_name}")
        df = ctx.sql(f"show columns from {table_name};")
        df.show()

    df = None
    try:
        query = input("Enter the SQL query:\n ")
        df = ctx.sql(query)
        df.show()
    except Exception as e:
        print(type(e))
        if "datafusion" in str(e):
            print(f"Caught a DataFusion-specific error: {e}")
        else:
            print(f"An unexpected error occurred: {e}")

    if df is not None:
        try:
            df.write_csv(output_dir + "/sql_results/")
        except Exception as e:
            print(
                f"Writing CSV failed with an error. Falling back to Parquet. Error: {e}"
            )
            df.write_parquet(output_dir + "/sql_results/")
