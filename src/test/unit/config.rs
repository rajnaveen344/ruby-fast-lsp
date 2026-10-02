#[cfg(test)]
mod tests {
    use crate::environment::config::{FormatterKind, LinterKind, RubyFastLspConfig};
    use serde_json::json;

    #[test]
    fn test_config_default() {
        let config = RubyFastLspConfig::default();

        assert_eq!(config.ruby_version, "auto");
        assert_eq!(config.linter, LinterKind::None);
        assert!(config.linter_command.is_empty());
    }

    #[test]
    fn test_linter_configuration_deserialization() {
        let config: RubyFastLspConfig = serde_json::from_value(json!({
            "linter": "standard",
            "linterCommand": ["bundle", "exec", "standardrb"]
        }))
        .unwrap();

        assert_eq!(config.linter, LinterKind::Standard);
        assert_eq!(config.linter_command, vec!["bundle", "exec", "standardrb"]);
    }

    #[test]
    fn test_indexing_configuration_round_trips() {
        let input = json!({
            "indexing": {
                "excludedPatterns": ["vendor/**/*", "tmp/**/*.rb"],
                "includedPatterns": ["bin/*"],
                "excludedGems": ["debug"],
                "includedGems": ["rails"],
                "projectRoots": ["services/billing", "services/identity"],
                "loadPaths": {
                    "default": ["shared/lib"],
                    "projects": [
                        {
                            "root": "/repo/server",
                            "paths": ["custom_lib"]
                        }
                    ]
                }
            }
        });

        let config: RubyFastLspConfig = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(
            config
                .indexing
                .load_paths
                .paths_for_project(std::path::Path::new("/repo/server")),
            &["custom_lib".to_string()]
        );
        assert_eq!(
            config
                .indexing
                .load_paths
                .paths_for_project(std::path::Path::new("/repo/admin")),
            &["shared/lib".to_string()]
        );
        let output = serde_json::to_value(config).unwrap();
        assert_eq!(output["indexing"], input["indexing"]);
    }

    #[test]
    fn legacy_flat_load_paths_become_workspace_default() {
        let config: RubyFastLspConfig = serde_json::from_value(json!({
            "indexing": {
                "loadPaths": ["custom_lib", "shared/lib"]
            }
        }))
        .unwrap();

        assert_eq!(
            config.indexing.load_paths.default,
            vec!["custom_lib".to_string(), "shared/lib".to_string()]
        );
        assert!(config.indexing.load_paths.projects.is_empty());
    }

    #[test]
    fn test_formatter_configuration_deserialization() {
        let config: RubyFastLspConfig = serde_json::from_value(json!({
            "formatter": "standard",
            "formatterCommand": ["bin/standardrb"]
        }))
        .unwrap();

        assert_eq!(config.formatter, FormatterKind::Standard);
        assert_eq!(config.formatter_command, vec!["bin/standardrb"]);
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
    fn test_config_partial_deserialization() {
        // Test that partial configuration works with defaults
        let json_config = json!({
            "rubyVersion": "2.7"
        });

        let config: RubyFastLspConfig = serde_json::from_value(json_config).unwrap();

        assert_eq!(config.ruby_version, "2.7");
    }
}
