# MLH Archiver

A multi-threaded Rust application for archiving mailing list emails from a few different sources.
The MLH Archiver fetches emails from the configured source (e.g., NNTP servers) and saves them from specified mailing lists as raw email files.

- NNTP (Network News Transfer Protocol) servers.
- Public-Inbox Local Git Repositories

## Architecture

The MLH Archiver uses a producer-consumer pattern with multiple worker threads:

### Worker Model

- **Workers** are created in `lib.rs::start()` and owned by `WorkerManager`
- Each worker is **moved to its own thread** before execution
- Workers receive tasks via **crossbeam channels** (one channel per worker group)
- **Shutdown** is coordinated via `Arc<AtomicBool>` flag passed from `main.rs`

### Thread Communication

```
Producer Thread ──► Sender<Group> ──► Receiver<Group> (cloned to each worker)
                                           ├─► Worker 1 (thread 1)
                                           ├─► Worker 2 (thread 2)
                                           └─► Worker N (thread N)
```

When a task (mailing list name) is sent to the channel, only **one** worker receives it,
enabling natural load balancing.

### Shutdown Mechanism

1. Ctrl+C signal sets shared `AtomicBool` flag in `main.rs`
2. Flag is cloned to each worker at creation time
3. Workers check flag:
   - At start of each task iteration
   - During reconnection waits (60s)
   - During error recovery waits (10s)
   - During email fetching (per email)

### Design Principles

- **Respectful bandwidth**: Not designed to fetch as fast as possible
- **Continuous operation**: Can keep local files up-to-date with new emails
- **Graceful shutdown**: Clean exit on Ctrl+C with progress preservation

See the [architecture diagram](../docs/fluxogram.svg) for a visual representation.

## Features

- **Multi-threaded**: Process multiple mailing lists concurrently
- **Configurable**: Support for JSON, YAML, and TOML configuration files
- **Interactive TUI**: Select mailing lists from an interactive terminal interface
- **Flexible email selection**: Read specific email ranges or all emails
- **Continuous or one-shot mode**: Loop to keep archives updated or run once

## Prerequisites

### Native Build

- Rust toolchain (cargo, rustc)
- `libiconv` (for character encoding support)

### Container Build (Alternative)

- Podman or Docker
- No Rust installation required

## Building

### Using Make

```bash
# Build the archiver
make build

# Build and run
make run
```

### Using Devbox

```bash
devbox run build
devbox run run
```

### Manual Build

```bash
# Native build
cargo build --release

# Container build with Podman
podman run --rm -it -u $(id -u):$(id -g) \
  --network=host \
  -v ./:/usr/src/app:z \
  -w /usr/src/app \
  docker.io/rust:1.94-slim \
  cargo build --release
```

The compiled binary will be at `target/release/mlh_archiver`.

## Usage

### Command Line Arguments

```bash
Usage: mlh_archiver [OPTIONS]

Options:
  -c, --config-file <CONFIG_FILE>      Path to config file [default: archiver_config*]
  -h, --help                           Print help
```

**Note:** All configuration is done via the config file.

### Environment Variables

- `RUST_LOG` - Log level (e.g., `debug`, `info`, `warn`, `error`)

### Examples

```bash
# Using a config file
cargo run -- -c archiver_config.yaml

# With debug logging
RUST_LOG=debug cargo run -- -c archiver_config.yaml
```

## Configuration

The archiver looks for configuration files matching `archiver_config*.{json,yaml,toml}` in the current directory by default.

Configuration is **nested**: global settings at the top level, NNTP-specific settings under the `nntp:` block.

### Example YAML Configuration

```yaml
# archiver_config.yaml
nthreads: 2
output_dir: "./output"
loop_groups: true
write_mode: "parquet:10000"  # or "raw_email"

nntp:
  hostname: "nntp.example.com"
  port: 119

read_lists:
  nntp:
    - dev.rcpassos.me.lists.gfs2
    - dev.rcpassos.me.lists.iommu
```

### Configuration Options

#### Global Options

| Option | Type | Description |
|--------|------|-------------|
| `nthreads` | integer | Number of parallel worker threads (default: 1) |
| `output_dir` | string | Directory to store archived emails (default: "./output") |
| `loop_groups` | boolean | Continuously check for new emails (default: true) |
| `write_mode` | string | Output format: `"raw_email"` or `"parquet:SIZE"` (default: `"parquet:10000"`) |
| `read_lists` | map(source:list) | Mailing list names to archive (e.g., `["*"]` for all, or specific lists/globs) |

#### Public-Inbox Options (under `public_inbox:` block)

