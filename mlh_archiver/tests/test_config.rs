use mlh_archiver::archive_writer::WriteMode;
use mlh_archiver::config::{AppConfig, RunMode};
use mlh_archiver::errors::ConfigError;
use mlh_archiver::nntp_source::nntp_config::NntpConfig;

// =============================================================================
// AppConfig Deserialization Tests
// =============================================================================

#[test]
fn test_app_config_defaults() {
    let config = AppConfig::default();
    assert_eq!(config.nthreads, 1);
    assert_eq!(config.output_dir, "./output/archiver");
    assert!(config.loop_groups);
    assert!(config.nntp.is_none());
}

#[test]
fn test_app_config_deserialize_nested_format() {
    let yaml = r#"
nthreads: 4
output_dir: "./custom_output"
loop_groups: false
read_lists:
  nntp: ["list1", "list2"]
nntp:
  hostname: "nntp.example.com"
  port: 563
"#;
    let config: AppConfig = serde_yaml::from_str(yaml).expect("Failed to parse");
    assert_eq!(config.nthreads, 4);
    assert_eq!(config.output_dir, "./custom_output");
    assert!(!config.loop_groups);
    assert!(config.nntp.is_some());
    let nntp = config.nntp.unwrap();
    assert_eq!(nntp.hostname, "nntp.example.com");
    assert_eq!(nntp.port, Some(563));
    assert_eq!(
        config.read_lists.get("nntp"),
        Some(&vec!["list1".to_string(), "list2".to_string()])
    );
}

#[test]
fn test_app_config_deserialize_defaults() {
    let yaml = r#"
nntp:
  hostname: "nntp.example.com"
"#;
    let config: AppConfig = serde_yaml::from_str(yaml).expect("Failed to parse");
    assert_eq!(config.nthreads, 1);
    assert_eq!(config.output_dir, "./output/archiver");
    assert!(config.loop_groups);
}

#[test]
fn test_app_config_deserialize_missing_nntp() {
    let yaml = r#"
nthreads: 2
output_dir: "./test"
"#;
    let config: AppConfig = serde_yaml::from_str(yaml).expect("Failed to parse");
    assert!(config.nntp.is_none());
}

#[test]
fn test_app_config_deserialize_invalid_port_type() {
    let yaml = r#"
nntp:
  hostname: "nntp.example.com"
  port: "not_a_number"
"#;
    let result: Result<AppConfig, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err());
}

#[test]
fn test_nntp_config_default_port() {
    let yaml = r#"
nntp:
  hostname: "nntp.example.com"
"#;
    let config: AppConfig = serde_yaml::from_str(yaml).expect("Failed to parse");
    assert_eq!(config.nntp.as_ref().unwrap().port, None);
}

#[test]
fn test_app_config_deserialize_with_auth() {
    let yaml = r#"
nntp:
  hostname: "nntp.example.com"
  port: 563
  username: "myuser"
  password: "mypass"
read_lists:
  nntp: ["list1"]
"#;
    let config: AppConfig = serde_yaml::from_str(yaml).expect("Failed to parse");
    assert!(config.nntp.is_some());
    let nntp = config.nntp.unwrap();
    assert_eq!(nntp.hostname, "nntp.example.com");
    assert_eq!(nntp.port, Some(563));
    assert_eq!(nntp.username, Some("myuser".to_string()));
    assert_eq!(nntp.password, Some("mypass".to_string()));
    assert_eq!(
        config.read_lists.get("nntp"),
        Some(&vec!["list1".to_string()])
    );
}

#[test]
fn test_app_config_deserialize_auth_defaults_to_none() {
    let yaml = r#"
nntp:
  hostname: "nntp.example.com"
"#;
    let config: AppConfig = serde_yaml::from_str(yaml).expect("Failed to parse");
    assert!(config.nntp.is_some());
    let nntp = config.nntp.unwrap();
    assert!(nntp.username.is_none());
    assert!(nntp.password.is_none());
}

// =============================================================================
// NntpConfig Tests
// =============================================================================

#[test]
fn test_nntp_config_validate_success() {
    let config = NntpConfig {
        hostname: "nntp.example.com".to_string(),
        ..NntpConfig::default()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn test_nntp_config_validate_missing_hostname() {
    let config = NntpConfig {
        hostname: String::new(),
        ..NntpConfig::default()
    };
    let result = config.validate();
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), ConfigError::MissingHostname));
}

