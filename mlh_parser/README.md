# MLH Parser

A Rust tool for parsing raw email archives from the MLH Archiver into a structured Parquet columnar dataset.

## Overview

The MLH Parser processes email files produced by the MLH Archiver and converts them into an efficient, queryable Parquet dataset with Hive partitioning by mailing list name. It automatically detects and reads both `.eml` (RFC 822) and `.parquet` (columnar) input formats.

![Parser Diagram](/docs/parser.avif)

## Features

- **Parquet Output**: Columnar storage format optimized for analytics via Apache Arrow
- **Hive Partitioning**: Data organized by mailing list (`list=<name>/`) for efficient querying
- **Auto-detection of Input Format**: Reads `.eml` and `.parquet` files transparently from the same input directory
- **Email Field Extraction**: Parses headers, body, attachments, and metadata (including trailers, patches, and code snippets)
- **Error Handling**: Failed parses are saved separately per mailing list for review
- **Multi-threaded**: Configurable thread pool for parallel processing of mailing lists
- **Batch Processing**: Auto-splits large datasets into multiple Parquet row groups to stay within Arrow's 2 GB offset limits

## Prerequisites

- Rust toolchain (cargo, rustc) 1.80+

### Container Build (Alternative)

If you don't have Rust installed, the parser can be built in a container:

- Podman or Docker

## Installation

### Using Devbox (Recommended)

```bash
devbox shell
devbox run build
```

### Manual Build

```bash
cargo build --release
```

The compiled binary will be at `target/release/mlh_parser`.

## Usage

### Running the Parser

The parser reads all settings from a YAML (or JSON/TOML) configuration file.
Create your config from the example:

```bash
cp example_parser_config.yaml parser_config.yaml
```

Then run:

```bash
# Uses default glob pattern: parser_config* (matches parser_config.yaml)
./target/release/mlh_parser

# With a specific config file
./target/release/mlh_parser -c my_config.yaml
./target/release/mlh_parser --config-file my_config.yaml

# With debug logging
RUST_LOG=debug ./target/release/mlh_parser
```

All configuration is in the config file — no environment variables are used
for parser settings (only `RUST_LOG` controls log verbosity).

### Input/Output Directories

Defaults as set in `example_parser_config.yaml`:

| Directory | Purpose |
|-----------|---------|
| `./output/archiver/` | Input: Root directory with mailing list subdirectories from archiver |
| `./output/parser/dataset/` | Output: Parquet dataset (Hive partitioned by list) |
| `./output/parser/errors/` | Failed parses per mailing list |
| `./output/parser/lineage/` | Audit trail (reserved) |

All paths can be changed via the `input_dir_path` and `output_dir_path` options
in `parser_config.yaml`.

### Input Formats

The parser automatically detects and processes both input formats within each mailing list directory:

| Format | Extension | Description |
|--------|-----------|-------------|
| Raw email | `.eml` | Individual RFC 822 email files (one file per email) |
| Columnar | `.parquet` | Parquet files containing multiple emails in columnar form |

#### Parquet Input

When reading `.parquet` files, each file must contain the following columns:

| Column | Type | Description |
|--------|------|-------------|
| `email_id` | string | Unique identifier for each email |
| `content` | string / list\<string\> | Full raw email content |

Each row in the parquet file is yielded as an individual email for parsing, with the composite name `{email_id}:{parquet_filename}` used for provenance tracking.

Both `.eml` and `.parquet` files can coexist in the same input directory — the parser automatically dispatches to the correct reader based on file extension.

## Output Format

The output is written to `<output_dir_path>/dataset/list=<mailing_list>/list_data.parquet`.
Each mailing list gets its own Hive-partitioned directory with a single Parquet
file containing all parsed emails (split into multiple row groups as needed).

### Schema

The Parquet dataset includes the following columns:

| Column | Type | Description |
|--------|------|-------------|
| `message-id` | string | Email Message-ID header |
| `from` | string | Sender email address |
| `to` | list\<string\> | Recipients (To field) |
| `cc` | list\<string\> | CC recipients |
| `subject` | string | Email subject line |
| `date` | datetime | dataset email date (corrected) |
| `client-date` | list\<string\> | Raw date from email client (may be incorrect) |
| `in-reply-to` | string | In-Reply-To header |
| `references` | list\<string\> | References headers |
| `x-mailing-list` | string | Mailing list name |
| `trailers` | list\<struct\<attribution: string, identification: string\>\> | Signature block attribution and identification |
| `code` | list\<string\> | Code snippets extracted from email |
| `raw_body` | string | Complete raw email body |

## Configuration

All operational settings are in a YAML (or JSON/TOML) configuration file matched
by the glob pattern `parser_config*`. No environment variables are used for
parser settings — only `RUST_LOG` controls log verbosity.

### Configuration Options

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `nthreads` | integer | **Yes** | — | Number of worker threads. `<= 1` runs sequentially; `2+` spawns a thread pool |
| `input_dir_path` | string | **Yes** | — | Root directory containing mailing list subdirectories from the archiver |
| `output_dir_path` | string | **Yes** | — | Root directory for parsed output (dataset, errors, lineage) |
| `fail_on_parsing_error` | boolean | **Yes** | — | If `true`, abort on first parse error. If `false`, log the error and continue |
| `lists_to_parse` | list of strings | No | `null` (all subdirectories) | Specific mailing list directories to parse. Omit or leave empty to parse all |

