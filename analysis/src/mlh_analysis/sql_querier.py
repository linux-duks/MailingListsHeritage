from datafusion import SessionContext

try:
    import readline  # noqa: F401
except:  # noqa: E722
    # will use default pytyhon input othwise
    pass


def main(input_map, output_dir):
    ctx = SessionContext()

    for name, data_path in input_map.items():
        if not data_path:
            continue
        # TODO: detect if there are "hive-style" partitions
        # instead of this hard coded condition
        # data_path/partition=value/...parquet
        if "lineage" not in name:
            ctx.register_parquet(
                name, data_path, table_partition_cols=[("list", "string")]
            )
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

    df.write_csv(output_dir)
