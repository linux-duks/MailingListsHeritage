# MLH Analysis

Example analyses and research scripts for exploring mailing list datasets.

## Overview

This component contains example analysis scripts that demonstrate how to query and visualize mailing list data. These scripts were used during research and can serve as templates for custom analyses.

![Analysis Diagram](/docs/analysis.avif)

## Prerequisites

### Container Runtime (Required)
- Podman with Podman Compose, or
- Docker with Docker Compose

### Native Development (Optional)
- Python 3.12+
- [uv](https://docs.astral.sh/uv/) package manager

## Installation

### Using Devbox (Recommended)

```bash
devbox shell
```

This sets up Python, uv, and all required dependencies automatically.

### Manual Setup

```bash
# Install uv if not already installed
curl -LsSf https://astral.sh/uv/install.sh | sh

# Install dependencies
uv sync
```

## Usage

### Running Analyses

The analysis scripts expect the anonymized dataset from the MLH Anonymizer.

```bash
# Using Make (runs all analyses)
make run-analysis

# Run a single analysis by name
make run-analysis ANALYSIS_SCRIPT=unique_authors
make run-analysis ANALYSIS_SCRIPT=list_comparison LISTS_OF_INTEREST=amd-gfx,intel-gfx

# Debug mode (native execution)
INPUT_DIR="../output/parser/dataset,../output/anonymizer" OUTPUT_DIR="results" uv run src/main.py
```

### Input/Output Directories

| Directory | Purpose |
|-----------|---------|
| `../output/anonymizer/` | Input: Anonymized Parquet dataset |
| `results/` | Output: Analysis results and visualizations |

## Available Analyses

### main.py

Orchestrator that runs all analysis scripts. Supports selecting a single script via the `ANALYSIS_SCRIPT` environment variable.

```bash
# Run all analyses
uv run src/main.py

# Run a single analysis
ANALYSIS_SCRIPT=unique_authors uv run src/main.py
```

### list_comparison.py

Compares mailing list activity and cross-list interactions. Supports `LISTS_OF_INTEREST` to narrow the selection.

### list_sizes.py

Computes the size (email count) of each mailing list.

### unique_authors.py

Analyzes unique authors/contributors across mailing lists using the anonymized ID map.

### date_analysis.py

Generates date-related statistics and distributions.

### patch_missing.py

Identifies patch emails with missing code blocks. Supports `LISTS_OF_INTEREST`.

### date_missing.py

Finds emails with missing or malformed date headers. Supports `LISTS_OF_INTEREST`.

### author_distribution.py

Shows an author's email activity distribution across mailing lists and years. Requires `AUTHOR_IDENTITY` and optionally `LISTS_OF_INTEREST`.

### sql_querier.py

Interactive SQL query interface using [Apache DataFusion](https://datafusion.apache.org/user-guide/sql/index.html). Registers all available Parquet tables, displays schemas, then prompts for a SQL query. Results are written to a CSV file in the output directory. This is a non-default script — only runs when explicitly selected via `ANALYSIS_SCRIPT=sql_querier`.

```bash
ANALYSIS_SCRIPT=sql_querier uv run src/main.py
or
make run-analysis ANALYSIS_SCRIPT=sql_querier
```

## Output

Analysis results are stored in the `results/` directory:

```
analysis/results/
├── summary_stats.csv
├── activity_over_time.png
├── top_contributors.csv
└── ...
```

## Configuration

Configuration is done via environment variables:

### General

| Variable | Default | Description |
|----------|---------|-------------|
| `INPUT_DIR` | `/input` | Comma-separated directories containing Parquet files (parser output and/or anonymizer output) |
| `OUTPUT_DIR` | `results` | Directory for analysis output |

### Script Selection

| Variable | Default | Description |
|----------|---------|-------------|
| `ANALYSIS_SCRIPT` | *(empty)* | Run a single analysis by name instead of all. Valid names: `list_comparison`, `list_sizes`, `unique_authors`, `date_analysis`, `patch_missing`, `date_missing`, `author_distribution`†, `sql_querier`†. († non-default, only runs when explicitly selected) |

### Per-Script Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `LISTS_OF_INTEREST` | *(discover all)* | Comma-separated mailing list names to analyze (e.g. `amd-gfx,intel-gfx`). Used by: `list_comparison`, `patch_missing`, `date_missing`, `author_distribution` |
| `AUTHOR_IDENTITY` | *(prompted)* | Author email to filter by. Used by: `author_distribution` |

### Docker Compose Configuration

The `compose.yaml` file configures volume mounts:

```yaml
volumes:
  - ../output/anonymizer:/input:z   # Input dataset
  - ./results:/results:z            # Output results
```

## Development

### Running Natively

For development and debugging, run scripts directly:

```bash
# Set input and output directories
export INPUT_DIR="../output/parser/dataset,../output/anonymizer"
export OUTPUT_DIR="results"

# Run all analyses
uv run src/main.py

# Run a single analysis
ANALYSIS_SCRIPT=date_analysis uv run src/main.py

# With per-script variables
LISTS_OF_INTEREST="amd-gfx,rust-for-linux" ANALYSIS_SCRIPT=list_comparison uv run src/main.py
```


### Development
- Python 3.12+

## Container Build

The analysis runs in a container using the `ghcr.io/astral-sh/uv:python3.14-trixie-slim` base image.

```bash
# Rebuild container image
make rebuild
```

## Example Analysis Code

Here's an example of querying mailing list data with Polars:

```python
import polars as pl

# Load the dataset
df = pl.scan_parquet("../output/anonymizer/**/*.parquet")

# Count emails per day
daily_activity = (
    df
    .with_columns(pl.col("date").dt.date().alias("date_only"))
    .group_by("date_only")
    .agg(pl.len().alias("email_count"))
    .sort("date_only")
    .collect()
)

# Plot with seaborn
import seaborn as sns
import matplotlib.pyplot as plt

sns.lineplot(data=daily_activity.to_pandas(), x="date_only", y="email_count")
plt.title("Daily Mailing List Activity")
plt.savefig("results/activity_over_time.png")
```


## Cleaning Up

```bash
# Remove build artifacts (not results)
make clean

# This removes:
# - .venv/
# - .ruff_cache/
# - __pycache__/
```

## Troubleshooting

### "Input directory is missing or empty"
Run the anonymizer first to generate the anonymized dataset:
```bash
make anonymize
```

### Missing Visualization Dependencies
For native execution, ensure matplotlib and seaborn are installed:
```bash
uv sync
```

### Memory Issues
For large datasets:
- Use Polars lazy evaluation (`scan_parquet` instead of `read_parquet`)
- Filter data early in the query pipeline
- Process data in chunks

## Extending Analyses

To add new analyses:

1. Create a new Python script in `src/`
2. Use Polars to query the Parquet dataset
3. Save results to the `results/` directory
4. Optionally, add a new Makefile target

Example:
```python
# src/my_analysis.py
import polars as pl

df = pl.scan_parquet("../output/anonymizer/**/*.parquet")
# ... your analysis ...
```

## License

See the root [LICENSE](../LICENSE) file.