If using a large number of public-inbox repositores, we recommend cloning them with [Grokmirror](https://github.com/mricon/grokmirror).
We have our complete guide available in the [linux-duks/Public-Inbox-Stack](https://github.com/linux-duks/Public-Inbox-Stack). If using only for this, follow the mirroring steps only.

It is expected that

| Option | Type | Description |
|--------|------|-------------|
| `import_directory` | string | **Required.** The parent folder of all mailing lists|
| `origin` | string | **Required**. server hostname were the lists were cloned from |
| `public_inbox_config` | string | Optional.  TODO: public-inbox configuration file, to automatically select lists |
| `email_range` | string | Optional. Read specific range of emails (e.g., `"1-100"` or `"1,5,10-20"`) |

#### Public-Inbox Options (under `public_inbox:` block)

If using a large number of public-inbox repositores, we recommend cloning them with [Grokmirror](https://github.com/mricon/grokmirror).
We have our complete guide available in the [linux-duks/Public-Inbox-Stack](https://github.com/linux-duks/Public-Inbox-Stack). If using only for this, follow the mirroring steps only.

It is expected that

| Option | Type | Description |
|--------|------|-------------|
| `import_directory` | string | **Required.** The parent folder of all mailing lists|
| `origin` | string | **Required**. server hostname were the lists were cloned from |
| `public_inbox_config` | string | Optional.  TODO: public-inbox configuration file, to automatically select lists |
| `group_lists` | list | Mailing list names to archive (e.g., `["*"]` for all, or specific lists/globs) |
| `email_range` | string | Optional. Read specific range of emails (e.g., `"1-100"` or `"1,5,10-20"`) |

#### NNTP Options (under `nntp:` block)

| Option | Type | Description |
|--------|------|-------------|
| `hostname` | string | **Required.** NNTP server hostname or IP |
| `port` | integer | NNTP server port (default: 119) |
| `read_lists` | list | Mailing list names to archive (e.g., `["*"]` for all, or specific lists/globs) |
| `email_range` | string | Optional. Read specific range of emails (e.g., `"1-100"` or `"1,5,10-20"`) |
| `username` | string | Optional. NNTP server username for authentication |
| `password` | string | Optional. NNTP server password for authentication |

## email Range Selection

The `email_range` configuration option allows fetching specific emails instead of all new emails:

```yaml
nntp:
  hostname: "nntp.example.com"
  email_range: "1,5,10-15"  # Fetch emails 1, 5, and 10-15
```

**Supported formats:**

- Single numbers: `"100"`
- Ranges: `"1-50"`
- Comma-separated: `"1,5,10"`
- Mixed: `"1,3-5,10-15"`

**Memory efficiency:** Range parsing is lazy - the range string is stored and parsed per mailing list, avoiding memory issues with large ranges.

**Use cases:**

- Retry failed emails: `email_range: "42,108,256"`
- Fetch specific date ranges (if you know email numbers)
- Test runs with small samples: `email_range: "1-10"`

## Authentication

If your NNTP server requires authentication, provide credentials in the config:

```yaml
nntp:
  hostname: "nntp.example.com"
  port: 563
  username: "myuser"
  password: "mypass"
  read_lists: ["*"]
```

Both `username` and `password` are optional. If omitted, the archiver connects without authentication.

## Output Format

Emails are stored according to the `write_mode` configuration:

- **`raw_email`**: Raw RFC 822 email files (`.eml`) organized by mailing list:
```
output/
├── dev.rcpassos.me.lists.gfs2/
│   ├── 000001.eml
│   ├── 000002.eml
│   └── ...
```

- **`parquet:<buffer_size>`** (default): Parquet files with batched writes:
```
output/
├── dev.rcpassos.me.lists.gfs2/
│   ├── list.name_0.parquet
│   ├── list.name_1.parquet
│   └── ...
```

Both modes also produce:
```
├── __progress.yaml          # YAML: last processed ID (resume)
├── __lineage.yaml           # YAML stream: DataLineage audit trail
└── __errors.csv             # CSV: id,error_message
```
output/
├── dev.rcpassos.me.lists.gfs2/
│   ├── 000001.eml
│   ├── 000002.eml
│   └── ...
└── dev.rcpassos.me.lists.iommu/
    ├── 000001.eml
    └── ...
```

## Testing

```bash
# Run all tests
make test

# Or with devbox
devbox run test-archiver

# Or directly
cargo test
```

### Test Coverage

**Unit Tests** (`cargo test --lib`):

- Range parsing (`range_inputs.rs`)
- Configuration loading and validation
- Error types

**Integration Tests** (`cargo test --test test_nntp`):

- Full list download from mock NNTP server
- Single email by range (`"5"`)
- email range (`"1-3"`)
- Multiple emails (`"1,5,10"`)
- Mixed ranges (`"1,3-5,10"`)

Integration tests use testcontainers to spin up a mock NNTP server. Requires Docker/Podman.

## Documentation

### Rust API Documentation

Generate and open the Rust API documentation in your browser:

```bash
# Using make
make doc

# Using devbox
devbox run doc

# Or directly
cargo doc --document-private-items --open
```

This generates comprehensive documentation including:

- All public and private items
- Function signatures with parameters and return values
- Struct and enum field descriptions
- Usage examples where provided
- Intra-doc links between modules

Documentation is output to `target/doc/mlh_archiver/` and automatically opened in your default browser.

## Project Structure

Here are the important files and folders:

```
mlh_archiver/
├── src/
│   ├── lib.rs               # Core start() function, worker initialization
│   ├── config.rs            # Configuration loading and RunMode handling
│   ├── scheduler.rs         # Thread orchestration, producer/consumer pattern
│   ├── worker.rs            # Worker trait, WorkerManager ownership
│   ├── errors.rs            # Error types (Error, ConfigError)
│   ├── archive_writer/      # Reusable storage facade (MUST be used by all workers)
│   └── *_source/            # Implementation of the Mailing List Sources
│       └─── mod.rs          # Module exports
├── rust-nntp/               # Forked NNTP library
└── tests/                   # Integration Tests
```

## Dependencies

- `clap` - Command line argument parsing
- `config` - Configuration file loading (JSON, YAML, TOML)
- `crossbeam-channel` - Thread communication
- `env_logger` - Logging with environment variable support
- `inquire` - Interactive TUI prompts
- `nntp` - NNTP protocol implementation (forked, local)
- `serde` / `serde_yaml` - Serialization
- `chrono` - Date/time handling
- `testcontainers` - Integration testing with containers

## Development: Implementing a New Source

To add a new email source (e.g., ListArchiveX, IMAP, local mbox), follow these steps:

### ArchiveWriter — The Reusable Storage Interface

**All worker implementations MUST use [`ArchiveWriter`](src/archive_writer/) for:**

- Writing fetched emails to disk (`.eml` files)
- Tracking progress (`__progress.yaml` YAML)
- Logging errors for unavailable emails (`__errors.csv` CSV)

The `ArchiveWriter` provides a consistent storage interface so that:

1. Progress is tracked uniformly across all sources
2. Resume from last position works the same way regardless of source
3. File layout is consistent across all implementations

Since each worker writes to a distinct output path per list, **no concurrency control is needed**. Workers create their own `ArchiveWriter` instance per task.

```rust
use mlh_archiver::archive_writer::{ArchiveWriter, WriteMode};
use mlh_archiver::config::RunModeConfig;
use std::path::Path;

// Inside worker's consume_list or read_email_by_index:
let write_mode = WriteMode::Parquet { buffer_size: 10000 }; // or WriteMode::RawEmails
let writer = ArchiveWriter::new(
    Path::new(&self.base_output_path),
    &list_name,
    run_mode, // RunModeConfig from your worker's config
    write_mode,
);

// Get last processed email ID (for resume)
let last_id = writer.last_processed_id();

// Archive a fetched email
writer.archive_email(email_id, &raw_lines)?;

// Log unavailable emails (non-fatal)
writer.log_error(email_id, &error.to_string());
```

**File layout produced by `ArchiveWriter`:**

See [Output Format](#output-format) for details on file layout based on `write_mode`. Both modes produce:
```
output/
├── list.name/
│   ├── __progress.yaml          # YAML: last processed ID (resume)
│   ├── __lineage.yaml           # YAML stream: DataLineage audit trail
│   └── __errors.csv             # CSV: id,error_message
```
output/
├── list.name/
│   ├── 1.eml                    # Fetched email
│   ├── 2.eml
│   ├── __progress.yaml          # YAML: last processed ID (resume)
│   ├── __lineage.yaml           # YAML stream: DataLineage audit trail
│   └── __errors.csv             # CSV: id,error_message
```

### 1. Create Source Module

Create `src/list_archive_x_source/` (or your source name):

```
src/
└── list_archive_x_source/
    ├── mod.rs
    ├── list_archive_x_config.rs
    └── list_archive_x_worker.rs
```

### 2. Implement Configuration

**`src/list_archive_x_source/list_archive_x_config.rs`:**

```rust
use crate::errors::ConfigError;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ListArchiveXConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub read_lists: Option<Vec<String>>,
    pub email_range: Option<String>,
}

impl ListArchiveXConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.base_url.is_empty() {
            return Err(ConfigError::MissingHostname);
        }
        Ok(())
    }
}
```

### 3. Implement Worker

**`src/list_archive_x_source/list_archive_x_worker.rs`:**

```rust
use crate::archive_writer::{ArchiveWriter, WriteMode};
use crate::worker::Worker;
use std::path::Path;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

pub struct ListArchiveXWorker {
    id: u8,
    config: ListArchiveXConfig,
    base_output_path: String,
    shutdown_flag: Arc<AtomicBool>,
    write_mode: WriteMode,
    // ... other fields (e.g., HTTP client)
}

impl ListArchiveXWorker {
    pub fn new(
        id: u8,
        config: ListArchiveXConfig,
        base_output_path: String,
        shutdown_flag: Arc<AtomicBool>,
        write_mode: WriteMode,
    ) -> Self {
        ListArchiveXWorker {
            id,
            config,
            base_output_path,
            shutdown_flag,
            write_mode,
            // ...
        }
    }
}

impl Worker for ListArchiveXWorker {
    fn consumme_list(
        self: Box<Self>,
        receiver: crossbeam_channel::Receiver<String>,
    ) -> crate::Result<()> {
        loop {
            // Check shutdown flag at start of each iteration
            if self.shutdown_flag.load(Ordering::Relaxed) {
                log::info!("W{}: Shutdown requested, exiting...", self.id);
                return Ok(());
            }

            // Receive task from channel
            let list_name = match receiver.recv() {
                Ok(name) => name,
                Err(_) => return Ok(()), // Channel closed
            };

            // Create ArchiveWriter for this list
            let mut writer = ArchiveWriter::new(
                Path::new(&self.base_output_path),
                &list_name,
                run_mode, // RunModeConfig for this source
                self.write_mode,
            );

            // Get last processed ID for resume
            let last_id = writer.last_processed_id();

            // Fetch emails for list_name using writer for storage...
            // writer.archive_email(id, &lines)?;
            // writer.log_error(id, &error);
        }
    }

    fn read_email_by_index(
        &self,
        list_name: String,
        email_index: usize,
    ) -> crate::Result<()> {
        // Create writer for this list
        let mut writer = ArchiveWriter::new(
            Path::new(&self.base_output_path),
            &list_name,
            run_mode,
            self.write_mode,
        );

        // Fetch and store the specific email...
        // writer.archive_email(email_index, &lines)?;
        Ok(())
    }
}
```

**Key requirements:**

- Store `shutdown_flag: Arc<AtomicBool>` for graceful shutdown
- Check shutdown flag at:
  - Start of each task iteration
  - During long waits or retries
  - During email fetching loops
- **Use `ArchiveWriter` for all file I/O** — do NOT write files directly
- Use `RefCell` or `Mutex` for mutable connection state

### 4. Update Configuration

**`src/config.rs`:**

Add new variant to enums:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    NNTP,
    ListArchiveX,  // Add this
    LocalMbox,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunModeConfig {
    NNTP(nntp_config::NntpConfig),
    ListArchiveX(list_archive_x_config::ListArchiveXConfig),  // Add this
    LocalMbox,
}
```

Add to `AppConfig` struct:

```rust
pub struct AppConfig {
    // ... existing fields
    pub list_archive_x: Option<list_archive_x_config::ListArchiveXConfig>,
}
```

Update `get_run_mode_config()`:

```rust
pub fn get_run_mode_config(&self, run_mode: RunMode) -> Option<RunModeConfig> {
    match run_mode {
        RunMode::NNTP => Some(RunModeConfig::NNTP(self.nntp.clone()?)),
        RunMode::ListArchiveX => Some(RunModeConfig::ListArchiveX(self.list_archive_x.clone()?)),
        RunMode::LocalMbox => Some(RunModeConfig::LocalMbox),
    }
}
```

Update `get_run_modes()`:

```rust
pub fn get_run_modes(&self) -> Vec<RunMode> {
    let mut run_modes = vec![];
    if self.nntp.is_some() {
        run_modes.push(RunMode::NNTP);
    }
    if self.list_archive_x.is_some() {
        run_modes.push(RunMode::ListArchiveX);
    }
    run_modes
}
```

### 5. Register Worker

**`src/worker.rs`:**

```rust
use crate::list_archive_x_source::list_archive_x_worker::ListArchiveXWorker;

impl WorkerManager {
    pub fn create_workers(
        &mut self,
        run_mode: RunMode,
        tasks: Vec<String>,
        app_config: &AppConfig,
        shutdown_flag: Arc<AtomicBool>,
    ) {
        match run_mode {
            RunMode::NNTP => { /* existing */ }
            RunMode::ListArchiveX => {
                if let Some(RunModeConfig::ListArchiveX(config)) =
                    app_config.get_run_mode_config(run_mode)
                {
                    let num_workers = app_config.nthreads.max(1) as usize;
                    for id in 0..num_workers {
                        let worker = ListArchiveXWorker::new(
                            id as u8,
                            config.clone(),
                            app_config.output_dir.clone(),
                            shutdown_flag.clone(),
                        );
                        workers.push(Box::new(worker));
                    }
                }
            }
            RunMode::LocalMbox => { /* existing */ }
        }
    }
}
```

### 6. Update Module Exports

**`src/lib.rs`:**

```rust
pub mod list_archive_x_source;
```

### 7. Update Configuration File Format

Document new config structure:

```yaml
nthreads: 2
output_dir: "./output"
loop_groups: true

nntp:
  hostname: "nntp.example.com"
  port: 119
  read_lists: ["list1"]

list_archive_x:
  base_url: "https://archive.example.com/api"
  api_key: "your-api-key"
  read_lists: ["list1", "list2"]
```

### 8. Add Tests

Create `tests/test_list_archive_x.rs` following the pattern in `tests/test_nntp.rs`.

## Troubleshooting

### Connection Issues

- Verify NNTP server hostname and port in your config file
- Check firewall rules for NNTP traffic (typically port 119 or 563 for SSL)
- Some NNTP servers require authentication (not currently supported)

### Configuration Issues

- Ensure `nntp.hostname` is set in your config file
- The `nntp:` block is required
- Check that YAML syntax is valid

### Build Issues

- Ensure `libiconv` is installed for character encoding support
- For container builds, verify Podman/Docker is running

### Logging

Enable debug logging for troubleshooting:

```bash
RUST_LOG=debug cargo run -- -c archiver_config.yaml
```

## OpenTelemetry Tracing

The archiver includes optional OpenTelemetry instrumentation via the `otel` Cargo feature. When enabled, it collects traces using the `tracing` crate and exports them to any OpenTelemetry-compatible backend.

### Building and Running

```bash
# Build with tracing support
cargo build --features=otel

# Run with default endpoint (http://localhost:4318)
cargo run --features=otel -- -c archiver_config.yaml

# Run with a custom OTLP endpoint
OTEL_EXPORTER_OTLP_ENDPOINT=http://jaeger.example.com:4318 cargo run --features=otel -- -c archiver_config.yaml
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP HTTP endpoint URL | `http://localhost:4318` |
| `RUST_LOG` | Log level (also affects trace verbosity) | `info` |

### Quickstart with Jaeger

OpenTelemetry is an open standard. Any OTLP HTTP-compatible backend works.
The easiest way to visualize traces is to run Jaeger's all-in-one Docker image:

```bash
podman run --rm --name jaeger \
  -p 16686:16686 \
  -p 4317:4317 \
  -p 4318:4318 \
  -p 5778:5778 \
  -p 9411:9411 \
  cr.jaegertracing.io/jaegertracing/jaeger:2.17.0
```

Then run the archiver:

```bash
cargo run --features=otel -- -c archiver_config.yaml
```

Open <http://localhost:16686> in your browser to view traces.

### Adding Tracing to New Code

The `tracing` crate is used to instrument code. When the `otel` feature is enabled, `tracing` events and spans are automatically exported as OpenTelemetry traces.

#### Using `#[instrument]`

The simplest way to add tracing is the `#[instrument]` attribute:

```rust
#[cfg_attr(feature = "otel", tracing::instrument)]
fn my_function() {
    // Function body — a span is automatically created
}
```

#### Controlling Fields in Spans

Adding specific fields:

```rust
#[cfg_attr(feature = "otel", tracing::instrument(fields(list = %list_name)))]
fn process_list(list_name: &str) {
    // The `list` field appears in the span metadata
}
```

Removing fields - Large entities, or entities that do not implement Debug need to be skipped :

```rust
#[cfg_attr(feature = "otel", tracing::instrument(skip(receiver, self)))]
fn consumme_list(

```

#### Key Points

- All existing `log::info!`, `log::debug!`, etc. calls are captured as OpenTelemetry log events within spans
- Spans are exported synchronously via `SimpleSpanProcessor` — no async runtime is used
- The `otel` module in `src/otel.rs` handles initialization. To add new exporters or layers, modify that file
- When `otel` is not enabled, the application falls back to `env_logger` for console logging only

## License

See the root [LICENSE](../LICENSE) file.
