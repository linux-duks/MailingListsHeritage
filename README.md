# Mailing Lists Heritage - Create Datasets from Mailing Lists

Collect and archive locally all emails from mailing lists, parse them into structured datasets, and analyze them while preserving privacy.

This software is extensible. It currently supports reading from NNTP endpoints and public-inbox git repositories, and new sources can be added by implementing a clear interface (a rust `Trait`).

## Pipeline Overview

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  MLH Archiver   │ ──► │   MLH Parser    │ ──► │   Anonymizer    │ ──► │    Analysis     │
│  (raw emails)   │     │  (Parquet DS)   │     │ (anonymized DS) │     │  (insights)     │
└─────────────────┘     └─────────────────┘     └─────────────────┘     └─────────────────┘
```

See the [architecture diagram](/docs/fluxogram.svg) for a visual representation.

## Project Components

This project consists of four main components:

| Component | Description | Language |
|-----------|-------------|----------|
| **[MLH Archiver](mlh_archiver/)** | Downloads emails from NNTP servers and stores them as raw emails or Parquet (configurable) | Rust |
| **[MLH Parser](mlh_parser/)** | Parses raw emails into structured Parquet datasets with Hive partitioning | Rust |
| **[MLH Anonymizer](anonymizer/)** | Pseudo-anonymizes personal identification using SHA1 digests | Rust |
| **[MLH Analysis](analysis/)** | Example analysis scripts for exploring mailing list data | Python |

Each component has its own detailed documentation:

- [Archiver Documentation](mlh_archiver/README.md)
- [Parser Documentation](mlh_parser/README.md)
- [Anonymizer Documentation](anonymizer/README.md)
- [Analysis Documentation](analysis/README.md)

---
## Using with a Published dataset

If you are using this repository to test and analyse a built dataset:

1. Download the compressed (.tar.gz) files into the [./dataset/](./dataset/) folder.
2. Run `make decompress_dataset` to decompress the dataset
3. Run `make move_dataset_into_default_folder` to have the files in their expected locations

Now, the analysis should work as if the tool had just created the dataset.

---
## Quick Start

### Demo Mode (self-contained)

To quickly explore the pipeline with sample mailing list data, run the one-command demo setup:

```bash
make setup-demo
```

This generates a local public-inbox v2 repository from the test emails, installs pre-configured config files (backing up any existing ones), and sets up the full pipeline to run against the local data. Once complete, run `make run` to execute the entire pipeline.

### Step 1: Cloning

1. Clone recursively

One of the dependencies is a git submodule. To build correctly

```bash
# with http
git clone --recurse-submodules https://gitlab.com/ccsl-usp/codev/MailingListsHeritage.git

# with ssh
git clone --recurse-submodules git@gitlab.com:ccsl-usp/codev/MailingListsHeritage.git
```

Or, if you cloned without recursive: 
```bash
# in case you did:
# git clone https://gitlab.com/ccsl-usp/codev/MailingListsHeritage.git
cd MailingListsHeritage
git submodule update --init --recursive
```

Tip: you can configure git to automatically convert cloning to your preferred protocol:

```bash
git config --global url."https://gitlab.com/".insteadOf "git@gitlab.com:"
git submodule update --init --recursive
# and to revert the config:
git config --global --remove-section url."https://gitlab.com/"
   ```


### Step 2: Configuring

1. Copy the example configuration file:

```bash
make create-default-configs
```

or manually copy:

```bash
cp example_archiver_config.yaml   archiver_config.yaml
cp example_parser_config.yaml     parser_config.yaml
cp example_anonymizer_config.yaml anonymizer_config.yaml
```

2. Edit `archiver_config.yaml` with your Source details. You can read from a NNTP server or a local directory with PublicInbox repositories:


### PublicInbox Examples: 

```yaml
nthreads: 2
output_dir: "./output/archiver"
public_inbox:
  origin: public-inbox.example.org
  import_directory: /media/public-inbox-data

```

### NNTP Examples: 
```yaml
nthreads: 2
output_dir: "./output/archiver"
loop_groups: false
write_mode: "parquet:50000"  # or "raw_email"

read_lists:
    nntp:
    - dev.example.me.lists.gfs2
    - dev.example.me.lists.iommu
nntp:
    hostname: "nntps://nntp.example.com"
```

**Glob patterns** are also supported in `read_lists`. Use `*` or `?` to match multiple lists:

```yaml
nntp:
    hostname: "nntp.example.com"
    port: 119
