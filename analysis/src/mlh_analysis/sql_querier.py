import os
import gc
import time

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
    result_path = os.path.join(output_dir, "sql_results")
    while True:
        try:
            query = ""
            print('Enter the SQL query terminated by ";" (Ctrl+C to exit):\n▸')
            while True:
                line = input("  ").strip()
                if not line:
                    continue
                query = query + "\n" + line
                if line.endswith(";"):
                    break
            print("$ Sending query ...")
            start = time.time()
            df = ctx.sql(query)
            df.show(num=30)
            end = time.time()
            elapsed_time = end - start
            print(f"! Completed in {elapsed_time:.4f}s. First Lines ⬆️")

            print(f"(Attempting to save results in {result_path})\n")
        except KeyboardInterrupt:
            print("Leaving.")
            break
        except Exception as e:
            print(f"Error: {e}\n")
        if df is not None:
            try:
                df.write_csv(result_path)
                print("Saved results as CSV")
            except Exception as e:
                print(
                    f"Writing CSV failed with an error. Falling back to Parquet. Error: {e}"
                )
                df.write_parquet(result_path)
                print("Saved results as Parquet")
        df = None
        gc.collect()
