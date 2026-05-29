//! check_nntp - NNTP mailing list browser & article fetcher
//!
//! This tool allows you to browse NNTP mailing lists interactively or fetch
//! specific articles by glob pattern and article number/range.
//!
//! # Usage
//!
//! ## Interactive mode (default)
//!
//! ```bash
//! cargo run --package check_nntp
//! cargo run --package check_nntp -- -s nntp://nntp.example.com
//! cargo run --package check_nntp -- -s nntps://nntp.example.com
//! cargo run --package check_nntp -- -s nntp://nntp.example.com:8119
//! ```
//!
//! ## Batch mode (fetch specific articles)
//!
//! ```bash
//! cargo run --package check_nntp -- -s nntp://nntp.example.com -l "*-kernel" --id 42
//! cargo run --package check_nntp -- -s nntp://nntp.example.com -l "*-kernel" --id 1-10
//! cargo run --package check_nntp -- -s nntp://nntp.example.com -l "*-kernel" --id '1..10'
//! cargo run --package check_nntp -- -s nntp://nntp.example.com -l "*-kernel" --id '1,3,5-7'
//! ```

use clap::Parser;
use glob::Pattern;
use inquire::{Confirm, MultiSelect, Select, Text};
use mlh_archiver::nntp_source::{
    connect_to_nntp_server, nntp_utils::server_address, retrieve_groups_info_with_connection,
    retrieve_lists_with_connection,
};
use mlh_archiver::range_inputs::parse_sequence;
use nntp::NNTPStream;
use std::env;

/// Parsed server configuration from a URL.
struct ServerConfig {
    hostname: String,
    port: Option<u16>,
    use_tls: bool,
}

/// Parses an NNTP server URL into a [`ServerConfig`].
///
/// # Supported formats
///
/// - `nntp://hostname` → port 119, plaintext
/// - `nntps://hostname` → port 563, TLS
/// - `nntp://hostname:port` → custom port, plaintext
/// - `nntps://hostname:port` → custom port, TLS
///
/// # Examples
///
/// ```
/// let cfg = parse_server_url("nntp://example.com").unwrap();
/// assert_eq!(cfg.hostname, "nntp://example.com");
/// assert_eq!(cfg.port, None);
/// assert!(!cfg.use_tls);
///
/// let cfg = parse_server_url("nntps://example.com").unwrap();
/// assert_eq!(cfg.hostname, "nntps://example.com");
/// assert_eq!(cfg.port, None);
/// assert!(cfg.use_tls);
///
/// let cfg = parse_server_url("nntp://example.com:8119").unwrap();
/// assert_eq!(cfg.hostname, "nntp://example.com");
/// assert_eq!(cfg.port, Some(8119));
/// ```
fn parse_server_url(input: &str) -> Result<ServerConfig, String> {
    if input.is_empty() {
        return Err("empty hostname".to_string());
    }

    let input = input.trim();

    // Determine scheme and strip it
    let use_tls = if input.starts_with("nntps://") {
        true
    } else if input.starts_with("nntp://") {
        false
    } else {
        // No recognized scheme — default to plaintext NNTP
        false
    };

    // Only treat the last ':' as a port separator if what follows is purely numeric.
    // This avoids splitting on colons in malformed URLs like "s://hostname".
    let (hostname, port) = if let Some((host, port_str)) = input.rsplit_once(':') {
        if port_str.chars().all(|c| c.is_ascii_digit()) && !port_str.is_empty() {
            let port = port_str
                .parse::<u16>()
                .map_err(|_| format!("invalid port '{}'", port_str))?;
            (host.to_string(), Some(port))
        } else {
            // Not a valid port — treat entire rest as hostname
            (input.to_string(), None)
        }
    } else {
        (input.to_string(), None)
    };

    if hostname.is_empty() {
        return Err("empty hostname".to_string());
    }

    Ok(ServerConfig {
        hostname,
        port,
        use_tls,
    })
}

/// Interactive NNTP mailing list browser and configuration generator
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// NNTP server URL (e.g., nntp://hostname, nntps://hostname, nntp://hostname:port)
    #[arg(short = 's', long = "server")]
    server: Option<String>,

    /// Optional: username
    #[arg(short = 'u', long = "username")]
    username: Option<String>,

    /// Optional: password
    #[arg(short = 'P', long = "password")]
    password: Option<String>,

    /// Export configuration to YAML file after browsing
    #[arg(long = "export-config")]
    export_config: bool,

    /// Enable verbose logging
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,

    /// Glob pattern to filter mailing lists (e.g., "*-kernel"). Triggers batch mode.
    #[arg(short = 'l', long = "list")]
    list: Option<String>,

    /// Article ID or range to fetch (e.g., "1", "1-10", "1..10", "1,3-5,7")
    #[arg(long = "id")]
    id: Option<String>,
}

