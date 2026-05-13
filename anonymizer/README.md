# MLH Anonymizer

A Python tool for pseudo-anonymizing personal identification data in mailing list datasets.

## Overview

The MLH Anonymizer processes Parquet datasets produced by the MLH Parser and replaces personally identifiable information (PII) with SHA1 digests. This enables analysis of mailing list data while protecting user privacy.

![Anonymizer Diagram](/docs/anonymizer.avif)

## Features

- **SHA1 Hashing**: Replaces email addresses and names with consistent hashes
- **Deterministic**: Same input always produces the same hash, enabling longitudinal analysis
- **Polars-Powered**: Fast, memory-efficient processing using the Polars DataFrame library
- **Compressed Output**: Produces smaller, optimized Parquet files

## How It Works

The anonymizer applies SHA1 hashing to personal identification fields:

```
Original:  user@example.com
           ↓
Anonymized: a94a8fe5ccb19ba61c4c0873d391e987982fbbd3
```

The same email address always produces the same hash, allowing you to:
- Track user activity across multiple emails
- Perform user-level analytics
- Maintain data utility while protecting privacy

## Prerequisites

### Container Runtime (Required for production)
- Podman with Podman Compose, or
- Docker with Docker Compose

### Native Development (Optional)
- Python 3.14+
- [uv](https://docs.astral.sh/uv/) package manager
- Nox (for testing)

## Installation

### Using Devbox (Recommended)

```bash
devbox shell
```

This sets up Python 3.14, uv, and all required dependencies automatically.

### Manual Setup

```bash
# Install uv if not already installed
curl -LsSf https://astral.sh/uv/install.sh | sh

# Install dependencies
uv sync --locked
```

## Usage

### Running the Anonymizer

The anonymizer expects the parsed Parquet dataset from the MLH Parser.

```bash
# Using Make
make anonymize

# Using Devbox
devbox run anonymize

# Debug mode (native execution)
make debug-anonymizer
# or
INPUT_DIR="../output/parser/dataset" OUTPUT_DIR="../output/anonymizer" uv run src/main.py
```

### Input/Output Directories

| Directory | Purpose |
|-----------|---------|
| `../output/parser/dataset/` | Input: Non-anonymized Parquet dataset |
| `../output/anonymizer/` | Output: Anonymized dataset |

## Output Format

The anonymized dataset maintains the same structure as the input but with hashed PII fields:

```
output/anonymizer/
├── mailing_list=dev.rcpassos.me.lists.gfs2/
│   ├── part-0.parquet
│   └── part-1.parquet
├── mailing_list=dev.rcpassos.me.lists.iommu/
│   └── part-0.parquet
└── _common_metadata
```

### Anonymized Fields

The following fields are typically anonymized:

| Original Field | Anonymized Form |
|----------------|-----------------|
| `from` (email) | SHA1 hash |
| `to` (email) | SHA1 hash |
| `from_name` | SHA1 hash |
| Any other PII fields | SHA1 hash |

## Configuration

Configuration is done via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `INPUT_DIR` | `/input` | Directory containing parsed Parquet files |
| `OUTPUT_DIR` | `/output` | Output directory for anonymized files |

### Docker Compose Configuration

The `compose.yaml` file configures volume mounts:

```yaml
volumes:
  - ../output/parser/dataset/:/input:z    # Input dataset
  - ../output/anonymizer:/output:z       # Output directory
```

## Development

### Running Tests

```bash
# Using Make
make test

# Using Devbox
devbox run test-anonymizer

# Native with nox
nox

# Native with pytest
uv run pytest
```

### Debug Mode

Run the anonymizer directly without containers:

```bash
INPUT_DIR="../output/parser/dataset" OUTPUT_DIR="../output/anonymizer" uv run src/main.py
```

### Project Structure

```
anonymizer/
├── src/
│   └── mlh_anonymizer/
│       ├── __init__.py      # Module entry point
│       ├── main.py          # Main execution logic
│       └── constants.py     # Configuration constants
├── tests/                   # Test suite
├── Containerfile            # Docker/Podman image
├── compose.yaml             # Container orchestration
├── pyproject.toml           # Python project configuration
├── uv.lock                  # Locked dependencies
├── noxfile.py               # Test automation
└── Makefile                 # Build automation
```

## Dependencies

### Runtime
- `polars` (~1.39) - Fast DataFrame library for data processing

### Development
- `pytest` (>=9.0) - Testing framework
- `nox` (>=2026.2) - Test automation
- `freezegun` (>=1.5) - Time mocking for tests

## Container Build

The anonymizer runs in a container using the `ghcr.io/astral-sh/uv:python3.14-trixie-slim` base image.

```bash
# Rebuild container image
make rebuild

# Or with devbox
devbox run rebuild
```

## Security Considerations

### SHA1 Hashing

While SHA1 is considered cryptographically broken for security-critical applications, it is sufficient for pseudo-anonymization where:
- The goal is privacy protection, not cryptographic security
- Collision resistance is not critical
- Deterministic hashing is needed for data linkage

### Re-identification Risk

Be aware that:
- Hashed emails can be reversed via rainbow table attacks
- Common email addresses are easily reversible
- Consider adding a secret salt for additional protection

### Adding Salt (Advanced)

For enhanced security, modify the hashing to include a salt:

```python
import hashlib

def hash_with_salt(email: str, salt: str) -> str:
    return hashlib.sha1((email + salt).encode()).hexdigest()
```

Store the salt securely and separately from the data.

## Integration with Other Components

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  MLH Archiver   │ ──► │   MLH Parser    │ ──► │   Anonymizer    │
│  (raw emails)   │     │  (Parquet DS)   │     │ (anonymized DS) │
└─────────────────┘     └─────────────────┘     └─────────────────┘
                                                        │
                                                        ▼
                                               ┌─────────────────┐
                                               │    Analysis     │
                                               └─────────────────┘
```

Full pipeline:
```bash
make run        # Archive emails
make parse      # Parse to Parquet
make anonymize  # Anonymize data
make analysis   # Run analysis
```

## Example Usage with Polars

```python
import polars as pl

# Read the anonymized dataset
df = pl.scan_parquet("../output/anonymizer/**/*.parquet")

# Count emails per anonymized user
result = (
    df
    .group_by("from_hash")
    .agg(pl.len().alias("email_count"))
    .sort("email_count", descending=True)
    .limit(10)
    .collect()
)
```

## Cleaning Up

```bash
# Remove build artifacts
make clean

# This removes:
# - .venv/
# - .nox/
# - .ruff_cache/
# - .pytest_cache/
# - __pycache__/
```

## Troubleshooting

### "Input directory is missing or empty"
Run the parser first to generate the Parquet dataset:
```bash
make parse
```

### Container Permission Issues
The compose file uses `user: "${UID}:${GID}"` to match your user ID. Ensure your user has read/write access to the input/output directories.

### Memory Issues
For large datasets, consider:
- Processing in smaller chunks
- Increasing container memory limits
- Using Polars streaming mode

## License

See the root [LICENSE](../LICENSE) file.
