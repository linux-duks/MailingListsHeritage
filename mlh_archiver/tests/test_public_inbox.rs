use std::io;
use std::path::Path;
use std::sync::{Arc, atomic::AtomicBool};
use std::{fs, thread, vec};

use mlh_archiver::archive_writer::WriteMode;
use mlh_archiver::config::{AppConfig, RunMode};
use mlh_archiver::public_inbox_source::pi_config::PIConfig;
use mlh_archiver::public_inbox_source::pi_utils::{self, PublicInbox, parse_email_id};
use mlh_archiver::start;
use testcontainers::{
    GenericBuildableImage, GenericImage, ImageExt, core::Mount, core::WaitFor,
    runners::SyncBuilder, runners::SyncRunner,
};
use walkdir::WalkDir;

fn file_list_dir(path: String) -> Vec<String> {
    let mut file_list = vec![];

    for file in WalkDir::new(path).into_iter().filter_map(|file| file.ok()) {
        println!("{}", file.path().display());
        file_list.push(file.path().display().to_string());
    }

    file_list
}

pub fn check_and_delete_folder(folder_path: String) -> io::Result<()> {
    let p = Path::new(&folder_path);
    if p.exists() {
        println!("Clearing output dir");
        fs::remove_dir_all(&folder_path)?;
    }
    Ok(())
}

/// Pads numeric IDs with zeros to at least 3 digits (e.g., 1 -> "001", 42 -> "042").
fn pad_ids(ids: &[usize]) -> Vec<String> {
    ids.iter().map(|&n| format!("{:0>3}", n)).collect()
}

/// Validates the content of a `__progress.yaml` file.
///
/// Reads the YAML file and verifies:
/// - The file exists and contains `last_email` field
/// - The `last_email` value matches the expected maximum article ID
///   Supports both plain numeric IDs and formatted IDs (e.g., "123-e2-abc")
fn validate_progress_file(path: &str, expected_last_email: usize) {
    let content = fs::read_to_string(path).expect("Progress file should exist");
    assert!(
        content.contains("last_email:"),
        "Progress file should contain 'last_email' field: {}",
        path
    );
    let yaml_value: serde_yaml::Value =
        serde_yaml::from_str(&content).expect("Failed to parse YAML content");
    let last_email_str = yaml_value
        .get("last_email")
        .expect("YAML should have last_email field")
        .as_str()
        .expect("last_email should be a string");

    let actual_email_num = if let Some(parsed) = parse_email_id(last_email_str) {
        parsed.email_num
    } else {
        last_email_str
            .parse()
            .expect("Failed to parse last_email as numeric")
    };

    assert_eq!(
        actual_email_num, expected_last_email,
        "Progress file {} should have email_num={}, got {}",
        path, expected_last_email, actual_email_num
    );
}

/// Validates the content of a `__lineage.yaml` file.
///
/// Reads the multi-document YAML file and verifies:
/// - The file exists and contains expected number of lineage entries
/// - Each entry has: email_index, list_name, source_type, timestamp, archiver_build_info
/// - The email_index values match the expected article IDs (in order)
///   Supports both plain numeric indices and formatted IDs (e.g., "listname/2")
fn validate_lineage_file(path: &str, expected_list_name: &str, expected_email_indices: &[usize]) {
    let content = fs::read_to_string(path).expect("Lineage file should exist");

    assert!(
        content.contains("source_type:"),
        "Lineage file should contain 'source_type' field: {}",
        path
    );
    assert!(
        content.contains("PublicInbox"),
        "Lineage file source_type should contain 'PublicInbox': {}",
        path
    );

    assert!(
        content.contains(expected_list_name),
        "Lineage file should have list_name={}: {}",
        expected_list_name,
        path
    );

    assert!(
        content.contains("timestamp:"),
        "Lineage file should contain 'timestamp' field: {}",
        path
    );

    assert!(
        content.contains("archiver_build_info:"),
        "Lineage file should contain 'archiver_build_info' field: {}",
        path
    );

    let entry_count = content.matches("email_index:").count();
    assert_eq!(
        entry_count,
        expected_email_indices.len(),
        "Lineage file should have {} entries, found {}: {}",
        expected_email_indices.len(),
        entry_count,
        path
    );

    for &email_index in expected_email_indices {
        let search = "email_index:".to_string();
        let mut found = false;
        for line in content.lines() {
            if line.contains(&search) && line.contains(&email_index.to_string()) {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "Lineage file should contain email_index={}: {}",
            email_index, path
        );
    }
}

// =============================================================================
// Expected file list helpers
// =============================================================================

/// Returns the root output directory path as a single-element vector.
fn root_dir(dir: &str) -> Vec<String> {
    vec![dir.to_string()]
}

/// Generates all expected file paths for a single mailing list.
///
/// Always includes:
/// - The list directory
///
/// Conditionally includes:
/// - `__progress.yaml` — if `mail_files` is non-empty (created by `archive_email`)
/// - `__lineage.yaml` — if `mail_files` is non-empty
/// - `{N}.eml` — for each N in `mail_files`
/// - `__errors.csv` — if `has_errors` is true
fn list_entry(dir: &str, list_name: &str, mail_files: &[String], has_errors: bool) -> Vec<String> {
    let mut files = vec![format!("{}/{}", dir, list_name)];

    // Progress and lineage files only exist when at least one article was fetched
    if !mail_files.is_empty() {
        files.push(format!("{}/{}/__progress.yaml", dir, list_name));
        files.push(format!("{}/{}/__lineage.yaml", dir, list_name));
    }

    // Article files
    for n in mail_files {
        files.push(format!("{}/{}/{}.eml", dir, list_name, n));
    }

    // Errors file
    if has_errors {
        files.push(format!("{}/{}/__errors.csv", dir, list_name));
    }

    files
}

/// Validates both progress and lineage files for a mailing list.
///
/// Checks `__progress.yaml` has the expected `last_email` value,
/// and `__lineage.yaml` contains the expected article indices.
/// Skips all validation for empty article lists (no files created).
fn validate_list(dir: &str, list_name: &str, mail_files: &[usize]) {
    if mail_files.is_empty() {
        return;
    }
    let max_article = *mail_files.iter().max().unwrap_or(&0);
    validate_progress_file(
        &format!("{}/{}/__progress.yaml", dir, list_name),
        max_article,
    );
    validate_lineage_file(
        &format!("{}/{}/__lineage.yaml", dir, list_name),
        list_name,
        mail_files,
    );
}

/// Validates that the output directory contains exactly the expected files.
///
/// Compares the actual files found against the expected files generated
/// by `list_entry`. This ensures no extra files and no missing files.
fn validate_exact_file_structure(
    output_dir: &str,
    list_name: &str,
    expected_article_count: usize,
    has_errors: bool,
) {
    let list_dir = format!("{}/{}", output_dir, list_name);
    assert!(
        Path::new(&list_dir).is_dir(),
        "Expected directory missing: {}",
        list_dir
    );

    let actual_eml_count = count_eml_files(&list_dir);
    assert_eq!(
        actual_eml_count, expected_article_count,
        "Expected {} .eml files, found {}",
        expected_article_count, actual_eml_count
    );

    if expected_article_count > 0 {
        let progress_path = format!("{}/__progress.yaml", list_dir);
        let lineage_path = format!("{}/__lineage.yaml", list_dir);
        assert!(
            Path::new(&progress_path).is_file(),
            "Missing: {}",
            progress_path
        );
        assert!(
            Path::new(&lineage_path).is_file(),
            "Missing: {}",
            lineage_path
        );
    }

    if has_errors {
        let errors_path = format!("{}/__errors.csv", list_dir);
        assert!(
            Path::new(&errors_path).is_file(),
            "Missing: {}",
            errors_path
        );
    }

    let actual_files: Vec<String> = WalkDir::new(&list_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.path().display().to_string())
        .collect();

    let expected_file_count = expected_article_count + 2 + if has_errors { 1 } else { 0 };
    assert_eq!(
        actual_files.len(),
        expected_file_count,
        "Expected {} total files, found {}",
        expected_file_count,
        actual_files.len()
    );
}

// =============================================================================
// Container-based test infrastructure
// =============================================================================

/// Builds the test public inbox Docker image.
fn build_test_pi_image() -> GenericImage {
    println!("loading test_public_inbox/Containerfile");
    GenericBuildableImage::new("test_public_inbox", "latest")
        .with_dockerfile("./tests/test_public_inbox/Containerfile")
        .with_file("./tests/test_public_inbox", ".")
        .with_file("./tests/test_public_inbox/public-inbox", "./public-inbox")
        .with_file("./tests/test_nntp_server/fixtures", "./fixtures")
        .build_image()
        .unwrap()
}

/// Returns the host user's (uid, gid) so the copier container can run as
/// the same user, ensuring bind-mounted files have correct ownership.
fn get_host_uid_gid() -> (u32, u32) {
    use std::process::Command;
    let run = |arg: &str| -> u32 {
        String::from_utf8(Command::new("id").arg(arg).output().unwrap().stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    };
    (run("-u"), run("-g"))
}

/// Runs a public inbox archiver test with a custom configuration builder.
/// Returns the list of files created in the output directory.
fn run_pi_test_with_config<F>(
    config_builder: F,
    test_name: &str,
    read_lists: Vec<String>,
) -> Vec<String>
where
    F: FnOnce(&str) -> AppConfig,
{
    let _image = build_test_pi_image();

    let test_data_dir = format!("./test_pi_data_{}", test_name);
    check_and_delete_folder(test_data_dir.clone()).unwrap();
    std::fs::create_dir_all(&test_data_dir).expect("Failed to create test data directory");

    // Resolve absolute path for Docker bind mount
    let abs_test_data_dir = std::fs::canonicalize(&test_data_dir)
        .expect("Failed to resolve absolute path for test data dir");

    // Use a helper container to copy /test-data out via a bind-mounted volume.
    // This avoids depending on the `docker` CLI tool (docker cp / docker exec),
    // using only the Docker API that testcontainers already uses.
    // Run as the host user so bind-mounted files have correct ownership.
    let (host_uid, host_gid) = get_host_uid_gid();
    let copier = GenericImage::new("test_public_inbox", "latest")
        .with_wait_for(WaitFor::message_on_stdout("COPY_DONE"))
        .with_user(format!("{}:{}", host_uid, host_gid))
        .with_cmd(["sh", "-c", "cp -a /test-data/. /output/ && echo COPY_DONE"])
        .with_mount(Mount::bind_mount(
            abs_test_data_dir.to_str().unwrap(),
            "/output",
        ))
        .start()
        .expect("Failed to start data extraction container");

    copier.stop().unwrap();
    copier.rm().unwrap();

    let output_dir = format!("./test_public_inbox_output_pi_{}", test_name);
    check_and_delete_folder(output_dir.clone()).unwrap();

    let test_data_path = test_data_dir.clone();
    let mut app_config = config_builder(&test_data_path);
    app_config.output_dir = output_dir.clone();

    // Set read_lists for PublicInbox run mode
    if !read_lists.is_empty() {
        app_config
            .read_lists
            .insert(RunMode::PublicInbox.to_string(), read_lists);
    }
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let test_name_owned = test_name.to_string();

    let child_handle = thread::spawn(move || {
        println!("Child thread started for {}.", test_name_owned);
        let result = start(&mut app_config, shutdown_flag);
        if let Err(ref e) = result {
            println!("Error in archiver: {:?}", e);
        }
        assert!(result.is_ok());
        println!("Child thread stopped for {}.", test_name_owned);
    });

    println!("waiting server thread to finish for {}", test_name);
    child_handle.join().expect("Child thread panicked");

    // Cleanup test data
    check_and_delete_folder(test_data_dir.clone()).unwrap();

    println!("Loading list of files for {}", test_name);
    file_list_dir(output_dir.clone())
}

// =============================================================================
// Container-based Integration Tests
// =============================================================================

/// Counts .eml files in a directory.
fn count_eml_files(dir: &str) -> usize {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "eml")
                .unwrap_or(false)
        })
        .count()
}

