#![allow(dead_code)]
// why cant clippy not find these functions being used in other test files ?

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use mlh_parser::Attribution;

pub fn list_files_with_extension(directory: &str, extension: &str) -> Vec<PathBuf> {
    let ext = if extension.starts_with('.') {
        extension.to_string()
    } else {
        format!(".{}", extension)
    };

    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let dir = base.join(directory.trim_start_matches("./"));

    let dot_ext = ext;

    let mut files: Vec<PathBuf> = match fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().ends_with(&dot_ext))
            })
            .filter(|e| e.file_type().is_ok_and(|ft| ft.is_file()))
            .map(|e| e.path())
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files
}

pub fn map_to_file_extensions(email_file_path: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let stem = email_file_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let parent = email_file_path.parent().unwrap_or(Path::new(""));

    extensions
        .iter()
        .map(|ext| parent.join(format!("{}{}", stem, ext)))
        .collect()
}

pub fn parse_date_file(date_file: &Path) -> String {
    match fs::read_to_string(date_file) {
        Ok(content) => content.lines().next().unwrap_or("").trim().to_string(),
        Err(_) => String::new(),
    }
}

pub fn parse_body_file(body_file: &Path) -> String {
    match fs::read_to_string(body_file) {
        Ok(content) => content.replace("\r\n", "\n"),
        Err(_) => String::new(),
    }
}

pub fn list_fixture_pairs(directory: &str, expected_ext: &str) -> Vec<(PathBuf, PathBuf)> {
    let expected_files = list_files_with_extension(directory, expected_ext);
    expected_files
        .into_iter()
        .filter_map(|expected_file| {
            let eml_file = expected_to_eml(&expected_file, expected_ext);
            eml_file.exists().then_some((expected_file, eml_file))
        })
        .collect()
}

fn expected_to_eml(expected_path: &Path, expected_ext: &str) -> PathBuf {
    let suffix = expected_ext.trim_start_matches('.');
    let file_name = expected_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let base = match file_name.strip_suffix(suffix) {
        Some(b) => b.trim_end_matches('.').to_string(),
        None => expected_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    };
    let parent = expected_path.parent().unwrap_or(Path::new(""));
    parent.join(format!("{}.eml", base))
}

pub fn parse_headers_file(headers_file: &Path) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    let content = match fs::read_to_string(headers_file) {
        Ok(c) => c.replace("\r\n", "\n"),
        Err(_) => return headers,
    };

    let mut current_header: Option<String> = None;
    let mut current_value = String::new();

    for line in content.lines() {
        let line = line.to_string();

        if line.trim().is_empty() || line.starts_with("--") {
            // End of headers
            if let Some(ref key) = current_header {
                headers.insert(key.clone(), current_value.clone());
            }
            break;
        }

        // Check for continuation line (starts with space/tab)
        if line.starts_with(' ') || line.starts_with('\t') {
            if current_header.is_some() {
                current_value.push(' ');
                current_value.push_str(line.trim());
            }
            continue;
        }

        // Save previous header
        if let Some(ref key) = current_header {
            headers.insert(key.clone(), current_value.clone());
        }

        // Parse new header
        if let Some(colon_pos) = line.find(':') {
            current_header = Some(line[..colon_pos].trim().to_lowercase());
            current_value = line[colon_pos + 1..].trim().to_string();
        } else {
            current_header = None;
            current_value = String::new();
        }
    }

    headers
}

pub fn parse_patches_file(patches_file: &Path) -> Vec<String> {
    let mut patches = Vec::new();
    let content = match fs::read_to_string(patches_file) {
        Ok(c) => c.replace("\r\n", "\n"),
        Err(_) => return patches,
    };

    let content = content.trim();
    let inner = content
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(content)
        .trim();

    let mut remaining = inner;
    while let Some(start) = remaining.find("\"\"\"") {
        let after_open = &remaining[start + 3..];
        match after_open.find("\"\"\"") {
            Some(end) => {
                let patch = after_open[..end].trim().to_string();
                patches.push(patch);
                remaining = &after_open[end + 3..];
            }
            None => break,
        }
    }

    patches
}

pub fn parse_trailers_file(file: &Path) -> Vec<Attribution> {
    let mut trailers = Vec::new();
    let content = match fs::read_to_string(file) {
        Ok(c) => c.replace("\r\n", "\n"),
        Err(_) => return trailers,
    };

    let mut current_attribution = String::new();
    let mut current_identification = String::new();
    let mut in_block = false;

    for line in content.lines() {
        let line = line.trim();

        if line.contains('{') {
            in_block = true;
            current_attribution.clear();
            current_identification.clear();
        }

        if in_block {
            if let Some(value) = extract_json_value(line, "attribution") {
                current_attribution = value;
            } else if let Some(value) = extract_json_value(line, "identification") {
                current_identification = value;
            }
        }

        if line.contains('}') {
            if in_block && !current_attribution.is_empty() {
                trailers.push(Attribution {
                    attribution: current_attribution.clone(),
                    identification: current_identification.clone(),
                });
            }
            in_block = false;
        }
    }

    trailers
}

fn extract_json_value(line: &str, key: &str) -> Option<String> {
    let line = line.trim();
    for quote in ['"', '\''] {
        let prefix = format!("{q}{key}{q}: {q}", q = quote);
        if let Some(pos) = line.find(&prefix) {
            let after = &line[pos + prefix.len()..];
            if let Some(end_pos) = after.find(quote) {
                return Some(after[..end_pos].to_string());
            }
        }
    }
    None
}
