use crate::worker::Worker;
use crate::{
    is_shutdown_requested,
    public_inbox_source::{pi_config::PIConfig, pi_utils::*},
};
use log::{Level, log_enabled};
use std::collections::HashSet;
use std::sync::{Arc, atomic::AtomicBool};

use crate::archive_writer::{ArchiveWriter, WriteMode};
use crate::config::RunModeConfig;
use std::path::Path;

/// Result of processing a single epoch.
///
/// This struct encapsulates the output from processing one epoch,
/// including the number of emails processed and the updated counters.
#[derive(Debug)]
struct ProcessEpochResult {
    /// Number of emails successfully processed in this epoch
    emails_processed: usize,
    /// Total number of commits processed in this epoch
    commit_count: usize,
}

/// A worker that processes public inbox email archives.
///
/// This struct represents a worker that consumes inbox names frpodmanhannel and processes
/// the emails contained within those inboxes. It handles both V1 and V2 public inbox
/// formats, supports resuming from a specific email, and can filter emails by email range.
#[derive(std::fmt::Debug)]
pub struct PIWorker {
    /// Unique identifier for this worker instance
    id: u8,
    /// Configuration for the public inbox source
    pi_config: PIConfig,
    /// Base output path where processed emails will be stored
    base_output_path: String,
    /// Flag used to signal the worker to shut down gracefully
    shutdown_flag: Arc<AtomicBool>,
    write_mode: WriteMode,
}

impl PIWorker {
    /// Creates a new PIWorker instance.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for this worker
    /// * `pi_config` - Configuration for accessing the public inbox
    /// * `base_output_path` - Directory where processed emails will be written
    /// * `shutdown_flag` - Atomic boolean used to request worker shutdown
    ///
    /// # Returns
    ///
    /// * `PIWorker` - A configured worker instance ready to process inboxes
    pub fn new(
        id: u8,
        pi_config: PIConfig,
        base_output_path: String,
        shutdown_flag: Arc<AtomicBool>,
        write_mode: WriteMode,
    ) -> PIWorker {
        return PIWorker {
            id,
            pi_config,
            base_output_path,
            shutdown_flag,
            write_mode,
        };
    }
}

impl Worker for PIWorker {
    /// Consumes inbox names from a channel and processes each one.
    ///
    /// This function runs in a loop, receiving inbox names from the provided channel
    /// and processing each inbox by calling `process_inbox`. It continues until
    /// a shutdown is requested or the channel is closed.
    ///
    /// # Arguments
    ///
    /// * `self` - The worker instance (boxed for trait object compatibility)
    /// * `receiver` - Channel that provides inbox names to process
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the worker exits cleanly (shutdown requested or channel closed)
    /// * `Err` - If an error occurs while processing an inbox (logged but doesn't stop the worker)
    fn consumme_list(
        self: Box<Self>,
        receiver: crossbeam_channel::Receiver<String>,
    ) -> crate::Result<()> {
        log::info!("W{}: started consuming tasks", self.id);
        loop {
            if is_shutdown_requested(&self.shutdown_flag) {
                log::info!("W{}: Shutdown requested, exiting...", self.id);
                return Ok(());
            }

            log::info!("W{}: Reading new group from channel", self.id);
            let list_name = match receiver.recv() {
                Ok(name) => name,
                Err(crossbeam_channel::RecvError) => {
                    log::info!("W{}: Channel closed and empty, worker exiting", self.id);
                    return Ok(());
                }
            };
            match self.process_inbox(list_name.as_str()) {
                Ok(mail_count) => {
                    log::info!(
                        "W{}: completed a task with: {mail_count} emails saved from {list_name}",
                        self.id
                    );
                }
                Err(e) => {
                    log::error!("W{}: Failed to process inbox {}: {}", self.id, list_name, e);
                }
            };
        }
    }

