//! Configured gem selection and exact effective runtime selection.

use super::*;

#[test]
fn test_configured_gem_selection_augments_inferred_and_preserves_exclusions() {
    let indexing = crate::environment::config::IndexingConfig {
        included_gems: vec!["rails".to_string(), "debug".to_string()],
        excluded_gems: vec!["debug".to_string(), "rack".to_string()],
        ..crate::environment::config::IndexingConfig::default()
    };

    let (required, excluded) =
        configured_gem_selection(vec!["rack".to_string(), "rspec".to_string()], &indexing);

    assert_eq!(
        required,
        HashSet::from(["rails".to_string(), "rspec".to_string()])
    );
    assert_eq!(
        excluded,
        HashSet::from(["debug".to_string(), "rack".to_string()])
    );
}

#[test]
fn configured_ruby_version_overrides_runtime_auto_detection() {
    let fixture = TestProjectFixture::new();
    let config = RubyFastLspConfig {
        ruby_version: "2.5".to_string(),
        ..RubyFastLspConfig::default()
    };
    let mut coordinator = IndexingCoordinator::new(fixture.project_root().clone(), config);

    assert_eq!(
        coordinator.detect_ruby_version(),
        Some(RubyVersion::new(2, 5)),
        "an explicit Ruby version must select its matching core stubs"
    );
}

#[tokio::test]
async fn unavailable_auto_runtime_uses_conservative_core_fallback() {
    let fixture = TestProjectFixture::new();
    std::fs::write(fixture.project_root().join(".ruby-version"), "3.2.999\n").unwrap();
    let server = create_test_server();
    server.set_discovered_runtimes_for_tests(Vec::new());
    let mut coordinator =
        IndexingCoordinator::new(fixture.project_root().clone(), RubyFastLspConfig::default());
    coordinator
        .resolve_effective_runtime(&server)
        .await
        .unwrap();
    assert!(coordinator.effective_runtime.is_none());
    assert_eq!(coordinator.detect_ruby_version_off_reactor(&server).await.unwrap(), None,
        "an unavailable automatic runtime must use the bundled Ruby 3.0 fallback, not derive compatibility from an unfulfilled marker");
    coordinator.config.ruby_version = "2.5".to_string();
    assert_eq!(
        coordinator
            .detect_ruby_version_off_reactor(&server)
            .await
            .unwrap(),
        Some(RubyVersion::new(2, 5)),
        "explicit compatibility configuration must still win over automatic fallback"
    );
}

#[tokio::test]
async fn auto_runtime_marker_becomes_the_exact_effective_runtime() {
    use crate::environment::runtime::catalog::{
        DiscoveredRuntime, RuntimeDiscoverySource, RuntimeSupportStatus,
    };

    let fixture = TestProjectFixture::new();
    std::fs::write(
        fixture.project_root().join(".ruby-version"),
        "jruby-9.2.21.0\n",
    )
    .unwrap();
    let server = create_test_server();
    server.add_workspace(Url::from_directory_path(fixture.project_root()).unwrap());
    server.set_discovered_runtimes_for_tests(vec![DiscoveredRuntime {
        implementation: RuntimeImplementation::Jruby,
        implementation_label: "JRuby".to_string(),
        family: "9.2".to_string(),
        family_label: "JRuby 9.2 (Ruby 2.5)".to_string(),
        compatibility_version: "2.5".to_string(),
        compatibility_label: "Ruby 2.5".to_string(),
        engine_version: "9.2.21.0".to_string(),
        display_name: "JRuby 9.2.21.0 (Ruby 2.5)".to_string(),
        executable: fixture.project_root().join("runtime/bin/jruby"),
        discovery_source: RuntimeDiscoverySource::Rvm,
        support_status: RuntimeSupportStatus::Supported,
        java_home: Some(fixture.project_root().join("jdk")),
    }]);
    let mut coordinator =
        IndexingCoordinator::new(fixture.project_root().clone(), RubyFastLspConfig::default());

    coordinator
        .resolve_effective_runtime(&server)
        .await
        .unwrap();
    assert_eq!(
        coordinator.detect_ruby_version(),
        Some(RubyVersion::new_with_implementation(
            2,
            5,
            RubyImplementation::JRuby
        ))
    );
    assert_eq!(
        coordinator
            .effective_runtime
            .as_ref()
            .map(|runtime| runtime.engine_version.as_str()),
        Some("9.2.21.0")
    );
}