#[test]
fn test_read_from_synthetic_public_inbox() {
    let _found_files = run_pi_test_with_config(
        |test_data_path| AppConfig {
            output_dir: "".to_string(),
            nthreads: 1,
            loop_groups: false,
            write_mode: WriteMode::RawEmails,
            public_inbox: Some(PIConfig {
                import_directory: test_data_path.to_owned(),
                origin: "synthetic".to_owned(),
                public_inbox_config: None,
                email_range: None,
            }),
            ..Default::default()
        },
        "synthetic",
        vec!["v2_test.groups.synthetic".to_string()],
    );

    let output_dir = "./test_public_inbox_output_pi_synthetic";
    let list_dir = format!("{}/v2_test.groups.synthetic", output_dir);

    let eml_count = count_eml_files(&list_dir);
    assert_eq!(eml_count, 12, "Expected 12 .eml files, found {}", eml_count);

    assert!(Path::new(&list_dir).exists());
    assert!(Path::new(&format!("{}/__progress.yaml", list_dir)).exists());
    assert!(Path::new(&format!("{}/__lineage.yaml", list_dir)).exists());

    validate_list(
        output_dir,
        "v2_test.groups.synthetic",
        &(1..=12).collect::<Vec<usize>>(),
    );
    validate_exact_file_structure(output_dir, "v2_test.groups.synthetic", 12, false);

    check_and_delete_folder(output_dir.to_string()).unwrap();
}

#[test]
fn test_pi_email_range() {
    let _found_files = run_pi_test_with_config(
        |test_data_path| AppConfig {
            output_dir: "".to_string(),
            nthreads: 1,
            write_mode: WriteMode::RawEmails,
            loop_groups: false,
            public_inbox: Some(PIConfig {
                import_directory: test_data_path.to_owned(),
                origin: "synthetic".to_owned(),
                public_inbox_config: None,
                email_range: Some("1-3".to_owned()),
            }),
            ..Default::default()
        },
        "range",
        vec!["v2_test.groups.synthetic".to_string()],
    );

    let output_dir = "./test_public_inbox_output_pi_range";
    let list_dir = format!("{}/v2_test.groups.synthetic", output_dir);

    let eml_count = count_eml_files(&list_dir);
    assert_eq!(eml_count, 3, "Expected 3 .eml files, found {}", eml_count);

    validate_list(output_dir, "v2_test.groups.synthetic", &[1, 2, 3]);
    validate_exact_file_structure(output_dir, "v2_test.groups.synthetic", 3, false);

    check_and_delete_folder(output_dir.to_string()).unwrap();
}

// =============================================================================
// Demo-based Integration Tests
// =============================================================================

