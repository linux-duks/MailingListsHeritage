use crate::errors;
use std::fmt;
use std::path::{Path, PathBuf};

/// Represents a detected public-inbox directory.
///
/// This struct contains information about a public inbox that has been discovered
/// in the filesystem, including its name, version (V1 or V2), and the path to its
/// underlying git repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicInbox {
    /// Display name of the inbox (typically the directory name)
    pub name: String,
    /// Version of the public inbox format (V1 or V2, with variants)
    pub version: String,
    /// Path to the git repository containing the emails
    pub git_dir: PathBuf,
}

/// Represents a single epoch within a V2 public inbox.
///
/// In V2 public inboxes, emails are organized into epochs (typically numbered
/// directories like 0.git, 1.git, etc.) that represent time-based partitions
/// of the email archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochRepo {
    /// Epoch name derived from directory name (e.g., "0", "1", "all")
    pub epoch_name: String,
    /// Path to the epoch's git repository
    pub git_dir: PathBuf,
}

/// Display implementation for PublicInbox.
///
/// This implementation is used in the data-lineage module to save information
/// about how the inbox was used.
impl fmt::Display for PublicInbox {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Scans the base directory for public-inbox subdirectories.
///
/// This function iterates through all entries in the base directory and attempts
/// to identify which ones are valid public inbox repositories by checking for
/// the characteristic git structure and metadata.
///
/// # Arguments
///
/// * `base_dir` - The directory to scan for public inbox subdirectories
///
/// # Returns
///
/// * `Ok(Vec<PublicInbox>)` - A vector of discovered public inboxes, sorted by name
/// * `Err` - If an I/O error occurs while reading the directory
pub fn find_public_inboxes(base_dir: &Path) -> errors::Result<Vec<PublicInbox>> {
    let mut inboxes = Vec::new();

    for entry in std::fs::read_dir(base_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        // Try to detect if this is a public-inbox directory
        if let Some(inbox) = detect_inbox(&path)? {
            inboxes.push(inbox);
        }
    }

    // Sort by name for consistent output
    inboxes.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(inboxes)
}

/// Checks if a git repository's alternates paths (if any) are accessible.
///
/// Reads `objects/info/alternates` if it exists and verifies that each listed
/// path exists on the filesystem. Logs a debug message for broken alternates
/// but returns true in all cases — git2 can often operate with local objects
/// even when alternates point to unavailable paths.
///
/// # Arguments
///
/// * `dir` - The git repository directory to check
///
/// # Returns
///
/// * `true` — this function no longer gates repo acceptance; it only provides
///   diagnostics. Use `is_git2_openable` to determine usability.
fn log_broken_alternates(dir: &Path) {
    let alternates_path = dir.join("objects/info/alternates");
    if !alternates_path.is_file() {
        return;
    }
    if let Ok(content) = std::fs::read_to_string(&alternates_path) {
        for line in content.lines() {
            let alt = line.trim();
            if !alt.is_empty() && !std::path::Path::new(alt).exists() {
                log::debug!("Repo at {:?} has broken alternates path: {}", dir, alt);
            }
        }
    }
}

/// Attempts to open a directory as a git2 repository to verify it is functional.
///
/// This is a stricter validation than file-based checks. It ensures git2 can
/// actually read the repository, including resolving alternates and pack files.
///
/// # Arguments
///
/// * `dir` - The directory to validate
///
/// # Returns
///
/// * `true` if git2::Repository::open succeeds
/// * `false` if git2::Repository::open fails
fn is_git2_openable(dir: &Path) -> bool {
    git2::Repository::open_bare(dir).is_ok()
}

/// Check if a directory is a git repository (has HEAD and objects).
///
/// This function performs a basic check to determine if a directory is a git
/// repository by verifying the presence of the HEAD file and objects directory.
///
/// # Arguments
///
/// * `dir` - The directory to check
///
/// # Returns
///
/// * `true` if the directory appears to be a git repository
/// * `false` otherwise
fn is_git_repo(dir: &Path) -> bool {
    dir.join("HEAD").is_file() && dir.join("objects").is_dir()
}

/// Check if a git repository has a master ref (either in refs/heads/master or packed-refs).
///
/// This function checks for the existence of a master branch reference, which
/// is necessary for processing emails from the repository. It checks both the
/// standard refs/heads/master file and the packed-refs file for performance.
///
/// # Arguments
///
/// * `dir` - The git repository directory to check
///
/// # Returns
///
/// * `true` if the repository has a master ref
/// * `false` otherwise
fn has_master_ref(dir: &Path) -> bool {
    if dir.join("refs/heads/master").is_file() {
        return true;
    }
    // Check packed-refs for master ref
    if let Ok(content) = std::fs::read_to_string(dir.join("packed-refs")) {
        content.lines().any(|line| {
            let line = line.trim();
            !line.starts_with('#') && line.ends_with(" refs/heads/master")
        })
    } else {
        false
    }
}

/// Finds an epoch repository (git/*.git) that contains the master ref.
///
/// This helper function searches through a git directory's subdirectories for
/// epoch repositories (named like 0.git, 1.git, etc.) that contain a master
/// ref and are valid git repositories.
///
/// # Arguments
///
/// * `git_dir` - The git directory to search for epoch repositories
///
/// # Returns
///
/// * `Ok(Some(PathBuf))` - Path to an epoch repository with master ref
/// * `Ok(None)` - No epoch repository with master ref was found
/// * `Err` - If an I/O error occurs while reading the directory
fn find_epoch_repo_with_master(git_dir: &Path) -> crate::Result<Option<PathBuf>> {
    for entry in std::fs::read_dir(git_dir)? {
        let entry = entry?;
        let epoch_path = entry.path();
        if epoch_path.is_dir() {
            let epoch_name = epoch_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            // Check if it ends with .git (like 0.git) and is a git repo with master ref
            if epoch_name.ends_with(".git")
                && is_git_repo(&epoch_path)
                && has_master_ref(&epoch_path)
                && is_git2_openable(&epoch_path)
            {
                return Ok(Some(epoch_path));
            }
        }
    }
    Ok(None)
}

/// Check if a git repository has any objects (non-empty objects directory).
///
/// This function checks whether a git repository has any objects stored in its
/// objects directory, which indicates whether it contains any commits.
///
/// # Arguments
///
/// * `dir` - The git repository directory to check
///
/// # Returns
///
/// * `true` if the repository has objects
/// * `false` if the objects directory is empty or doesn't exist
fn has_objects(dir: &Path) -> bool {
    let objects_dir = dir.join("objects");
    if !objects_dir.is_dir() {
        return false;
    }
    // Check if objects directory has any files (excluding info/ and pack/)
    match std::fs::read_dir(&objects_dir) {
        Ok(mut entries) => entries.any(|e| e.is_ok()),
        Err(_) => false,
    }
}

/// Detects if a directory is a public-inbox (V1 or V2) and returns its info.
///
/// This function examines a directory to determine if it follows the structure
/// of a public inbox email archive. It supports both V1 (single repository)
/// and V2 (epoch-based) layouts, including various combinations like those
/// using git alternates.
///
/// # Arguments
///
/// * `dir` - The directory to check for public inbox structure
///
/// # Returns
///
/// * `Ok(Some(PublicInbox))` - Information about the detected public inbox
/// * `Ok(None)` - The directory does not appear to be a public inbox
/// * `Err` - If an I/O error occurs while reading files
#[cfg_attr(feature = "otel", tracing::instrument)]
pub fn detect_inbox(dir: &Path) -> crate::Result<Option<PublicInbox>> {
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Check for V2: has git/ directory with numbered epoch repos (git/0.git, git/1.git, etc.)
    // and optionally an all.git that chains them via git alternates.
    let git_dir = dir.join("git");
    if git_dir.is_dir() {
        // Check if there's at least one valid epoch repo to confirm this is a real V2 inbox
        let has_valid_epoch = find_epoch_repo_with_master(&git_dir)?
            .as_ref()
            .map(|r| has_objects(r))
            .unwrap_or(false);

        if has_valid_epoch {
            return Ok(Some(PublicInbox {
                name,
                version: "V2".to_string(),
                git_dir,
            }));
        }

        // If no epoch repo with master found, check for all.git with alternates
        let all_git = dir.join("all.git");
        if all_git.is_dir() && is_git_repo(&all_git) {
            // Check if all.git has alternates pointing to git/*.git
            let alternates_path = all_git.join("objects/info/alternates");
            if alternates_path.is_file() {
                // Try to read alternates to find which epoch repo to use
                if let Ok(alternates_content) = std::fs::read_to_string(&alternates_path) {
                    for line in alternates_content.lines() {
                        let alt_path = line.trim();
                        if alt_path.ends_with("/objects") {
                            // Extract the git repo path (remove /objects suffix)
                            if let Some(parent) = Path::new(alt_path).parent()
                                && parent.is_dir()
                                && is_git_repo(parent)
                                && has_master_ref(parent)
                                && has_objects(parent)
                                && is_git2_openable(parent)
                            {
                                // Found a valid epoch via all.git alternates.
                                // Return git/ directory so find_epochs discovers all epochs.
                                return Ok(Some(PublicInbox {
                                    name,
                                    version: "V2".to_string(),
                                    git_dir,
                                }));
                            }
                        }
                    }
                }
                // If can't find via alternates, check if all.git itself has objects and refs
                if has_objects(&all_git) && has_master_ref(&all_git) {
                    return Ok(Some(PublicInbox {
                        name,
                        version: "V2 (combined)".to_string(),
                        git_dir: all_git,
                    }));
                } else {
                    // Incomplete: has all.git but missing objects or refs
                    return Ok(Some(PublicInbox {
                        name,
                        version: "V2 (incomplete)".to_string(),
                        git_dir: all_git,
                    }));
                }
            } else {
                // all.git without alternates - treat as V1 style
                if has_objects(&all_git) && has_master_ref(&all_git) {
                    return Ok(Some(PublicInbox {
                        name,
                        version: "V1".to_string(),
                        git_dir: all_git,
                    }));
                } else if has_objects(&all_git) {
                    // Has objects but no master ref - might be empty
                    return Ok(Some(PublicInbox {
                        name,
                        version: "V1 (empty)".to_string(),
                        git_dir: all_git,
                    }));
                }
            }
        }
        // git/ directory exists but no valid epoch repos found with master ref.
        // Check structural markers to detect empty/incomplete public-inboxes.
        let has_epoch_dirs = std::fs::read_dir(&git_dir)
            .map(|entries| {
                entries.filter_map(|e| e.ok()).any(|e| {
                    e.path().is_dir()
                        && e.file_name()
                            .to_str()
                            .is_some_and(|n| n.ends_with(".git"))
                        && e.path().join("objects").is_dir()
                })
            })
            .unwrap_or(false);

        if has_epoch_dirs {
            return Ok(Some(PublicInbox {
                name,
                version: "V2 (empty)".to_string(),
                git_dir,
            }));
        }

        // Also check all.git alongside git/ for single-repo public-inbox layouts
        let all_git = dir.join("all.git");
        if all_git.is_dir() && all_git.join("objects").is_dir() {
            return Ok(Some(PublicInbox {
                name,
                version: "V1 (empty)".to_string(),
                git_dir: all_git,
            }));
        }

        return Ok(None);
    }

    // No git/ directory - check for V1 layouts

    // Check for V1: single bare git repo at the inbox directory itself
    // (or an all.git that IS the main repo, not using alternates)
    let all_git = dir.join("all.git");
    if all_git.is_dir() && is_git_repo(&all_git) && has_master_ref(&all_git) {
        return Ok(Some(PublicInbox {
            name,
            version: "V1".to_string(),
            git_dir: all_git,
        }));
    }

    // Also check if the directory itself is a bare git repo with master ref
    if is_git_repo(dir) && has_master_ref(dir) {
        return Ok(Some(PublicInbox {
            name,
            version: "V1 (bare)".to_string(),
            git_dir: dir.to_path_buf(),
        }));
    }

    // Finally, check for all.git without master ref (might be empty repo)
    if all_git.is_dir() && all_git.join("objects").is_dir() {
        // Even without master ref, could be a public-inbox (empty)
        return Ok(Some(PublicInbox {
            name,
            version: "V1 (empty)".to_string(),
            git_dir: all_git,
        }));
    }

    Ok(None)
}

/// Get commit at given position (0-indexed from newest).
///
/// This function retrieves a specific commit from a repository's history,
/// counting from the newest commit (position 0) toward older commits.
///
/// # Arguments
///
/// * `repo` - The git repository to query
/// * `position` - The zero-indexed position from newest (0 = newest commit)
///
/// # Returns
///
/// * `Ok(git2::Oid)` - The object ID of the commit at the specified position
/// * `Err` - If the position is out of bounds or an error occurs during revision walking
pub fn get_commit_at_position(
    repo: &git2::Repository,
    position: usize,
) -> crate::Result<git2::Oid> {
    let _head_id = repo
        .refname_to_id("refs/heads/master")
        .map_err(|_| anyhow::anyhow!("refs/heads/master does not point to an object"))?;

    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;

    for (i, commit_id) in revwalk.flatten().enumerate() {
        if i == position {
            return Ok(commit_id);
        }
    }
    Err(crate::errors::Error::Config(
        crate::errors::ConfigError::MissingHostname,
    ))
}

/// Extract email content from a commit.
///
/// This function extracts the raw email content from a public inbox commit by
/// finding the 'm' blob in the commit's tree, which contains the email message.
///
/// # Arguments
///
/// * `repo` - The git repository containing the commit
/// * `commit` - The commit from which to extract the email
///
/// # Returns
///
/// * `Ok((String, String))` - A tuple of (commit_hash, raw_email_content)
/// * `Err` - If no 'm' blob is found in the commit tree or an error occurs
pub fn extract_email_from_commit(
    repo: &git2::Repository,
    commit: &git2::Commit,
) -> crate::Result<(String, String)> {
    let tree_id = commit.tree_id();
    let tree = repo.find_tree(tree_id)?;

    let blob_oid = tree
        .iter()
        .find(|entry| entry.name() == Ok("m"))
        .map(|entry| entry.id());

    match blob_oid {
        Some(blob_oid) => {
            let raw_body = read_by_blob_id(repo, blob_oid)?;
            Ok((commit.id().to_string(), raw_body))
        }
        None => Err(crate::errors::Error::Config(
            crate::errors::ConfigError::MissingHostname,
        )),
    }
}

/// Read the raw content of a blob by its object ID.
///
/// This function retrieves the raw binary data of a git blob object and converts
/// it to a UTF-8 string, which represents the email content in public inbox
/// repositories.
///
/// # Arguments
///
/// * `repo` - The git repository containing the blob
/// * `blob_oid` - The object ID of the blob to read
///
/// # Returns
///
/// * `Ok(String)` - The raw content of the blob as a UTF-8 string
/// * `Err` - If the blob cannot be found or read
pub fn read_by_blob_id(repo: &git2::Repository, blob_oid: git2::Oid) -> crate::Result<String> {
    let blob = repo.find_blob(blob_oid)?;
    let raw_email = String::from_utf8_lossy(blob.content()).to_string();
    Ok(raw_email)
}

/// Counts the total number of commits in a repository from refs/heads/master.
///
/// This function counts all commits reachable from the master branch ref by
/// performing a revision walk from HEAD and counting each commit encountered.
///
/// # Arguments
///
/// * `repo` - The git repository to count commits in
///
/// # Returns
///
/// * `Ok(usize)` - The total number of commits in the repository
/// * `Err` - If an error occurs during revision walking
#[cfg_attr(feature = "otel", tracing::instrument(skip(repo)))]
pub fn count_commits(repo: &git2::Repository) -> crate::Result<usize> {
    let head_id = repo
        .refname_to_id("refs/heads/master")
        .map_err(|_| anyhow::anyhow!("refs/heads/master does not point to an object"))?;

    let mut revwalk = repo.revwalk()?;
    revwalk.push(head_id)?;

    let count = revwalk.count();

    Ok(count)
}

/// Formats an email ID from its sequential number, epoch name, and commit SHA.
///
/// This function creates a standardized email ID format for public inbox
/// archiving that includes sequential numbering, epoch identification, and
/// a shortened commit SHA for traceability.
///
/// Format: "{padded_id}-e{epoch}-{commit_sha}"
/// Example: "0000000001-e1-d3ed66e"
///
/// # Arguments
///
/// * `email_num` - The sequential email number (will be zero-padded to 10 digits)
/// * `epoch_name` - The name of the epoch (e.g., "0", "1", "all")
/// * `commit_sha` - The full commit SHA (will be shortened to 7 characters)
///
/// # Returns
///
/// * `String` - The formatted email ID
pub fn format_email_id(email_num: usize, epoch_name: &str, commit_sha: &str) -> String {
    let padded = format!("{:010}", email_num);
    format!("{}-e{}-{}", padded, epoch_name, commit_sha)
}

/// Parsed components of a formatted email ID.
///
/// This struct represents the three components that make up a formatted public
/// inbox email ID: the sequential number, epoch name, and shortened commit SHA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEmailId {
    /// The sequential email number (1-indexed)
    pub email_num: usize,
    /// The epoch name (e.g., "0", "1", "all")
    pub epoch_name: String,
    /// The commit SHA
    pub commit_sha: String,
}