#[test]
fn test_nntp_config_server_address_default_port() {
    let config = NntpConfig {
        hostname: "nntp://nntp.example.com".to_string(),
        ..NntpConfig::default()
    };
    assert_eq!(config.server_address(), "nntp://nntp.example.com");
}

#[test]
fn test_nntp_config_server_address_custom_port() {
    let config = NntpConfig {
        hostname: "nntp://nntp.example.com".to_string(),
        port: Some(8119),
        ..NntpConfig::default()
    };
    assert_eq!(config.server_address(), "nntp://nntp.example.com:8119");
}

#[test]
fn test_nntp_config_with_credentials() {
    let config = NntpConfig {
        hostname: "nntp.example.com".to_string(),
        username: Some("user".to_string()),
        password: Some("pass".to_string()),
        ..NntpConfig::default()
    };
    assert_eq!(config.username, Some("user".to_string()));
    assert_eq!(config.password, Some("pass".to_string()));
}

#[test]
fn test_nntp_config_defaults_no_credentials() {
    let config = NntpConfig::default();
    assert!(config.username.is_none());
    assert!(config.password.is_none());
}

// =============================================================================
// AppConfig Methods Tests
// =============================================================================

#[test]
fn test_app_config_get_nntp_config_with_nntp() {
    let config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        nntp: Some(NntpConfig {
            hostname: "nntp.example.com".to_string(),
            ..NntpConfig::default()
        }),
        ..Default::default()
    };
    let nntp = config.nntp.unwrap();
    assert_eq!(nntp.hostname, "nntp.example.com");
}

#[test]
fn test_app_config_get_nntp_config_without_nntp() {
    let config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        nntp: None,
        ..Default::default()
    };
    assert!(config.nntp.is_none());
}

// =============================================================================
// get_read_lists() Tests
// =============================================================================

#[test]
fn test_get_read_lists_star_glob() {
    let mut read_lists = std::collections::HashMap::new();
    read_lists.insert("nntp".to_string(), vec!["*".to_string()]);

    let mut config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        read_lists,
        nntp: Some(NntpConfig {
            hostname: "nntp.example.com".to_string(),
            ..NntpConfig::default()
        }),
        ..Default::default()
    };

    let available_lists = vec![
        "list1".to_string(),
        "list2".to_string(),
        "list3".to_string(),
    ];

    let result = config.get_read_lists(available_lists.clone(), RunMode::NNTP);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), available_lists);
}

#[test]
fn test_get_read_lists_specific_lists() {
    let mut read_lists = std::collections::HashMap::new();
    read_lists.insert(
        "nntp".to_string(),
        vec!["list1".to_string(), "list2".to_string()],
    );

    let mut config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        read_lists,
        nntp: Some(NntpConfig {
            hostname: "nntp.example.com".to_string(),
            ..NntpConfig::default()
        }),
        ..Default::default()
    };

    let available_lists = vec![
        "list1".to_string(),
        "list2".to_string(),
        "list3".to_string(),
    ];

    let result = config.get_read_lists(available_lists, RunMode::NNTP);
    assert!(result.is_ok());
    let lists = result.unwrap();
    assert_eq!(lists.len(), 2);
    assert!(lists.contains(&"list1".to_string()));
    assert!(lists.contains(&"list2".to_string()));
}

#[test]
fn test_get_read_lists_filters_invalid() {
    let mut read_lists = std::collections::HashMap::new();
    read_lists.insert(
        "nntp".to_string(),
        vec!["valid_list".to_string(), "invalid_list".to_string()],
    );

    let mut config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        read_lists,
        nntp: Some(NntpConfig {
            hostname: "nntp.example.com".to_string(),
            ..NntpConfig::default()
        }),
        ..Default::default()
    };

    let available_lists = vec!["valid_list".to_string(), "another_valid_list".to_string()];

    let result = config.get_read_lists(available_lists, RunMode::NNTP);
    assert!(result.is_ok());
    let lists = result.unwrap();
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0], "valid_list");
}

