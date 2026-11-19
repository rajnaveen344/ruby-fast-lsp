//! Signature files a locked gem ships in its own `sig/` folder.

use super::*;
use crate::utils::persistent_cache::PersistentProductStat;

const WIDGET_SOURCE: &str = concat!(
    "class SharedWidget\n",
    "  def label(prefix)\n",
    "    compute(prefix)\n",
    "  end\n",
    "end\n",
);
const WIDGET_SIGNATURE: &str = concat!(
    "class SharedWidget\n",
    "  def label: (String prefix) -> String\n",
    "end\n",
);

fn widget_indexer(project_root: &Path, gem_root: &Path) -> IndexerGem {
    let mut indexer = IndexerGem::new(Some(project_root.to_path_buf()));
    indexer.set_required_gems(HashSet::from(["shared_widget".to_string()]));
    indexer.discovered_gems.insert(
        "shared_widget".to_string(),
        vec![GemInfo {
            name: "shared_widget".to_string(),
            version: "1.0.0".to_string(),
            platform: "ruby".to_string(),
            locked_version: "1.0.0".to_string(),
            source: GemSource::BundlerInstalled,
            path: gem_root.to_path_buf(),
            lib_paths: vec![gem_root.join("lib")],
            dependencies: Vec::new(),
            is_default: false,
        }],
    );
    indexer.set_file_processor(FileProcessor::new());
    indexer.set_dependency_seed_engine(Arc::new(Project::new()));
    indexer
}

fn write_widget_gem(gem_root: &Path) {
    fs::create_dir_all(gem_root.join("lib")).unwrap();
    fs::create_dir_all(gem_root.join("sig")).unwrap();
    fs::write(gem_root.join("lib/shared_widget.rb"), WIDGET_SOURCE).unwrap();
    fs::write(gem_root.join("sig/shared_widget.rbs"), WIDGET_SIGNATURE).unwrap();
}

fn label_return_labels(engine: &Project) -> Vec<Option<String>> {
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("SharedWidget").unwrap()]);
    let query = engine.view();
    let request = ruby_analysis::engine::lookup::MethodRequest::new(
        ruby_analysis::engine::lookup::LookupReceiver::Namespace(&owner),
        RubyMethod::new("label").unwrap(),
        ruby_analysis::engine::lookup::MethodWant::Signatures,
    );
    ruby_analysis::engine::lookup::method(&query, request)
        .into_signature_vec()
        .into_iter()
        .map(|signature| signature.return_type_label.map(|label| label.to_string()))
        .collect()
}

async fn index_widget(server: &Server, project_root: &Path, gem_root: &Path) -> Project {
    let engine = Arc::new(parking_lot::RwLock::new(Project::new()));
    widget_indexer(project_root, gem_root)
        .index_required_gems_with_shared_product(
            &server.load_context_for_project(project_root),
            engine.clone(),
        )
        .await
        .unwrap();
    Arc::try_unwrap(engine)
        .unwrap_or_else(|_| panic!("the gem indexer retained the test engine"))
        .into_inner()
}

#[tokio::test]
async fn locked_gem_signature_declares_a_return_its_source_does_not_prove() {
    let fixture = TempDir::new().unwrap();
    let project_root = fixture.path().join("project");
    let gem_root = project_root.join("bundle/shared_widget-1.0.0");
    write_widget_gem(&gem_root);
    // Signature folders the locked gem does not own must contribute nothing.
    let other_version = project_root.join("bundle/shared_widget-2.0.0");
    fs::create_dir_all(other_version.join("sig")).unwrap();
    fs::write(
        other_version.join("sig/shared_widget.rbs"),
        "class SharedWidget\n  def label: (String prefix) -> Integer\nend\n",
    )
    .unwrap();
    fs::create_dir_all(gem_root.join("stdlib")).unwrap();
    fs::write(
        gem_root.join("stdlib/bundled.rbs"),
        "class BundledElsewhere\nend\n",
    )
    .unwrap();
    let server = Server::with_user_cache_root(fixture.path().join("user-cache"))
        .expect("construct isolated cache server");

    let cold = index_widget(&server, &project_root, &gem_root).await;

    assert_eq!(label_return_labels(&cold), [Some("String".to_string())]);
    let signature_path = gem_root.join("sig/shared_widget.rbs");
    let signature_file = cold.view().file_id(&signature_path).unwrap();
    assert_eq!(
        cold.view().file(signature_file).unwrap().kind,
        SourceKind::Signature
    );
    assert!(cold
        .view()
        .file_id(&other_version.join("sig/shared_widget.rbs"))
        .is_none());
    assert!(cold
        .view()
        .file_id(&gem_root.join("stdlib/bundled.rbs"))
        .is_none());

    let warm = index_widget(&server, &project_root, &gem_root).await;

    let persistent = server.shared_products().persistent().gem_product_snapshot();
    assert_eq!(persistent.get(PersistentProductStat::Producers), 1);
    assert_eq!(persistent.get(PersistentProductStat::Hits), 1);
    assert_eq!(label_return_labels(&warm), label_return_labels(&cold));
}