read_lists:
    nntp:
    # Match all lists starting with "dev.example."
    - "dev.example.*"
    # Match any list containing ".synth"
    - "*.synth*"
    # Mix exact names and patterns
    - specific.list.name
```

**Authentication** is also supported:

```yaml
nntp:
  hostname: "news.example-server.org"
  username: username
  password: password
  port: 119
```

> [!WARNING]
> **Do not set `nthreads` above 4 if you don't control the server you are fetching from.**
> Be respectful to public infrastructure. This tool is designed to avoid being seen as an abusive scraping bot.

### Step 3: Run the Pipeline

```bash
# Build and run the whole Pipeline
make run
```

or

```bash
# Build and run the archiver (collects emails)
make run_archiver

# Parse raw emails into Parquet dataset
make run-parser

# Anonymize the dataset
make run-anonymizer

# Run example analyses
make run-analysis
```

---

## Development

### Setup Options

You have three options for setting up the development environment:

#### Option 1: Devbox (Recommended)

[Devbox](https://www.jetify.com/devbox/) is a command-line tool that creates isolated development shells using Nix packages. It provides all required dependencies (Python, uv, Rust, git) in a single command.

**Installation:** See the [Devbox installation guide](https://www.jetify.com/docs/devbox/quickstart).

**Setup:**

```bash
devbox shell
```

This sets up:

- Python 3.14
- uv package manager
- Rust toolchain (rustup)
- Git

**Available Commands:**

| Command | Description |
|---------|-------------|
| `devbox run build` | Build the archiver |
| `devbox run run` | Run all steps |
| `devbox run run` | Run all steps |
| `devbox run parse` | Run the mailing list parser |
| `devbox run anonymize` | Run the anonymizer |
| `devbox run analysis` | Run example analyses |
| `devbox run rebuild` | Rebuild all components |
| `devbox run test` | Run all tests |
| `devbox run test-archiver` | Run archiver tests only |
| `devbox run test-parser` | Run parser tests only |
| `devbox run test-anonymizer` | Run anonymizer tests only |
| `devbox run clean` | Clean all build artifacts |
| `devbox run debug-parser` | Run parser in debug mode |
| `devbox run debug-anonymizer` | Run anonymizer in debug mode |
| `devbox run debug-analysis` | Run analysis in debug mode |
| `devbox run peek <path>` | Quick inspection of Parquet files |
| `devbox run doc` | Generate and open Rust package docs |

#### Option 2: Manual Installation

Install the required toolchains manually:

**Rust/Cargo:**

Install rustup (Rust toolchain manager): <https://rustup.rs/>

**Python/uv:**
Assuming you have Python installed,
Install the uv package manager:
<https://docs.astral.sh/uv/getting-started/installation/>

#### Option 3: Dev Container

This repository includes a [`.devcontainer`](.devcontainer/) configuration for VS Code or other compatible editors.

**Features:**

- Pre-configured Rust and Python environment
- Integrated with the Devbox setup

**Usage:**

1. Open the project in your editor (that supports devcontainer)
2. Click "Reopen in Container" when prompted
3. The dev container will build automatically

---

### Implementing New Sources

To add a new email source (e.g., ListArchiveX, IMAP, local mbox), see the
[Development Guide](mlh_archiver/README.md#development-implementing-a-new-source)
in the Archiver documentation.

### Makefile Commands

The root [`Makefile`](Makefile) orchestrates all components. Run commands from the project root:

| Command | Description |
|---------|-------------|
| `make` or `make all` | Build and run the archiver |
| `make build` | Build the archiver (Rust) |
| `make build-check-git` | Build the check-git utility (Rust) |
| `make build-check-nntp` | Build the check-nntp utility (Rust) |
| `make run` | Run the archiver |
| `make parse` | Run the mailing list parser (configure via `parser_config.yaml`) |
| `make anonymize` | Run the anonymizer |
| `make analysis` | Run example analyses |
| `make rebuild` | Rebuild all components |
| `make test` | Run all tests |
| `make test-archiver` | Run archiver tests only |
| `make test-parser` | Run parser tests only |
| `make test-anonymizer` | Run anonymizer tests only |
| `make clean` | Clean all build artifacts |
| `make debug-parser` | Run parser in debug mode |
| `make debug-anonymizer` | Run anonymizer in debug mode |
| `make debug-analysis` | Run analysis in debug mode |
| `make check_config_files` | Verify required config files are present |
| `make create-default-configs` | Create config files from examples |
| `make setup-demo` | One-command demo setup with sample data |
| `make peek PEEK_PATH=dataset_dir` |  Get basic Statistics about a parquet dataset|

---

## Container Runtime Configuration

The project supports multiple container runtimes with automatic detection:

**Priority:** podman > docker compose (v2) > docker-compose (v1)

**Override the detected runtime:**

```bash
# Use nerdctl
make run CONTAINER=nerdctl COMPOSE="nerdctl compose"