    /// Reads a specific email by its 1-indexed position in the inbox.
    ///
    /// This function retrieves a single email from a public inbox based on its
    /// position in the overall email sequence (across all epochs for V2 inboxes).
    /// It's used for testing and random access to specific emails.
    ///
    /// # Arguments
    ///
    /// * `self` - The worker instance
    /// * `list_name` - The name of the public inbox to read from
    /// * `email_index` - The 1-indexed position of the email to retrieve
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the email was successfully retrieved and archived
    /// * `Err` - If the inbox is not found, the index is out of bounds, or an error occurs
    #[cfg_attr(feature = "otel", tracing::instrument(skip(self)))]
    fn read_email_by_index(&self, list_name: String, email_index: usize) -> crate::Result<()> {
        let mut writer = ArchiveWriter::new(
            Path::new(&self.base_output_path),
            &list_name,
            RunModeConfig::PublicInbox(self.pi_config.clone()),
            self.write_mode,
        );

        let inboxes = find_public_inboxes(std::path::Path::new(&self.pi_config.import_directory))?;
        let inbox = inboxes
            .iter()
            .find(|inbox| inbox.name == list_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Public inbox '{}' not found in {}",
                    list_name,
                    self.pi_config.import_directory
                )
            })?;

        let epochs = find_epochs(&inbox.git_dir)?;
        let epochs_to_use = if epochs.is_empty() {
            vec![EpochRepo {
                epoch_name: "1".to_string(),
                git_dir: inbox.git_dir.clone(),
            }]
        } else {
            epochs
        };

        if email_index == 0 {
            return Err(crate::errors::Error::Config(
                crate::errors::ConfigError::MissingHostname,
            ));
        }

        let mut remaining = email_index;
        for epoch in &epochs_to_use {
            let repo = git2::Repository::open_bare(&epoch.git_dir)?;

            let commit_count = count_commits(&repo)?;
            if remaining <= commit_count {
                let position = remaining - 1;
                let commit_id = get_commit_at_position(&repo, position)?;
                let commit = repo.find_commit(commit_id)?;
                let (commit_hash, raw_email) = extract_email_from_commit(&repo, &commit)?;
                let email_id = format_email_id(email_index, &epoch.epoch_name, &commit_hash);
                writer.archive_email(&email_id, raw_email.lines())?;
                log::info!(
                    "W{}: Successfully fetched email {} from {} (epoch {})",
                    self.id,
                    email_index,
                    list_name,
                    epoch.epoch_name
                );
                return Ok(());
            }
            remaining -= commit_count;
        }

        Err(anyhow::anyhow!(
            "Email index {} exceeds total emails in {}",
            email_index,
            list_name
        )
        .into())
    }
}