/// Extract raw email content from a public inbox using git2 directly.
/// Returns a vector of (commit_hash, raw_email) in order from newest to oldest.
fn extract_emails_from_inbox(inbox: &PublicInbox) -> Vec<(String, String)> {
    let repo = git2::Repository::open(&inbox.git_dir).expect("Failed to open git repo");
    let head_ref = repo.head().expect("No HEAD ref");
    let head_id = head_ref.target().expect("HEAD does not point to an object");

    let mut revwalk = repo.revwalk().expect("Failed to create revwalk");
    revwalk.push(head_id).expect("Failed to push head");

    let mut commits = Vec::new();
    for oid in revwalk.flatten() {
        commits.push(oid);
    }

    let mut emails = Vec::new();
    for commit_id in commits {
        let commit = repo.find_commit(commit_id).expect("find commit");
        let tree_id = commit.tree_id();
        let tree = repo.find_tree(tree_id).expect("find tree");

        let blob_oid = tree.iter().find(|e| e.name() == Ok("m")).map(|e| e.id());

        match blob_oid {
            Some(blob_oid) => {
                let blob = repo.find_blob(blob_oid).expect("find blob");
                let raw_email = String::from_utf8_lossy(blob.content()).to_string();
                emails.push((commit_id.to_string(), raw_email));
            }
            None => {
                // Skip commits without m blob (should not happen in valid public inbox)
                println!("Warning: commit {} missing 'm' blob", commit_id);
            }
        }
    }

    emails
}

#[test]
fn test_read_from_demo_public_inbox() {
    let demo_dir = Path::new("../../public-inbox-demo");
    if !demo_dir.exists() {
        println!("Demo directory not found, skipping test");
        return;
    }

    // Find all public inboxes in demo directory
    let inboxes = pi_utils::find_public_inboxes(demo_dir).expect("Failed to find inboxes");
    assert!(
        !inboxes.is_empty(),
        "No public inboxes found in demo directory"
    );

    // Pick the first inbox for testing
    let inbox = &inboxes[0];
    println!(
        "Testing with inbox: {} (version: {})",
        inbox.name, inbox.version
    );

    // Extract expected emails
    let expected_emails = extract_emails_from_inbox(inbox);
    assert!(!expected_emails.is_empty(), "Inbox has no emails");

    // Create temporary output directory
    let output_dir = "./test_public_inbox_output_demo".to_owned();
    check_and_delete_folder(output_dir.clone()).unwrap();

    // Configure archiver for this single inbox
    let mut read_lists = std::collections::HashMap::new();
    read_lists.insert(RunMode::PublicInbox.to_string(), vec![inbox.name.clone()]);

    let mut app_config = AppConfig {
        output_dir: output_dir.clone(),
        nthreads: 1,
        write_mode: WriteMode::RawEmails,
        loop_groups: false,
        read_lists,
        public_inbox: Some(PIConfig {
            import_directory: demo_dir.to_string_lossy().to_string(),
            origin: "demo".to_owned(),
            public_inbox_config: None,
            email_range: None,
        }),
        ..Default::default()
    };

    println!("Starting archiver with public inbox source");

    let shutdown_flag = Arc::new(AtomicBool::new(false));

    let child_handle = thread::spawn(move || {
        println!("Child thread started.");
        let result = start(&mut app_config, shutdown_flag);
        assert!(result.is_ok());
        println!("Child thread stopped.");
    });

    child_handle.join().expect("Child thread panicked");

    // Validate output
    let list_output_dir = format!("{}/{}", output_dir, inbox.name);
    assert!(
        Path::new(&list_output_dir).exists(),
        "Output directory for inbox not created"
    );

    // Check that we have the right number of email files
    let found_files: Vec<String> = WalkDir::new(&list_output_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "eml")
                .unwrap_or(false)
        })
        .map(|e| e.path().display().to_string())
        .collect();

    assert_eq!(
        found_files.len(),
        expected_emails.len(),
        "Expected {} .eml files, found {}",
        expected_emails.len(),
        found_files.len()
    );

    // Build a map of email files: parse filename to get article number, then verify content
    let mut email_files: Vec<(usize, String)> = Vec::new();
    for file_path in &found_files {
        let filename = Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("Invalid filename");

        if let Some(parsed) = parse_email_id(filename) {
            email_files.push((parsed.email_num, file_path.clone()));
        } else {
            panic!("Unexpected email filename format: {}", filename);
        }
    }

    email_files.sort_by_key(|(num, _)| *num);

    assert_eq!(
        email_files.len(),
        expected_emails.len(),
        "Could not parse all email filenames"
    );

    // Verify content matches expected for each article
    for (position, (_expected_commit_hash, expected_raw_email)) in
        expected_emails.iter().enumerate()
    {
        let article_num = position + 1; // 1-indexed
        let (_, email_file) = email_files
            .iter()
            .find(|(num, _)| *num == article_num)
            .unwrap_or_else(|| panic!("Email file for article {} not found", article_num));

        let actual_content = fs::read_to_string(email_file).expect("Failed to read email file");
        assert_eq!(
            actual_content.trim_end(),
            expected_raw_email.trim_end(),
            "Email content mismatch for article {}",
            article_num
        );
    }

    let article_nums: Vec<usize> = (1..=expected_emails.len()).collect();
    validate_list(&output_dir, &inbox.name, &article_nums);

    // Cleanup
    check_and_delete_folder(output_dir).unwrap();

    println!("Test passed for inbox {}", inbox.name);
}

/// Test email range functionality
#[test]
fn test_read_email_range_from_demo() {
    let demo_dir = Path::new("../../public-inbox-demo");
    if !demo_dir.exists() {
        println!("Demo directory not found, skipping test");
        return;
    }

    let inboxes = pi_utils::find_public_inboxes(demo_dir).expect("Failed to find inboxes");
    if inboxes.is_empty() {
        println!("No inboxes found, skipping test");
        return;
    }

    let inbox = &inboxes[0];
    let expected_emails = extract_emails_from_inbox(inbox);
    if expected_emails.len() < 3 {
        println!("Inbox has less than 3 emails, skipping range test");
        return;
    }

    // Test fetching only article 2 (second newest)
    let output_dir = "./test_public_inbox_output_range".to_owned();
    check_and_delete_folder(output_dir.clone()).unwrap();

    let mut read_lists = std::collections::HashMap::new();
    read_lists.insert(RunMode::PublicInbox.to_string(), vec![inbox.name.clone()]);

    let mut app_config = AppConfig {
        output_dir: output_dir.clone(),
        nthreads: 1,
        write_mode: WriteMode::RawEmails,
        loop_groups: false,
        read_lists,
        public_inbox: Some(PIConfig {
            import_directory: demo_dir.to_string_lossy().to_string(),
            origin: "demo".to_owned(),
            public_inbox_config: None,
            email_range: Some("2".to_owned()), // Only article 2
        }),
        ..Default::default()
    };

    let shutdown_flag = Arc::new(AtomicBool::new(false));

    let child_handle = thread::spawn(move || {
        let result = start(&mut app_config, shutdown_flag);
        assert!(result.is_ok());
    });

    child_handle.join().expect("Child thread panicked");

    // Should have only article 2
    let list_output_dir = format!("{}/{}", output_dir, inbox.name);

    // Find all .eml files and parse their article numbers
    let eml_files: Vec<usize> = WalkDir::new(&list_output_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "eml")
                .unwrap_or(false)
        })
        .filter_map(|e| {
            let filename = e.path().file_stem().and_then(|s| s.to_str())?;
            parse_email_id(filename).map(|p| p.email_num)
        })
        .collect();

    assert!(!eml_files.contains(&1), "Article 1 should not be fetched");
    assert!(eml_files.contains(&2), "Article 2 should be fetched");
    assert!(!eml_files.contains(&3), "Article 3 should not be fetched");

    // Verify content matches expected email at position 1 (0-indexed)
    let expected_email = &expected_emails[1].1;
    let email_file = WalkDir::new(&list_output_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "eml")
                .unwrap_or(false)
        })
        .find(|e| {
            let filename = e.path().file_stem().and_then(|s| s.to_str()).unwrap_or("");
            parse_email_id(filename)
                .map(|p| p.email_num == 2)
                .unwrap_or(false)
        })
        .expect("Article 2 file not found");

    let actual_content = fs::read_to_string(email_file.path()).unwrap();
    assert_eq!(actual_content.trim_end(), expected_email.trim_end());

    check_and_delete_folder(output_dir).unwrap();
}

