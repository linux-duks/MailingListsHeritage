use std::path::{Path, PathBuf};

/// A public-inbox email reader using libgit2.
/// Scans a directory for public-inbox subdirectories and reads the last N emails from each.
use chrono::DateTime;
use clap::Parser;
use inquire::{Confirm, MultiSelect, Select, Text};

use mlh_archiver::public_inbox_source::pi_utils::*;

fn main() {
    let args = Args::parse_from(std::env::args_os());

    // Initialize logging
    let log_level = if args.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    match run(args) {
        Ok(()) => {}
        Err(e) => eprintln!("error: {e}"),
    }
}

#[derive(Debug, clap::Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to directory containing public-inbox directories
    #[arg(short, long)]
    inbox_dir: PathBuf,

    /// Number of recent emails to read per inbox (for quick preview)
    #[arg(short, long, default_value = "5")]
    count: usize,

    /// Export configuration to YAML file after browsing
    #[arg(long = "export-config")]
    export_config: bool,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Run a test fetch of a sample article (non-interactive)
    #[arg(long = "test")]
    test: bool,

    /// Specify list (folder) name for --test or --email-id lookups
    #[arg(long = "list")]
    list_name: Option<String>,

    /// Article number (position) to fetch (requires --test)
    #[arg(long = "article")]
    article: Option<usize>,

    /// Look up and print a single email by its formatted identifier
    /// Format: {10-digit-padded}-e{epoch}-{commit_sha}
    /// Example: 0000000056-e0-5dadd9f0f9884ed3852f090bd05eed898db64966
    #[arg(long = "email-id")]
    email_id: Option<String>,
}

// ── helpers ────────────────────────────────────────────────────────────

/// Returns the epoch repos to process for an inbox.
/// V2 inboxes: returns epochs from git/*.git subdirs.
/// V1 / single-repo: returns a single pseudo-epoch using the git_dir itself.
fn get_epoch_repos(inbox: &PublicInbox) -> anyhow::Result<Vec<EpochRepo>> {
    let epochs = find_epochs(&inbox.git_dir)?;
    if epochs.is_empty() {
        Ok(vec![EpochRepo {
            epoch_name: "all".to_string(),
            git_dir: inbox.git_dir.clone(),
        }])
    } else {
        Ok(epochs)
    }
}

/// Collects all commit OIDs across all epochs, tagged with a global
/// position (0 = newest overall) and the epoch they came from.
fn collect_inbox_commits(
    inbox: &PublicInbox,
) -> anyhow::Result<Vec<(usize, EpochRepo, git2::Oid)>> {
    let epochs = get_epoch_repos(inbox)?;
    let mut all: Vec<(usize, EpochRepo, git2::Oid)> = Vec::new();
    let mut global_pos = 0usize;

    // Epochs are sorted by `find_epochs` (numeric first, "all" last).
    // We iterate in reverse so that the highest-numbered (latest) epoch
    // gets the earliest global positions.
    for epoch in epochs.iter().rev() {
        let repo = git2::Repository::open(&epoch.git_dir)?;
        let commits = collect_all_commits(&repo)
            .map_err(|e| anyhow::anyhow!("count commits in {}: {e}", epoch.epoch_name))?;
        for oid in commits {
            all.push((global_pos, epoch.clone(), oid));
            global_pos += 1;
        }
    }
    Ok(all)
}

/// Reads the last `count` emails from an inbox and prints each one.
fn process_inbox_preview(inbox: &PublicInbox, count: usize) -> anyhow::Result<usize> {
    if inbox.version.contains("incomplete") {
        println!("  Skipping incomplete repository: {}", inbox.version);
        return Ok(0);
    }

    let epochs = get_epoch_repos(inbox)?;
    let commits = collect_inbox_commits(inbox)?;
    println!(
        "  {} epoch(s), {} total commit(s)",
        epochs.len(),
        commits.len()
    );
    let to_process: Vec<_> = commits.iter().take(count).collect();

    let mut email_count = 0;
    for (global_pos, epoch, oid) in to_process {
        let repo = git2::Repository::open(&epoch.git_dir)?;
        let commit = repo.find_commit(*oid)?;
        let author = commit.author();
        let subject = commit.message().unwrap_or("");

        let (commit_hash, raw_email) = match extract_email_from_commit(&repo, &commit) {
            Ok(result) => result,
            Err(_) => continue,
        };

        let timestamp = DateTime::from_timestamp(author.when().seconds(), 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| format!("timestamp={}", author.when().seconds()));

        email_count += 1;
        println!("  --- Email {email_count} (global #{global_pos}) ---");
        println!("  Subject: {}", subject.lines().next().unwrap_or(""));
        println!(
            "  Author:  {} <{}>",
            author.name().unwrap_or(""),
            author.email().unwrap_or("")
        );
        println!("  Date:    {timestamp}");
        println!("  Commit:  {}", commit_hash);
        println!("  Raw email:");
        for line in raw_email.lines() {
            println!("    {line}");
        }
        println!();
    }

    Ok(email_count)
}