/// Parses a formatted email ID back into its components.
///
/// This function reverses the format_email_id function, extracting the
/// sequential number, epoch name, and commit SHA from a formatted email ID.
///
/// Format: "{padded_id}-e{epoch}-{commit_sha}"
///
/// # Arguments
///
/// * `id` - The formatted email ID string to parse
///
/// # Returns
///
/// * `Some(ParsedEmailId)` - The parsed components if the format matches
/// * `None` - If the format doesn't match the expected pattern
pub fn parse_email_id(id: &str) -> Option<ParsedEmailId> {
    let parts: Vec<&str> = id.splitn(3, '-').collect();
    if parts.len() != 3 {
        return None;
    }

    let email_num = parts[0].parse::<usize>().ok()?;

    let epoch_and_sha = parts[1];
    if !epoch_and_sha.starts_with('e') {
        return None;
    }
    let epoch_name = epoch_and_sha[1..].to_string();

    let commit_sha = parts[2].to_string();

    Some(ParsedEmailId {
        email_num,
        epoch_name,
        commit_sha,
    })
}

/// Collects all commit IDs from a repository, ordered from newest to oldest.
///
/// This function performs a revision walk from HEAD (refs/heads/master) and
/// collects the object IDs of all commits in the repository, ordered from
/// newest commit first to oldest commit last.
///
/// # Arguments
///
/// * `repo` - The git repository to collect commits from
///
/// # Returns
///
/// * `Ok(Vec<git2::Oid>)` - A vector of commit object IDs, newest first
/// * `Err` - If an error occurs during revision walking
#[cfg_attr(feature = "otel", tracing::instrument(skip(repo)))]
pub fn collect_all_commits(repo: &git2::Repository) -> crate::Result<Vec<git2::Oid>> {
    let head_id = repo
        .refname_to_id("refs/heads/master")
        .map_err(|_| anyhow::anyhow!("refs/heads/master does not point to an object"))?;

    let mut revwalk = repo.revwalk()?;
    revwalk.push(head_id)?;

    let commits: Vec<_> = revwalk.flatten().collect();

    Ok(commits)
}