#[test]
fn test_validate_file_structure_using_helpers() {
    let demo_dir = Path::new("../../public-inbox-demo");
    if !demo_dir.exists() {
        println!("Demo directory not found, skipping test");
        return;
    }

    let inboxes = pi_utils::find_public_inboxes(demo_dir).expect("Failed to find inboxes");
    if inboxes.is_empty() {
        println!("No inboxes found, skipping test");
        return;
    }

    let inbox = &inboxes[0];
    let expected_emails = extract_emails_from_inbox(inbox);
    if expected_emails.is_empty() {
        println!("Inbox has no emails, skipping test");
        return;
    }

    let output_dir = "./test_public_inbox_output_structure".to_owned();
    check_and_delete_folder(output_dir.clone()).unwrap();

    let mut read_lists = std::collections::HashMap::new();
    read_lists.insert(RunMode::PublicInbox.to_string(), vec![inbox.name.clone()]);

    let mut app_config = AppConfig {
        output_dir: output_dir.clone(),
        nthreads: 1,
        write_mode: WriteMode::RawEmails,
        loop_groups: false,
        read_lists,
        public_inbox: Some(PIConfig {
            import_directory: demo_dir.to_string_lossy().to_string(),
            origin: "demo".to_owned(),
            public_inbox_config: None,
            email_range: None,
        }),
        ..Default::default()
    };

    let shutdown_flag = Arc::new(AtomicBool::new(false));

    let child_handle = thread::spawn(move || {
        let result = start(&mut app_config, shutdown_flag);
        assert!(result.is_ok());
    });

    child_handle.join().expect("Child thread panicked");

    let article_nums: Vec<usize> = (1..=expected_emails.len()).collect();

    let root = root_dir(&output_dir);
    assert!(!root.is_empty(), "root_dir should return output dir");

    let expected_files = list_entry(&output_dir, &inbox.name, &pad_ids(&article_nums), false);
    assert!(
        !expected_files.is_empty(),
        "list_entry should generate expected files"
    );

    validate_list(&output_dir, &inbox.name, &article_nums);

    validate_exact_file_structure(&output_dir, &inbox.name, article_nums.len(), false);

    check_and_delete_folder(output_dir).unwrap();

    println!("test_validate_file_structure_using_helpers passed");
}

// =============================================================================
// Multi-Epoch Integration Tests
// =============================================================================

#[test]
fn test_multi_epoch_all_emails() {
    let _found_files = run_pi_test_with_config(
        |test_data_path| AppConfig {
            output_dir: "".to_string(),
            nthreads: 1,
            write_mode: WriteMode::RawEmails,
            loop_groups: false,
            public_inbox: Some(PIConfig {
                import_directory: test_data_path.to_owned(),
                origin: "synthetic".to_owned(),
                public_inbox_config: None,
                email_range: None,
            }),
            ..Default::default()
        },
        "multi_epoch_all",
        vec!["v2_multi_epoch.list".to_string()],
    );

    let output_dir = "./test_public_inbox_output_pi_multi_epoch_all";
    let list_dir = format!("{}/v2_multi_epoch.list", output_dir);

    let eml_count = count_eml_files(&list_dir);
    assert_eq!(eml_count, 12, "Expected 12 .eml files, found {}", eml_count);

    assert!(Path::new(&list_dir).exists());
    assert!(Path::new(&format!("{}/__progress.yaml", list_dir)).exists());
    assert!(Path::new(&format!("{}/__lineage.yaml", list_dir)).exists());

    validate_list(
        output_dir,
        "v2_multi_epoch.list",
        &(1..=12).collect::<Vec<usize>>(),
    );
    validate_exact_file_structure(output_dir, "v2_multi_epoch.list", 12, false);

    check_and_delete_folder(output_dir.to_string()).unwrap();
}

#[test]
fn test_multi_epoch_email_range() {
    let _found_files = run_pi_test_with_config(
        |test_data_path| AppConfig {
            output_dir: "".to_string(),
            nthreads: 1,
            write_mode: WriteMode::RawEmails,
            loop_groups: false,
            public_inbox: Some(PIConfig {
                import_directory: test_data_path.to_owned(),
                origin: "synthetic".to_owned(),
                public_inbox_config: None,
                email_range: Some("5-8".to_owned()), // All in epoch 1
            }),
            ..Default::default()
        },
        "multi_epoch_range",
        vec!["v2_multi_epoch.list".to_string()],
    );

    let output_dir = "./test_public_inbox_output_pi_multi_epoch_range";
    let list_dir = format!("{}/v2_multi_epoch.list", output_dir);

    let eml_count = count_eml_files(&list_dir);
    assert_eq!(
        eml_count, 4,
        "Expected 4 .eml files (5-8 inclusive), found {}",
        eml_count
    );

    validate_list(output_dir, "v2_multi_epoch.list", &[5, 6, 7, 8]);
    validate_exact_file_structure(output_dir, "v2_multi_epoch.list", 4, false);

    check_and_delete_folder(output_dir.to_string()).unwrap();
}

#[test]
fn test_multi_epoch_resume() {
    let read_lists = vec!["v2_multi_epoch.list".to_string()];

    // Phase 1: Process first 4 emails (epoch 0)
    let _found_files1 = run_pi_test_with_config(
        |test_data_path| AppConfig {
            output_dir: "./test_public_inbox_output_pi_multi_epoch_resume_phase1".to_string(),
            nthreads: 1,
            write_mode: WriteMode::RawEmails,
            loop_groups: false,
            public_inbox: Some(PIConfig {
                import_directory: test_data_path.to_owned(),
                origin: "synthetic".to_owned(),
                public_inbox_config: None,
                email_range: Some("1-4".to_owned()), // Only first 4 emails
            }),
            ..Default::default()
        },
        "multi_epoch_resume_phase1",
        read_lists.clone(),
    );

    // Phase 2: Resume without range (should start from epoch 1)
    let _found_files2 = run_pi_test_with_config(
        |test_data_path| AppConfig {
            output_dir: "./test_public_inbox_output_pi_multi_epoch_resume_phase2".to_string(),
            nthreads: 1,
            write_mode: WriteMode::RawEmails,
            loop_groups: false,
            public_inbox: Some(PIConfig {
                import_directory: test_data_path.to_owned(),
                origin: "synthetic".to_owned(),
                public_inbox_config: None,
                email_range: None, // No range - should resume
            }),
            ..Default::default()
        },
        "multi_epoch_resume_phase2",
        read_lists.clone(),
    );

    // Verify all 12 emails are now present
    let list_dir = "./test_public_inbox_output_pi_multi_epoch_resume_phase2/v2_multi_epoch.list";
    let eml_count = count_eml_files(list_dir);
    assert_eq!(
        eml_count, 12,
        "Expected 12 .eml files after resume, found {}",
        eml_count
    );

    validate_list(
        "./test_public_inbox_output_pi_multi_epoch_resume_phase2",
        "v2_multi_epoch.list",
        &(1..=12).collect::<Vec<usize>>(),
    );
    validate_exact_file_structure(
        "./test_public_inbox_output_pi_multi_epoch_resume_phase2",
        "v2_multi_epoch.list",
        12,
        false,
    );

    // Cleanup
    check_and_delete_folder("./test_public_inbox_output_pi_multi_epoch_resume_phase1".to_string())
        .unwrap();
    check_and_delete_folder("./test_public_inbox_output_pi_multi_epoch_resume_phase2".to_string())
        .unwrap();
}