/// Fetch a single commit by global position (1-indexed from newest).
fn fetch_single_commit(inbox: &PublicInbox, position: usize) -> anyhow::Result<()> {
    if inbox.version.contains("incomplete") {
        anyhow::bail!("Incomplete repository: {}", inbox.version);
    }

    let commits = collect_inbox_commits(inbox)?;
    if position == 0 || position > commits.len() {
        anyhow::bail!(
            "Position {} out of range (total commits: {})",
            position,
            commits.len()
        );
    }

    let (_, epoch, oid) = &commits[position - 1];
    let repo = git2::Repository::open(&epoch.git_dir)?;
    let commit = repo.find_commit(*oid)?;
    view_commit(&repo, &commit, position)
}

/// Browse commits with pagination across all epochs.
fn browse_inbox(inbox: &PublicInbox) -> anyhow::Result<()> {
    if inbox.version.contains("incomplete") {
        println!("Skipping incomplete repository: {}", inbox.version);
        return Ok(());
    }

    let epochs = get_epoch_repos(inbox)?;
    let commits = collect_inbox_commits(inbox)?;
    let total_commits = commits.len();
    if total_commits == 0 {
        println!("No commits found in this inbox.");
        return Ok(());
    }

    println!(
        "Inbox: {} ({} commits across {} epochs)",
        inbox.name,
        total_commits,
        epochs.len()
    );

    let page_size = 20;
    let mut current_page = 0;
    let total_pages = total_commits.div_ceil(page_size);

    loop {
        let start = current_page * page_size;
        let end = (start + page_size).min(total_commits);
        let page_commits = &commits[start..end];

        let mut commit_details = Vec::new();
        for (i, (_global_pos, epoch, oid)) in page_commits.iter().enumerate() {
            let repo = git2::Repository::open(&epoch.git_dir)?;
            let commit = repo.find_commit(*oid)?;
            let author = commit.author();
            let subject = commit.message().unwrap_or("");
            let subject_preview = subject.lines().next().unwrap_or("").to_string();
            let date = DateTime::from_timestamp(author.when().seconds(), 0)
                .map(|dt| dt.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| format!("timestamp={}", author.when().seconds()));

            commit_details.push(format!(
                "{:4} | {} | e{} | {} | {}",
                start + i + 1,
                date,
                epoch.epoch_name,
                author.name().unwrap_or(""),
                truncate_subject(&subject_preview, 50)
            ));
        }

        println!(
            "\nPage {} of {} (commits {} to {})",
            current_page + 1,
            total_pages,
            start + 1,
            end
        );
        println!("─────────────────────────────────────────────────────────────");
        for detail in &commit_details {
            println!("{}", detail);
        }
        println!("─────────────────────────────────────────────────────────────");

        let mut actions = vec![
            "Select commits on this page",
            "View a single commit by number",
        ];
        if current_page > 0 {
            actions.push("Previous page");
        }
        if current_page < total_pages - 1 {
            actions.push("Next page");
        }
        actions.push("Back to inbox selection");

        let action = Select::new("Choose action:", actions).prompt()?;

        match action {
            "Select commits on this page" => {
                let selections =
                    MultiSelect::new("Select commits to view:", commit_details.clone())
                        .with_help_message("Space to select, Enter to confirm")
                        .prompt()?;
                for selected in selections {
                    if let Some(index) = commit_details.iter().position(|c| c == &selected) {
                        let commit_idx = start + index;
                        let (_pos, epoch, oid) = &commits[commit_idx];
                        let repo = git2::Repository::open(&epoch.git_dir)?;
                        let commit = repo.find_commit(*oid)?;
                        view_commit(&repo, &commit, commit_idx + 1)?;
                    }
                }
            }
            "View a single commit by number" => {
                let commit_num = Text::new("Enter commit number (position):")
                    .with_default(&format!("{}", start + 1))
                    .prompt()?;
                if let Ok(num) = commit_num.parse::<usize>() {
                    if num >= 1 && num <= total_commits {
                        let commit_idx = num - 1;
                        let (_pos, epoch, oid) = &commits[commit_idx];
                        let repo = git2::Repository::open(&epoch.git_dir)?;
                        let commit = repo.find_commit(*oid)?;
                        view_commit(&repo, &commit, num)?;
                    } else {
                        println!(
                            "Invalid commit number. Must be between 1 and {}.",
                            total_commits
                        );
                    }
                } else {
                    println!("Invalid number.");
                }
            }
            "Previous page" => current_page -= 1,
            "Next page" => current_page += 1,
            "Back to inbox selection" => break,
            _ => unreachable!(),
        }
    }

    Ok(())
}