#[test]
fn test_get_read_lists_all_invalid() {
    // Configuring only invalid (non-existent) list names should return an error
    let mut read_lists = std::collections::HashMap::new();
    read_lists.insert(
        "nntp".to_string(),
        vec!["invalid1".to_string(), "invalid2".to_string()],
    );

    let mut config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        read_lists,
        nntp: Some(NntpConfig {
            hostname: "nntp.example.com".to_string(),
            ..NntpConfig::default()
        }),
        ..Default::default()
    };

    let available_lists = vec!["valid_list".to_string(), "another_valid_list".to_string()];

    let result = config.get_read_lists(available_lists, RunMode::NNTP);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ConfigError::AllListsUnavailable
    ));
}

#[test]
fn test_get_read_lists_deduplicates() {
    let mut read_lists = std::collections::HashMap::new();
    read_lists.insert(
        "nntp".to_string(),
        vec![
            "list1".to_string(),
            "list2".to_string(),
            "list1".to_string(),
        ],
    );

    let mut config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        read_lists,
        nntp: Some(NntpConfig {
            hostname: "nntp.example.com".to_string(),
            ..NntpConfig::default()
        }),
        ..Default::default()
    };

    let available_lists = vec!["list1".to_string(), "list2".to_string()];

    let result = config.get_read_lists(available_lists, RunMode::NNTP);
    assert!(result.is_ok());
    let lists = result.unwrap();
    assert_eq!(lists.len(), 2);
}

// =============================================================================
// get_email_range() Tests
// =============================================================================

#[test]
fn test_get_email_range_none() {
    let config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        nntp: Some(NntpConfig {
            hostname: "nntp.example.com".to_string(),
            ..NntpConfig::default()
        }),
        ..Default::default()
    };

    let result = config.get_range_selection_text(RunMode::NNTP);
    assert!(result.is_none());
}

#[test]
fn test_get_email_range_single_number() {
    let config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        nntp: Some(NntpConfig {
            hostname: "nntp.example.com".to_string(),
            port: Some(119),
            email_range: Some("100".to_string()),
            ..NntpConfig::default()
        }),
        ..Default::default()
    };

    let result = config.get_range_selection_text(RunMode::NNTP);
    assert!(result.is_some());
    let range: Vec<usize> = mlh_archiver::range_inputs::parse_sequence(&result.unwrap())
        .unwrap()
        .collect();
    assert_eq!(range, vec![100]);
}

#[test]
fn test_get_email_range_multiple_numbers() {
    let config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        nntp: Some(NntpConfig {
            hostname: "nntp.example.com".to_string(),
            email_range: Some("1,5,10".to_string()),
            ..NntpConfig::default()
        }),
        ..Default::default()
    };

    let result = config.get_range_selection_text(RunMode::NNTP);
    assert!(result.is_some());
    let range: Vec<usize> = mlh_archiver::range_inputs::parse_sequence(&result.unwrap())
        .unwrap()
        .collect();
    assert_eq!(range, vec![1, 5, 10]);
}

#[test]
fn test_get_email_range_dash_range() {
    let config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        nntp: Some(NntpConfig {
            hostname: "nntp.example.com".to_string(),
            port: Some(119),
            email_range: Some("1-5".to_string()),
            ..NntpConfig::default()
        }),
        ..Default::default()
    };

    let result = config.get_range_selection_text(RunMode::NNTP);
    assert!(result.is_some());
    let range: Vec<usize> = mlh_archiver::range_inputs::parse_sequence(&result.unwrap())
        .unwrap()
        .collect();
    assert_eq!(range, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_get_email_range_mixed() {
    let config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        nntp: Some(NntpConfig {
            hostname: "nntp.example.com".to_string(),
            email_range: Some("1,3-5,10".to_string()),
            ..NntpConfig::default()
        }),
        ..Default::default()
    };

    let result = config.get_range_selection_text(RunMode::NNTP);
    assert!(result.is_some());
    let range: Vec<usize> = mlh_archiver::range_inputs::parse_sequence(&result.unwrap())
        .unwrap()
        .collect();
    assert_eq!(range, vec![1, 3, 4, 5, 10]);
}