/// Look up and retrieve raw email content by its parsed email identifier.
///
/// Given a parsed email identifier and a public inbox, this function locates the
/// correct epoch repository, finds the commit by its full SHA, and extracts the
/// raw RFC 822 email content from the commit's tree.
///
/// # Arguments
///
/// * `inbox` - The public inbox to search in
/// * `email_id` - Parsed components of the email identifier (number, epoch, commit SHA)
///
/// # Returns
///
/// * `Ok(String)` - The raw email content
/// * `Err` - If the epoch is not found, the commit SHA can't be resolved,
///   or the email can't be extracted from the commit
pub fn find_email_by_id(inbox: &PublicInbox, email_id: &ParsedEmailId) -> crate::Result<String> {
    let epochs = find_epochs(&inbox.git_dir)?;

    let epoch_repo = if epochs.is_empty() {
        return Err(anyhow::anyhow!(
            "No epoch repositories found in inbox '{}' (may be empty or incomplete)",
            inbox.name
        )
        .into());
    } else {
        epochs
            .iter()
            .find(|e| e.epoch_name == email_id.epoch_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Epoch '{}' not found in inbox '{}'",
                    email_id.epoch_name,
                    inbox.name
                )
            })?
            .clone()
    };

    let repo = git2::Repository::open_bare(&epoch_repo.git_dir)?;

    let obj = repo.revparse_single(&email_id.commit_sha).map_err(|e| {
        anyhow::anyhow!(
            "Commit SHA '{}' not found in epoch '{}': {}",
            email_id.commit_sha,
            email_id.epoch_name,
            e
        )
    })?;

    let commit = obj.as_commit().ok_or_else(|| {
        anyhow::anyhow!(
            "SHA '{}' resolved to a non-commit object",
            email_id.commit_sha
        )
    })?;

    let (_hash, raw_email) = extract_email_from_commit(&repo, commit)?;
    Ok(raw_email)
}

