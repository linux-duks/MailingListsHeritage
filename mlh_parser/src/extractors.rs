//! Extracts trailers (Signed-off-by, Reviewed-by, etc.) and patch diffs from
//! email body text.

use regex::Regex;
use std::sync::LazyLock;

use crate::Attribution;
use crate::address_parser::normalize_address;
use crate::entities::SubjectTags;

static RE_COPYPASTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(\S+:\s+[\da-f]+\s+\([^)]+)\n([^\n]+\))")
        .expect("RE_COPYPASTE regex must compile")
});

static RE_WRAPPED_SIGNATURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(\S+:\s+[^<]+)\n(<[^>]+>)$").expect("RE_WRAPPED_SIGNATURE regex must compile")
});

static RE_SIGNATURE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*(?P<type>[a-zA-Z\-]+-by):\s*(?P<name>[^<\n]+?)\s*<(?P<email>[^>\n]+)>")
        .expect("RE_SIGNATURE regex must compile")
});

/// Extracts git-style trailer lines from a commit message / email body.
///
/// Matches patterns like `Signed-off-by: Name <email>` and `Reviewed-by: Name <email>`.
/// Handles common copy-paste line wrapping and broken signature lines.
pub fn extract_attributions(commit_message: &str) -> Vec<Attribution> {
    let mut attributions = Vec::new();

    // Split on signature marker
    let body = commit_message.split("\n-- \n").next().unwrap_or("");

    // Fix common copypaste trailer wrapping
    let body = RE_COPYPASTE.replace_all(body, "$1 $2");

    // Fix line broken signature: Signed-off-by: Long Name\n<email.here@example.com>
    let body = RE_WRAPPED_SIGNATURE.replace_all(&body, "$1 $2");

    for caps in RE_SIGNATURE.captures_iter(&body) {
        let attr_type = caps.name("type").map_or("", |m| m.as_str()).trim();
        let name = caps.name("name").map_or("", |m| m.as_str()).trim();
        let email = caps.name("email").map_or("", |m| m.as_str()).trim();
        let identification = normalize_address(&format!("{} <{}>", name, email));
        attributions.push(Attribution {
            attribution: attr_type.to_string(),
            identification,
        });
    }

    attributions
}

static RE_DIFF_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?im)^diff (?:--git |-r )").expect("RE_DIFF_BLOCK must compile"));

/// Extracts patch diffs from an email body.
///
/// Adapted from B4's `LoreMessage.get_body_parts()` and DIFF_RE detection.
/// Splits the body on `---` separators (the git-format-patch commit/diff
/// boundary). Each section that contains `diff --git` content is treated as
/// a separate patch. Patches without a preceding `---` (commit-less diffs)
/// are also handled.
///
/// Multiple patches (multiple `---` sections) in a single body are returned
/// as separate entries. Multiple `diff --git` blocks within a single `---`
/// section are kept together as one patch (multi-file patches).
///
/// Source: https://github.com/mricon/b4/blob/main/src/b4/__init__.py
/// Licensed under GPLv2
pub fn extract_patches(email_body: &str) -> Vec<String> {
    if !RE_DIFF_BLOCK.is_match(email_body) {
        return Vec::new();
    }

    let sep_re = match regex::Regex::new(r"(?m)^---\s*$") {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let sep_positions: Vec<usize> = sep_re.find_iter(email_body).map(|m| m.start()).collect();

    let mut starts: Vec<usize> = Vec::new();

    for &pos in &sep_positions {
        starts.push(pos);
    }

    if RE_DIFF_BLOCK.is_match(email_body) {
        let body_before_first_sep = if let Some(&first_sep) = sep_positions.first() {
            &email_body[..first_sep]
        } else {
            email_body
        };
        if sep_positions.is_empty() || RE_DIFF_BLOCK.is_match(body_before_first_sep) {
            starts.push(0);
        }
    }

    starts.sort();
    starts.dedup();

    let mut patches = Vec::new();
    for i in 0..starts.len() {
        let start = starts[i];
        let end = if i + 1 < starts.len() {
            starts[i + 1]
        } else {
            email_body.len()
        };

        let section = &email_body[start..end];

        if RE_DIFF_BLOCK.is_match(section) {
            let patch = section.trim().to_string();
            if !patch.is_empty() {
                patches.push(patch);
            }
        }
    }

    patches
}

static RE_BRACKETS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]*?)\]").expect("re_brackets regex must compile"));