/// View a single commit in detail.
fn view_commit(
    repo: &git2::Repository,
    commit: &git2::Commit,
    position: usize,
) -> anyhow::Result<()> {
    let author = commit.author();
    let subject = commit.message().unwrap_or("");

    let date = DateTime::from_timestamp(author.when().seconds(), 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| format!("timestamp={}", author.when().seconds()));

    println!("\nCommit #{}", position);
    println!("─────────────────────────────────────");
    println!("Subject: {}", subject.lines().next().unwrap_or(""));
    println!(
        "Author:  {} <{}>",
        author.name().unwrap_or(""),
        author.email().unwrap_or("")
    );
    println!("Date:    {}", date);

    let (commit_hash, raw_email) = match extract_email_from_commit(repo, commit) {
        Ok(result) => result,
        Err(_) => {
            println!("No 'm' file found in commit tree");
            println!("─────────────────────────────────────\n");
            return Ok(());
        }
    };

    println!("Commit:  {}", commit_hash);
    println!("─────────────────────────────────────");
    for line in raw_email.lines() {
        println!("{}", line);
    }
    println!("─────────────────────────────────────\n");
    Ok(())
}

fn truncate_subject(subject: &str, max_len: usize) -> String {
    if subject.len() <= max_len {
        subject.to_string()
    } else {
        format!("{}...", &subject[..max_len - 3])
    }
}

// ── test mode ──────────────────────────────────────────────────────────

fn run_test_mode(
    valid_inboxes: &[PublicInbox],
    list_name: Option<&str>,
    article_pos: Option<usize>,
) -> anyhow::Result<()> {
    if valid_inboxes.is_empty() {
        anyhow::bail!("No valid public inboxes available for testing");
    }

    let inbox = if let Some(name) = list_name {
        valid_inboxes
            .iter()
            .find(|inbox| inbox.name == name)
            .ok_or_else(|| anyhow::anyhow!("List '{}' not found", name))?
    } else {
        &valid_inboxes[0]
    };

    let position = article_pos.unwrap_or(1);
    println!(
        "Testing fetch from list '{}', article position {}",
        inbox.name, position
    );
    fetch_single_commit(inbox, position)
}

// ── email-id lookup ────────────────────────────────────────────────────

fn lookup_email_by_id(
    valid_inboxes: &[PublicInbox],
    list_name: &str,
    email_id_str: &str,
) -> anyhow::Result<()> {
    let parsed = parse_email_id(email_id_str)
        .ok_or_else(|| anyhow::anyhow!("Invalid email ID format: {}", email_id_str))?;

    let inbox = valid_inboxes
        .iter()
        .find(|inbox| inbox.name == list_name)
        .ok_or_else(|| anyhow::anyhow!("List '{}' not found", list_name))?;

    let raw_email = find_email_by_id(inbox, &parsed)?;

    print!("{}", raw_email);
    Ok(())
}

// ── config export ──────────────────────────────────────────────────────

fn generate_config_yaml(inboxes: &[PublicInbox], inbox_dir: &Path) -> anyhow::Result<()> {
    println!("\nGenerating configuration for {} inbox(es)", inboxes.len());

    let origin = Text::new("Enter origin identifier:")
        .with_default("public-inbox")
        .prompt()?;

    let email_range = Text::new("Article range (optional, e.g., '1-100'):")
        .with_default("")
        .prompt()?;
    let email_range_str = if email_range.trim().is_empty() {
        None
    } else {
        Some(email_range.trim().to_string())
    };

    let mut yaml = String::new();
    yaml.push_str("# MLH Archiver Configuration - Public Inbox\n");
    yaml.push_str("# Generated by check_git\n\n");
    yaml.push_str("nthreads: 2\n");
    yaml.push_str("output_dir: \"./output/archiver\"\n");
    yaml.push_str("loop_groups: false\n\n");
    yaml.push_str("public_inbox:\n");
    yaml.push_str(&format!(
        "  import_directory: \"{}\"\n",
        inbox_dir.display()
    ));
    yaml.push_str(&format!("  origin: \"{}\"\n", origin));
    yaml.push_str("  read_lists:\n");
    for inbox in inboxes {
        yaml.push_str(&format!("    - \"{}\"\n", inbox.name));
    }
    if let Some(range) = email_range_str {
        yaml.push_str(&format!("  email_range: \"{}\"\n", range));
    }

    println!("\n{}\n", yaml);

    let save = Confirm::new("Save this configuration to archiver_config.yaml?")
        .with_default(false)
        .prompt()?;
    if save {
        match std::fs::write("archiver_config.yaml", yaml) {
            Ok(_) => println!("Configuration saved to archiver_config.yaml"),
            Err(e) => eprintln!("Failed to save configuration: {}", e),
        }
    }

    Ok(())
}