#[test]
fn test_get_email_range_invalid() {
    let config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        nntp: Some(NntpConfig {
            hostname: "nntp.example.com".to_string(),
            port: Some(119),
            email_range: Some("invalid".to_string()),
            ..NntpConfig::default()
        }),
        ..Default::default()
    };

    // get_range_selection_text returns the raw string
    let result = config.get_range_selection_text(RunMode::NNTP);
    assert!(result.is_some());
    assert_eq!(result.unwrap(), "invalid");

    // But parsing it fails
    let parsed = mlh_archiver::range_inputs::parse_sequence("invalid");
    assert!(parsed.is_err());
}

#[test]
fn test_get_email_range_no_nntp() {
    let config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        nntp: None,
        ..Default::default()
    };

    // When nntp is None, get_range_selection_text returns None
    let result = config.get_range_selection_text(RunMode::NNTP);
    assert!(result.is_none());
}

// =============================================================================
// Integration Tests
// =============================================================================

#[test]
fn test_full_config_workflow() {
    let yaml = r#"
nthreads: 3
output_dir: "./integration_test_output"
loop_groups: true
read_lists:
  nntp: ["list1", "list2"]
nntp:
  hostname: "nntp.example.com"
  port: 119
  email_range: "1-10"
"#;

    let mut config: AppConfig = serde_yaml::from_str(yaml).expect("Failed to parse");

    // Verify parsed values
    assert_eq!(config.nthreads, 3);
    assert_eq!(config.output_dir, "./integration_test_output");
    assert!(config.loop_groups);
    assert!(config.nntp.is_some());

    // Verify email range parsing
    let range = config.get_range_selection_text(RunMode::NNTP);
    assert!(range.is_some());
    let range_vec: Vec<usize> = mlh_archiver::range_inputs::parse_sequence(&range.unwrap())
        .unwrap()
        .collect();
    assert_eq!(range_vec, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

    // Verify group lists
    let available_lists = vec![
        "list1".to_string(),
        "list2".to_string(),
        "list3".to_string(),
    ];
    let groups = config.get_read_lists(available_lists, RunMode::NNTP);
    assert!(groups.is_ok());
    assert_eq!(groups.unwrap().len(), 2);
}

#[test]
fn test_config_validation_workflow() {
    // Test that validation catches missing hostname
    let config = AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        nntp: Some(NntpConfig {
            hostname: String::new(),
            ..NntpConfig::default()
        }),
        ..Default::default()
    };

    if let Some(ref nntp) = config.nntp {
        let result = nntp.validate();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::MissingHostname));
    }
}

// =============================================================================
// Glob Pattern Matching Tests
// =============================================================================

/// Helper to create a config with specific read_lists
fn config_with_read_lists(lists: Vec<String>) -> AppConfig {
    let mut read_lists = std::collections::HashMap::new();
    read_lists.insert("nntp".to_string(), lists);
    AppConfig {
        nthreads: 1,
        output_dir: "./output".to_string(),
        loop_groups: true,
        read_lists,
        nntp: Some(NntpConfig {
            hostname: "nntp.example.com".to_string(),
            port: Some(119),
            ..NntpConfig::default()
        }),
        ..Default::default()
    }
}

/// Available lists matching the test fixtures (db.yml)
fn test_fixture_lists() -> Vec<String> {
    vec![
        "test.groups.foo".to_string(),
        "test.groups.bar".to_string(),
        "test.groups.empty".to_string(),
        "test.groups.synthetic".to_string(),
    ]
}

#[test]
fn test_get_read_lists_glob_star_suffix() {
    // test.groups.* should match all 4 test groups
    let mut config = config_with_read_lists(vec!["test.groups.*".to_string()]);
    let available = test_fixture_lists();

    let result = config.get_read_lists(available, RunMode::NNTP);
    assert!(result.is_ok());
    let lists = result.unwrap();
    assert_eq!(lists.len(), 4);
    assert!(lists.contains(&"test.groups.foo".to_string()));
    assert!(lists.contains(&"test.groups.bar".to_string()));
    assert!(lists.contains(&"test.groups.empty".to_string()));
    assert!(lists.contains(&"test.groups.synthetic".to_string()));
}

#[test]
fn test_get_read_lists_glob_partial_match() {
    // *.synth* should match only test.groups.synthetic
    let mut config = config_with_read_lists(vec!["*.synth*".to_string()]);
    let available = test_fixture_lists();

    let result = config.get_read_lists(available, RunMode::NNTP);
    assert!(result.is_ok());
    let lists = result.unwrap();
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0], "test.groups.synthetic");
}

