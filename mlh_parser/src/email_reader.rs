//! Low-level RFC 822 email decoding and header/body extraction via `mail-parser`.
//!
//! Also handles obfuscated email addresses (e.g. `user (a) domain.tld` → `user@domain.tld`).

use chrono::{DateTime, FixedOffset};
use mail_parser::{Message, MessageParser};
use regex::Regex;

use std::collections::HashMap;

pub fn header_value_to_string(val: &mail_parser::HeaderValue<'_>) -> Option<String> {
    match val {
        mail_parser::HeaderValue::Text(s) => Some(s.to_string()),
        mail_parser::HeaderValue::TextList(v) => Some(v.join(", ")),
        mail_parser::HeaderValue::Address(a) => {
            let merged = crate::address_parser::normalize_address_list(a);
            if merged.is_empty() {
                None
            } else if merged.len() > 1 {
                log::debug!("Converting many addresses to a single string: {:?}", val);
                Some(merged.join(" "))
            } else {
                Some(merged.into_iter().next().unwrap())
            }
        }
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
                Some(format!("{}; {}", st, attr_strs.join("; ")))
            } else {
                None
            }
        }
        mail_parser::HeaderValue::DateTime(d) => {
            log::error!(
                "Converting Date type to String on header read: 'DateTime' {:?}",
                val
            );
            Some(d.to_rfc3339())
        }
        mail_parser::HeaderValue::Received(r) => {
            log::error!(
                "Converting Date type to String on header read: 'Received' - {:?}",
                val
            );
            r.date.as_ref().map(|dt| dt.to_rfc3339())
        }
        other => Some(format!("{:?}", other)),
    }
}

pub fn header_value_to_string_list(val: &mail_parser::HeaderValue<'_>) -> Option<Vec<String>> {
    match val {
        mail_parser::HeaderValue::Text(s) => Some(vec![s.to_string()]),
        mail_parser::HeaderValue::TextList(v) => {
            Some(v.iter().map(|c| c.clone().into_owned()).collect())
        }
        mail_parser::HeaderValue::Address(a) => {
            let addrs = crate::address_parser::normalize_address_list(a);
            if addrs.is_empty() { None } else { Some(addrs) }
        }
        mail_parser::HeaderValue::ContentType(ct) => {
            // let st = if let Some(ref s) = ct.c_subtype {
            //     format!("{}/{}", ct.c_type, s)
            // } else {
            //     ct.c_type.to_string()
            // };
            if let Some(ref attrs) = ct.attributes {
                let attr_strs: Vec<String> = attrs
                    .iter()
                    .map(|a| format!("{}={}", a.name, a.value))
                    .collect();
                Some(attr_strs)
            } else {
                None
            }
        }
        mail_parser::HeaderValue::DateTime(_) => {
            unreachable!();
        }
        mail_parser::HeaderValue::Received(_) => {
            unreachable!();
        }
        other => Some(vec![format!("{:?}", other)]),
    }
}

pub fn header_value_date(val: &mail_parser::HeaderValue<'_>) -> Option<DateTime<FixedOffset>> {
    let date_result = match val {
        mail_parser::HeaderValue::DateTime(d) => Some(d.to_rfc3339()),
        mail_parser::HeaderValue::Received(r) => r.date.as_ref().map(|dt| dt.to_rfc3339()),
        _ => None,
    };
    match date_result {
        Some(d) => chrono::DateTime::parse_from_rfc3339(&d).ok(),
        None => None,
    }
}

/// some malformed emails leak the From headers into the body
pub fn extract_all_from_from_body(raw_email: &[u8]) -> Vec<String> {
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

pub fn extract_all_headers(msg: &Message<'_>) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    for header in msg.headers() {
        let key = header.name().to_lowercase();
        if let Some(val) = header_value_to_string(header.value()) {
            headers.insert(key, val);
        }
    }
    headers
}
