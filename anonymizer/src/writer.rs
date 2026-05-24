//! Parquet batched writer using Polars with ZSTD compression.

use crate::Result;
use polars::io::parquet::write::{BatchedWriter, ParquetCompression, ParquetWriter};
use polars::prelude::*;
use polars_utils::compression::ZstdLevel;
use std::fs;
use std::path::Path;

/// Create a [`BatchedWriter`] for incremental parquet writing.
/// Each [`BatchedWriter::write_batch`] call appends a row group to the
/// underlying file. Call [`BatchedWriter::finish`] to finalize the file.
pub fn create_batched_writer(
    path: &Path,
    compression: usize,
    schema: &Schema,
) -> Result<BatchedWriter<fs::File>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let zstd_level = if compression == 0 {
        None
    } else {
        Some(
            ZstdLevel::try_new(compression as i32)
                .map_err(|e| format!("invalid zstd level {compression}: {e}"))?,
        )
    };

    let file = fs::File::create(path)?;
    let writer = ParquetWriter::new(file)
        .with_compression(ParquetCompression::Zstd(zstd_level))
        .batched(schema)?;

    Ok(writer)
}