impl PIWorker {
    /// Processes an entire public inbox, archiving all emails.
    ///
    /// This is the main processing function for the PIWorker. It iterates through
    /// all commits in the inbox (across all epochs for V2 inboxes), extracts the
    /// email content from each commit, and archives it using the ArchiveWriter.
    /// It supports resuming from a specific email based on progress tracking
    /// and filtering by email range.
    ///
    /// # Arguments
    ///
    /// * `self` - The worker instance
    /// * `list_name` - The name of the public inbox to process
    ///
    /// # Returns
    ///
    /// * `Ok(usize)` - The number of emails successfully processed
    /// * `Err` - If the inbox is not found or an error occurs during processing
    #[cfg_attr(feature = "otel", tracing::instrument(skip(self)))]
    fn process_inbox(&self, list_name: &str) -> crate::Result<usize> {
        log::info!(
            "W{}: Starting processing emails from {}",
            self.id,
            list_name
        );

        let mut writer = ArchiveWriter::new(
            Path::new(&self.base_output_path),
            list_name,
            RunModeConfig::PublicInbox(self.pi_config.clone()),
            self.write_mode,
        );

        // Check for progress to determine where to resume from
        let last_email = writer.last_processed_id();
        let resume_info = last_email.and_then(|id| parse_email_id(&id));

        let mut list_path = std::path::Path::new(&self.pi_config.import_directory).to_path_buf();
        list_path.push(list_name);
        let inbox = detect_inbox(list_path.as_path())
            .expect("Detected inbox should be re-detected here")
            .expect("and it should exist");

        let epochs = find_epochs(&inbox.git_dir)?;
        // V1 repositories do not have epochs. The "all" epoch type fits well
        let mut epochs_to_use = if epochs.is_empty() {
            vec![EpochRepo {
                epoch_name: "all".to_string(),
                git_dir: inbox.git_dir.clone(),
            }]
        } else {
            epochs.clone()
        };

        let mut emails_processed = 0;
        let mut skip_until_epoch = None;
        let mut skip_until_sha = None;
        let mut global_position: usize = 0;

        // If we have resume information, set up skipping to that point
        if let Some(ref parsed) = resume_info {
            skip_until_epoch = Some(parsed.epoch_name.clone());
            skip_until_sha = Some(parsed.commit_sha.clone());
            global_position = parsed.email_num;
        }

        // Parse email range if configured
        let email_range_positions: Option<std::collections::HashSet<usize>> = match &self
            .pi_config
            .email_range
        {
            Some(range_str) => {
                let parsed_range =
                    crate::range_inputs::parse_sequence(range_str).map_err(|_e| {
                        crate::errors::Error::Config(crate::errors::ConfigError::MissingHostname)
                    })?;
                Some(
                    parsed_range
                        .map(|article_num| article_num.saturating_sub(1)) // Convert to 0-indexed
                        .collect(),
                )
            }
            None => None,
        };

        // epoch-filter: filter out explored epochs if continuing from a previous point
        // TODO: filtering epochs in v1 repos must always be "all"
        if let Some(skip_until_epoch) = &skip_until_epoch {
            epochs_to_use = if skip_until_epoch == "all" {
                epochs_to_use
                    .into_iter()
                    .filter(|e| e.epoch_name == "all")
                    .collect()
            } else {
                epochs_to_use
                    .into_iter()
                    .filter(|e| {
                        e.epoch_name.parse::<usize>().unwrap()
                            >= skip_until_epoch.parse::<usize>().unwrap()
                    })
                    .collect()
            };
        }

        // Process each epoch in order
        //
        // When resuming, global_position holds the total number of emails
        // already processed (1-indexed).  We need to know how many commits
        // fall before the first epoch we'll actually visit (i.e. epochs
        // filtered out by skip_until_epoch) so we can detect whether each
        // remaining epoch has been fully processed.
        let commits_before_first_epoch: usize = {
            let mut before = 0usize;
            if let Some(ref skip_epoch) = skip_until_epoch {
                for ep in &epochs {
                    // Stop once we reach the epoch we begin resuming from.
                    if ep.epoch_name == *skip_epoch
                        || (skip_epoch != "all"
                            && ep
                                .epoch_name
                                .parse::<usize>()
                                .ok()
                                .zip(skip_epoch.parse::<usize>().ok())
                                .is_some_and(|(a, b)| a >= b))
                    {
                        break;
                    }
                    if let Ok(r) = git2::Repository::open_bare(&ep.git_dir) {
                        before += count_commits(&r).unwrap_or(0);
                    }
                }
            }
            before
        };

        let mut cumulative_emails_total: usize = commits_before_first_epoch;

        for epoch in &epochs_to_use {
            // Check for shutdown request
            if is_shutdown_requested(&self.shutdown_flag) {
                log::info!(
                    "W{}: Shutdown requested while processing {}, processed {} emails",
                    self.id,
                    list_name,
                    emails_processed
                );
                return Ok(emails_processed);
            }

            let repo = git2::Repository::open_bare(&epoch.git_dir)?;
            let epoch_commits = count_commits(&repo)?;
            cumulative_emails_total += epoch_commits;

            // If every commit in this epoch falls at or before the resume
            // point, skip the epoch entirely.  This prevents re-processing
            // already-fetched emails when the service restarts after a
            // complete run.
            if global_position >= cumulative_emails_total {
                skip_until_sha = None;
                log::info!(
                    "W{}: Skipping already-processed epoch {} ({} commits, cumulative={})",
                    self.id,
                    epoch.epoch_name,
                    epoch_commits,
                    cumulative_emails_total
                );
                continue;
            }

            let result = self.process_epoch(
                &repo,
                epoch,
                &mut writer,
                &email_range_positions,
                &skip_until_sha,
                global_position,
            )?;

            emails_processed += result.emails_processed;
            global_position += result.commit_count;

            // Reset skipping flags after processing the first epoch
            skip_until_sha = None;
        }

        log::info!(
            "W{}: Processed {} emails from {}",
            self.id,
            emails_processed,
            list_name
        );
        Ok(emails_processed)
    }

