#[cfg(test)]
mod tests {
    use crate::config::RubyFastLspConfig;
    use serde_json::json;

    #[test]
    fn test_config_default() {
        let config = RubyFastLspConfig::default();

        assert_eq!(config.ruby_version, "auto");
    }

    #[test]
    fn test_config_deserialization() {
        let json_config = json!({
            "rubyVersion": "3.0"
        });

        let config: RubyFastLspConfig = serde_json::from_value(json_config).unwrap();

        assert_eq!(config.ruby_version, "3.0");
    }

    #[test]
    fn test_config_get_ruby_version() {
        // Test with configured version
        let mut config = RubyFastLspConfig::default();
        config.ruby_version = "3.1".to_string();

        let version = config.get_ruby_version();
        assert_eq!(version, Some((3, 1)));

        // Test with auto version
        config.ruby_version = "auto".to_string();
        let version = config.get_ruby_version();
        assert_eq!(version, None);

        // Test with invalid version
        config.ruby_version = "invalid".to_string();
        let version = config.get_ruby_version();
        assert_eq!(version, None);
    }

    #[test]
    fn test_config_partial_deserialization() {
        // Test that partial configuration works with defaults
        let json_config = json!({
            "rubyVersion": "2.7"
        });

        let config: RubyFastLspConfig = serde_json::from_value(json_config).unwrap();

        assert_eq!(config.ruby_version, "2.7");
    }

    #[test]
    fn test_ruby_version_parsing() {
        let test_cases = vec![
            ("3.0", Some((3, 0))),
            ("3.1", Some((3, 1))),
            ("2.7", Some((2, 7))),
            ("1.9", Some((1, 9))),
            ("auto", None),
            ("invalid", None),
            ("3", None),             // Missing minor version
            ("3.0.1", Some((3, 0))), // Should ignore patch version
        ];

        for (input, expected) in test_cases {
            let mut config = RubyFastLspConfig::default();
            config.ruby_version = input.to_string();

            let result = config.get_ruby_version();
            assert_eq!(result, expected, "Failed for input: {}", input);
        }
    }
}