static RE_COLON_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b([a-z][a-z0-9_-]*):").expect("re_colon_tag regex must compile")
});

static RE_PATCH_STANDALONE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bpatch\b").expect("re_patch_standalone regex must compile"));

static RE_GLUED_PARTS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(patch|rfc|v\d+)").expect("re_glued_parts regex must compile")
});

static RE_IS_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^v(\d+)$").expect("re_is_version regex must compile"));

static RE_IS_SEQUENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+/\d+$").expect("re_is_sequence regex must compile"));

fn split_glued(token: &str) -> Vec<String> {
    let parts: Vec<String> = RE_GLUED_PARTS
        .find_iter(token)
        .map(|m| m.as_str().to_string())
        .collect();
    if parts.is_empty() {
        vec![token.to_string()]
    } else {
        parts
    }
}

struct TagState {
    tags: Vec<String>,
    has_patch: bool,
    has_rfc: bool,
    has_response: bool,
    has_forward: bool,
    version: Option<u16>,
    sequence: Option<String>,
}

impl TagState {
    fn new() -> Self {
        Self {
            tags: Vec::new(),
            has_patch: false,
            has_rfc: false,
            has_response: false,
            has_forward: false,
            version: None,
            sequence: None,
        }
    }

    fn push_tag(&mut self, token: &str) {
        for sub in &split_glued(token) {
            let lower = sub.to_lowercase();
            if lower == "patch" {
                self.has_patch = true;
                self.tags.push(sub.clone());
            } else if lower == "rfc" {
                self.has_rfc = true;
                self.tags.push(sub.clone());
            } else if lower == "re" || lower == "res" {
                self.has_response = true;
                self.tags.push(sub.clone());
            } else if lower == "fw" || lower == "fwd" || lower == "forward" {
                self.has_forward = true;
                self.tags.push(sub.clone());
            } else if let Some(caps) = RE_IS_VERSION.captures(sub) {
                self.version = caps[1].parse::<u16>().ok();
                self.tags.push(sub.clone());
            } else if RE_IS_SEQUENCE.is_match(sub) {
                self.sequence = Some(sub.clone());
                self.tags.push(sub.clone());
            } else {
                self.tags.push(sub.clone());
            }
        }
    }
}

