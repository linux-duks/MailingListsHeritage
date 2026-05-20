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
    value.to_string()
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