#[test]
fn test_get_read_lists_glob_no_match() {
    // nonexistent.* should match nothing → error
    let mut config = config_with_read_lists(vec!["nonexistent.*".to_string()]);
    let available = test_fixture_lists();

    let result = config.get_read_lists(available, RunMode::NNTP);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ConfigError::AllListsUnavailable
    ));
}

#[test]
fn test_get_read_lists_glob_mixed_patterns() {
    // Mix of exact name and glob: test.groups.foo + *.synth*
    let mut config =
        config_with_read_lists(vec!["test.groups.foo".to_string(), "*.synth*".to_string()]);
    let available = test_fixture_lists();

    let result = config.get_read_lists(available, RunMode::NNTP);
    assert!(result.is_ok());
    let lists = result.unwrap();
    assert_eq!(lists.len(), 2);
    assert!(lists.contains(&"test.groups.foo".to_string()));
    assert!(lists.contains(&"test.groups.synthetic".to_string()));
}

#[test]
fn test_get_read_lists_glob_question_mark() {
    // test.groups.fo? should match test.groups.foo (single char wildcard)
    // but NOT test.groups.foobar (if it existed)
    let mut config = config_with_read_lists(vec!["test.groups.fo?".to_string()]);
    let available = test_fixture_lists();

    let result = config.get_read_lists(available, RunMode::NNTP);
    assert!(result.is_ok());
    let lists = result.unwrap();
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0], "test.groups.foo");
}

#[test]
fn test_get_read_lists_glob_deduplication() {
    // test.groups.* matches all, test.groups.foo is already included → no dupes
    let mut config = config_with_read_lists(vec![
        "test.groups.*".to_string(),
        "test.groups.foo".to_string(),
    ]);
    let available = test_fixture_lists();

    let result = config.get_read_lists(available, RunMode::NNTP);
    assert!(result.is_ok());
    let lists = result.unwrap();
    assert_eq!(lists.len(), 4); // Should be 4, not 5
}

#[test]
fn test_get_read_lists_glob_partial_warning() {
    // Valid glob + invalid pattern → should succeed but warn about invalid one
    let mut config = config_with_read_lists(vec![
        "test.groups.foo".to_string(),
        "nonexistent.list".to_string(),
    ]);
    let available = test_fixture_lists();

    let result = config.get_read_lists(available, RunMode::NNTP);
    assert!(result.is_ok());
    let lists = result.unwrap();
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0], "test.groups.foo");
}

#[test]
fn test_get_read_lists_glob_multiple_globs() {
    // Multiple glob patterns: test.groups.* + *.empty (overlapping, tests dedup)
    let mut config =
        config_with_read_lists(vec!["test.groups.*".to_string(), "*.empty".to_string()]);
    let available = test_fixture_lists();

    let result = config.get_read_lists(available, RunMode::NNTP);
    assert!(result.is_ok());
    let lists = result.unwrap();
    // test.groups.* matches all 4, *.empty matches empty (already included) → 4 total
    assert_eq!(lists.len(), 4);
    assert!(lists.contains(&"test.groups.foo".to_string()));
    assert!(lists.contains(&"test.groups.bar".to_string()));
    assert!(lists.contains(&"test.groups.empty".to_string()));
    assert!(lists.contains(&"test.groups.synthetic".to_string()));
}

#[test]
fn test_is_glob_pattern_detection() {
    use mlh_archiver::config::is_glob_pattern;

    // Star should be detected as glob
    assert!(is_glob_pattern("test.*"));
    assert!(is_glob_pattern("*"));
    assert!(is_glob_pattern("test.*.list"));

    // Question mark should be detected as glob
    assert!(is_glob_pattern("test.?"));
    assert!(is_glob_pattern("?"));

    // No glob characters should not be detected
    assert!(!is_glob_pattern("test.groups.foo"));
    assert!(!is_glob_pattern(""));
    assert!(!is_glob_pattern("test.groups.foo.bar"));
}

