//! Mailmap-based email identity canonicalization.
//!
//! Parses a git-format [`.mailmap`](https://git-scm.com/docs/gitmailmap) file
//! to build a lookup table mapping alias emails to canonical identities.
//! The mailmap is validated via [`git2::Mailmap`] at parse time; resolution
//! uses an internal [`HashMap`] for thread-safe, zero-allocation lookups.
//!
//! # Format
//!
//! ```text
//! # comment
//! Proper Name <canonical@email> <alias@email>
//! <canonical@email> <alias@email>
//! Proper Name <canonical@email> Commit Name <alias@email>
//! ```

use crate::entities::Attribution;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// In-memory mailmap for resolving email addresses to canonical identities.
pub struct Mailmap {
    /// Lowercase email → canonical "Name <email>" string.
    map: HashMap<String, String>,
}

impl Mailmap {
    pub fn from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let map = parse_mailmap_content(&content)?;
        Ok(Mailmap { map })
    }

    pub fn resolve(&self, raw_addr: &str) -> Option<String> {
        let (_name, email) = split_address(raw_addr)?;
        self.map.get(&email.to_lowercase()).cloned()
    }

    pub fn resolve_trailers(&self, trailers: &[Attribution]) -> Vec<Option<String>> {
        trailers
            .iter()
            .map(|attr| self.resolve(&attr.identification))
            .collect()
    }
}

fn parse_mailmap_content(
    content: &str,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    // Use git2 for validation and canonical parsing.
    git2::Mailmap::from_buffer(content)?;
    // git2 validates the format but doesn't expose entries for iteration,
    // so we parse the content ourselves for thread-safe HashMap access.
    let mut map: HashMap<String, String> = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Extract email addresses from the line using angle brackets.
        let parts: Vec<(&str, &str)> = extract_angle_bracket_pairs(trimmed);
        if parts.is_empty() {
            continue;
        }
        // The FIRST angle-bracketed entry is canonical, subsequent ones are aliases.
        let canonical = parts[0];
        let canonical_str = if canonical.0.is_empty() {
            // Email-only canonical: just the email.
            canonical.1.to_string()
        } else {
            format!("{} <{}>", canonical.0, canonical.1)
        };
        // Map each alias email to the canonical form.
        for alias in &parts[1..] {
            map.insert(alias.1.to_lowercase(), canonical_str.clone());
        }
    }

    Ok(map)
}

/// Extracts all `(name, email)` pairs from angle brackets in a line.
/// e.g. "Name1 <email1> Name2 <email2>" → [("Name1", "email1"), ("Name2", "email2")]
fn extract_angle_bracket_pairs(line: &str) -> Vec<(&str, &str)> {
    let mut result = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' {
            let start = i + 1;
            i += 1;
            while i < chars.len() && chars[i] != '>' {
                i += 1;
            }
            if i < chars.len() {
                let email = &line[start..i];
                i += 1; // skip '>'
                // Extract name: everything before the '<' position,
                // walking backwards from the '<', excluding leading whitespace
                // and stopping at the previous '>' or start.
                let before_lt = &line[..start - 1];
                let name = before_lt.trim_end();
                result.push((name, email));
            }
        } else {
            i += 1;
        }
    }
    result
}