// =============================================================================
// Resume Integration Tests (local git repo, no container)
// =============================================================================

/// Creates a bare git repo (V1-style public inbox) with the given number of emails.
fn create_v1_inbox_with_emails(inbox_dir: &str, inbox_name: &str, email_count: usize) {
    std::fs::create_dir_all(inbox_dir).expect("create inbox_dir");
    let abs_inbox_dir = std::fs::canonicalize(inbox_dir).expect("canonicalize inbox_dir");
    let inbox_path = abs_inbox_dir.join(inbox_name);
    std::fs::create_dir_all(&inbox_path).expect("create inbox_path");

    // Create a bare repo
    let bare_repo = git2::Repository::init_bare(&inbox_path).expect("init bare repo");
    // Ensure HEAD points to refs/heads/master regardless of git's init.defaultBranch
    std::fs::write(inbox_path.join("HEAD"), "ref: refs/heads/master\n").expect("write HEAD");

    // Create a temporary working repo to make commits
    let work_dir = abs_inbox_dir.join("work");
    std::fs::remove_dir_all(&work_dir).ok();
    let clone = git2::build::RepoBuilder::new()
        .clone(inbox_path.to_str().unwrap(), &work_dir)
        .expect("clone bare repo");

    let sig = git2::Signature::now("Test User", "test@example.com").expect("create sig");

    for i in 1..=email_count {
        let email_content = format!(
            "From: tester@example.org\n\
             Subject: Test email {}\n\
             Message-ID: <test-{}@example.org>\n\
             Date: Mon, 01 Jan 2024 00:{:02}:00 +0000\n\
             \n\
             This is test email number {}.\n",
            i,
            i,
            i % 60,
            i
        );

        let m_path = work_dir.join("m");
        std::fs::write(&m_path, &email_content).expect("write email");

        let mut index = clone.index().expect("get index");
        index.add_path(Path::new("m")).expect("add path");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = clone.find_tree(tree_id).expect("find tree");

        let parent_commit = clone.head().ok().and_then(|h| h.peel_to_commit().ok());

        if let Some(parent) = parent_commit {
            clone
                .commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("email {}", i),
                    &tree,
                    &[&parent],
                )
                .expect("commit");
        } else {
            clone
                .commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("email {}", i),
                    &tree,
                    &[],
                )
                .expect("commit");
        }
    }

    // Ensure refs/heads/master points to HEAD
    let head_commit = clone
        .head()
        .expect("get HEAD")
        .peel_to_commit()
        .expect("peel to commit");
    clone
        .reference("refs/heads/master", head_commit.id(), true, "update master")
        .expect("create master ref");

    // Push to bare repo
    let mut remote = clone.find_remote("origin").expect("find remote");
    remote
        .push(&["refs/heads/master:refs/heads/master"], None)
        .expect("push");

    std::fs::remove_dir_all(&work_dir).ok();
    drop(bare_repo);
}

/// Adds new emails to an existing bare git repo inbox.
fn add_emails_to_inbox(inbox_dir: &str, inbox_name: &str, start_num: usize, count: usize) {
    let abs_inbox_dir = std::fs::canonicalize(inbox_dir).expect("canonicalize inbox_dir");
    let inbox_path = abs_inbox_dir.join(inbox_name);
    let work_dir = abs_inbox_dir.join("work_add");
    std::fs::remove_dir_all(&work_dir).ok();

    let clone = git2::build::RepoBuilder::new()
        .clone(inbox_path.to_str().unwrap(), &work_dir)
        .expect("clone bare repo");

    let sig = git2::Signature::now("Test User", "test@example.com").expect("create sig");

    for i in start_num..start_num + count {
        let email_content = format!(
            "From: tester@example.org\n\
             Subject: Test email {}\n\
             Message-ID: <test-{}@example.org>\n\
             Date: Mon, 01 Jan 2024 01:{:02}:00 +0000\n\
             \n\
             This is test email number {} (new).\n",
            i,
            i,
            i % 60,
            i
        );

        let m_path = work_dir.join("m");
        std::fs::write(&m_path, &email_content).expect("write email");

        let mut index = clone.index().expect("get index");
        index.add_path(Path::new("m")).expect("add path");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = clone.find_tree(tree_id).expect("find tree");

        let parent_commit = clone.head().ok().and_then(|h| h.peel_to_commit().ok());

        if let Some(parent) = parent_commit {
            clone
                .commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("email {}", i),
                    &tree,
                    &[&parent],
                )
                .expect("commit");
        }
    }

    // Ensure refs/heads/master points to HEAD
    let head_commit = clone
        .head()
        .expect("get HEAD")
        .peel_to_commit()
        .expect("peel to commit");
    clone
        .reference("refs/heads/master", head_commit.id(), true, "update master")
        .expect("create master ref");

    let mut remote = clone.find_remote("origin").expect("find remote");
    remote
        .push(&["refs/heads/master:refs/heads/master"], None)
        .expect("push");

    std::fs::remove_dir_all(&work_dir).ok();
}

/// Runs the archiver once against a local inbox directory.
fn run_pi_archiver_once(inbox_dir: &str, output_dir: &str, read_lists: Vec<String>) {
    let abs_inbox = std::fs::canonicalize(inbox_dir).expect("canonicalize inbox_dir");
    let abs_output = std::fs::canonicalize(output_dir).unwrap_or_else(|_| {
        std::fs::create_dir_all(output_dir).expect("create output_dir");
        std::fs::canonicalize(output_dir).expect("canonicalize output_dir")
    });

    // Build read_lists HashMap from the parameter
    let mut read_lists_map = std::collections::HashMap::new();
    if !read_lists.is_empty() {
        read_lists_map.insert(RunMode::PublicInbox.to_string(), read_lists);
    }

    let mut app_config = AppConfig {
        output_dir: abs_output.to_string_lossy().to_string(),
        nthreads: 1,
        write_mode: WriteMode::RawEmails,
        loop_groups: false,
        read_lists: read_lists_map,
        public_inbox: Some(PIConfig {
            import_directory: abs_inbox.to_string_lossy().to_string(),
            origin: "local-test".to_owned(),
            public_inbox_config: None,
            email_range: None,
        }),
        ..Default::default()
    };

    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let child_handle = thread::spawn(move || {
        let result = start(&mut app_config, shutdown_flag);
        if let Err(ref e) = result {
            eprintln!("Archiver error: {:?}", e);
        }
        assert!(result.is_ok(), "Archiver should succeed");
    });
    child_handle.join().expect("Child thread panicked");
}

