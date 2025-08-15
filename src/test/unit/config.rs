#[cfg(test)]
mod tests {
    use crate::config::{RubyFastLspConfig, VersionDetectionConfig};
    use serde_json::json;

    #[test]
    fn test_config_default() {
        let config = RubyFastLspConfig::default();

        assert_eq!(config.ruby_version, "auto");
        assert_eq!(config.enable_core_stubs, true);
        assert_eq!(config.version_detection.enable_rbenv, true);
        assert_eq!(config.version_detection.enable_rvm, true);
        assert_eq!(config.version_detection.enable_chruby, true);
        assert_eq!(config.version_detection.enable_system_ruby, true);
    }

    #[test]
    fn test_config_deserialization() {
        let json_config = json!({
            "rubyVersion": "3.2",
            "enableCoreStubs": false,
            "versionDetection": {
                "enableRbenv": false,
                "enableRvm": true,
                "enableChruby": false,
                "enableSystemRuby": true
            }
        });

        let config: RubyFastLspConfig = serde_json::from_value(json_config).unwrap();

        assert_eq!(config.ruby_version, "3.2");
        assert_eq!(config.enable_core_stubs, false);
        assert_eq!(config.version_detection.enable_rbenv, false);
        assert_eq!(config.version_detection.enable_rvm, true);
        assert_eq!(config.version_detection.enable_chruby, false);
        assert_eq!(config.version_detection.enable_system_ruby, true);
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
        assert_eq!(config.enable_core_stubs, true); // Should use default

        // Version detection should use defaults
        assert_eq!(config.version_detection.enable_rbenv, true);
        assert_eq!(config.version_detection.enable_rvm, true);
        assert_eq!(config.version_detection.enable_chruby, true);
        assert_eq!(config.version_detection.enable_system_ruby, true);
    }

    #[test]
    fn test_version_detection_config_default() {
        let config = VersionDetectionConfig::default();

        assert_eq!(config.enable_rbenv, true);
        assert_eq!(config.enable_rvm, true);
        assert_eq!(config.enable_chruby, true);
        assert_eq!(config.enable_system_ruby, true);
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