fn split_address(raw: &str) -> Option<(String, String)> {
    if let Some(lt) = raw.rfind('<') {
        let after_lt = &raw[lt..];
        let gt = after_lt.find('>')?;
        let name = raw[..lt].trim().to_string();
        let email = raw[lt + 1..lt + gt].to_string();
        Some((name, email))
    } else if raw.contains('@') {
        Some((String::new(), raw.trim().to_string()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Attribution;
    use std::io::Write;

    fn temp_mailmap(content: &str) -> Mailmap {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "{}", content).unwrap();
        Mailmap::from_file(tmp.path()).unwrap()
    }

    #[test]
    fn test_split_address_name_email() {
        let (name, email) = split_address("Cassian Andor <cassian.andor@ferrix.local>").unwrap();
        assert_eq!(name, "Cassian Andor");
        assert_eq!(email, "cassian.andor@ferrix.local");
    }

    #[test]
    fn test_split_address_email_only() {
        let (name, email) = split_address("cass@ferrix.local").unwrap();
        assert_eq!(name, "");
        assert_eq!(email, "cass@ferrix.local");
    }

    #[test]
    fn test_split_address_no_email() {
        assert!(split_address("just a name").is_none());
    }

    #[test]
    fn test_resolve_maps_to_canonical() {
        let mailmap =
            temp_mailmap("Cassian Andor <cassian.andor@ferrix.local> <cass@ferrix.local>\n");
        let result = mailmap.resolve("Cass Andor <cass@ferrix.local>");
        assert_eq!(
            result,
            Some("Cassian Andor <cassian.andor@ferrix.local>".to_string())
        );
    }

    #[test]
    fn test_resolve_no_mapping_returns_none() {
        let mailmap =
            temp_mailmap("Cassian Andor <cassian.andor@ferrix.local> <cass@ferrix.local>\n");
        let result = mailmap.resolve("Unknown Person <unknown@test.local>");
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_email_only_map() {
        let mailmap = temp_mailmap("<cassian.andor@ferrix.local> <cass@ferrix.local>\n");
        let result = mailmap.resolve("Cass Andor <cass@ferrix.local>");
        assert_eq!(result, Some("cassian.andor@ferrix.local".to_string()));
    }

    #[test]
    fn test_resolve_trailers() {
        let mailmap =
            temp_mailmap("Cassian Andor <cassian.andor@ferrix.local> <cass@ferrix.local>\n");
        let trailers = vec![
            Attribution {
                attribution: "Signed-off-by".into(),
                identification: "Cass Andor <cass@ferrix.local>".into(),
            },
            Attribution {
                attribution: "Reviewed-by".into(),
                identification: "Unknown Person <unknown@test.local>".into(),
            },
        ];
        let resolved = mailmap.resolve_trailers(&trailers);
        assert_eq!(
            resolved[0],
            Some("Cassian Andor <cassian.andor@ferrix.local>".to_string())
        );
        assert_eq!(resolved[1], None);
    }

    #[test]
    fn test_comment_lines_ignored() {
        let mailmap = temp_mailmap(
            "# This is a comment\nCassian Andor <cassian.andor@ferrix.local> <cass@ferrix.local>\n",
        );
        let result = mailmap.resolve("Cass Andor <cass@ferrix.local>");
        assert!(result.is_some());
    }

    #[test]
    fn test_blank_lines_ignored() {
        let mailmap =
            temp_mailmap("\nCassian Andor <cassian.andor@ferrix.local> <cass@ferrix.local>\n\n");
        let result = mailmap.resolve("Cass Andor <cass@ferrix.local>");
        assert!(result.is_some());
    }

    #[test]
    fn test_multiple_aliases() {
        let mailmap = temp_mailmap(
            "Canon Name <canon@example.com> <alias1@example.com> <alias2@example.com>\n",
        );
        assert_eq!(
            mailmap.resolve("Old Name <alias1@example.com>"),
            Some("Canon Name <canon@example.com>".to_string())
        );
        assert_eq!(
            mailmap.resolve("Old Name <alias2@example.com>"),
            Some("Canon Name <canon@example.com>".to_string())
        );
    }

    #[test]
    fn test_complex_mapping() {
        let mailmap = temp_mailmap("Real Name <real@example.com> Old Name <old@example.com>\n");
        let result = mailmap.resolve("Old Name <old@example.com>");
        assert_eq!(result, Some("Real Name <real@example.com>".to_string()));
    }

    #[test]
    fn test_case_insensitive_match() {
        let mailmap =
            temp_mailmap("Cassian Andor <cassian.andor@ferrix.local> <Cass@Ferrix.local>\n");
        let result = mailmap.resolve("Cass Andor <CASS@ferrix.local>");
        assert_eq!(
            result,
            Some("Cassian Andor <cassian.andor@ferrix.local>".to_string())
        );
    }
}