#[test]
fn test_resume_only_collects_new_emails() {
    let base_dir = "./test_resume_data";
    let inbox_name = "test.resume.list";
    let inbox_dir = format!("{}/{}", base_dir, inbox_name);
    let output_dir = "./test_resume_output";

    check_and_delete_folder(base_dir.to_string()).unwrap();
    check_and_delete_folder(output_dir.to_string()).unwrap();
    std::fs::create_dir_all(&inbox_dir).expect("create inbox_dir");

    // Phase 1: Create inbox with 10 emails, run archiver
    // create_v1_inbox_with_emails creates repo at inbox_dir/inbox_name, so pass base_dir as inbox_dir
    create_v1_inbox_with_emails(base_dir, inbox_name, 10);
    run_pi_archiver_once(
        base_dir, // import_directory should be parent dir containing inboxes
        output_dir,
        vec![inbox_name.to_string()],
    );

    let list_dir = format!("{}/{}", output_dir, inbox_name);
    let eml_count_1 = count_eml_files(&list_dir);
    assert_eq!(
        eml_count_1, 10,
        "Phase 1: Expected 10 .eml files, found {}",
        eml_count_1
    );
    validate_progress_file(&format!("{}/__progress.yaml", list_dir), 10);

    // Phase 2: Add 5 more emails (11-15), run archiver again
    add_emails_to_inbox(base_dir, inbox_name, 11, 5);
    run_pi_archiver_once(base_dir, output_dir, vec![inbox_name.to_string()]);

    let eml_count_2 = count_eml_files(&list_dir);
    assert_eq!(
        eml_count_2, 15,
        "Phase 2: Expected 15 total .eml files, found {}",
        eml_count_2
    );
    validate_progress_file(&format!("{}/__progress.yaml", list_dir), 15);

    // Verify new emails (11-15) exist
    for i in 11..=15 {
        let found = WalkDir::new(&list_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .any(|e| {
                let filename = e.path().file_stem().and_then(|s| s.to_str()).unwrap_or("");
                parse_email_id(filename)
                    .map(|p| p.email_num == i)
                    .unwrap_or(false)
            });
        assert!(found, "Phase 2: Email {} should exist after resume", i);
    }

    // Verify original emails (1-10) still exist
    for i in 1..=10 {
        let found = WalkDir::new(&list_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .any(|e| {
                let filename = e.path().file_stem().and_then(|s| s.to_str()).unwrap_or("");
                parse_email_id(filename)
                    .map(|p| p.email_num == i)
                    .unwrap_or(false)
            });
        assert!(found, "Phase 2: Original email {} should still exist", i);
    }

    check_and_delete_folder(base_dir.to_string()).ok();
    check_and_delete_folder(output_dir.to_string()).ok();
}

// =============================================================================
// Overwrite prevention: restart after complete run should not modify files
// =============================================================================

/// Counts files with a given extension in a directory.
fn count_files_by_ext(dir: &str, ext: &str) -> usize {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| e.path().extension().map(|ex| ex == ext).unwrap_or(false))
        .count()
}

/// Records file paths and content snapshots for later comparison.
fn snapshot_files(dir: &str) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| {
            let path = e.path().display().to_string();
            let content = fs::read_to_string(e.path()).unwrap_or_default();
            (path, content)
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

#[test]
fn test_no_overwrite_on_restart_raw() {
    let base_dir = "./test_no_overwrite_raw_data";
    let inbox_name = "test.nooverwrite.list";
    let output_dir = "./test_no_overwrite_raw_output";

    check_and_delete_folder(base_dir.to_string()).unwrap();
    check_and_delete_folder(output_dir.to_string()).unwrap();
    std::fs::create_dir_all(base_dir).expect("create base_dir");

    // Phase 1: Create inbox with 5 emails, run archiver
    create_v1_inbox_with_emails(base_dir, inbox_name, 5);
    run_pi_archiver_once(base_dir, output_dir, vec![inbox_name.to_string()]);

    let list_dir = format!("{}/{}", output_dir, inbox_name);
    let first_count = count_eml_files(&list_dir);
    assert_eq!(first_count, 5, "Phase 1: expected 5 .eml files");
    validate_progress_file(&format!("{}/__progress.yaml", list_dir), 5);

    // Snapshot files after phase 1
    let snapshot = snapshot_files(&list_dir);

    // Phase 2: Run archiver again (no new emails, same inbox)
    run_pi_archiver_once(base_dir, output_dir, vec![inbox_name.to_string()]);

    // Verify: same number of .eml files (no new files)
    let second_count = count_eml_files(&list_dir);
    assert_eq!(
        second_count, 5,
        "Phase 2: expected 5 .eml files, found {} (no re-processing)",
        second_count
    );

    // Verify: progress file unchanged
    validate_progress_file(&format!("{}/__progress.yaml", list_dir), 5);

    // Verify: all files identical (no overwriting)
    let snapshot2 = snapshot_files(&list_dir);
    assert_eq!(
        snapshot.len(),
        snapshot2.len(),
        "File count should not change on restart"
    );
    for ((path1, content1), (path2, content2)) in snapshot.iter().zip(snapshot2.iter()) {
        assert_eq!(path1, path2, "File list should be identical");
        assert_eq!(
            content1, content2,
            "File content should not change: {}",
            path1
        );
    }

    // Verify no extra lineage entries (no duplicate processing)
    let lineage_path = format!("{}/__lineage.yaml", list_dir);
    let lineage = fs::read_to_string(&lineage_path).unwrap();
    let entry_count = lineage.matches("email_index:").count();
    assert_eq!(
        entry_count, 5,
        "Lineage should have 5 entries, found {}",
        entry_count
    );

    check_and_delete_folder(base_dir.to_string()).ok();
    check_and_delete_folder(output_dir.to_string()).ok();
}

#[test]
fn test_no_overwrite_on_restart_parquet() {
    let base_dir = "./test_no_overwrite_parquet_data";
    let inbox_name = "test.nooverwrite.pq";
    let output_dir = "./test_no_overwrite_parquet_output";

    check_and_delete_folder(base_dir.to_string()).unwrap();
    check_and_delete_folder(output_dir.to_string()).unwrap();
    std::fs::create_dir_all(base_dir).expect("create base_dir");

    // Phase 1: Create inbox with 5 emails, run archiver with Parquet mode
    create_v1_inbox_with_emails(base_dir, inbox_name, 5);
    let abs_base = std::fs::canonicalize(base_dir).expect("canonicalize base_dir");
    let abs_output = std::fs::canonicalize(output_dir).unwrap_or_else(|_| {
        std::fs::create_dir_all(output_dir).expect("create output_dir");
        std::fs::canonicalize(output_dir).expect("canonicalize output_dir")
    });

    // Run with Parquet mode
    {
        let mut app_config = AppConfig {
            output_dir: abs_output.to_string_lossy().to_string(),
            nthreads: 1,
            write_mode: WriteMode::Parquet { buffer_size: 2 },
            loop_groups: false,
            read_lists: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    RunMode::PublicInbox.to_string(),
                    vec![inbox_name.to_string()],
                );
                m
            },
            public_inbox: Some(PIConfig {
                import_directory: abs_base.to_string_lossy().to_string(),
                origin: "local-test".to_owned(),
                public_inbox_config: None,
                email_range: None,
            }),
            ..Default::default()
        };
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let child_handle = thread::spawn(move || {
            let result = start(&mut app_config, shutdown_flag);
            assert!(result.is_ok(), "Phase 1 should succeed");
        });
        child_handle.join().unwrap();
    }

    let list_dir = format!("{}/{}", output_dir, inbox_name);

    // Count parquet files (data_000, data_001, data_002 = 3 files for 5 emails with buffer=2)
    let first_count = count_files_by_ext(&list_dir, "parquet");
    assert_eq!(first_count, 3, "Phase 1: expected 3 .parquet files");
    validate_progress_file(&format!("{}/__progress.yaml", list_dir), 5);

    // Snapshot
    let snapshot = snapshot_files(&list_dir);

    // Phase 2: Run archiver again (same inbox, no new emails)
    {
        let mut app_config = AppConfig {
            output_dir: abs_output.to_string_lossy().to_string(),
            nthreads: 1,
            write_mode: WriteMode::Parquet { buffer_size: 2 },
            loop_groups: false,
            read_lists: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    RunMode::PublicInbox.to_string(),
                    vec![inbox_name.to_string()],
                );
                m
            },
            public_inbox: Some(PIConfig {
                import_directory: abs_base.to_string_lossy().to_string(),
                origin: "local-test".to_owned(),
                public_inbox_config: None,
                email_range: None,
            }),
            ..Default::default()
        };
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let child_handle = thread::spawn(move || {
            let result = start(&mut app_config, shutdown_flag);
            assert!(result.is_ok(), "Phase 2 should succeed");
        });
        child_handle.join().unwrap();
    }

    // Verify: same number of parquet files
    let second_count = count_files_by_ext(&list_dir, "parquet");
    assert_eq!(
        second_count, 3,
        "Phase 2: expected 3 .parquet files, found {}",
        second_count
    );

    // Verify progress unchanged
    validate_progress_file(&format!("{}/__progress.yaml", list_dir), 5);

    // Verify: all files identical
    let snapshot2 = snapshot_files(&list_dir);
    assert_eq!(snapshot.len(), snapshot2.len());
    for ((path1, content1), (path2, content2)) in snapshot.iter().zip(snapshot2.iter()) {
        assert_eq!(path1, path2);
        assert_eq!(content1, content2, "File changed: {}", path1);
    }

    // Verify no extra lineage entries
    let lineage_path = format!("{}/__lineage.yaml", list_dir);
    let lineage = fs::read_to_string(&lineage_path).unwrap();
    let entry_count = lineage.matches("email_index:").count();
    assert_eq!(entry_count, 5);

    check_and_delete_folder(base_dir.to_string()).ok();
    check_and_delete_folder(output_dir.to_string()).ok();
}

