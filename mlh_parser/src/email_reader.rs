//! Low-level RFC 822 email decoding and header/body extraction via `mail-parser`.
//!
//! Also handles obfuscated email addresses (e.g. `user (a) domain.tld` → `user@domain.tld`).

use mail_parser::{Message, MessageParser};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

static EMAIL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\s]*([^<]*?)?\s*<?([a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,})>?\s*$")
        .unwrap()
});

static EMAIL_OBFUSCATED_A_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^[\s]*([^<]*?)?\s*<?([a-zA-Z0-9._%+-]+)\s*\(a\)\s*([a-zA-Z0-9.-]+\.[a-zA-Z]{2,})>?\s*$",
    )
    .unwrap()
});

static EMAIL_OBFUSCATED_AT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^[\s]*([^<]*?)?\s*<?([a-zA-Z0-9._%+-]+)\s+at\s+([a-zA-Z0-9.-]+\.[a-zA-Z]{2,})>?\s*$",
    )
    .unwrap()
});

// --- Helper functions ---
fn header_value_to_string(val: &mail_parser::HeaderValue<'_>) -> String {
    match val {
        mail_parser::HeaderValue::Text(s) => s.to_string(),
        mail_parser::HeaderValue::TextList(v) => v.join(", "),
        mail_parser::HeaderValue::Address(a) => {
            if let Some(first) = a.first() {
                addr_to_string(first)
            } else {
                String::new()
            }
        }
        mail_parser::HeaderValue::DateTime(d) => d.to_rfc3339(),
        mail_parser::HeaderValue::ContentType(ct) => {
            let st = if let Some(ref s) = ct.c_subtype {
                format!("{}/{}", ct.c_type, s)
            } else {
                ct.c_type.to_string()
            };
            if let Some(ref attrs) = ct.attributes {
                let attr_strs: Vec<String> = attrs
                    .iter()
                    .map(|a| format!("{}={}", a.name, a.value))
                    .collect();
                format!("{}; {}", st, attr_strs.join("; "))
            } else {
                st
            }
        }
        mail_parser::HeaderValue::Received(r) => {
            if let Some(ref dt) = r.date {
                let sign = if dt.tz_before_gmt { "-" } else { "+" };
                format!(
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}{:02}:{:02}",
                    dt.year, dt.month, dt.day,
                    dt.hour, dt.minute, dt.second,
                    sign, dt.tz_hour, dt.tz_minute,
                )
            } else {
                String::new()
            }
        }
        other => format!("{:?}", other),
    }
}

fn addr_to_string(addr: &mail_parser::Addr<'_>) -> String {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct EmailScore {
    has_name: bool,
    is_standard: bool,
    obfuscation: Option<&'static str>,
}

fn score_email_address(value: &str) -> EmailScore {
    if let Some(caps) = EMAIL_PATTERN.captures(value) {
        let name = caps.get(1).map_or("", |m| m.as_str()).trim();
        return EmailScore {
            has_name: !name.is_empty(),
            is_standard: true,
            obfuscation: None,
        };
    }
    if let Some(caps) = EMAIL_OBFUSCATED_A_PATTERN.captures(value) {
        let name = caps.get(1).map_or("", |m| m.as_str()).trim();
        return EmailScore {
            has_name: !name.is_empty(),
            is_standard: false,
            obfuscation: Some("(a)"),
        };
    }
    if let Some(caps) = EMAIL_OBFUSCATED_AT_PATTERN.captures(value) {
        let name = caps.get(1).map_or("", |m| m.as_str()).trim();
        return EmailScore {
            has_name: !name.is_empty(),
            is_standard: false,
            obfuscation: Some(" at "),
        };
    }
    EmailScore {
        has_name: false,
        is_standard: false,
        obfuscation: None,
    }
}

fn select_best_from_header(values: &[String]) -> String {
    if values.is_empty() {
        return String::new();
    }
    if values.len() == 1 {
        return normalize_email(&values[0]);
    }

    let mut scored: Vec<(EmailScore, &String)> =
        values.iter().map(|v| (score_email_address(v), v)).collect();
    scored.sort_by_key(|b| std::cmp::Reverse(b.0));
    normalize_email(scored[0].1)
}

