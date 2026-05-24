//! Parquet file reader using Polars.

use crate::Result;
use polars::prelude::*;
use std::fs;
use std::path::Path;

/// Discover all `.parquet` files in the given directory (non-recursive).
pub fn discover_parquet_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "parquet") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Read a single Parquet file into a Polars DataFrame.
pub fn read_parquet_file(path: &Path) -> Result<DataFrame> {
    let file = fs::File::open(path)?;
    let df = ParquetReader::new(file).finish()?;
    Ok(df)
}

/// Read all Parquet files in a directory into a single DataFrame by concatenation.
pub fn read_parquet_dir(dir: &Path) -> Result<DataFrame> {
    let files = discover_parquet_files(dir)?;
    if files.is_empty() {
        return Err("No parquet files found".into());
    }
    let mut combined = read_parquet_file(&files[0])?;
    for f in &files[1..] {
        combined.vstack_mut(&read_parquet_file(f)?)?;
    }
    Ok(combined)
}

/// Read a parquet file in slices, calling `f` for each batch of rows.
/// Re-opens the file per slice to keep memory bounded.
pub fn read_parquet_file_batched<F>(path: &Path, batch_rows: usize, mut f: F) -> Result<()>
where
    F: FnMut(DataFrame) -> Result<()>,
{
    let total_rows = {
        let file = fs::File::open(path)?;
        let mut reader = ParquetReader::new(file);
        reader.num_rows()?
    };

    if total_rows == 0 {
        return Ok(());
    }

    let n_batches = total_rows.div_ceil(batch_rows);
    let mut offset = 0;

    for batch_idx in 0..n_batches {
        let len = std::cmp::min(batch_rows, total_rows - offset);

        log::debug!(
            "Reading batch {}/{} (offset={}, len={}) from {}",
            batch_idx + 1,
            n_batches,
            offset,
            len,
            path.display()
        );

        let file = fs::File::open(path)?;
        let df = ParquetReader::new(file)
            .with_slice(Some((offset, len)))
            .finish()?;

        f(df)?;
        offset += len;
    }

    Ok(())
}

/// Read all parquet files in a directory in slices, calling `f` for each batch.
pub fn read_parquet_dir_batched<F>(dir: &Path, batch_rows: usize, mut f: F) -> Result<()>
where
    F: FnMut(DataFrame) -> Result<()>,
{
    let files = discover_parquet_files(dir)?;
    if files.is_empty() {
        return Err("No parquet files found".into());
    }

    for file_path in &files {
        log::debug!("Processing file: {}", file_path.display());
        read_parquet_file_batched(file_path, batch_rows, &mut f)?;
    }

    Ok(())
}
