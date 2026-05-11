# MLH Analysis

Example analyses and research scripts for exploring mailing list datasets.

## Overview

This component contains example analysis scripts that demonstrate how to query and visualize mailing list data. These scripts were used during research and can serve as templates for custom analyses.

## Features

- **Polars-Based**: Efficient data processing with the Polars DataFrame library
- **Visualization**: Seaborn integration for statistical visualizations
- **Reproducible**: Containerized execution for consistent results

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
make analysis

# Using Devbox
devbox run analysis

# Debug mode (native execution)
make debug-analysis
# or
INPUT_DIR="../output/anonymizer" uv run src/make_analysis.py
```

### Input/Output Directories

| Directory | Purpose |
|-----------|---------|
| `../output/anonymizer/` | Input: Anonymized Parquet dataset |
| `results/` | Output: Analysis results and visualizations |

## Available Analyses

### make_analysis.py

Main analysis script that generates various statistics and visualizations from the mailing list data.

```bash
# Run the main analysis
uv run src/make_analysis.py
```

### unique_authors.py

Analyzes unique authors/contributors across mailing lists.

```bash
# Run author analysis
uv run src/unique_authors.py
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

| Variable | Default | Description |
|----------|---------|-------------|
| `INPUT_DIR` | `/input` | Directory containing anonymized Parquet files |

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
# Set input directory
export INPUT_DIR="../output/anonymizer"

# Run analysis
uv run src/make_analysis.py

# Or with inline variable
INPUT_DIR="../output/anonymizer" uv run src/make_analysis.py
```

### Project Structure

```
analysis/
├── src/
│   ├── make_analysis.py    # Main analysis script
│   └── unique_authors.py   # Author analysis
├── results/                # Analysis output
├── Containerfile           # Docker/Podman image
├── compose.yaml            # Container orchestration
├── pyproject.toml          # Python project configuration
├── uv.lock                 # Locked dependencies
├── .python-version         # Python version specification
└── Makefile                # Build automation
```

## Dependencies

### Runtime
- `polars` (>=1.36.1) - Fast DataFrame library
- `seaborn` (>=0.13.2) - Statistical visualization

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

## Integration with Other Components

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  MLH Archiver   │ ──► │   MLH Parser    │ ──► │   Anonymizer    │ ──► │    Analysis     │
│  (raw emails)   │     │  (Parquet DS)   │     │ (anonymized DS) │     │  (insights)     │
└─────────────────┘     └─────────────────┘     └─────────────────┘     └─────────────────┘
```

Full pipeline:
```bash
make run        # Archive emails
make parse      # Parse to Parquet
make anonymize  # Anonymize data
make analysis   # Run analysis
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
