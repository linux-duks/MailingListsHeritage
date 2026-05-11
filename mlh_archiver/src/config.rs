use crate::archive_writer::{self, WriteMode};
use crate::nntp_source::nntp_config;
use crate::public_inbox_source::pi_config;
use crate::{errors::ConfigError, file_utils};
use clap::{Parser, ValueHint};
use config::Config;
use core::fmt;
use glob::glob;
use globset::{Glob, GlobMatcher};
use inquire::MultiSelect;
use std::collections::{HashMap, HashSet};

// The file `built.rs` was placed there by cargo and `build.rs`
pub(crate) mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

/// Main application configuration
///
/// Global settings (nthreads, output_dir, loop_groups) are at the top level.
/// Source-specific settings are nested, and private (e.g., nntp, imap, local, mbox).
/// Their values should be accessed using the [`RunMode`] ENUM.
///
/// The `read_lists` field is a HashMap that stores selected mailing lists per run mode.
///
/// # Example
///
/// ```yaml
/// nthreads: 2
/// output_dir: "./output"
/// loop_groups: true
/// read_lists:
///   NNTP: ["list1", "list2"]
///   PublicInbox: ["list3"]
///
/// nntp:
///   hostname: "nntp.example.com"
///   port: 119
/// ```
#[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq, Clone)]
pub struct AppConfig {
    /// Number of worker threads connecting to different lists
    #[serde(default = "default_nthreads")]
    pub nthreads: u8,

    /// Output directory where results will be stored
    #[serde(default = "default_output_dir")]
    pub output_dir: String,

    /// If true, the app will keep running forever. Otherwise, stop after reading all groups
    #[serde(default = "default_loop_groups")]
    pub loop_groups: bool,

    /// Group lists per run mode: run_mode display name -> list of groups
    #[serde(default)]
    pub read_lists: HashMap<String, Vec<String>>,

    /// NNTP-specific configuration
    pub nntp: Option<nntp_config::NntpConfig>,

    /// PublicInbox configuration
    pub public_inbox: Option<pi_config::PIConfig>,

    /// Archiver Write configuration
    #[serde(default)]
    pub write_mode: archive_writer::WriteMode,
}

/// Represents a source type that can be processed by the archiver.
///
/// Each variant corresponds to a different email source implementation.
/// New sources should add a variant here and implement the corresponding
/// configuration and worker logic.
///
/// # Variants
///
/// * `NNTP` - Network News Transfer Protocol source
/// * `LocalMbox` - Local mbox file source (not yet implemented)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    NNTP,
    PublicInbox,
    LocalMbox,
}

impl fmt::Display for RunMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RunMode::NNTP => write!(f, "nntp"),
            RunMode::PublicInbox => write!(f, "public_inbox"),
            RunMode::LocalMbox => write!(f, "local_mbox"),
        }
    }
}

/// Configuration for a specific [`RunMode`].
///
/// This enum wraps the source-specific configuration structs.
/// Use [`AppConfig::get_run_mode_config()`] to retrieve the config for a given mode.
///
/// # Variants
///
/// * `NNTP(nntp_config::NntpConfig)` - NNTP server configuration
/// * `LocalMbox` - Local mbox configuration (not yet implemented)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunModeConfig {
    NNTP(nntp_config::NntpConfig),
    PublicInbox(pi_config::PIConfig),
    LocalMbox,
}

/// Display implementation for RunModeConfig does not need to provide every field
/// It it used in the data-lineage module to save info about how it was used
impl fmt::Display for RunModeConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RunModeConfig::NNTP(config) => write!(f, "NNTP h={}", config.clone().hostname),
            RunModeConfig::PublicInbox(config) => {
                write!(f, "PublicInbox h={}", config.clone().origin)
            }
            RunModeConfig::LocalMbox => unimplemented!(),
        }
    }
}