/// Finds all epoch repositories within a V2 public inbox's git/ directory.
///
/// This function scans the git/ directory of a V2 public inbox for all epoch
/// repositories (directories ending in .git that are valid git repositories
/// with a master ref). It returns them sorted with numeric epochs first,
/// followed by the "all" epoch last.
///
/// # Arguments
///
/// * `git_dir` - The git directory of the public inbox to search
///
/// # Returns
///
/// * `Ok(Vec<EpochRepo>)` - A vector of epoch repositories, sorted appropriately
/// * `Err` - If an I/O error occurs while reading the directory
pub fn find_epochs(git_dir: &Path) -> crate::Result<Vec<EpochRepo>> {
    let mut epochs = Vec::new();

    if !git_dir.is_dir() {
        return Ok(epochs);
    }

    for entry in std::fs::read_dir(git_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if !name.ends_with(".git") {
            continue;
        }

        if !path.join("objects").is_dir() || !has_master_ref(&path) {
            continue;
        }

        if !is_git2_openable(&path) {
            log::debug!(
                "Skipping epoch {} at {:?}: repo not openable by git2",
                name,
                path
            );
            continue;
        }
        // Log diagnostic if alternates are broken but repo is still openable
        log_broken_alternates(&path);

        let epoch_name = name.strip_suffix(".git").unwrap_or(name).to_string();
        epochs.push(EpochRepo {
            epoch_name,
            git_dir: path,
        });
    }

    epochs.sort_by(|a, b| {
        let a_is_all = a.epoch_name == "all";
        let b_is_all = b.epoch_name == "all";
        if a_is_all && !b_is_all {
            return std::cmp::Ordering::Greater;
        }
        if !a_is_all && b_is_all {
            return std::cmp::Ordering::Less;
        }
        // Numeric sort for numeric epoch names
        let a_num = a.epoch_name.parse::<usize>();
        let b_num = b.epoch_name.parse::<usize>();
        match (a_num, b_num) {
            (Ok(a), Ok(b)) => a.cmp(&b),
            _ => a.epoch_name.cmp(&b.epoch_name),
        }
    });

    Ok(epochs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Creates a bare git repo at `path` with the given number of commits.
    /// Each commit stores a fake email in a blob named "m" so that
    /// `extract_email_from_commit` sees valid public-inbox data.
    fn create_test_epoch_repo(path: &Path, commit_count: usize) {
        std::fs::create_dir_all(path).unwrap();
        let _bare = git2::Repository::init_bare(path).unwrap();

        let work_dir = path.parent().unwrap().join("_work");
        std::fs::remove_dir_all(&work_dir).ok();
        let clone = git2::build::RepoBuilder::new()
            .clone(path.to_str().unwrap(), &work_dir)
            .unwrap();

        let sig = git2::Signature::now("Test", "test@test").unwrap();
        for i in 1..=commit_count {
            let email = format!(
                "From: test@test\nSubject: Commit {}\nMessage-ID: <{}@test>\n\nBody {}\n",
                i, i, i
            );
            let m_path = work_dir.join("m");
            std::fs::write(&m_path, &email).unwrap();
            let mut index = clone.index().unwrap();
            index.add_path(Path::new("m")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = clone.find_tree(tree_id).unwrap();
            let parent = clone.head().ok().and_then(|h| h.peel_to_commit().ok());
            let parents = parent.iter().collect::<Vec<_>>();
            clone
                .commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("commit {}", i),
                    &tree,
                    parents.as_slice(),
                )
                .unwrap();
        }

        let mut remote = clone.find_remote("origin").unwrap();
        remote.push(&["HEAD:refs/heads/master"], None).unwrap();
        std::fs::remove_dir_all(&work_dir).ok();
    }

    /// Writes an alternates file pointing to a (usually non-existent) path.
    fn write_alternates(repo_path: &Path, alternates_content: &str) {
        let info_dir = repo_path.join("objects").join("info");
        std::fs::create_dir_all(&info_dir).unwrap();
        std::fs::write(info_dir.join("alternates"), alternates_content).unwrap();
    }

    /// Creates a V2-style directory structure:
    ///   {inbox_dir}/
    ///     git/
    ///       0.git/   (epoch 0)
    ///       1.git/   (epoch 1)
    ///       2.git/   (epoch 2)
    ///
    /// Each epoch repo has the specified commit count.
    fn create_v2_inbox_structure(inbox_dir: &Path, epoch_specs: &[(&str, usize)]) {
        let git_dir = inbox_dir.join("git");
        std::fs::create_dir_all(&git_dir).unwrap();
        for (epoch_name, commit_count) in epoch_specs {
            let repo_path = git_dir.join(format!("{}.git", epoch_name));
            create_test_epoch_repo(&repo_path, *commit_count);
        }
    }

    #[test]
    fn test_find_epochs_with_broken_alternates() {
        let test_dir = std::env::temp_dir().join("pi_test_broken_alternates");
        std::fs::remove_dir_all(&test_dir).ok();
        std::fs::create_dir_all(&test_dir).unwrap();

        create_v2_inbox_structure(&test_dir, &[("0", 3), ("1", 5), ("2", 2)]);

        write_alternates(
            &test_dir.join("git").join("0.git"),
            "/nonexistent/objstore/abc.git/objects\n",
        );
        write_alternates(
            &test_dir.join("git").join("1.git"),
            "/nonexistent/objstore/def.git/objects\n",
        );

        let git_dir = test_dir.join("git");
        let epochs = find_epochs(&git_dir).expect("find_epochs should succeed");

        assert_eq!(
            epochs.len(),
            3,
            "Expected 3 epochs, got {}: {:?}",
            epochs.len(),
            epochs
        );

        let names: Vec<&str> = epochs.iter().map(|e| e.epoch_name.as_str()).collect();
        assert!(names.contains(&"0"), "Epoch 0 should be found");
        assert!(names.contains(&"1"), "Epoch 1 should be found");
        assert!(names.contains(&"2"), "Epoch 2 should be found");

        for epoch in &epochs {
            let repo = git2::Repository::open_bare(&epoch.git_dir).unwrap();
            let count = count_commits(&repo).unwrap();
            let expected = match epoch.epoch_name.as_str() {
                "0" => 3,
                "1" => 5,
                "2" => 2,
                _ => 0,
            };
            assert_eq!(
                count, expected,
                "Epoch {} should have {} commits, got {}",
                epoch.epoch_name, expected, count
            );
        }

        std::fs::remove_dir_all(&test_dir).ok();
    }

    #[test]
    fn test_detect_inbox_v2_with_broken_alternates() {
        let test_dir = std::env::temp_dir().join("pi_test_detect_broken_alt");
        std::fs::remove_dir_all(&test_dir).ok();
        std::fs::create_dir_all(&test_dir).unwrap();

        let inbox_dir = test_dir.join("v2_test.list");
        create_v2_inbox_structure(&inbox_dir, &[("0", 4), ("1", 3)]);

        write_alternates(
            &inbox_dir.join("git").join("0.git"),
            "/nonexistent/objstore/xyz.git/objects\n",
        );
        write_alternates(
            &inbox_dir.join("git").join("1.git"),
            "/nonexistent/objstore/xyz.git/objects\n",
        );

        let inbox = detect_inbox(&inbox_dir)
            .expect("detect_inbox should succeed")
            .expect("should detect a V2 inbox");

        assert_eq!(inbox.name, "v2_test.list");
        assert!(
            inbox.version.contains("V2"),
            "Version should be V2, got: {}",
            inbox.version
        );
        assert!(
            inbox.git_dir.ends_with("git"),
            "git_dir should be the git/ directory, got: {:?}",
            inbox.git_dir
        );

        let epochs = find_epochs(&inbox.git_dir).expect("find_epochs should succeed");
        assert_eq!(epochs.len(), 2, "Should find both epochs: {:?}", epochs);

        std::fs::remove_dir_all(&test_dir).ok();
    }

    #[test]
    fn test_broken_alternates_does_not_affect_commit_extraction() {
        let test_dir = std::env::temp_dir().join("pi_test_commit_extraction");
        std::fs::remove_dir_all(&test_dir).ok();
        std::fs::create_dir_all(&test_dir).unwrap();

        create_v2_inbox_structure(&test_dir, &[("0", 2)]);

        let repo_path = test_dir.join("git").join("0.git");
        write_alternates(&repo_path, "/nonexistent/objstore/deadbeef.git/objects\n");

        let repo =
            git2::Repository::open_bare(&repo_path).expect("should open despite broken alternates");
        let commits = collect_all_commits(&repo).expect("should collect commits");
        assert_eq!(commits.len(), 2, "Should have 2 commits");

        let latest_commit = repo.find_commit(commits[0]).unwrap();
        let (hash, body) =
            extract_email_from_commit(&repo, &latest_commit).expect("should extract email");
        assert!(
            body.contains("Body 2"),
            "Body should contain 'Body 2', got: {}",
            body
        );
        assert!(!hash.is_empty(), "Commit hash should not be empty");

        std::fs::remove_dir_all(&test_dir).ok();
    }
}