pub fn extract_tags_from_subject(email_subject: &str) -> SubjectTags {
    let mut state = TagState::new();
    let mut tag_end: usize = 0;

    let subject = email_subject.trim();

    let first_bracket = subject.find('[');
    let raw_prefix = if let Some(pos) = first_bracket {
        &subject[..pos]
    } else {
        subject
    };
    let prefix = raw_prefix.split('"').next().unwrap_or("");
    let has_brackets = first_bracket.is_some();

    for caps in RE_COLON_TAG.captures_iter(prefix) {
        let tag = caps.get(1).unwrap().as_str();
        tag_end = tag_end.max(caps.get(0).unwrap().end());
        state.push_tag(tag);
    }

    if RE_PATCH_STANDALONE.is_match(prefix) {
        state.has_patch = true;
        if !has_brackets && let Some(m) = RE_PATCH_STANDALONE.find(prefix) {
            state.tags.push(m.as_str().to_string());
            tag_end = tag_end.max(m.end());
        }
    }

    for m in RE_BRACKETS.find_iter(subject) {
        let start = m.start();
        if start > 0 && subject.as_bytes()[start - 1].is_ascii_alphanumeric() {
            continue;
        }
        tag_end = tag_end.max(m.end());
        let full = m.as_str();
        let content = &full[1..full.len() - 1];
        let cleaned = content.replace(['[', ']'], "");
        for token in cleaned.split_whitespace() {
            let token = token.strip_suffix(':').unwrap_or(token);
            let token = token.trim_end_matches('.');
            if token.is_empty() {
                continue;
            }
            state.push_tag(token);
        }
    }

    if !has_brackets && let Some(col_pos) = subject.find(':') {
        tag_end = tag_end.max(col_pos + 1);
    }

    let untagged_subject = subject[tag_end..]
        .trim()
        .trim_start_matches(']')
        .trim()
        .to_string();

    SubjectTags {
        has_patch_tag: state.has_patch,
        has_rfc_tag: state.has_rfc,
        has_response_tag: state.has_response,
        has_forward_tag: state.has_forward,
        patch_version: state.version,
        patchset_sequence_number: state.sequence,
        subject_tags: state.tags,
        untagged_subject,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> String {
        v.to_string()
    }

    #[test]
    fn test_extract_tags_from_subject() {
        let cases = vec![
            (
                "[PATCH 5.15.y] wifi: mac80211: check tdls flag in ieee80211_tdls_oper",
                SubjectTags {
                    has_patch_tag: true,
                    subject_tags: vec![s("PATCH"), s("5.15.y")],
                    untagged_subject: s("wifi: mac80211: check tdls flag in ieee80211_tdls_oper"),
                    ..Default::default()
                },
            ),
            (
                "[RFC] 2.4.0-test6-pre2 Merge softirq, local_irq_count, local_bh_count",
                SubjectTags {
                    has_rfc_tag: true,
                    subject_tags: vec![s("RFC")],
                    untagged_subject: s(
                        "2.4.0-test6-pre2 Merge softirq, local_irq_count, local_bh_count",
                    ),
                    ..Default::default()
                },
            ),
            (
                "[patch] 2.4.0-test11 Elf64_Word incorrectly defined",
                SubjectTags {
                    has_patch_tag: true,
                    subject_tags: vec![s("patch")],
                    untagged_subject: s("2.4.0-test11 Elf64_Word incorrectly defined"),
                    ..Default::default()
                },
            ),
            (
                "[PATCH] xfrm: move policy_bydst RCU sync from per-netns .exit to .pre_exit",
                SubjectTags {
                    has_patch_tag: true,
                    subject_tags: vec![s("PATCH")],
                    untagged_subject: s(
                        "xfrm: move policy_bydst RCU sync from per-netns .exit to .pre_exit",
                    ),
                    ..Default::default()
                },
            ),
            (
                "[PATCH v2 0/3] libbpf: support STRUCT_OPS in light skeletons",
                SubjectTags {
                    has_patch_tag: true,
                    patch_version: Some(2),
                    patchset_sequence_number: Some(s("0/3")),
                    subject_tags: vec![s("PATCH"), s("v2"), s("0/3")],
                    untagged_subject: s("libbpf: support STRUCT_OPS in light skeletons"),
                    ..Default::default()
                },
            ),
            (
                "[PATCH v2 1/3] libbpf: load vmlinux BTF in gen_loader mode for struct_ops",
                SubjectTags {
                    has_patch_tag: true,
                    patch_version: Some(2),
                    patchset_sequence_number: Some(s("1/3")),
                    subject_tags: vec![s("PATCH"), s("v2"), s("1/3")],
                    untagged_subject: s(
                        "libbpf: load vmlinux BTF in gen_loader mode for struct_ops",
                    ),
                    ..Default::default()
                },
            ),
            (
                "[PATCH 0/2] bpf: cgroup: fix sysctl new-value handling in __cgroup_bpf_run_filter_sysctl",
                SubjectTags {
                    has_patch_tag: true,
                    patchset_sequence_number: Some(s("0/2")),
                    subject_tags: vec![s("PATCH"), s("0/2")],
                    untagged_subject: s(
                        "bpf: cgroup: fix sysctl new-value handling in __cgroup_bpf_run_filter_sysctl",
                    ),
                    ..Default::default()
                },
            ),
            (
                "Re: [PATCH RFC bpf-next 2/6] bpf: compute loops hierarchy",
                SubjectTags {
                    has_patch_tag: true,
                    has_rfc_tag: true,
                    has_response_tag: true,
                    patchset_sequence_number: Some(s("2/6")),
                    subject_tags: vec![s("Re"), s("PATCH"), s("RFC"), s("bpf-next"), s("2/6")],
                    untagged_subject: s("bpf: compute loops hierarchy"),
                    ..Default::default()
                },
            ),
            (
                "[PATCHv3 00/12] uprobes/x86: Fix red zone issue for optimized uprobes",
                SubjectTags {
                    has_patch_tag: true,
                    patch_version: Some(3),
                    patchset_sequence_number: Some(s("00/12")),
                    subject_tags: vec![s("PATCH"), s("v3"), s("00/12")],
                    untagged_subject: s("uprobes/x86: Fix red zone issue for optimized uprobes"),
                    ..Default::default()
                },
            ),
            (
                "[PATCH dwarves v5 00/11] pahole: Encode true signatures in kernel BTF",
                SubjectTags {
                    has_patch_tag: true,
                    patch_version: Some(5),
                    patchset_sequence_number: Some(s("00/11")),
                    subject_tags: vec![s("PATCH"), s("dwarves"), s("v5"), s("00/11")],
                    untagged_subject: s("pahole: Encode true signatures in kernel BTF"),
                    ..Default::default()
                },
            ),
            (
                "[PATCH 6.6.y v3 0/4] ksmbd: validate owner of durable handle on reconnect",
                SubjectTags {
                    has_patch_tag: true,
                    patch_version: Some(3),
                    patchset_sequence_number: Some(s("0/4")),
                    subject_tags: vec![s("PATCH"), s("6.6.y"), s("v3"), s("0/4")],
                    untagged_subject: s("ksmbd: validate owner of durable handle on reconnect"),
                    ..Default::default()
                },
            ),
            (
                "[to-be-updated] mm-cma-fix-reserved-page-leak-on-activation-failure.patch removed from -mm tree",
                SubjectTags {
                    subject_tags: vec![s("to-be-updated")],
                    untagged_subject: s(
                        "mm-cma-fix-reserved-page-leak-on-activation-failure.patch removed from -mm tree",
                    ),
                    ..Default::default()
                },
            ),
            (
                "Patch \"nvme: add quirk NVME_QUIRK_IGNORE_DEV_SUBNQN for 144d:a808 (Samsung PM981/983/970 EVO Plus )\" has been added to the 7.0-stable tree",
                SubjectTags {
                    has_patch_tag: true,
                    subject_tags: vec![s("Patch")],
                    untagged_subject: s(
                        "add quirk NVME_QUIRK_IGNORE_DEV_SUBNQN for 144d:a808 (Samsung PM981/983/970 EVO Plus )\" has been added to the 7.0-stable tree",
                    ),
                    ..Default::default()
                },
            ),
            (
                "FAILED: patch \"[PATCH] net: skbuff: propagate shared-frag marker through\" failed to apply to 5.15-stable tree",
                SubjectTags {
                    has_patch_tag: true,
                    subject_tags: vec![s("FAILED"), s("PATCH")],
                    untagged_subject: s(
                        "net: skbuff: propagate shared-frag marker through\" failed to apply to 5.15-stable tree",
                    ),
                    ..Default::default()
                },
            ),
            (
                "fwd: [Bug 9106] Sun Fire v100 dmfe driver bug",
                SubjectTags {
                    has_forward_tag: true,
                    subject_tags: vec![s("fwd"), s("Bug"), s("9106")],
                    untagged_subject: s("Sun Fire v100 dmfe driver bug"),
                    ..Default::default()
                },
            ),
            (
                "[PATCH v19 00/14] crypto/dmaengine: qce: introduce BAM locking and use DMA for register I/O",
                SubjectTags {
                    has_patch_tag: true,
                    patch_version: Some(19),
                    patchset_sequence_number: Some(s("00/14")),
                    subject_tags: vec![s("PATCH"), s("v19"), s("00/14")],
                    untagged_subject: s(
                        "crypto/dmaengine: qce: introduce BAM locking and use DMA for register I/O",
                    ),
                    ..Default::default()
                },
            ),
            (
                "[2.6 patch] fix dependencies of HUGETLB_PAGE_SIZE_64K",
                SubjectTags {
                    has_patch_tag: true,
                    subject_tags: vec![s("2.6"), s("patch")],
                    untagged_subject: s("fix dependencies of HUGETLB_PAGE_SIZE_64K"),
                    ..Default::default()
                },
            ),
            (
                "[2.4 PATCH] sparc64 dma parenthesis fixes",
                SubjectTags {
                    has_patch_tag: true,
                    subject_tags: vec![s("2.4"), s("PATCH")],
                    untagged_subject: s("sparc64 dma parenthesis fixes"),
                    ..Default::default()
                },
            ),
            (
                "[PATCH/RFC] io_remap_pfn_range()",
                SubjectTags {
                    has_patch_tag: true,
                    has_rfc_tag: true,
                    subject_tags: vec![s("PATCH"), s("RFC")],
                    untagged_subject: s("io_remap_pfn_range()"),
                    ..Default::default()
                },
            ),
            (
                "[Re: [Re: Linux Status for Sun Netra T1 200 ]]",
                SubjectTags {
                    has_response_tag: true,
                    subject_tags: vec![
                        s("Re"),
                        s("Re"),
                        s("Linux"),
                        s("Status"),
                        s("for"),
                        s("Sun"),
                        s("Netra"),
                        s("T1"),
                        s("200"),
                    ],
                    untagged_subject: s(""),
                    ..Default::default()
                },
            ),
            (
                "[Netra T1 200 status...]",
                SubjectTags {
                    subject_tags: vec![s("Netra"), s("T1"), s("200"), s("status")],
                    untagged_subject: s(""),
                    ..Default::default()
                },
            ),
            (
                "[CALL FOR TESTERS] SILO 0.8.7",
                SubjectTags {
                    subject_tags: vec![s("CALL"), s("FOR"), s("TESTERS")],
                    untagged_subject: s("SILO 0.8.7"),
                    ..Default::default()
                },
            ),
            (
                "[ath9k-devel] Shutting down the ath[59]k-devel mailing lists",
                SubjectTags {
                    subject_tags: vec![s("ath9k-devel")],
                    untagged_subject: s("Shutting down the ath[59]k-devel mailing lists"),
                    ..Default::default()
                },
            ),
            (
                "[ath9k-devel] [PATCH] ath9k: Switch to using",
                SubjectTags {
                    has_patch_tag: true,
                    subject_tags: vec![s("ath9k-devel"), s("PATCH")],
                    untagged_subject: s("ath9k: Switch to using"),
                    ..Default::default()
                },
            ),
            (
                "[ath9k-devel] [PATCH v3] ath9k: Switch to using mac80211 intermediate software queues.",
                SubjectTags {
                    has_patch_tag: true,
                    patch_version: Some(3),
                    subject_tags: vec![s("ath9k-devel"), s("PATCH"), s("v3")],
                    untagged_subject: s(
                        "ath9k: Switch to using mac80211 intermediate software queues.",
                    ),
                    ..Default::default()
                },
            ),
            (
                "[ath9k-devel] [RFC] ath9k: add devicetree support to ath9k",
                SubjectTags {
                    has_rfc_tag: true,
                    subject_tags: vec![s("ath9k-devel"), s("RFC")],
                    untagged_subject: s("ath9k: add devicetree support to ath9k"),
                    ..Default::default()
                },
            ),
            (
                "[PATCH AUTOSEL for 4.15 119/124] signal/metag: Document a conflict with SI_USER with SIGFPE",
                SubjectTags {
                    has_patch_tag: true,
                    patchset_sequence_number: Some(s("119/124")),
                    subject_tags: vec![s("PATCH"), s("AUTOSEL"), s("for"), s("4.15"), s("119/124")],
                    untagged_subject: s(
                        "signal/metag: Document a conflict with SI_USER with SIGFPE",
                    ),
                    ..Default::default()
                },
            ),
            (
                "Re: Demand dial doesn't raise ISP connection",
                SubjectTags {
                    has_response_tag: true,
                    subject_tags: vec![s("Re")],
                    untagged_subject: s("Demand dial doesn't raise ISP connection"),
                    ..Default::default()
                },
            ),
            (
                "FW: [Lustre-discuss] Troubles compile lustre with --enable-quota",
                SubjectTags {
                    has_forward_tag: true,
                    subject_tags: vec![s("FW"), s("Lustre-discuss")],
                    untagged_subject: s("Troubles compile lustre with --enable-quota"),
                    ..Default::default()
                },
            ),
            (
                "Re: c++ code - not getting compiled !!!!!!!!!",
                SubjectTags {
                    has_response_tag: true,
                    subject_tags: vec![s("Re")],
                    untagged_subject: s("c++ code - not getting compiled !!!!!!!!!"),
                    ..Default::default()
                },
            ),
            (
                "c++ code - not getting compiled !!!!!!!!!",
                SubjectTags {
                    untagged_subject: s("c++ code - not getting compiled !!!!!!!!!"),
                    ..Default::default()
                },
            ),
            (
                "Assembler errors in optimization level 3 (-O3) - gcc (4.1.2)",
                SubjectTags {
                    untagged_subject: s(
                        "Assembler errors in optimization level 3 (-O3) - gcc (4.1.2)",
                    ),
                    ..Default::default()
                },
            ),
        ];

        for (input, expected) in &cases {
            let result = extract_tags_from_subject(input);
            assert_eq!(&result, expected, "extract_tags_from_subject({:?})", input);
        }
    }
}