fn normalize_email(value: &str) -> String {
    if let Some(caps) = EMAIL_OBFUSCATED_A_PATTERN.captures(value) {
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
    if let Some(caps) = EMAIL_OBFUSCATED_AT_PATTERN.captures(value) {
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
    value.to_string()
}

/// some malformed emails leak the From headers into the body
fn extract_all_from_from_body(raw_email: &[u8]) -> Vec<String> {
    let email_text = String::from_utf8_lossy(raw_email);
    let mut candidates = Vec::new();

    let from_patterns: &[&str] = &[
        r"(?im)^From:\s*([^<\n]*?)?\s*<([a-zA-Z0-9._%+-]+)@([a-zA-Z0-9.-]+\.[a-zA-Z]{2,})>",
        r"(?im)^From:\s*([^<\n]*?)?\s*<([a-zA-Z0-9._%+-]+)\s*\(a\)\s*([a-zA-Z0-9.-]+\.[a-zA-Z]{2,})>",
        r"(?im)^From:\s*([^<\n]*?)?\s*<?([a-zA-Z0-9._%+-]+)\s+at\s+([a-zA-Z0-9.-]+\.[a-zA-Z]{2,})>?",
    ];

    for pat_str in from_patterns {
        let re = Regex::new(pat_str).unwrap();
        for caps in re.captures_iter(&email_text) {
            let name = caps.get(1).map_or("", |m| m.as_str()).trim();
            let email = format!(
                "{}@{}",
                caps.get(2).unwrap().as_str(),
                caps.get(3).unwrap().as_str()
            );
            if name.is_empty() {
                candidates.push(email);
            } else {
                candidates.push(format!("{} <{}>", name, email));
            }
        }
    }

    candidates
}

/// Decodes raw email bytes into a [`Message`] using the default parser.
pub fn decode_mail(email_raw: &[u8]) -> Option<Message<'_>> {
    MessageParser::default().parse(email_raw)
}

/// Extracts all headers from a parsed message into a `HashMap`.
///
/// Evaluates headers using body information to better guide `From` selection.
/// The `From` header is chosen by scoring candidates (name presence, valid
/// email address). Obfuscated addresses are normalized.
pub fn get_headers(msg: &Message<'_>) -> HashMap<String, String> {
    let mut headers: HashMap<String, String> = HashMap::new();
    let mut from_candidates: Vec<String> = Vec::new();

    for header in msg.headers() {
        let key = header.name().to_lowercase();
        let val_str = header_value_to_string(header.value());

        if key == "from" {
            from_candidates.push(val_str);
        } else if key == "date" {
            headers.insert("date".to_string(), val_str);
        } else {
            headers
                .entry(key)
                .and_modify(|existing| {
                    *existing = format!("{}, {}", existing, val_str);
                })
                .or_insert(val_str);
        }
    }

    if from_candidates.is_empty()
        && let Some(from) = msg.from()
    {
        for addr in from.iter() {
            from_candidates.push(addr_to_string(addr));
        }
    }

    let body_from = extract_all_from_from_body(&msg.raw_message);

    from_candidates.extend(body_from);

    if !from_candidates.is_empty() {
        headers.insert(
            "from".to_string(),
            select_best_from_header(&from_candidates),
        );
    }

    headers
}

/// Extracts the body text from a parsed message.
///
/// Prefers `text/plain` parts. Falls back to `text/html` if no plain text
/// is found. Multi-part bodies are joined with newlines, and CRLF is
/// normalized to LF.
pub fn get_body(msg: &Message<'_>) -> String {
    let mut body_parts: Vec<String> = Vec::new();

    for i in 0.. {
        if let Some(text) = msg.body_text(i) {
            if !text.is_empty() {
                body_parts.push(text.to_string());
            }
        } else {
            break;
        }
    }

    if body_parts.is_empty() {
        for i in 0.. {
            if let Some(html) = msg.body_html(i) {
                if !html.is_empty() {
                    body_parts.push(html.to_string());
                }
            } else {
                break;
            }
        }
    }

    // join multi part messages
    let body = body_parts.join("\n");
    // replace CRLF for line feed
    body.replace("\r\n", "\n")
}