#[tokio::test]
async fn editing_a_locked_gem_signature_changes_the_product_identity() {
    let fixture = TempDir::new().unwrap();
    let project_root = fixture.path().join("project");
    let gem_root = project_root.join("bundle/shared_widget-1.0.0");
    write_widget_gem(&gem_root);
    let indexer = widget_indexer(&project_root, &gem_root);
    let seed = Project::new().view().semantic_context_fingerprint();
    let before = indexer.required_gem_manifests(seed).unwrap()[0]
        .key()
        .clone();

    fs::write(
        gem_root.join("sig/shared_widget.rbs"),
        "class SharedWidget\n  def label: (String prefix) -> Symbol\nend\n",
    )
    .unwrap();

    let after = indexer.required_gem_manifests(seed).unwrap()[0]
        .key()
        .clone();
    assert_ne!(before, after);
}

#[tokio::test]
async fn unreadable_gem_signature_keeps_the_gem_source_and_other_signatures() {
    let fixture = TempDir::new().unwrap();
    let project_root = fixture.path().join("project");
    let gem_root = project_root.join("bundle/shared_widget-1.0.0");
    write_widget_gem(&gem_root);
    fs::write(
        gem_root.join("sig/broken.rbs"),
        "class SharedWidget\n  def label: (\n",
    )
    .unwrap();
    let server = Server::with_user_cache_root(fixture.path().join("user-cache"))
        .expect("construct isolated cache server");

    let engine = index_widget(&server, &project_root, &gem_root).await;

    assert_eq!(label_return_labels(&engine), [Some("String".to_string())]);
    let source = engine
        .view()
        .file_id(&gem_root.join("lib/shared_widget.rb"))
        .unwrap();
    assert_eq!(engine.view().file(source).unwrap().kind, SourceKind::Gem);
    assert!(engine
        .view()
        .file_id(&gem_root.join("sig/broken.rbs"))
        .is_some());
}

/// The return of `SharedWidget#label` as a caller observes it.
fn label_return(engine: &Project) -> Option<String> {
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("SharedWidget").unwrap()]);
    let view = engine.view();
    let request = ruby_analysis::engine::lookup::MethodRequest::new(
        ruby_analysis::engine::lookup::LookupReceiver::Namespace(&owner),
        RubyMethod::new("label").unwrap(),
        ruby_analysis::engine::lookup::MethodWant::Return,
    );
    ruby_analysis::engine::lookup::method(&view, request)
        .into_return_type()
        .map(|ruby_type| ruby_type.to_string())
}

async fn label_return_with_documented_type(documented: &str) -> Option<String> {
    let fixture = TempDir::new().unwrap();
    let project_root = fixture.path().join("project");
    let gem_root = project_root.join("bundle/shared_widget-1.0.0");
    write_widget_gem(&gem_root);
    fs::write(
        gem_root.join("lib/shared_widget.rb"),
        format!(
            "class SharedWidget\n  # @return [{documented}]\n  def label(prefix)\n    compute(prefix)\n  end\nend\n"
        ),
    )
    .unwrap();
    let server = Server::with_user_cache_root(fixture.path().join("cache")).unwrap();
    let engine = index_widget(&server, &project_root, &gem_root).await;
    label_return(&engine)
}

#[tokio::test]
async fn gem_signature_answers_when_the_documented_return_names_no_known_type() {
    assert_eq!(
        label_return_with_documented_type("unsigned_count").await,
        Some("String".to_string()),
        "a documented return this server cannot resolve proves nothing, so the shipped signature answers",
    );
}

#[tokio::test]
async fn gem_signature_outranks_the_documented_return_as_a_project_signature_does() {
    assert_eq!(
        label_return_with_documented_type("Integer").await,
        Some("String".to_string()),
    );
}

#[tokio::test]
async fn gem_signature_answers_when_part_of_the_documented_return_names_no_known_type() {
    assert_eq!(
        label_return_with_documented_type("Integer, unsigned_count").await,
        Some("String".to_string()),
    );
}