    /// Process a single epoch, streaming commits and archiving emails.
    ///
    /// This function handles the core logic of processing one epoch of a public inbox,
    /// including commit iteration, email range filtering, resume-from-SHA logic,
    /// and email extraction and archiving.
    ///
    /// # Arguments
    ///
    /// * `repo` - The git repository for this epoch
    /// * `epoch` - Information about the epoch being processed
    /// * `writer` - Archive writer for storing processed emails
    /// * `email_range_positions` - Optional set of positions to filter by email range
    /// * `skip_until_sha` - Optional short SHA to skip commits until found
    /// * `global_position` - Current global email position across all epochs
    /// * `shutdown_flag` - Flag to check for shutdown requests
    ///
    /// # Returns
    ///
    /// * `Ok(ProcessEpochResult)` - Results including emails processed and updated counters
    /// * `Err` - If an error occurs during processing
    #[cfg_attr(feature = "otel", tracing::instrument(skip(repo, writer, self)))]
    fn process_epoch(
        &self,
        repo: &git2::Repository,
        epoch: &EpochRepo,
        writer: &mut ArchiveWriter,
        email_range_positions: &Option<HashSet<usize>>,
        skip_until_sha: &Option<String>,
        global_position: usize,
    ) -> crate::Result<ProcessEpochResult> {
        let mut emails_processed = 0;
        let mut next_email_num = global_position + 1;
        let mut commit_count = 0;

        let resume_sha_full: Option<String> = if let Some(target_sha) = skip_until_sha {
            if let Ok(object) = repo.revparse_single(target_sha)
                && let Some(commit) = object.as_commit()
            {
                Some(commit.id().to_string())
            } else {
                log::warn!(
                    "configured to resume from {}, but commit not found in epoch {}",
                    skip_until_sha.clone().unwrap_or("--empty--".to_string()),
                    epoch.epoch_name
                );
                None
            }
        } else {
            None
        };

        // First pass: count how many commits are newer than the resume SHA
        // Among those, the first (newest) `new_commit_count` are new emails.
        // The rest (older) were already processed in a previous run.
        let mut total_before_resume: usize = 0;
        {
            let mut revwalk = repo.revwalk()?;
            revwalk.push_head()?;
            for commit_id_result in revwalk {
                let commit_id = match commit_id_result {
                    Ok(id) => id,
                    Err(e) => {
                        log::error!(
                            "W{}: Revwalk error in epoch {} (first pass): {}",
                            self.id,
                            epoch.epoch_name,
                            e
                        );
                        break;
                    }
                };
                if let Some(ref sha) = resume_sha_full
                    && commit_id.to_string() == *sha
                {
                    break;
                }
                total_before_resume += 1;
            }
        }
        // total_before_resume already counts only commits newer than the
        // resume SHA (or all commits when not resuming). In a multi-epoch
        // setup, global_position spans epochs and must not be subtracted
        // from per-epoch commit counts.
        //
        // When total_before_resume == 0 in a resume epoch, the resume SHA is
        // the HEAD commit itself (already processed in a previous run).
        // Process all commits in this epoch — the break at resume SHA below
        // will skip the already-processed HEAD.
        let new_commits = if total_before_resume == 0 && resume_sha_full.is_some() {
            count_commits(repo)?
        } else {
            let old_non_resume = if total_before_resume > global_position {
                // Single-epoch resume: count includes both new and old commits
                // behind the resume SHA. Subtract already-processed position.
                global_position.saturating_sub(1)
            } else {
                0
            };
            total_before_resume.saturating_sub(old_non_resume)
        };

        // Second pass: process only the new commits
        let mut revwalk = repo.revwalk()?;
        revwalk.push_head()?;
        let mut processed_new = 0;

        for commit_id_result in revwalk {
            let commit_id = match commit_id_result {
                Ok(id) => id,
                Err(e) => {
                    log::error!(
                        "W{}: Revwalk error in epoch {} (second pass): {}",
                        self.id,
                        epoch.epoch_name,
                        e
                    );
                    break;
                }
            };
            if is_shutdown_requested(&self.shutdown_flag) {
                log::info!(
                    "W{}: Shutdown requested during epoch {} processing",
                    self.id,
                    epoch.epoch_name
                );
                break;
            }

            if let Some(ref sha) = resume_sha_full
                && commit_id.to_string() == *sha
            {
                if total_before_resume == 0 {
                    // Resume SHA is the HEAD — skip this already-processed
                    // commit but continue with older (unprocessed) commits.
                    continue;
                } else {
                    // Resume SHA is in the middle of history — all newer
                    // commits were processed, stop here.
                    break;
                }
            }

            // Skip old (already processed) commits
            if processed_new >= new_commits && total_before_resume > 0 {
                // All remaining commits before the resume SHA are old
                // We still need to count them for commit_count tracking
                commit_count += 1;
                continue;
            }

            let current_global_position = global_position + commit_count;

            if let Some(positions) = email_range_positions
                && !positions.contains(&current_global_position)
            {
                commit_count += 1;
                continue;
            }

            let commit = repo.find_commit(commit_id)?;
            commit_count += 1;
            processed_new += 1;
            match extract_email_from_commit(repo, &commit) {
                Ok((commit_hash, raw_email)) => {
                    let email_id = format_email_id(next_email_num, &epoch.epoch_name, &commit_hash);
                    writer.archive_email(&email_id, [raw_email.as_str()])?;
                    emails_processed += 1;
                }
                Err(e) => {
                    writer.log_error(&commit_id.to_string(), format!("Error reading content for commit. Possibly missing 'm' blob in commit tree. Error: {}", e).as_str());
                    if log_enabled!(Level::Debug) {
                        let subject = commit
                            .message()
                            .map(|msg| msg.to_string())
                            .unwrap_or_else(|_| "<no message>".to_string());
                        let tree_id = commit.tree_id();
                        let tree_str = format!("{}", tree_id);

                        log::debug!(
                            "W{}: Commit {} missing 'm' blob - subject: '{}', parents: {}, tree: {}, error: {}",
                            self.id,
                            commit_id,
                            subject,
                            commit.parent_ids().count(),
                            tree_str,
                            e
                        );
                    }
                }
            }
            next_email_num += 1;
        }

        Ok(ProcessEpochResult {
            emails_processed,
            commit_count,
        })
    }
}