fn main() -> mlh_archiver::Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = if args.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    println!("📬 check_nntp - NNTP Mailing List Browser");
    println!("=========================================\n");

    // Get server config from CLI, env, or prompt
    let server = if let Some(ref url) = args.server {
        match parse_server_url(url) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("❌ Invalid server URL: {}", e);
                eprintln!("Expected format: nntp://hostname[:port] or nntps://hostname[:port]");
                std::process::exit(1);
            }
        }
    } else if let Ok(env_input) = env::var("NNTP_SERVER") {
        match parse_server_url(&env_input) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("❌ Invalid NNTP_SERVER env var: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        prompt_for_server()
    };

    let server_url = server_address(&server.hostname, server.port);
    let tls_label = if server.use_tls { " (TLS)" } else { "" };
    log::info!("Connecting to NNTP server: {}{}", server_url, tls_label);

    // Connect to NNTP server and retrieve list of groups
    println!("🔍 Connecting to {}{}...", server_url, tls_label);
    let mut conn = match connect_to_nntp_server(
        &server.hostname,
        server.port,
        args.username.clone(),
        args.password.clone(),
    ) {
        Ok(stream) => NntpConnection::new(stream),
        Err(e) => {
            eprintln!("❌ Failed to connect to NNTP server: {}", e);
            return Err(e);
        }
    };

    let groups = match retrieve_lists_with_connection(conn.stream()) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("❌ Failed to fetch mailing lists: {}", e);
            return Err(e);
        }
    };

    println!("✅ Found {} mailing lists\n", groups.len());

    if groups.is_empty() {
        println!("No mailing lists available on this server.");
        return Ok(());
    }

    if let Some(ref list_pattern) = args.list {
        let id = args.id.as_deref().unwrap_or_else(|| {
            eprintln!("❌ --id is required when using --list");
            eprintln!("   Examples: --id 42, --id 1-10, --id '1..10', --id '1,3,5-7'");
            std::process::exit(1);
        });
        return batch_mode(conn.stream(), &groups, list_pattern, id);
    }

    // Interactive selection + fetch loop
    loop {
        let mut select_options = vec!["*".to_string()];
        select_options.extend(groups.clone());

        let selected = MultiSelect::new("Select mailing lists to preview:", select_options)
            .with_help_message("Space to select, Enter to confirm, Esc to quit")
            .prompt()
            .unwrap_or_else(|_| std::process::exit(0));

        if selected.is_empty() {
            println!("No lists selected. Exiting.");
            return Ok(());
        }

        let groups_to_preview = if selected.iter().any(|s| s == "*") {
            println!("📋 Previewing all {} lists...\n", groups.len());
            groups.clone()
        } else {
            println!("📋 Previewing {} selected lists...\n", selected.len());
            selected.clone()
        };

        println!("📊 Fetching email ranges...");
        let groups_info =
            match retrieve_groups_info_with_connection(conn.stream(), &groups_to_preview) {
                Ok(info) => info,
                Err(e) => {
                    eprintln!("⚠️  Warning: Failed to fetch some group info: {}", e);
                    Vec::new()
                }
            };

        println!("\n📈 Article Range Preview:");
        println!("─────────────────────────────────────────────────────────────");
        println!("{:<50} {:>12}", "Group", "Articles");
        println!("─────────────────────────────────────────────────────────────");

        for (group_name, group_info) in &groups_info {
            let article_count = group_info.high - group_info.low + 1;
            let range_str = format!("[{}..{}]", group_info.low, group_info.high);
            println!("{:<50} {:>12}", truncate_str(group_name, 49), range_str);
            println!("{:<50} {:>12}", "", format!("({} total)", article_count));
        }

        println!("─────────────────────────────────────────────────────────────\n");

        if groups_info.is_empty() {
            continue;
        }

        let input =
            Text::new("Fetch emails? (number/range, Enter=latest, n=back to lists, q=quit)")
                .with_help_message("Examples: 42, 1-10, 1..10, 1,3,5-7")
                .prompt()
                .unwrap_or_else(|_| std::process::exit(0));

        let input = input.trim();
        match input {
            "q" | "Q" => {
                println!("\n✨ Done!");
                return Ok(());
            }
            "n" | "N" => continue,
            "" => {
                let list_options: Vec<&String> = groups_info.iter().map(|(name, _)| name).collect();
                if let Ok(selection) =
                    Select::new("Select a list to fetch latest article:", list_options).prompt()
                {
                    if let Some((_, group_info)) =
                        groups_info.iter().find(|(name, _)| name == selection)
                    {
                        if group_info.high >= group_info.low {
                            let latest = vec![group_info.high as usize];
                            fetch_and_display_articles(conn.stream(), selection, &latest);
                        } else {
                            println!("⚠️  Group appears to be empty (low > high)");
                        }
                    }
                }
            }
            range_str => match try_parse_id_range(range_str) {
                Some(ids) => {
                    let count = groups_info.len();
                    for (i, (group_name, _)) in groups_info.iter().enumerate() {
                        if i > 0 {
                            let proceed = Confirm::new(&format!(
                                "Continue to '{}'? ({}/{})",
                                group_name,
                                i + 1,
                                count
                            ))
                            .with_default(true)
                            .prompt()
                            .unwrap_or(false);

                            if !proceed {
                                break;
                            }
                        }
                        fetch_and_display_articles(conn.stream(), group_name, &ids);
                    }
                }
                None => {
                    eprintln!("⚠️  Try again or enter 'n' to go back, 'q' to quit.");
                }
            },
        }
    }
}