/// Here are implemented the functions for config related to the RunMode and its configs
impl AppConfig {
    /// Retrieves the configuration for a specific run mode.
    ///
    /// # Arguments
    ///
    /// * `run_mode` - The run mode to get configuration for
    ///
    /// # Returns
    ///
    /// * `Some(RunModeConfig)` if the configuration exists for the given mode
    /// * `None` if the configuration is not set (e.g., `nntp` field is `None`)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use mlh_archiver::config::{AppConfig, RunMode};
    ///
    /// let config = AppConfig::default();
    /// if let Some(mode_config) = config.get_run_mode_config(RunMode::NNTP) {
    ///     // Access NNTP configuration
    /// }
    /// ```
    pub fn get_run_mode_config(&self, run_mode: RunMode) -> Option<RunModeConfig> {
        match run_mode {
            RunMode::NNTP => Some(RunModeConfig::NNTP(self.nntp.clone()?)),
            RunMode::PublicInbox => Some(RunModeConfig::PublicInbox(self.public_inbox.clone()?)),
            RunMode::LocalMbox => Some(RunModeConfig::LocalMbox),
        }
    }

    /// Retrieves the email range selection text for a specific run mode.
    ///
    /// The range text is a string like `"1,5,10-15"` that specifies which
    /// articles to fetch. It should be parsed by [`crate::range_inputs::parse_sequence()`]
    /// into a lazy iterator.
    ///
    /// # Arguments
    ///
    /// * `run_mode` - The run mode to get range selection for
    ///
    /// # Returns
    ///
    /// * `Some(String)` containing the range text if configured
    /// * `None` if no range is configured or run mode has no config
    pub fn get_range_selection_text(&self, run_mode: RunMode) -> Option<String> {
        match self.get_run_mode_config(run_mode)? {
            RunModeConfig::NNTP(nntp_config) => nntp_config.email_range,
            RunModeConfig::PublicInbox(pi_config) => pi_config.email_range,
            RunModeConfig::LocalMbox => unimplemented!(),
        }
    }

    /// Returns a list of active run modes based on configuration.
    ///
    /// Checks which source configurations are present (e.g., `nntp.is_some()`)
    /// and returns the corresponding run modes.
    ///
    /// # Returns
    ///
    /// A vector of [`RunMode`] variants that have configuration.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use mlh_archiver::config::AppConfig;
    ///
    /// let config = AppConfig::default();
    /// let modes = config.get_run_modes();
    /// // modes will be empty since nntp is None by default
    /// ```
    pub fn get_run_modes(&self) -> Vec<RunMode> {
        let mut run_modes: Vec<RunMode> = vec![];
        if self.nntp.is_some() {
            run_modes.push(RunMode::NNTP);
        }
        if self.public_inbox.is_some() {
            run_modes.push(RunMode::PublicInbox);
        }
        return run_modes;
    }

    /// Retrieves the list selection for a specific run mode from the top-level read_lists HashMap
    fn get_list_selection(&self, run_mode: RunMode) -> Option<Vec<String>> {
        let key = run_mode.to_string();
        self.read_lists.get(&key).cloned()
    }

    /// Saves the list selection for a run mode to the config and persists to file.
    ///
    /// Updates the top-level `read_lists` HashMap and writes the full config
    /// to `archiver_config.yml`.
    fn set_list_selection(
        &mut self,
        run_mode: RunMode,
        list_options: Vec<String>,
    ) -> Result<(), ConfigError> {
        let key = run_mode.to_string();
        self.read_lists.insert(key, list_options);

        let lists = ReadLists {
            read_lists: self.read_lists.clone(),
        };

        // Persist the full config to the default config file
        match file_utils::write_yaml_truncate("archiver_config_selected_lists.yaml", &lists) {
            Ok(_) => {
                log::info!("Saved list selection to archiver_config.yml");
                Ok(())
            }
            Err(e) => Err(ConfigError::Io(e)),
        }
    }
}

/// ReadLists used jut to write back to a file if needed
#[derive(serde::Serialize)]
struct ReadLists {
    read_lists: HashMap<String, Vec<String>>,
}

/// Returns true if the pattern contains glob characters (`*` or `?`).
///
/// Patterns containing these characters are treated as glob patterns
/// and matched against available list names.
pub fn is_glob_pattern(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?')
}

