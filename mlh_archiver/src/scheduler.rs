use crate::config::AppConfig;
use crate::range_inputs::parse_sequence;
use crate::{
    interruptible_sleep, is_shutdown_requested,
    worker::{Worker, WorkerGroup},
};
use crossbeam_channel::bounded;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread::{self, JoinHandle};
use std::time::Duration;

// intervals in seconds
const INTERVAL_BETWEEN_RESCANS: usize = 60 * 60; // 1h

/// Channel capacity per worker group
const CHANNEL_CAPACITY: usize = 10;

/// Orchestrates worker threads and task distribution.
///
/// The scheduler is responsible for:
/// - Creating channels for worker communication
/// - Spawning worker threads (one per worker)
/// - Spawning producer threads (one per worker group)
/// - Waiting for all threads to complete
///
/// # Architecture
///
/// For each worker group:
/// 1. Creates a bounded channel (sender/receiver)
/// 2. Spawns workers, each with a clone of the receiver
/// 3. Spawns a producer with the sender
/// 4. Producer sends tasks; workers compete to receive them
///
/// # Thread Lifecycle
///
/// - **Worker threads**: Run until channel closes or shutdown requested
/// - **Producer threads**: Send all tasks, then drop sender to signal completion
/// - **Main thread**: Waits for all threads via `join()`
///
/// # Shutdown
///
/// Workers check the shared shutdown flag (passed at creation time).
/// When Ctrl+C is pressed:
/// 1. Signal handler sets the flag
/// 2. Workers detect flag and exit gracefully
/// 3. Producer threads detect closed channels and exit
pub struct Scheduler<'a> {
    app_config: &'a AppConfig,
    loop_groups: bool,
    worker_groups: &'a mut Vec<WorkerGroup>,
    shutdown_flag: Arc<AtomicBool>,
}