### Example Configuration

```yaml
# parser_config.yaml
nthreads: 4
input_dir_path: "./output/archiver/"
output_dir_path: "./output/parser/"
fail_on_parsing_error: false
# lists_to_parse applies to all input subdirectories:
# lists_to_parse:
#   - dev.example.me.lists.gfs2
#   - dev.example.me.lists.iommu
```

### Logging

Set `RUST_LOG` to control log verbosity (standard `env_logger` levels):

```bash
RUST_LOG=debug mlh_parser    # Verbose: shows per-email parsing detail
RUST_LOG=info mlh_parser     # Normal (default)
RUST_LOG=error mlh_parser    # Errors only
```

### Internal Constants

The following are hardcoded in the source and not configurable:

| Constant | Value | Description |
|----------|-------|-------------|
| `BATCH_MAX_RECORDS` | 50,000 | Max emails per Parquet row group before flushing |
| `BATCH_MAX_RAW_BYTES` | 400 MB | Max cumulative raw body bytes per row group before flushing |
| `PARQUET_FILE_NAME` | `list_data.parquet` | Output filename inside each list's partition directory |


## Development

### Running Tests

```bash
cargo test
```

### Debug Mode

Run the parser with verbose logging:

```bash
RUST_LOG=debug cargo run
```

### Project Structure

- `src/main.rs` — Entry point and Ctrl+C signal handling
- `src/lib.rs` — Core `start()` function, thread pool, batch flush orchestration
- `src/email_parser.rs` — Top-level email parsing: headers, body, dates, trailers, patches
- `src/email_reader.rs` — Low-level mail-parser integration: decode, headers, body text
- `src/extractors.rs` — Trailer and patch extraction from email body text
- `src/date_parser.rs` — Date parsing, validation, and millennium correction
- `src/dataset_writer.rs` — Parquet output with batched row group writes
- `src/email_file_reader.rs` — `.eml` and `.parquet` file reader with unified iterator
- `src/entities.rs` — `ParsedEmail` and `Attribution` data types
- `src/constants.rs` — Parquet schema, batch limits, column definitions
- `src/config.rs` — CLI argument parsing (`clap`) and config file loading (`config` crate)
- `src/errors.rs` — `ConfigError` and `ParseError` error types
- `tests/` — Integration test suite with real email fixtures

## Dependencies

- [`arrow` + `parquet`](https://docs.rs/arrow/) — Apache Arrow columnar storage backend
- [`mail-parser`](https://docs.rs/mail-parser/) — RFC 822 / MIME email parsing
- [`config`](https://docs.rs/config/) — YAML/JSON/TOML configuration file loading
- [`clap`](https://docs.rs/clap/) — CLI argument parsing
- [`env_logger`](https://docs.rs/env_logger/) — Log level control via `RUST_LOG`
- [`chrono`](https://docs.rs/chrono/) — Date/time parsing and formatting
- [`glob`](https://docs.rs/glob/) — Config file glob pattern matching
- [`ctrlc`](https://docs.rs/ctrlc/) — Graceful Ctrl+C shutdown

### Development Dependencies

- [`tempfile`](https://docs.rs/tempfile/) — Temporary directories for integration tests

## Container Build

If you don't have Rust installed, build using Podman or Docker:

```bash
make build CONTAINER=podman
# or
make build CONTAINER=docker
```

The build uses the `docker.io/rust:1.94-slim` image and mounts the workspace
read/write for incremental compilation caching.

## Error Handling

Emails that fail to parse are logged to stderr. Previous errors are cleaned
automatically on re-run. When `fail_on_parsing_error` is `false` (default),
processing continues past errors; when `true`, the process stops on the first
failure.

Failed parses are tracked per mailing list under:

```
<output_dir_path>/errors/list=<mailing_list>/
```

## Integration with Other Components

1. Run archiver to collect raw emails: `make run`
2. Configure and run parser: copy `example_parser_config.yaml` → `parser_config.yaml`, then `./target/release/mlh_parser`
3. Run anonymizer for privacy: `make anonymize`

### Example Usage with Polars

```python
import polars as pl

# Read the dataset dataset
df = pl.scan_parquet("../output/parser/dataset/**/*.parquet")

# Query emails by subject
result = (
    df
    .filter(pl.col("subject").str.contains("example"))
    .select(["date", "from", "subject"])
    .collect()
)
```

### Example Usage with Rust Polars

```Rust

use polars::prelude::*;

fn main() -> PolarsResult<()> {
    // Read the dataset dataset
    let mut args = ScanArgsParquet::default();
    let df = LazyFrame::scan_parquet("../output/parser/dataset/**/*.parquet", args)?
        .filter(col("subject").str().contains(lit("example"), true))
        .select([col("date"), col("from"), col("subject")])
        .collect()?;

    println!("{:?}", df);
    Ok(())
}
```

## Troubleshooting

### "No items found to parse"

The input directory has no subdirectories (mailing lists). Run the archiver
first (`make run`), or check that `input_dir_path` in your config points to
the correct archiver output.

### Config not loaded

The parser looks for files matching `parser_config*` (glob). Verify your file
matches the pattern, or pass an explicit path with `-c`:

```bash
mlh_parser -c my_parser_config.yaml
```

### Parsing errors

Check the error log output (stderr). Common issues:

- Malformed email headers
- Unsupported character encodings
- Corrupted email files

## License

See the root [LICENSE](../LICENSE) file.