/// Compiles a glob pattern string into a matcher.
///
/// # Arguments
///
/// * `pattern` - Glob pattern string (e.g., `"test.groups.*"`)
///
/// # Returns
///
/// * `Ok(GlobMatcher)` if the pattern is valid
/// * `Err(...)` if the pattern cannot be compiled
fn compile_glob(pattern: &str) -> Result<GlobMatcher, ConfigError> {
    let glob = Glob::new(pattern).map_err(|e| {
        ConfigError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid glob pattern '{}': {}", pattern, e),
        ))
    })?;
    Ok(glob.compile_matcher())
}

/// Expands a list of patterns (some may be globs) against available lists.
///
/// For each pattern:
/// - If it contains `*` or `?`, it's treated as a glob and matched against all available lists
/// - Otherwise, it's treated as an exact match
///
/// Returns a deduplicated, ordered list of matched list names.
/// Also returns a list of patterns that matched nothing (for warning purposes).
pub fn expand_glob_patterns(
    patterns: &[String],
    available_lists: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut matched: Vec<String> = Vec::new();
    let mut unmatched_patterns: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for pattern in patterns {
        if is_glob_pattern(pattern) {
            // Treat as glob pattern
            match compile_glob(pattern) {
                Ok(matcher) => {
                    let mut pattern_matched_any = false;
                    for list_name in available_lists {
                        if matcher.is_match(list_name) && seen.insert(list_name.clone()) {
                            matched.push(list_name.clone());
                            pattern_matched_any = true;
                        }
                    }
                    if !pattern_matched_any {
                        unmatched_patterns.push(pattern.clone());
                    }
                }
                Err(e) => {
                    log::warn!("Failed to compile glob pattern '{}': {}", pattern, e);
                    unmatched_patterns.push(pattern.clone());
                }
            }
        } else {
            // Exact match
            if available_lists.contains(pattern) && seen.insert(pattern.clone()) {
                matched.push(pattern.clone());
            } else {
                unmatched_patterns.push(pattern.clone());
            }
        }
    }

    (matched, unmatched_patterns)
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            nthreads: default_nthreads(),
            output_dir: default_output_dir(),
            loop_groups: default_loop_groups(),
            read_lists: HashMap::new(),
            nntp: None,
            public_inbox: None,
            write_mode: WriteMode::default(),
        }
    }
}

fn default_nthreads() -> u8 {
    1
}

fn default_output_dir() -> String {
    "./output/archiver".to_string()
}

fn default_loop_groups() -> bool {
    true
}

/// Command-line options for the archiver.
///
/// Parsed using `clap` from command-line arguments.
#[derive(Debug, Parser, Default)]
pub struct Opts {
    /// config file location override
    #[arg(short, long, default_value = "archiver_config*", value_hint = ValueHint::FilePath)]
    pub config_file: String,
}

/// Reads and validates configuration from files.
///
/// Searches for configuration files matching the glob pattern `archiver_config*`
/// (or custom pattern via `--config-file`). Supports JSON, YAML, and TOML formats.
///
/// Configuration is layered:
/// 1. Default values
/// 2. Config files found by glob pattern (later files override earlier)
///
/// # Returns
///
/// * `Ok(AppConfig)` if configuration is valid
/// * `Err(ConfigError)` if:
///   - Glob pattern is invalid
///   - Config files cannot be read
///   - Deserialization fails
///
/// # Example
///
/// ```rust,no_run
/// use mlh_archiver::config::read_config;
///
/// let config = read_config().expect("Failed to read config");
/// ```
pub fn read_config() -> Result<AppConfig, ConfigError> {
    let opts = Opts::parse();

    // Collect config files from glob pattern
    let config_files: Vec<_> = glob(&opts.config_file)
        .map_err(|e| {
            ConfigError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Invalid config file glob pattern '{}': {}",
                    opts.config_file, e
                ),
            ))
        })?
        .filter_map(|path_result| match path_result {
            Ok(path) => {
                log::debug!("Found config file: {}", path.display());
                Some(config::File::from(path))
            }
            Err(e) => {
                log::warn!("Error reading config file path: {}", e);
                None
            }
        })
        .collect();

    if config_files.is_empty() {
        log::warn!(
            "No config files found matching pattern: {}",
            opts.config_file
        );
    }

    // Build config with layered sources
    let mut config_builder = Config::builder();

    // Add each config file (highest priority)
    for config_file in config_files {
        config_builder = config_builder.add_source(config_file);
    }

    let config = config_builder.build().map_err(|e| {
        ConfigError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to build config: {}", e),
        ))
    })?;

    log::debug!("Config built: {:?}", config);

    let app_config: AppConfig = config.try_deserialize().map_err(|e| {
        ConfigError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to deserialize config: {}", e),
        ))
    })?;

    log::debug!(
        "Deserialized config: hostname={:?}",
        app_config.nntp.as_ref().map(|n| &n.hostname)
    );

    // return Err(ConfigError::MissingHostname);

    Ok(app_config)
}