# Force docker-compose (v1)
make parse COMPOSE=docker-compose
```

See [`containers.mk`](containers.mk) for the detection logic.

---

## Utility Scripts

### peek-files

Quick inspection tool for Parquet files and directories located in [`scripts/peek_parquet/peek_files.py`](scripts/peek_parquet/peek_files.py). Two modes are available:

**Inspection mode** — Open a file or directory to browse schema, row counts, and data preview:

```bash
# Inspect a single parquet file
devbox run peek output/parser/dataset/list=dev.rcpassos.me.lists.gfs2/list_data.parquet

# Inspect a directory (finds all .parquet files under it)
devbox run peek output/
```

**Row lookup mode** (`--select-by-column`) — Search across all parquet files in a directory for rows matching a column value. Each matching row is printed with all its fields:

```bash
# Look up by email_id (default column)
devbox run peek output/ --select-by-column 0000000056-e0-5dadd9f0f9884ed3852f090bd05eed898db64966

# Look up by a different column
devbox run peek output/ --select-by-column "Alice" --column from_name
```

| Option | Description |
|--------|-------------|
| `<PATH>` | Path to a parquet file or directory (inspection mode) |
| `--select-by-column <VALUE>` | Enable row lookup mode: search for rows matching this value |
| `--column <NAME>` | Column to search in (default: `email_id`) |

### check-git

CLI tool for browsing and inspecting local public-inbox git repositories located in [`scripts/check_git/`](scripts/check_git/). Provides an interactive TUI and a CLI mode for precise email lookups.

**Email Identifier Format:**

```
{10-digit-padded}-e{epoch}-{commit_sha}
```

- **10-digit-padded**: Sequential email number (e.g., `0000000056`)
- **e{epoch}**: Epoch repository identifier (e.g., `e0`, `eall`)
- **{commit_sha}**: Full 40-character commit SHA

Example: `0000000056-e0-5dadd9f0f9884ed3852f090bd05eed898db64966`

**Build:**

```bash
# Using make from the project root:
make build-check-git

# Or manually with cargo:
cargo build --release --package check_git
```

**Usage:**

```bash
# Interactive mode
check_git --inbox-dir /path/to/inboxes

# Test fetch by position
check_git --inbox-dir /path/to/inboxes --test --list my.list.name --article 1

# Look up and print a single email by its formatted identifier
check_git --inbox-dir /path/to/inboxes --email-id 0000000056-e0-5dadd9f0f9884ed3852f090bd05eed898db64966 --list my.list.name
```

| Option | Description |
|--------|-------------|
| `--inbox-dir <PATH>` | **Required.** Path to public-inbox directories |
| `--count <N>` | Number of recent emails to preview (default: 5) |
| `--test` | Run a non-interactive test fetch |
| `--list <NAME>` | List (folder) name for test or email-id lookup |
| `--article <N>` | Article position for test fetch (1-indexed) |
| `--email-id <ID>` | Look up and print a single email by its formatted identifier |
| `--verbose` | Enable verbose (debug) logging |

### check-nntp

CLI tool for browsing NNTP mailing lists and fetching specific emails, located in [`scripts/check_nntp/`](scripts/check_nntp/). Supports an interactive TUI and a batch mode for targeted article lookups.

**Build:**

```bash
# Using make from the project root:
make build-check-nntp

# Or manually with cargo:
cargo build --release --package check_nntp
```

**Usage:**

```bash
# Interactive mode (browse and preview lists)
check_nntp

# Interactive mode with explicit server
check_nntp -s nntp://nntp.example.com
check_nntp -s nntps://nntp.example.com
check_nntp -s nntp://nntp.example.com:8119