/// Run batch mode: filter groups by glob pattern, fetch articles by id range.
fn batch_mode(
    stream: &mut NNTPStream,
    groups: &[String],
    list_pattern: &str,
    id: &str,
) -> mlh_archiver::Result<()> {
    let matching = filter_by_glob(groups, list_pattern);
    let count = matching.len();

    println!("✅ Found {} list(s) matching '{}'\n", count, list_pattern);

    if matching.is_empty() {
        return Ok(());
    }

    let ids = parse_id_range(id);

    for (i, group_name) in matching.iter().enumerate() {
        if i > 0 {
            let proceed = Confirm::new(&format!(
                "Continue to '{}'? ({}/{})",
                group_name,
                i + 1,
                count
            ))
            .with_default(true)
            .prompt()
            .unwrap_or(false);

            if !proceed {
                println!("Exiting.");
                break;
            }
        }

        fetch_and_display_articles(stream, group_name, &ids);
    }

    println!("\n✨ Done!");
    Ok(())
}

fn filter_by_glob(groups: &[String], pattern: &str) -> Vec<String> {
    let pat = match Pattern::new(pattern) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ Invalid glob pattern '{}': {}", pattern, e);
            std::process::exit(1);
        }
    };
    let mut matching: Vec<String> = groups.iter().filter(|g| pat.matches(g)).cloned().collect();
    matching.sort();
    matching
}

fn parse_id_range(input: &str) -> Vec<usize> {
    let normalized = input.replace("..", "-");
    match parse_sequence(&normalized) {
        Ok(iter) => {
            let ids: Vec<usize> = iter.collect();
            if ids.is_empty() {
                eprintln!("❌ Empty id range: '{}'", input);
                std::process::exit(1);
            }
            ids
        }
        Err(e) => {
            eprintln!("❌ Invalid id range '{}': {}", input, e);
            eprintln!("   Supported formats: 42, 1-10, 1..10, 1,3,5-7");
            std::process::exit(1);
        }
    }
}

fn try_parse_id_range(input: &str) -> Option<Vec<usize>> {
    let normalized = input.replace("..", "-");
    match parse_sequence(&normalized) {
        Ok(iter) => {
            let ids: Vec<usize> = iter.collect();
            if ids.is_empty() {
                None
            } else {
                Some(ids)
            }
        }
        Err(_) => None,
    }
}

fn fetch_and_display_articles(stream: &mut NNTPStream, group_name: &str, ids: &[usize]) {
    match stream.group(group_name) {
        Ok(info) => {
            println!(
                "\n📁 {}  (articles {}..{})",
                group_name, info.low, info.high
            );
        }
        Err(e) => {
            eprintln!("⚠️  Failed to select group '{}': {}", group_name, e);
            let _ = stream.quit();
            return;
        }
    }

    for &id in ids {
        match stream.article_by_number(id as isize) {
            Ok(article) => {
                let subject = article
                    .headers
                    .get("Subject")
                    .map(|s| s.as_str())
                    .unwrap_or("(no subject)");
                let from = article
                    .headers
                    .get("From")
                    .map(|s| s.as_str())
                    .unwrap_or("(unknown)");
                let date = article
                    .headers
                    .get("Date")
                    .map(|s| s.as_str())
                    .unwrap_or("(unknown)");

                println!("\n── Article #{} ──", id);
                println!("Subject: {}", subject);
                println!("From:    {}", from);
                println!("Date:    {}", date);
                println!("{}", "─".repeat(50));
                for line in &article.body {
                    println!("{}", line);
                }
            }
            Err(e) => {
                println!("⚠️  Article #{} unavailable: {}", id, e);
            }
        }
    }
}

