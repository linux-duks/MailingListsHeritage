use regex::Regex;
use std::sync::LazyLock;

static ADDRESS_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\s]*([^<]*?)?\s*<?([a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,})>?\s*$")
        .unwrap()
});

static ADDRESS_OBFUSCATED_A_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^[\s]*([^<]*?)?\s*<?([a-zA-Z0-9._%+-]+)\s*\(a\)\s*([a-zA-Z0-9.-]+\.[a-zA-Z]{2,})>?\s*$",
    )
    .unwrap()
});

static ADDRESS_OBFUSCATED_AT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^[\s]*([^<]*?)?\s*<?([a-zA-Z0-9._%+-]+)\s+at\s+([a-zA-Z0-9.-]+\.[a-zA-Z]{2,})>?\s*$",
    )
    .unwrap()
});

pub fn normalize_address(value: &str) -> String {
    if let Some(caps) = ADDRESS_OBFUSCATED_A_PATTERN.captures(value) {
        let name = caps.get(1).map_or("", |m| m.as_str()).trim();
        let email = format!(
            "{}@{}",
            caps.get(2).unwrap().as_str(),
            caps.get(3).unwrap().as_str()
        );
        if name.is_empty() {
            return email;
        }
        return format!("{} <{}>", name, email);
    }
    if let Some(caps) = ADDRESS_OBFUSCATED_AT_PATTERN.captures(value) {
        let name = caps.get(1).map_or("", |m| m.as_str()).trim();
        let email = format!(
            "{}@{}",
            caps.get(2).unwrap().as_str(),
            caps.get(3).unwrap().as_str()
        );
        if name.is_empty() {
            return email;
        }
        return format!("{} <{}>", name, email);
    }
    // mail_parser treats `(a)` inside `<>` as an RFC 2822 comment and
    // strips it, collapsing `local(a)domain.tld` into `localdomain.tld`.
    // Reconstruct the missing `@` by detecting the domain portion.
    if let Some(reconstructed) = reconstruct_stripped_at(value) {
        return reconstructed;
    }
    value.to_string()
}

/// When `mail_parser` strips an `(a)` comment inside `<>`, the remaining
/// address looks like `localdomain.tld` with no delimiter. This function
/// finds the rightmost `domain.tld` pattern and inserts `@` before it.
fn reconstruct_stripped_at(value: &str) -> Option<String> {
    let lt = value.find('<')?;
    let gt = value[lt..].find('>')?;
    let inner = &value[lt + 1..lt + gt];

    if inner.contains('@') {
        return None;
    }

    let name_part = value[..lt].trim();

    // Walk backwards to find the rightmost `domain.tld` split.
    // Look for the last dot followed by a 2-4 letter TLD, then
    // find the start of the domain component before it.
    let tld_re = Regex::new(r"\.([a-zA-Z]{2,4})$").unwrap();
    let tld_caps = tld_re.captures(inner)?;
    let tld_dot = tld_caps.get(0)?.start();

    // Walk left from the TLD dot past domain characters
    // (letters, digits, hyphens) to find the true domain start.
    let mut domain_start = tld_dot;
    while domain_start > 0 {
        let c = inner.as_bytes()[domain_start - 1];
        if c.is_ascii_alphanumeric() || c == b'-' {
            domain_start -= 1;
        } else {
            break;
        }
    }

    let local = &inner[..domain_start];
    let domain = &inner[domain_start..];

    if local.is_empty() || domain.is_empty() {
        return None;
    }

    let email = format!("{}@{}", local, domain);

    if name_part.is_empty() {
        Some(email)
    } else {
        Some(format!("{} <{}>", name_part, email))
    }
}

pub fn addr_to_string(addr: &mail_parser::Addr<'_>) -> String {
    let name = addr.name.as_deref().unwrap_or("").to_string();
    let email = addr.address.as_deref().unwrap_or("").to_string();
    if name.is_empty() {
        email
    } else if email.is_empty() {
        name
    } else {
        format!("{} <{}>", name, email)
    }
}

/// Strips RFC 2822 comment artifacts that `mail_parser` may attach to
/// display names when `(a)` obfuscation is used inside angle brackets.
fn strip_comment_artifacts(s: &str) -> String {
    let re = Regex::new(r"(?i)\s*\(a\)\s*").unwrap();
    re.replace_all(s, " ").trim().to_string()
}