impl AppConfig {
    /// Retrieves and validates the list of mailing groups to archive.
    ///
    /// This method handles three scenarios:
    /// 1. **No configuration**: Prompts user via TUI to select lists interactively
    /// 2. **"*" configured**: Returns all available lists from the server
    /// 3. **Specific lists or glob patterns configured**: Expands glob patterns
    ///    against available lists and returns matched lists
    ///
    /// If lists are selected interactively, saves the selection to
    /// `archiver_config_selected_lists.yml` for future runs.
    ///
    /// # Glob Pattern Support
    ///
    /// Patterns containing `*` or `?` are treated as glob patterns:
    ///
    /// - `"*"` — matches all available lists
    /// - `"test.groups.*"` — matches all lists starting with `test.groups.`
    /// - `"*.synth*"` — matches lists containing `.synth` anywhere
    /// - `"list1"` — exact match (no glob characters)
    ///
    /// # Arguments
    ///
    /// * `list_options` - List of available group names from the server
    /// * `run_mode` - The run mode to get list selection for
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<String>)` - Validated list of group names to archive
    /// * `Err(ConfigError::ListSelectionEmpty)` - User selected no lists
    /// * `Err(ConfigError::AllListsUnavailable)` - Configured lists don't exist on server
    /// * `Err(ConfigError::ConfiguredListsNotAvailable)` - Some configured lists invalid
    ///
    /// # Side Effects
    ///
    /// Writes selection to config file if user selects interactively.
    pub fn get_read_lists(
        &mut self,
        list_options: Vec<String>,
        run_mode: RunMode,
    ) -> Result<Vec<String>, ConfigError> {
        let mut answer: Vec<String>;
        let read_lists = self.get_list_selection(run_mode);
        match read_lists {
            None => {
                log::info!("No read_lists defined");

                // list of options provides, with "*" as first
                let mut select_options = vec!["*".to_string()];
                select_options.extend(list_options.clone());

                answer = MultiSelect::new("No groups selected. Select them now:", select_options)
                    .prompt()
                    .unwrap_or_else(|_| std::process::exit(0));

                if answer[0] == "*" {
                    log::info!("All lists selected");
                    log::debug!("Lists selected: {:#?}", list_options);
                    answer = list_options;
                }

                if answer.is_empty() {
                    log::info!("empty selection");
                    return Err(ConfigError::ListSelectionEmpty);
                } else {
                    self.set_list_selection(run_mode, answer.clone())?;
                }
            }
            Some(_) => {
                let mut user_selection = read_lists.expect("is none was validated");
                // If "*" provided, load all lists
                if user_selection.len() == 1 && user_selection[0] == "*" {
                    log::info!("Configured to fetch all lists");
                    log::debug!("Lists selected: {:#?}", list_options);
                    answer = list_options;
                } else {
                    // Expand glob patterns against available lists
                    user_selection.dedup();
                    let (matched, unmatched) = expand_glob_patterns(&user_selection, &list_options);

                    if matched.is_empty() {
                        return Err(ConfigError::AllListsUnavailable);
                    }
                    if !unmatched.is_empty() {
                        log::warn!(
                            "Some lists are unavailable: {}",
                            ConfigError::ConfiguredListsNotAvailable {
                                unavailable_lists: unmatched
                            }
                        );
                    }
                    answer = matched;
                }
            }
        }

        Ok(answer)
    }
}