impl<'a> Scheduler<'a> {
    /// Creates a new scheduler instance.
    ///
    /// # Arguments
    ///
    /// * `app_config` - Application configuration (used for range selection)
    /// * `worker_groups` - Mutable reference to worker groups from [`WorkerManager`](crate::worker::WorkerManager)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use mlh_archiver::{config, scheduler::Scheduler, worker::WorkerManager};
    /// use std::sync::{Arc, atomic::AtomicBool};
    ///
    /// let app_config = config::read_config().unwrap();
    /// let mut manager = WorkerManager::new();
    /// // ... create workers ...
    /// let shutdown_flag = Arc::new(AtomicBool::new(false));
    /// let mut scheduler = Scheduler::new(&app_config, manager.get_groups(), shutdown_flag);
    /// ```
    pub fn new(
        app_config: &'a AppConfig,
        worker_groups: &'a mut Vec<WorkerGroup>,
        shutdown_flag: Arc<AtomicBool>,
    ) -> Scheduler<'a> {
        Scheduler {
            app_config,
            loop_groups: app_config.loop_groups,
            worker_groups,
            shutdown_flag,
        }
    }

    /// Runs the scheduler, processing all worker groups.
    ///
    /// This is the main execution method. It:
    /// 1. Drains all worker groups from the manager
    /// 2. For each group:
    ///    - Creates a channel for task distribution
    ///    - Spawns worker threads
    ///    - Spawns a producer thread
    /// 3. Waits for all threads to complete
    ///
    /// # Execution Modes
    ///
    /// ## Range Mode
    ///
    /// If `email_range` is configured:
    /// - Workers use `read_email_by_index()` to fetch specific articles
    /// - Producer parses range text and sends (list, index) pairs
    ///
    /// ## Normal Mode
    ///
    /// If no range is configured:
    /// - Workers use `consumme_list()` to fetch all new articles
    /// - Producer sends list names; workers determine what's new
    ///
    /// # Returns
    ///
    /// * `Ok(())` on successful completion
    /// * `Err(...)` if any thread panics during join
    pub fn run(&mut self) -> crate::Result<()> {
        // Collect thread handles
        let mut handles = Vec::new();

        // Process each worker group - collect tasks first to avoid borrow issues
        let groups: Vec<WorkerGroup> = std::mem::take(self.worker_groups);

        for group in groups {
            let WorkerGroup {
                tasks,
                workers,
                run_mode,
            } = group;

            // Create channel - sender goes to producer thread, receivers cloned to workers
            let (sender, receiver): (
                crossbeam_channel::Sender<String>,
                crossbeam_channel::Receiver<String>,
            ) = bounded(CHANNEL_CAPACITY);

            if let Some(range_text) = self.app_config.get_range_selection_text(run_mode) {
                // RUN in range mode
                if tasks.len() > 1 {
                    log::warn!(
                        "Multiple lists selected in Range Mode. This is likely a mistake..."
                    );

                    thread::sleep(Duration::from_millis(100));
                }
                let worker_handles = self.spawn_worker_to_read_email_by_index(workers, receiver);

                handles.extend(worker_handles);

                // Spawn producer thread that sends tasks
                let producer_handle = Self::spawn_producer_for_read_email_by_index(
                    sender,
                    tasks,
                    range_text,
                    self.shutdown_flag.clone(),
                );
                handles.push(producer_handle);

                // read_email_by_index
            } else {
                // Spawn worker threads - each worker is moved to its own thread
                // receiver is moved to the function, and dropped on return
                // When all workers drop their receivers, recv() will return RecvError
                let worker_handles = self.spawn_workers_to_consumme_list(workers, receiver);
                // Collect worker handles
                handles.extend(worker_handles);

                // Spawn producer thread that sends tasks
                let producer_handle = Self::spawn_producer_for_consume_list(
                    sender,
                    tasks,
                    self.loop_groups,
                    self.shutdown_flag.clone(),
                );
                handles.push(producer_handle);
            }
        }

        log::info!("Waiting for {} threads to finish...", handles.len());

        // Wait for all threads to finish
        for (i, handle) in handles.into_iter().enumerate() {
            log::debug!("Joining thread {i}...");
            if let Err(e) = handle.join() {
                log::error!("Failed to join thread {i}: {:?}", e);
            }
        }

        log::info!("All threads stopped");

        Ok(())
    }

    /// Spawns worker threads for normal (non-range) mode.
    ///
    /// Each worker receives a clone of the channel receiver and runs
    /// [`Worker::consumme_list()`] until the channel closes.
    ///
    /// # Arguments
    ///
    /// * `workers` - Vector of workers to spawn (moved to threads)
    /// * `receiver` - Channel receiver (cloned for each worker)
    ///
    /// # Returns
    ///
    /// Vector of thread join handles.
    fn spawn_workers_to_consumme_list(
        &self,
        workers: Vec<Box<dyn Worker>>,
        receiver: crossbeam_channel::Receiver<String>,
    ) -> Vec<JoinHandle<()>> {
        let mut worker_handles = Vec::with_capacity(workers.len());
        for worker in workers {
            let receiver_chan = receiver.clone();

            // Spawn worker thread - worker is moved here
            let handle = thread::spawn(move || {
                // Worker runs until channel is closed or shutdown requested
                match worker.consumme_list(receiver_chan) {
                    Ok(_) => {
                        log::debug!("Worker thread finished");
                    }
                    Err(err) => {
                        log::error!("Worker thread finished with error: {err}");
                    }
                }
            });
            worker_handles.push(handle);
        }
        return worker_handles;
    }

    /// Spawns worker threads for range mode.
    ///
    /// Each worker receives a clone of the channel receiver and runs
    /// a loop that calls [`Worker::read_email_by_index()`] for each task.
    ///
    /// Tasks are formatted as `"list_name_##index"` and parsed by workers.
    ///
    /// # Arguments
    ///
    /// * `workers` - Vector of workers to spawn (moved to threads)
    /// * `receiver` - Channel receiver (cloned for each worker)
    ///
    /// # Returns
    ///
    /// Vector of thread join handles.
    fn spawn_worker_to_read_email_by_index(
        &self,
        workers: Vec<Box<dyn Worker>>,
        receiver: crossbeam_channel::Receiver<String>,
    ) -> Vec<JoinHandle<()>> {
        let mut worker_handles: Vec<JoinHandle<()>> = Vec::with_capacity(workers.len());

        for worker in workers {
            let receiver_chan = receiver.clone();

            // Spawn worker thread - worker is moved here
            let handle = thread::spawn(move || {
                loop {
                    let task = match receiver_chan.recv() {
                        Ok(name) => name,
                        Err(crossbeam_channel::RecvError) => {
                            // log::info!("W{}: Channel closed and empty, worker exiting", self.id);
                            return;
                        }
                    };

                    // Worker runs until channel is closed or shutdown requested
                    let (list_name, index) = task
                        .split_once("_##")
                        .expect("This task message should not fail");

                    match worker.read_email_by_index(
                        list_name.to_string(),
                        index
                            .parse::<usize>()
                            .expect("Task index should parse into a usize"),
                    ) {
                        Ok(_) => {
                            log::debug!("Worker thread finished");
                        }
                        Err(err) => {
                            log::error!("Worker thread finished with error: {err}");
                        }
                    }
                }
            });
            worker_handles.push(handle);

            // Space out thread creation (to prevent multiple connections opening at once)
            interruptible_sleep(Duration::from_secs(5), &self.shutdown_flag);
        }
        return worker_handles;
    }

    /// Spawns a producer thread for normal (non-range) mode.
    ///
    /// The producer:
    /// 1. Sends all task (list names) to workers via channel
    /// 2. If `loop_groups` is true, sleeps and repeats indefinitely
    /// 3. If `loop_groups` is false, drops sender and exits
    ///
    /// # Arguments
    ///
    /// * `sender` - Channel sender (moved to thread)
    /// * `tasks` - List of mailing list names to send
    /// * `loop_groups` - If true, repeat sending tasks after sleep interval
    ///
    /// # Thread Safety
    ///
    /// The sender is dropped when the thread exits, signaling workers
    /// that no more tasks will arrive.
    fn spawn_producer_for_consume_list(
        sender: crossbeam_channel::Sender<String>,
        tasks: Vec<String>,
        loop_groups: bool,
        shutdown_flag: Arc<AtomicBool>,
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            // if in Looping mode: send tasks, then sleep and repeat
            // otherwise, send and drop the sender
            loop {
                if is_shutdown_requested(&shutdown_flag) {
                    log::info!("Shutdown requested, exiting...");
                    return;
                }
                // Send all tasks
                for task in &tasks {
                    if sender.send(task.clone()).is_err() {
                        log::warn!("Failed to send task, workers may have stopped");
                        drop(sender);
                        return;
                    }
                }

                // If not in looping mode, close sender and exit
                if !loop_groups {
                    log::info!("All tasks sent. Waiting for workers to complete...");
                    drop(sender);
                    return;
                }

                log::debug!("All tasks sent, waiting for rescan interval...");

                // Sleep between rescans
                interruptible_sleep(
                    Duration::from_secs(INTERVAL_BETWEEN_RESCANS as u64),
                    &shutdown_flag,
                );
            }
        })
    }

    /// Spawns a producer thread for range mode.
    ///
    /// The producer:
    /// 1. For each task (list name), parses the range text into an iterator
    /// 2. Sends formatted tasks `"list_name_##index"` for each article index
    /// 3. Drops sender when complete
    ///
    /// # Memory Efficiency
    ///
    /// The range text is parsed fresh for each task (mailing list).
    /// This avoids storing large vectors in memory for big ranges.
    ///
    /// # Arguments
    ///
    /// * `sender` - Channel sender (moved to thread)
    /// * `tasks` - List of mailing list names to process
    /// * `range_text` - Range specification string (e.g., `"1,5,10-15"`)
    ///
    /// # Error Handling
    ///
    /// If range parsing fails, logs an error and drops the sender.
    fn spawn_producer_for_read_email_by_index(
        sender: crossbeam_channel::Sender<String>,
        tasks: Vec<String>,
        range_text: String,
        shutdown_flag: Arc<AtomicBool>,
    ) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            for task in &tasks {
                if is_shutdown_requested(&shutdown_flag) {
                    log::info!("Shutdown requested, exiting...");
                    return;
                }
                // Parse range text for each task (memory efficient, lazy evaluation)
                let range = match parse_sequence(&range_text) {
                    Ok(iter) => iter,
                    Err(e) => {
                        log::error!("Failed to parse range '{}': {}", range_text, e);
                        drop(sender);
                        return;
                    }
                };

                for index in range {
                    if sender.send(format!("{}_##{}", task, index)).is_err() {
                        log::warn!("Failed to send task, workers may have stopped");
                        drop(sender);
                        return;
                    }
                }
            }

            // If not in looping mode, close sender and exit
            log::info!("All tasks sent. Waiting for workers to complete...");
            drop(sender);
            return;
        })
    }
}