#[test]
fn test_parquet_resume_continues_file_numbering() {
    let base_dir = "./test_pq_resume_numbering_data";
    let inbox_name = "test.pq.numbering";
    let output_dir = "./test_pq_resume_numbering_output";

    check_and_delete_folder(base_dir.to_string()).unwrap();
    check_and_delete_folder(output_dir.to_string()).unwrap();
    std::fs::create_dir_all(base_dir).expect("create base_dir");

    let abs_base = std::fs::canonicalize(base_dir).expect("canonicalize base_dir");
    let abs_output = {
        std::fs::create_dir_all(output_dir).expect("create output_dir");
        std::fs::canonicalize(output_dir).expect("canonicalize output_dir")
    };

    // Phase 1: Create inbox with 5 emails, run with Parquet (buffer_size=2)
    create_v1_inbox_with_emails(base_dir, inbox_name, 5);
    {
        let mut app_config = AppConfig {
            output_dir: abs_output.to_string_lossy().to_string(),
            nthreads: 1,
            write_mode: WriteMode::Parquet { buffer_size: 2 },
            loop_groups: false,
            read_lists: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    RunMode::PublicInbox.to_string(),
                    vec![inbox_name.to_string()],
                );
                m
            },
            public_inbox: Some(PIConfig {
                import_directory: abs_base.to_string_lossy().to_string(),
                origin: "local-test".to_owned(),
                public_inbox_config: None,
                email_range: None,
            }),
            ..Default::default()
        };
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let child_handle = thread::spawn(move || {
            let result = start(&mut app_config, shutdown_flag);
            assert!(result.is_ok(), "Phase 1 should succeed");
        });
        child_handle.join().unwrap();
    }

    let list_dir = format!("{}/{}", output_dir, inbox_name);
    let phase1_count = count_files_by_ext(&list_dir, "parquet");
    // 5 emails, buffer_size=2: flushes at 2, 4, and close flushes remaining 1 → 3 files
    assert_eq!(phase1_count, 3, "Phase 1: expected 3 parquet files");
    assert!(Path::new(&format!("{}/data_000.parquet", list_dir)).exists());
    assert!(Path::new(&format!("{}/data_001.parquet", list_dir)).exists());
    assert!(Path::new(&format!("{}/data_002.parquet", list_dir)).exists());
    validate_progress_file(&format!("{}/__progress.yaml", list_dir), 5);

    // Phase 2: Add 5 more emails (6-10), run again
    add_emails_to_inbox(base_dir, inbox_name, 6, 5);
    {
        let mut app_config = AppConfig {
            output_dir: abs_output.to_string_lossy().to_string(),
            nthreads: 1,
            write_mode: WriteMode::Parquet { buffer_size: 2 },
            loop_groups: false,
            read_lists: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    RunMode::PublicInbox.to_string(),
                    vec![inbox_name.to_string()],
                );
                m
            },
            public_inbox: Some(PIConfig {
                import_directory: abs_base.to_string_lossy().to_string(),
                origin: "local-test".to_owned(),
                public_inbox_config: None,
                email_range: None,
            }),
            ..Default::default()
        };
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let child_handle = thread::spawn(move || {
            let result = start(&mut app_config, shutdown_flag);
            assert!(result.is_ok(), "Phase 2 should succeed");
        });
        child_handle.join().unwrap();
    }

    // Verify: 5 more emails → 3 more parquet files (data_003, data_004, data_005)
    let phase2_count = count_files_by_ext(&list_dir, "parquet");
    assert_eq!(phase2_count, 6, "Phase 2: expected 6 total parquet files");
    assert!(Path::new(&format!("{}/data_003.parquet", list_dir)).exists());
    assert!(Path::new(&format!("{}/data_004.parquet", list_dir)).exists());
    assert!(Path::new(&format!("{}/data_005.parquet", list_dir)).exists());

    // No overshoot
    assert!(!Path::new(&format!("{}/data_006.parquet", list_dir)).exists());

    // Progress should be at 10
    validate_progress_file(&format!("{}/__progress.yaml", list_dir), 10);

    // All original files still exist
    for idx in 0..=5 {
        assert!(
            Path::new(&format!("{}/data_{:03}.parquet", list_dir, idx)).exists(),
            "data_{:03}.parquet should exist",
            idx
        );
    }

    // Total lineage entries should be 10
    let lineage_path = format!("{}/__lineage.yaml", list_dir);
    let lineage = fs::read_to_string(&lineage_path).unwrap();
    let entry_count = lineage.matches("email_index:").count();
    assert_eq!(entry_count, 10);

    check_and_delete_folder(base_dir.to_string()).ok();
    check_and_delete_folder(output_dir.to_string()).ok();
}

/// Creates a single bare git repo epoch with the given number of emails at `repo_path`.
fn create_epoch_repo(repo_path: &Path, email_count: usize, offset: usize) {
    std::fs::create_dir_all(repo_path).expect("create repo_path");
    let _bare_repo = git2::Repository::init_bare(repo_path).expect("init bare repo");
    // Ensure HEAD points to refs/heads/master regardless of git's init.defaultBranch
    std::fs::write(repo_path.join("HEAD"), "ref: refs/heads/master\n").expect("write HEAD");

    let work_dir = repo_path.parent().unwrap().join("_work_clone");
    std::fs::remove_dir_all(&work_dir).ok();
    let clone = git2::build::RepoBuilder::new()
        .clone(repo_path.to_str().unwrap(), &work_dir)
        .expect("clone bare repo");

    let sig = git2::Signature::now("Test User", "test@example.com").expect("create sig");

    for i in 1..=email_count {
        let num = offset + i;
        let email_content = format!(
            "From: tester@example.org\n\
             Subject: Test email {}\n\
             Message-ID: <test-{}@example.org>\n\
             Date: Mon, 01 Jan 2024 00:{:02}:00 +0000\n\
             \n\
             This is test email number {}.\n",
            num,
            num,
            num % 60,
            num
        );

        let m_path = work_dir.join("m");
        std::fs::write(&m_path, &email_content).expect("write email");

        let mut index = clone.index().expect("get index");
        index.add_path(Path::new("m")).expect("add path");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = clone.find_tree(tree_id).expect("find tree");
        let parent_commit = clone.head().ok().and_then(|h| h.peel_to_commit().ok());

        if let Some(parent) = parent_commit {
            clone
                .commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("email {}", num),
                    &tree,
                    &[&parent],
                )
                .expect("commit");
        } else {
            clone
                .commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("email {}", num),
                    &tree,
                    &[],
                )
                .expect("commit");
        }
    }

    // Ensure refs/heads/master points to HEAD
    let head_commit = clone
        .head()
        .expect("get HEAD")
        .peel_to_commit()
        .expect("peel to commit");
    clone
        .reference("refs/heads/master", head_commit.id(), true, "update master")
        .expect("create master ref");

    let mut remote = clone.find_remote("origin").expect("find remote");
    remote
        .push(&["refs/heads/master:refs/heads/master"], None)
        .expect("push");
    std::fs::remove_dir_all(&work_dir).ok();
}