#[test]
fn test_expand_glob_patterns_unit() {
    use mlh_archiver::config::expand_glob_patterns;

    let available = vec![
        "test.groups.foo".to_string(),
        "test.groups.bar".to_string(),
        "test.groups.synthetic".to_string(),
    ];

    // Star suffix
    let (matched, unmatched) = expand_glob_patterns(&["test.groups.*".to_string()], &available);
    assert_eq!(matched.len(), 3);
    assert!(unmatched.is_empty());

    // Partial glob
    let (matched, _) = expand_glob_patterns(&["*.synth*".to_string()], &available);
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0], "test.groups.synthetic");

    // Mixed: exact + glob
    let (matched, unmatched) = expand_glob_patterns(
        &["test.groups.foo".to_string(), "*.synth*".to_string()],
        &available,
    );
    assert_eq!(matched.len(), 2);
    assert!(unmatched.is_empty());

    // Unmatched pattern
    let (matched, unmatched) = expand_glob_patterns(&["nonexistent.*".to_string()], &available);
    assert!(matched.is_empty());
    assert_eq!(unmatched.len(), 1);
    assert_eq!(unmatched[0], "nonexistent.*");
}

// =============================================================================
// WriteMode Deserialization Tests
// =============================================================================

#[test]
fn test_app_config_default_write_mode() {
    let config = AppConfig::default();
    assert_eq!(
        config.write_mode,
        WriteMode::Parquet {
            buffer_size: 10_000
        }
    );
}

#[test]
fn test_app_config_deserialize_write_mode_raw() {
    let yaml = r#"
write_mode: raw_email
nntp:
  hostname: "nntp.example.com"
"#;
    let config: AppConfig = serde_yaml::from_str(yaml).expect("Failed to parse");
    assert_eq!(config.write_mode, WriteMode::RawEmails);
}

#[test]
fn test_app_config_deserialize_write_mode_parquet_default_buffer() {
    // Parquet without explicit buffer size is not valid via string; must use default
    // Test that the default is applied when write_mode is omitted instead
    let yaml = r#"
nntp:
  hostname: "nntp.example.com"
"#;
    let config: AppConfig = serde_yaml::from_str(yaml).expect("Failed to parse");
    // Default is applied via #[serde(default)]
    assert_eq!(
        config.write_mode,
        WriteMode::Parquet {
            buffer_size: 10_000
        }
    );
}

#[test]
fn test_app_config_deserialize_write_mode_parquet_with_buffer() {
    let yaml = r#"
write_mode: "parquet:5000"
nntp:
  hostname: "nntp.example.com"
"#;
    let config: AppConfig = serde_yaml::from_str(yaml).expect("Failed to parse");
    assert_eq!(config.write_mode, WriteMode::Parquet { buffer_size: 5000 });
}

#[test]
fn test_app_config_deserialize_write_mode_invalid() {
    let yaml = r#"
write_mode: "invalid_mode"
nntp:
  hostname: "nntp.example.com"
"#;
    let result: Result<AppConfig, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err());
}

#[test]
fn test_app_config_deserialize_write_mode_missing_uses_default() {
    // When write_mode is omitted, #[serde(default)] kicks in
    let yaml = r#"
nntp:
  hostname: "nntp.example.com"
"#;
    let config: AppConfig = serde_yaml::from_str(yaml).expect("Failed to parse");
    assert_eq!(
        config.write_mode,
        WriteMode::Parquet {
            buffer_size: 10_000
        }
    );
}

#[test]
fn test_write_mode_display() {
    assert_eq!(WriteMode::RawEmails.to_string(), "raw_email");
    assert_eq!(
        WriteMode::Parquet { buffer_size: 5000 }.to_string(),
        "'parquet:5000'"
    );
    assert_eq!(
        WriteMode::Parquet {
            buffer_size: 10_000
        }
        .to_string(),
        "'parquet:10000'"
    );
}

#[test]
fn test_write_mode_from_str() {
    use std::str::FromStr;
    assert_eq!(
        WriteMode::from_str("raw_email").unwrap(),
        WriteMode::RawEmails
    );
    assert_eq!(WriteMode::from_str("raw").unwrap(), WriteMode::RawEmails);
    assert_eq!(
        WriteMode::from_str("parquet:10000").unwrap(),
        WriteMode::Parquet {
            buffer_size: 10_000
        }
    );
    assert_eq!(
        WriteMode::from_str("parquet:3000").unwrap(),
        WriteMode::Parquet { buffer_size: 3000 }
    );
    assert!(WriteMode::from_str("parquet").is_err());
    assert!(WriteMode::from_str("invalid").is_err());
}