// ── main ───────────────────────────────────────────────────────────────

fn run(args: Args) -> anyhow::Result<()> {
    let inbox_dir = &args.inbox_dir;

    if !inbox_dir.is_dir() {
        anyhow::bail!(
            "Inbox directory does not exist or is not a directory: {}",
            inbox_dir.display()
        );
    }

    let inboxes = find_public_inboxes(inbox_dir)?;
    if inboxes.is_empty() {
        println!(
            "No public-inbox directories found in {}",
            inbox_dir.display()
        );
        return Ok(());
    }

    println!("Found {} public-inbox(es)\n", inboxes.len());
    for inbox in &inboxes {
        println!(
            "  - {} ({}): {}",
            inbox.name,
            inbox.version,
            inbox.git_dir.display()
        );
    }

    let valid_inboxes: Vec<_> = inboxes
        .into_iter()
        .filter(|inbox| !inbox.version.contains("incomplete"))
        .collect();

    if valid_inboxes.is_empty() {
        println!("No valid public-inboxes found.");
        return Ok(());
    }

    if args.test {
        return run_test_mode(&valid_inboxes, args.list_name.as_deref(), args.article);
    }

    if let Some(ref email_id) = args.email_id {
        let list_name = args
            .list_name
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--list is required when using --email-id"))?;
        return lookup_email_by_id(&valid_inboxes, list_name, email_id);
    }

    println!(
        "{} valid public-inbox(es) available for selection.\n",
        valid_inboxes.len()
    );

    let selected = MultiSelect::new("Select mailing lists:", valid_inboxes)
        .with_help_message("Space to select, Enter to confirm")
        .prompt()
        .unwrap_or_else(|_| std::process::exit(0));

    if selected.is_empty() {
        println!("No lists selected. Exiting.");
        return Ok(());
    }

    println!("\nSelected {} inbox(es):", selected.len());
    for inbox in &selected {
        println!("  - {}", inbox.name);
    }
    println!();

    loop {
        let actions = vec![
            "Quick preview (last N emails)",
            "Browse commits interactively",
            "Test fetch a sample article",
            "Generate configuration",
            "Exit",
        ];

        let action = Select::new("What would you like to do?", actions).prompt()?;

        match action {
            "Quick preview (last N emails)" => {
                for inbox in &selected {
                    println!("\nProcessing inbox: {}", inbox.name);
                    println!("  Version: {}", inbox.version);
                    println!("  Git repo: {}", inbox.git_dir.display());

                    match process_inbox_preview(inbox, args.count) {
                        Ok(email_count) => println!("  Read {} email(s)\n", email_count),
                        Err(e) => eprintln!("  Error reading emails: {e}\n"),
                    }
                }
            }
            "Browse commits interactively" => {
                let inbox_names: Vec<String> = selected.iter().map(|i| i.name.clone()).collect();
                let chosen = Select::new("Select inbox to browse:", inbox_names).prompt()?;
                if let Some(inbox) = selected.iter().find(|i| i.name == chosen) {
                    browse_inbox(inbox)?;
                }
            }
            "Test fetch a sample article" => {
                let inbox_names: Vec<String> = selected.iter().map(|i| i.name.clone()).collect();
                let chosen = Select::new("Select inbox to test:", inbox_names).prompt()?;
                if let Some(inbox) = selected.iter().find(|i| i.name == chosen) {
                    let position_str = Text::new("Article position (optional, default 1):")
                        .with_default("1")
                        .prompt()?;
                    let position = position_str.parse::<usize>().unwrap_or(1);
                    fetch_single_commit(inbox, position)?;
                }
            }
            "Generate configuration" => {
                generate_config_yaml(&selected, inbox_dir)?;
            }
            "Exit" => {
                println!("Goodbye!");
                break;
            }
            _ => unreachable!(),
        }
    }

    Ok(())
}