/// Writes an alternates file to a repo's objects/info/ directory pointing to
/// a non-existent absolute path, simulating a broken alternates reference.
fn write_broken_alternates(repo_path: &Path) {
    let info_dir = repo_path.join("objects").join("info");
    std::fs::create_dir_all(&info_dir).unwrap();
    std::fs::write(
        info_dir.join("alternates"),
        "/nonexistent/objstore/deadbeef-1234.git/objects\n",
    )
    .unwrap();
}

/// Creates a V2-style public inbox with multiple epoch repos.
///
/// Directory layout:
///   {inbox_dir}/
///     {inbox_name}/
///       git/
///         0.git/  (epoch 0)
///         1.git/  (epoch 1)
///         2.git/  (epoch 2)
///
/// `broken_epochs` specifies which epoch indices get a broken alternates file.
fn create_v2_multi_epoch_inbox(
    inbox_dir: &str,
    inbox_name: &str,
    epoch_commit_counts: &[usize],
    broken_epochs: &[usize],
) {
    let abs_inbox_dir = {
        std::fs::create_dir_all(inbox_dir).expect("create inbox_dir");
        std::fs::canonicalize(inbox_dir).expect("canonicalize inbox_dir")
    };
    let inbox_path = abs_inbox_dir.join(inbox_name);
    let git_dir = inbox_path.join("git");
    std::fs::create_dir_all(&git_dir).expect("create git dir");

    let mut offset = 0;
    for (i, count) in epoch_commit_counts.iter().enumerate() {
        let epoch_path = git_dir.join(format!("{}.git", i));
        create_epoch_repo(&epoch_path, *count, offset);
        if broken_epochs.contains(&i) {
            write_broken_alternates(&epoch_path);
        }
        offset += count;
    }
}

#[test]
fn test_broken_alternates_all_epochs_processed() {
    let base_dir = "./test_broken_alt_data";
    let output_dir = "./test_broken_alt_output";
    let inbox_name = "v2_broken.list";

    check_and_delete_folder(base_dir.to_string()).unwrap();
    check_and_delete_folder(output_dir.to_string()).unwrap();

    // 3 epochs: epoch0=4 commits, epoch1=3 commits, epoch2=2 commits = 9 total
    // Epochs 0 and 1 have broken alternates pointing to a non-existent path
    create_v2_multi_epoch_inbox(base_dir, inbox_name, &[4, 3, 2], &[0, 1]);

    run_pi_archiver_once(base_dir, output_dir, vec![inbox_name.to_string()]);

    let list_dir = format!("{}/{}", output_dir, inbox_name);
    let eml_count = count_eml_files(&list_dir);
    assert_eq!(
        eml_count, 9,
        "Expected 9 .eml files from all epochs, found {}",
        eml_count
    );

    for i in 1..=9 {
        let found = WalkDir::new(&list_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .any(|e| {
                let filename = e.path().file_stem().and_then(|s| s.to_str()).unwrap_or("");
                parse_email_id(filename)
                    .map(|p| p.email_num == i)
                    .unwrap_or(false)
            });
        assert!(found, "Email {} should exist across all epochs", i);
    }

    validate_progress_file(&format!("{}/__progress.yaml", list_dir), 9);

    check_and_delete_folder(base_dir.to_string()).ok();
    check_and_delete_folder(output_dir.to_string()).ok();
}

#[test]
fn test_broken_alternates_resume() {
    let base_dir = "./test_broken_alt_resume_data";
    let output_dir = "./test_broken_alt_resume_output";
    let inbox_name = "v2_broken_resume.list";

    check_and_delete_folder(base_dir.to_string()).unwrap();
    check_and_delete_folder(output_dir.to_string()).unwrap();

    // 3 epochs: epoch0=4 commits, epoch1=4 commits, epoch2=3 commits = 11 total
    // Epochs 0 and 1 have broken alternates, epoch 2 is clean.
    create_v2_multi_epoch_inbox(base_dir, inbox_name, &[4, 4, 3], &[0, 1]);

    // Phase 1: Process only first 5 emails with email_range
    let abs_base = std::fs::canonicalize(base_dir).expect("canonicalize base_dir");
    let abs_output = std::fs::canonicalize(output_dir).unwrap_or_else(|_| {
        std::fs::create_dir_all(output_dir).expect("create output_dir");
        std::fs::canonicalize(output_dir).expect("canonicalize output_dir")
    });
    {
        let mut app_config = AppConfig {
            output_dir: abs_output.to_string_lossy().to_string(),
            nthreads: 1,
            write_mode: WriteMode::RawEmails,
            loop_groups: false,
            read_lists: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    RunMode::PublicInbox.to_string(),
                    vec![inbox_name.to_string()],
                );
                m
            },
            public_inbox: Some(PIConfig {
                import_directory: abs_base.to_string_lossy().to_string(),
                origin: "local-test".to_owned(),
                public_inbox_config: None,
                email_range: Some("1-5".to_owned()),
            }),
            ..Default::default()
        };

        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let child_handle = thread::spawn(move || {
            let result = start(&mut app_config, shutdown_flag);
            assert!(result.is_ok(), "Phase 1 should succeed");
        });
        child_handle.join().unwrap();
    }

    let list_dir = format!("{}/{}", output_dir, inbox_name);
    let phase1_count = count_eml_files(&list_dir);
    assert_eq!(
        phase1_count, 5,
        "Phase 1: expected 5 emails, found {}",
        phase1_count
    );

    // Phase 2: Run without range — should resume and process remaining 6 emails
    {
        let mut app_config = AppConfig {
            output_dir: abs_output.to_string_lossy().to_string(),
            nthreads: 1,
            loop_groups: false,
            write_mode: WriteMode::RawEmails,
            read_lists: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    RunMode::PublicInbox.to_string(),
                    vec![inbox_name.to_string()],
                );
                m
            },
            public_inbox: Some(PIConfig {
                import_directory: abs_base.to_string_lossy().to_string(),
                origin: "local-test".to_owned(),
                public_inbox_config: None,
                email_range: None,
            }),
            ..Default::default()
        };

        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let child_handle = thread::spawn(move || {
            let result = start(&mut app_config, shutdown_flag);
            assert!(result.is_ok(), "Phase 2 should succeed");
        });
        child_handle.join().unwrap();
    }

    let eml_count = count_eml_files(&list_dir);
    assert_eq!(
        eml_count, 11,
        "Expected 11 total .eml files after resume, found {}",
        eml_count
    );

    for i in 1..=11 {
        let found = WalkDir::new(&list_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .any(|e| {
                let filename = e.path().file_stem().and_then(|s| s.to_str()).unwrap_or("");
                parse_email_id(filename)
                    .map(|p| p.email_num == i)
                    .unwrap_or(false)
            });
        assert!(found, "Email {} should exist after resume", i);
    }

    check_and_delete_folder(base_dir.to_string()).ok();
    check_and_delete_folder(output_dir.to_string()).ok();
}