/// Merges adjacent incomplete addresses and normalizes the result.
///
/// When `mail_parser` splits a display name containing an unquoted comma
/// (e.g. `Picard, Jean-Luc <email>`), the first address has a name but
/// no email. This function merges such incomplete identities with the
/// following complete one, strips commas from display names, and
/// de-obfuscates `at` / `(a)` email addresses.
pub fn normalize_address_list(address: &mail_parser::Address<'_>) -> Vec<String> {
    let addrs: Vec<&mail_parser::Addr<'_>> = address.iter().collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < addrs.len() {
        let cur = addrs[i];
        let cur_name = strip_comment_artifacts(cur.name.as_deref().unwrap_or(""));
        let cur_email = cur.address.as_deref().unwrap_or("");

        if !cur_name.is_empty() && cur_email.is_empty() && i + 1 < addrs.len() {
            let next = addrs[i + 1];
            let next_name =
                strip_comment_artifacts(next.name.as_deref().unwrap_or(""));
            let next_email = next.address.as_deref().unwrap_or("");

            if !next_name.is_empty() && !next_email.is_empty() {
                let merged_name = format!("{} {}", cur_name, next_name)
                    .replace(',', "");
                let merged_name = merged_name.trim().to_string();
                let identity =
                    normalize_address(&format!("{} <{}>", merged_name, next_email));
                if !identity.is_empty() {
                    result.push(identity);
                }
                i += 2;
                continue;
            }
        }

        let identity = addr_to_string(cur).replace(',', "");
        let identity = strip_comment_artifacts(&identity);
        let identity = normalize_address(identity.trim());
        if !identity.is_empty() {
            result.push(identity);
        }
        i += 1;
    }
    result
}

/// Pre-processes a raw header value line (e.g. from `To:` or `CC:`)
/// by normalising `(a)` → `@` and ` at ` → `@` before the mail
/// parser sees it, then splits on commas, normalises each part, and
/// merges incomplete adjacent identities.
pub fn normalize_raw_address_header(raw: &str) -> Vec<String> {
    // Strip the header name prefix (e.g. "To:" or "CC:")
    let value = raw.trim();
    let value = value
        .replacen("To:", "", 1)
        .replacen("to:", "", 1)
        .replacen("TO:", "", 1)
        .replacen("CC:", "", 1)
        .replacen("Cc:", "", 1)
        .replacen("cc:", "", 1)
        .replacen("From:", "", 1)
        .replacen("from:", "", 1)
        .replacen("FROM:", "", 1)
        .trim()
        .to_string();

    // Pre-normalize obfuscation so mail_parser's comment-stripping
    // doesn't break (a) addresses
    let value = value.replace("(a)", "@").replace("(A)", "@");
    let at_re = Regex::new(r"(?i)\s+at\s+").unwrap();
    let value = at_re.replace_all(&value, "@").to_string();

    // Split into individual identity strings, then normalize each
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    for ch in value.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                let part = current.trim().to_string();
                if !part.is_empty() {
                    let normalized = normalize_address(&part);
                    if !normalized.is_empty() {
                        parts.push(normalized);
                    }
                }
                current = String::new();
            }
            _ => current.push(ch),
        }
    }
    let part = current.trim().to_string();
    if !part.is_empty() {
        let normalized = normalize_address(&part);
        if !normalized.is_empty() {
            parts.push(normalized);
        }
    }

    // Merge incomplete identities that resulted from comma-in-display-name
    merge_incomplete_parts(&parts)
}

/// Merges adjacent strings where one has a display name but no email
/// (incomplete identity) with the following complete identity.
fn merge_incomplete_parts(parts: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        let cur = &parts[i];
        let has_email = cur.contains('@');

        if !has_email && i + 1 < parts.len() {
            let next = &parts[i + 1];
            if next.contains('@') {
                // cur is just a display name, merge with next
                let merged = format!("{} {}", cur, next);
                let merged = normalize_address(&merged);
                if !merged.is_empty() {
                    result.push(merged);
                }
                i += 2;
                continue;
            }
        }

        result.push(cur.clone());
        i += 1;
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AddressScore {
    pub has_name: bool,
    is_standard: bool,
    obfuscation: Option<&'static str>,
}

pub fn score_email_address(value: &str) -> AddressScore {
    if let Some(caps) = ADDRESS_PATTERN.captures(value) {
        let name = caps.get(1).map_or("", |m| m.as_str()).trim();
        return AddressScore {
            has_name: !name.is_empty(),
            is_standard: true,
            obfuscation: None,
        };
    }
    if let Some(caps) = ADDRESS_OBFUSCATED_A_PATTERN.captures(value) {
        let name = caps.get(1).map_or("", |m| m.as_str()).trim();
        return AddressScore {
            has_name: !name.is_empty(),
            is_standard: false,
            obfuscation: Some("(a)"),
        };
    }
    if let Some(caps) = ADDRESS_OBFUSCATED_AT_PATTERN.captures(value) {
        let name = caps.get(1).map_or("", |m| m.as_str()).trim();
        return AddressScore {
            has_name: !name.is_empty(),
            is_standard: false,
            obfuscation: Some(" at "),
        };
    }
    AddressScore {
        has_name: false,
        is_standard: false,
        obfuscation: None,
    }
}