# Batch mode: fetch specific articles by list glob and id range
check_nntp -s nntp://nntp.example.com -l "*.lkml" --id 42
check_nntp -s nntp://nntp.example.com -l "*.lkml" --id 1-10
check_nntp -s nntp://nntp.example.com -l "*.lkml" --id '1..10'
check_nntp -s nntp://nntp.example.com -l "*.lkml" --id '1,3,5-7'

# Batch mode with authentication
check_nntp -s nntp://nntp.example.com -l "*.lkml" --id 1-10 -u myuser -P mypassword
```

| Option | Description |
|--------|-------------|
| `-s, --server <URL>` | NNTP server URL (`nntp://host`, `nntps://host`, `nntp://host:port`) |
| `-l, --list <PATTERN>` | Glob pattern to filter mailing lists. Triggers batch mode (requires `--id`) |
| `--id <VALUE>` | Article ID or range in batch mode (`42`, `1-10`, `1..10`, `1,3,5-7`) |
| `-u, --username <USER>` | NNTP username for authentication |
| `-P, --password <PASS>` | NNTP password for authentication |
| `--export-config` | Export configuration to YAML file after browsing |
| `-v, --verbose` | Enable verbose (debug) logging |

In batch mode, if multiple lists match the glob pattern, the tool fetches articles one list at a time and prompts to confirm before moving to the next list.

---

## Architecture Details

### Archiver Implementation

The archiver is implemented in Rust and uses a forked NNTP library ([`rust-nntp`](mlh_archiver/rust-nntp/)).

**Design Principles:**

- **Multi-threaded**: Each worker thread handles one mailing list at a time
- **Respectful**: Not designed to pull emails as fast as possible to avoid being detected as a malicious scraping bot
- **Continuous**: Can keep local files up-to-date with new emails
- **Graceful shutdown**: Clean exit on Ctrl+C with progress preservation

**Architecture:**

- Workers are created and owned by `WorkerManager`, then moved to individual threads
- Tasks (mailing list names) are distributed via crossbeam channels
- Multiple workers per group enable load balancing (one worker receives each task)
- Shutdown is coordinated via shared `Arc<AtomicBool>` flag

**Configuration:**

The archiver uses a nested configuration format:

- Global settings (`nthreads`, `output_dir`, `loop_groups`,`read_lists`) at the top level
- NNTP-specific settings (`hostname`, `port`, `email_range`) under the `nntp:` block

**email Range Selection:**

The `email_range` option allows fetching specific emails instead of all new emails:

- Single numbers: `"100"`
- Ranges: `"1-50"`
- Comma-separated: `"1,5,10"`
- Mixed: `"1,3-5,10-15"`

See the [Archiver Documentation](mlh_archiver/README.md) for details.

### Parser Implementation

The parser is implemented in Rust and uses:

- **Arrow + Parquet**: Columnar storage format via the Apache Arrow ecosystem
- **Hive Partitioning**: Data organized by mailing list name (`list=<name>/`) for efficient querying
- **Error Handling**: Failed parses are saved per mailing list under `<output_dir>/errors/list=<name>/`
- **Batch Processing**: Large datasets are automatically split into multiple row groups to stay within Arrow's 2 GB offset limits

### Anonymizer Implementation

The anonymizer applies SHA1 hashing to personally identifiable information (PII):

- **Polars**: Fast columnar transformations with parallel execution per mailing list
- **SHA1 Hashing**: Deterministic pseudo-anonymization of PII fields (`from`, `to`, `cc`, `trailers`, `raw_body` (identities contained in it))
- **Parquet Output**: Output partitioned by mailing list under `dataset/` (anonymized data) and `id_map_from/` (identity map for validation)
- **Configuration**: YAML-based config file (`example_anonymizer_config.yaml`) supporting thread count, I/O paths, and list selection
- **Batch Processing**: Row-group controlled output with configurable batch size for memory efficiency
- See [Anonymizer Documentation](anonymizer/README.md#security-considerations) for security considerations

---

## Additional Resources

- [Archiver Detailed Documentation](mlh_archiver/README.md) - Includes development guide for new sources
- [Parser Detailed Documentation](mlh_parser/README.md)
- [Anonymizer Detailed Documentation](anonymizer/README.md)
- [Analysis Detailed Documentation](analysis/README.md)
- [Example Configuration](example_archiver_config.yaml)
- [Architecture Diagrams](./docs/)
- Generated docs via `cargo doc` (or )

## License

See the [LICENSE](LICENSE) file.