/// Prompt user for NNTP server URL
fn prompt_for_server() -> ServerConfig {
    let input = Text::new("Enter NNTP server URL:")
        .with_default("nntp://nntp.example.com")
        .with_help_message(
            "nntp://hostname (port 119), nntps://hostname (port 563), or nntp://hostname:port",
        )
        .prompt()
        .unwrap_or_else(|_| std::process::exit(0));

    match parse_server_url(&input) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("❌ Invalid URL: {}", e);
            std::process::exit(1);
        }
    }
}

/// Truncate string to max length with ellipsis
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

/// Wrapper around [`NNTPStream`] that calls `quit()` on drop, ensuring the
/// connection is cleanly closed on normal exit, error, or panic.
struct NntpConnection(Option<NNTPStream>);

impl NntpConnection {
    fn new(stream: NNTPStream) -> Self {
        NntpConnection(Some(stream))
    }

    fn stream(&mut self) -> &mut NNTPStream {
        self.0.as_mut().expect("NntpConnection already consumed")
    }
}

impl Drop for NntpConnection {
    fn drop(&mut self) {
        if let Some(ref mut stream) = self.0 {
            let _ = stream.quit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nntp_default_port() {
        let cfg = parse_server_url("nntp://example.com").unwrap();
        assert_eq!(cfg.hostname, "nntp://example.com");
        assert_eq!(cfg.port, None);
        assert!(!cfg.use_tls);
    }

    #[test]
    fn test_parse_nntps_default_port() {
        let cfg = parse_server_url("nntps://example.com").unwrap();
        assert_eq!(cfg.hostname, "nntps://example.com");
        assert_eq!(cfg.port, None);
        assert!(cfg.use_tls);
    }

    #[test]
    fn test_parse_nntp_with_port() {
        let cfg = parse_server_url("nntp://example.com:8119").unwrap();
        assert_eq!(cfg.hostname, "nntp://example.com");
        assert_eq!(cfg.port, Some(8119));
        assert!(!cfg.use_tls);
    }

    #[test]
    fn test_parse_nntps_with_port() {
        let cfg = parse_server_url("nntps://example.com:563").unwrap();
        assert_eq!(cfg.hostname, "nntps://example.com");
        assert_eq!(cfg.port, Some(563));
        assert!(cfg.use_tls);
    }

    #[test]
    fn test_parse_ip_with_port() {
        let cfg = parse_server_url("nntp://192.168.1.1:5119").unwrap();
        assert_eq!(cfg.hostname, "nntp://192.168.1.1");
        assert_eq!(cfg.port, Some(5119));
        assert!(!cfg.use_tls);
    }

    #[test]
    fn test_parse_ip_without_port() {
        let cfg = parse_server_url("nntp://192.168.1.1").unwrap();
        assert_eq!(cfg.hostname, "nntp://192.168.1.1");
        assert_eq!(cfg.port, None);
        assert!(!cfg.use_tls);
    }

    #[test]
    fn test_parse_no_scheme_defaults_to_nntp() {
        let cfg = parse_server_url("example.com").unwrap();
        assert_eq!(cfg.hostname, "example.com");
        assert_eq!(cfg.port, None);
        assert!(!cfg.use_tls);
    }

    #[test]
    fn test_parse_no_scheme_with_port() {
        let cfg = parse_server_url("example.com:8119").unwrap();
        assert_eq!(cfg.hostname, "example.com");
        assert_eq!(cfg.port, Some(8119));
        assert!(!cfg.use_tls);
    }

    #[test]
    fn test_parse_trims_whitespace() {
        let cfg = parse_server_url("  nntp://example.com:5119  ").unwrap();
        assert_eq!(cfg.hostname, "nntp://example.com");
        assert_eq!(cfg.port, Some(5119));
    }

    #[test]
    fn test_parse_invalid_port_falls_back_to_hostname() {
        // Non-numeric "port" is treated as part of the hostname
        let cfg = parse_server_url("nntp://example.com:abc").unwrap();
        assert_eq!(cfg.hostname, "nntp://example.com:abc");
        assert_eq!(cfg.port, None);
    }

    #[test]
    fn test_parse_port_out_of_range() {
        let result = parse_server_url("nntp://example.com:70000");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_hostname() {
        // "nntp://" has no hostname after scheme — rsplit_once gives ("nntp", "")
        // "" is not numeric, so falls back to full input as hostname
        let cfg = parse_server_url("nntp://").unwrap();
        assert_eq!(cfg.hostname, "nntp://");
        assert_eq!(cfg.port, None);
    }

    #[test]
    fn test_parse_empty_input() {
        let result = parse_server_url("");
        assert!(result.is_err());
    }
}
