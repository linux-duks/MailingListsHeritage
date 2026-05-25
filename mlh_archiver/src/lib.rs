#![allow(clippy::needless_return)]

//! MLH Archiver - Mailing Lists Archiver
//!
//! A multi-threaded Rust application for archiving mailing list emails from various sources
//! (NNTP, IMAP, local mbox, etc.) to local storage.
//!
//! # Architecture
//!
//! The archiver uses a producer-consumer pattern:
//! - [`WorkerManager`] creates and owns workers for each configured source
//! - [`scheduler::Scheduler`] orchestrates worker threads and task distribution
//! - Workers receive tasks via crossbeam channels and process them independently
//!
//! # Lifecycle
//!
//! 1. Configuration is loaded from YAML/JSON/TOML files
//! 2. Workers are created for each configured source (RunMode)
//! 3. Workers are moved to individual threads
//! 4. Producer threads send mailing list names to workers via channels
//! 5. Workers fetch emails and store them as RFC 822 files
//! 6. Graceful shutdown via Ctrl+C signal

pub mod archive_writer;
pub mod config;
pub mod errors;
pub mod file_utils;
pub mod nntp_source;
pub mod public_inbox_source;
pub mod range_inputs;
pub mod scheduler;
pub mod worker;

/// shortcut export of DataLineageRecord
pub use archive_writer::data_lineage::DataLineageRecord;

#[cfg(feature = "otel")]
pub mod otel;

pub use errors::Result;

use config::{RunMode, RunModeConfig};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread::sleep;
use std::time::{Duration, Instant};
use worker::WorkerManager;

/// Main entry point for the archiver application.
///
/// This function orchestrates the entire archiving process:
/// 1. Determines active run modes from configuration
/// 2. Creates workers for each run mode via [`WorkerManager`]
/// 3. Initializes the [`Scheduler`](scheduler::Scheduler) to manage threads
/// 4. Runs the scheduler to process all mailing lists
///
/// # Arguments
///
/// * `app_config` - Mutable reference to application configuration
/// * `shutdown_flag` - Shared atomic flag for graceful shutdown signaling.
///   Cloned and passed to each worker to enable Ctrl+C handling.
///
/// # Returns
///
/// * `Ok(())` on successful completion
/// * `Err(...)` if any error occurs during list retrieval, worker creation, or scheduling
///
/// # Example
///
/// ```rust,no_run
/// use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
/// use mlh_archiver::{config, start};
///
/// let mut app_config = config::read_config().unwrap();
/// let shutdown_flag = Arc::new(AtomicBool::new(false));
/// start(&mut app_config, shutdown_flag).unwrap();
/// ```
pub fn start(
    app_config: &mut config::AppConfig,
    shutdown_flag: Arc<AtomicBool>,
) -> crate::errors::Result<()> {
    let run_modes = app_config.get_run_modes();

    // Create worker manager to own all workers
    let mut worker = WorkerManager::new();

    // Create workers for each run mode
    for mode in run_modes {
        match &mode {
            RunMode::NNTP => {
                if let Some(RunModeConfig::NNTP(nntp_config)) = app_config.get_run_mode_config(mode)
                {
                    // Get available lists in endpoint
                    let groups = nntp_source::nntp_lister::retrieve_lists(nntp_config.clone())?;
                    // Filter with selected lists by user
                    let groups = app_config.get_read_lists(groups, mode)?;

                    log::info!("made a selection of {} {:#?}", groups.len(), groups);

                    // Create workers for this run mode
                    worker.create_workers(mode, groups, app_config, shutdown_flag.clone());
                }
            }
            RunMode::PublicInbox => {
                if let Some(RunModeConfig::PublicInbox(pi_config)) =
                    app_config.get_run_mode_config(mode)
                {
                    let groups = public_inbox_source::pi_lister::retrieve_lists(pi_config.clone())?;
                    let groups = app_config.get_read_lists(groups, mode)?;
                    log::info!("made a selection of {} {:#?}", groups.len(), groups);

                    worker.create_workers(mode, groups, app_config, shutdown_flag.clone());
                }
            }
            RunMode::LocalMbox => {
                unimplemented!()
            }
        }
    }

    file_utils::check_or_create_folder(app_config.output_dir.clone())?;

    let mut scheduler =
        scheduler::Scheduler::new(app_config, worker.get_groups(), shutdown_flag.clone());

    scheduler.run()
}

/// Sleeps for the specified duration, but checks the shutdown_flag every 2s.
/// Returns `true` if the full duration elapsed, `false` if shutdown was requested.
fn interruptible_sleep(duration: Duration, shutdown_flag: &Arc<AtomicBool>) -> bool {
    let two_sec = Duration::from_millis(2_000);

    if duration.as_millis() == 0 {
        return true;
    } else if duration <= two_sec {
        sleep(duration);
        return true;
    }

    let start = Instant::now();
    let poll_interval = two_sec;

    while start.elapsed() < duration {
        if is_shutdown_requested(shutdown_flag) {
            return false;
        }

        // Ensure we don't sleep past the remaining time
        let time_left = duration.saturating_sub(start.elapsed());
        sleep(time_left.min(poll_interval));
    }

    true
}

/// Helper function to check if a shutdown has been requested via the shared flag.
///
/// This is a convenience function for checking the shutdown flag using
/// the correct memory ordering (`Relaxed`).
///
/// # Arguments
///
/// * `shutdown_flag` - Reference to the shared atomic shutdown flag
///
/// # Returns
///
/// `true` if shutdown was requested, `false` otherwise
///
/// # Example
///
/// ```rust,no_run
/// use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
/// use mlh_archiver::is_shutdown_requested;
///
/// let flag = Arc::new(AtomicBool::new(false));
/// if is_shutdown_requested(&flag) {
///     // Clean up and exit
/// }
/// ```
#[inline]
pub fn is_shutdown_requested(shutdown_flag: &Arc<AtomicBool>) -> bool {
    shutdown_flag.load(std::sync::atomic::Ordering::Relaxed)
}
