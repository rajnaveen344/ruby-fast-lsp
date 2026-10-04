//! JRuby's method-style class lookup on a Java package module.

use super::jruby_replay::jruby_provider;
use super::*;
use crate::invariant::ExpectInvariant;
use ruby_analysis::core::{FullyQualifiedName, RubyType, TypeSubject};

#[test]
fn package_module_call_resolves_a_catalog_class_without_unresolved_constants() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let source_path = root.join("package_call.rb");
    std::fs::write(
        &source_path,
        "module Admin\n  WIDGET = Java::ComExample.Widget\n  INSTANCE = Java::ComExample.Widget.new\n  MISSING = Java::ComExample.Missing\nend\n",
    )
    .unwrap();
    let signature_cache = TempDir::new().unwrap();
    let provider = Arc::new(
        jruby_provider(&["com/example/Widget"])
            .with_signature_cache_root(signature_cache.path().to_path_buf()),
    );
    let server = Server::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new().with_jruby_import_provider(provider),
        IndexingConfig::default(),
    );
    indexer
        .collect_project_facts(&server.load_context_for_project(indexer.workspace_root()))
        .unwrap();
    workspace_state.handle().test_write().resolve();

    let engine = workspace_state.handle().test_read();
    let file_id = engine.view().file_id(&source_path).expect_invariant(
        "package call fixture was not indexed",
        "the project pass registers every source under the root",
        "keep the fixture inside the project root",
    );
    let proxy = FullyQualifiedName::try_from("Java::ComExample::Widget").unwrap();
    let types = engine.view().type_facts_in_file(file_id);
    for (constant, expected) in [
        ("Admin::WIDGET", RubyType::ClassReference(proxy.clone())),
        ("Admin::INSTANCE", RubyType::Class(proxy.clone())),
    ] {
        let constant = FullyQualifiedName::try_from(constant).unwrap();
        assert!(
            types.iter().any(|fact| {
                fact.subject == TypeSubject::Constant(constant.clone())
                    && fact.ruby_type == expected
            }),
            "{constant} must resolve through the catalog-proven package call to {expected}; \
             indexed types: {types:?}"
        );
    }
    let missing = FullyQualifiedName::try_from("Admin::MISSING").unwrap();
    assert!(
        types.iter().all(|fact| {
            fact.subject != TypeSubject::Constant(missing.clone())
                || !matches!(fact.ruby_type, RubyType::ClassReference(_))
        }),
        "a class absent from the catalog must not be guessed from package-call syntax; \
         indexed types: {types:?}"
    );
    let unresolved = engine
        .view()
        .diagnostic_facts_with_code_in_file(file_id, "unresolved-constant")
        .map(|fact| fact.message.clone())
        .collect::<Vec<_>>();
    assert!(
        unresolved.is_empty(),
        "a package module proven by the catalog must resolve; got: {unresolved:?}"
    );
}
