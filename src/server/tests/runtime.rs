use crate::environment::runtime::jruby::classpath;

use crate::environment::config::runtime::{
    ProjectRuntimeSelection, RuntimeMode, RuntimeSelection, RuntimeSelectionConfig,
    SelectedRuntimeDescriptor,
};
use crate::environment::config::RubyFastLspConfig;
use crate::environment::runtime::catalog::{
    DiscoveredRuntime, RuntimeDiscoverySource, RuntimeImplementation, RuntimeStatusParams,
    RuntimeSupportStatus,
};
use crate::loader::scheduling::status::{IndexingPhase, IndexingSingleFlightReuseSnapshot};
use crate::server::RubyLanguageServer;
use tower_lsp::lsp_types::Url;

#[test]
fn indexing_snapshot_reports_process_local_classpath_file_reuse() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("project");
    let jruby = fixture.path().join("jruby");
    let java_home = fixture.path().join("jdk");
    for (path, bytes) in [
        (jruby.join("bin/jruby"), b"jruby".as_slice()),
        (jruby.join("lib/jruby.jar"), b"runtime".as_slice()),
        (
            java_home.join("jmods/java.base.jmod"),
            b"java base".as_slice(),
        ),
    ] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    std::fs::create_dir_all(&project).unwrap();
    let inputs = classpath::ClasspathInputs {
        project_root: project,
        jruby_executable: jruby.join("bin/jruby"),
        java_home,
        maven_repository: None,
        java_gem_roots: Vec::new(),
        additional_classpath: Vec::new(),
        additional_sources: Vec::new(),
    };
    let server = RubyLanguageServer::default();
    for _ in 0..2 {
        classpath::discover_project_classpath_with_cache(
            &inputs,
            classpath::ClasspathLimits::default(),
            &server.products.classpath_files(),
        )
        .unwrap();
    }

    assert_eq!(
        server
            .indexing_status_snapshot()
            .reuse
            .classpath_file_single_flight,
        IndexingSingleFlightReuseSnapshot {
            lookups: 4,
            hits: 2,
            joined_flights: 0,
            producers: 2,
            failures: 0,
        }
    );
}

#[tokio::test]
async fn runtime_status_reports_server_owned_project_identity_and_classpath() {
    let fixture = tempfile::tempdir().unwrap();
    let admin = fixture.path().join("admin");
    let server_project = fixture.path().join("server");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::create_dir_all(&server_project).unwrap();
    let language_server = RubyLanguageServer::default();
    let admin_workspace = language_server.add_workspace(Url::from_directory_path(&admin).unwrap());
    language_server.add_workspace(Url::from_directory_path(&server_project).unwrap());
    *language_server.config.lock() = RubyFastLspConfig {
        runtime: RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectRuntimeSelection {
                root: admin.to_string_lossy().to_string(),
                selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                    implementation: RuntimeImplementation::Jruby,
                    family: "9.2".to_string(),
                    engine_version: "9.2.21.0".to_string(),
                    compatibility_version: "2.5".to_string(),
                    executable: fixture.path().join("jruby/bin/jruby"),
                    discovery_source: RuntimeDiscoverySource::Rvm,
                    java_home: Some(fixture.path().join("jdk")),
                }),
            }],
        },
        ..RubyFastLspConfig::default()
    };
    language_server.set_runtime_classpath_fingerprint(&admin, Some("a".repeat(64)));
    let generation = admin_workspace
        .indexing_status
        .begin_generation()
        .generation;
    admin_workspace
        .indexing_status
        .transition(generation, IndexingPhase::Ready, None, None)
        .expect("test workspace must transition to ready");

    let status = language_server
        .handle_runtime_status(RuntimeStatusParams {
            project_root: Some(admin.clone()),
        })
        .await
        .unwrap();

    assert_eq!(status.projects.len(), 1);
    let status = &status.projects[0];
    assert_eq!(status.root, admin);
    assert_eq!(status.mode, "explicit");
    assert_eq!(status.implementation, Some(RuntimeImplementation::Jruby));
    assert_eq!(status.family.as_deref(), Some("9.2"));
    assert_eq!(status.engine_version.as_deref(), Some("9.2.21.0"));
    assert_eq!(status.compatibility_version.as_deref(), Some("2.5"));
    assert_eq!(status.stub_overlay.as_deref(), Some("9.2"));
    assert_eq!(
        status.classpath_fingerprint_sha256.as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert!(status.indexing_complete);
    assert_eq!(status.indexing.phase, IndexingPhase::Ready);

    let auto_runtime = SelectedRuntimeDescriptor {
        implementation: RuntimeImplementation::Mri,
        family: "3.3".to_string(),
        engine_version: "3.3.11".to_string(),
        compatibility_version: "3.3".to_string(),
        executable: fixture.path().join("ruby-3.3.11/bin/ruby"),
        discovery_source: RuntimeDiscoverySource::Rbenv,
        java_home: None,
    };
    language_server.set_effective_runtime(&server_project, Some(auto_runtime));
    let auto = language_server
        .handle_runtime_status(RuntimeStatusParams {
            project_root: Some(server_project),
        })
        .await
        .unwrap();
    let auto = &auto.projects[0];
    assert_eq!(auto.mode, "auto");
    assert_eq!(auto.implementation, Some(RuntimeImplementation::Mri));
    assert_eq!(auto.engine_version.as_deref(), Some("3.3.11"));
    assert_eq!(
        auto.executable,
        Some(fixture.path().join("ruby-3.3.11/bin/ruby"))
    );
}

#[tokio::test]
async fn auto_runtime_resolves_exact_project_marker_through_server_catalog() {
    let fixture = tempfile::tempdir().unwrap();
    let admin = fixture.path().join("admin");
    std::fs::create_dir_all(&admin).unwrap();
    std::fs::write(admin.join(".ruby-version"), "jruby-9.2.21.0\n").unwrap();
    let executable = fixture.path().join("jruby-9.2.21.0/bin/jruby");
    let java_home = fixture.path().join("jdk-17");
    let language_server = RubyLanguageServer::default();
    language_server.add_workspace(Url::from_directory_path(&admin).unwrap());
    language_server.set_discovered_runtimes_for_tests(vec![DiscoveredRuntime {
        implementation: RuntimeImplementation::Jruby,
        implementation_label: "JRuby".to_string(),
        family: "9.2".to_string(),
        family_label: "JRuby 9.2 (Ruby 2.5)".to_string(),
        compatibility_version: "2.5".to_string(),
        compatibility_label: "Ruby 2.5".to_string(),
        engine_version: "9.2.21.0".to_string(),
        display_name: "JRuby 9.2.21.0 (Ruby 2.5)".to_string(),
        executable: executable.clone(),
        discovery_source: RuntimeDiscoverySource::Rvm,
        support_status: RuntimeSupportStatus::Supported,
        java_home: Some(java_home.clone()),
    }]);

    let resolved = language_server
        .resolve_auto_runtime(&admin)
        .await
        .unwrap()
        .expect("the exact installed JRuby marker must resolve");
    assert_eq!(resolved.engine_version, "9.2.21.0");
    assert_eq!(resolved.compatibility_version, "2.5");
    assert_eq!(resolved.executable, executable);
    assert_eq!(resolved.java_home, Some(java_home));
}
