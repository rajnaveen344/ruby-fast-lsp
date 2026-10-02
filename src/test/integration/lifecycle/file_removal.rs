//! A deleted file leaves the project: its diagnostics are cleared, other
//! files stop resolving into it, and a recreated file starts from fresh facts.
//! Open consumers of a changed or deleted closed file are republished.

use std::path::{Path, PathBuf};

use ruby_analysis::engine::SemanticResultFingerprint;
use tower_lsp::lsp_types::FileChangeType;

use crate::test::harness::FakeEditor;

const GATEWAY: &str =
    "require_relative \"missing_part\"\n\nclass Gateway\n  def capture\n  end\nend\n";
const BILLING: &str = "require_relative \"gateway\"\n\nGateway.new.capture\n";
const RECREATED_GATEWAY: &str = "class Gateway\n  def refund\n  end\nend\n";
const LEDGER: &str = "class Ledger\nend\n";

struct Project {
    _root: tempfile::TempDir,
    root: PathBuf,
}

impl Project {
    fn new() -> Self {
        let root = tempfile::TempDir::new().expect("create project root");
        let path = root.path().canonicalize().expect("canonical project root");
        Self {
            _root: root,
            root: path,
        }
    }

    fn file(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }

    fn write(&self, name: &str, content: &str) -> String {
        std::fs::write(self.root.join(name), content).expect("write project file");
        self.file(name)
    }

    fn delete(&self, name: &str) -> String {
        std::fs::remove_file(self.root.join(name)).expect("delete project file");
        self.file(name)
    }

    async fn editor(&self) -> FakeEditor {
        let editor = FakeEditor::new().await;
        editor.add_workspace(&self.root.to_string_lossy());
        editor
    }
}

fn registered_paths(editor: &FakeEditor, filename: &str) -> Vec<PathBuf> {
    let workspace = editor
        .workspace_for(filename)
        .expect("file has a workspace");
    let engine = workspace.analysis_engine.read();
    let mut paths = engine
        .view()
        .files()
        .filter(|file| file.path.starts_with(Path::new(filename).parent().unwrap()))
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn result_fingerprint(editor: &FakeEditor, filename: &str) -> SemanticResultFingerprint {
    let workspace = editor
        .workspace_for(filename)
        .expect("file has a workspace");
    let engine = workspace.analysis_engine.read();
    engine.view().semantic_result_fingerprint()
}

#[tokio::test]
async fn watched_delete_clears_diagnostics_and_cross_file_targets() {
    let project = Project::new();
    let mut editor = project.editor().await;
    let gateway = project.write("gateway.rb", GATEWAY);
    let billing = project.write("billing.rb", BILLING);

    editor.open(&gateway, GATEWAY).await;
    editor.close(&gateway).await;
    let closed = editor.delivered_diagnostics(&gateway).await;
    assert!(
        closed
            .iter()
            .any(|diagnostic| diagnostic.message.contains("missing_part")),
        "a closed file keeps its unresolved-require diagnostic: {closed:?}"
    );

    editor.open(&billing, BILLING).await;
    let gateway_uri = crate::test::harness::fixture_uri(&gateway);
    let targets = editor.goto_def_at(&billing, 2, 13).await;
    assert_eq!(
        targets.iter().map(|target| &target.uri).collect::<Vec<_>>(),
        vec![&gateway_uri],
        "the call resolves into the file before it is deleted"
    );
    let require_targets = editor.goto_def_at(&billing, 0, 19).await;
    assert_eq!(
        require_targets
            .iter()
            .map(|target| &target.uri)
            .collect::<Vec<_>>(),
        vec![&gateway_uri],
        "the require resolves to the file before it is deleted"
    );

    project.delete("gateway.rb");
    editor
        .watched_file_changed(&gateway, FileChangeType::DELETED)
        .await;

    assert_eq!(editor.delivered_diagnostics(&gateway).await, Vec::new());
    assert_eq!(editor.goto_def_at(&billing, 2, 13).await, Vec::new());
    assert_eq!(editor.goto_def_at(&billing, 0, 19).await, Vec::new());
    assert!(
        !editor
            .references_with_declaration_at(&billing, 2, 13, true)
            .await
            .iter()
            .any(|location| location.uri == gateway_uri),
        "references must not point into a deleted file"
    );
    assert_eq!(
        registered_paths(&editor, &billing),
        vec![PathBuf::from(&billing)]
    );
    let consumer = editor.diagnostics(&billing).await;
    assert!(
        consumer
            .iter()
            .any(|diagnostic| diagnostic.message.contains("\"gateway\"")),
        "the open consumer reports its require of the deleted file: {consumer:?}"
    );
}

#[tokio::test]
async fn watched_change_to_a_closed_definition_republishes_open_consumers() {
    let project = Project::new();
    let mut editor = project.editor().await;
    let invoice = project.write("invoice.rb", "class Invoice\n  CURRENCY = \"USD\"\nend\n");
    editor
        .watched_file_changed(&invoice, FileChangeType::CREATED)
        .await;
    let consumer = "class Billing\n  def charge\n    Invoice::CURRENCY\n  end\nend\n";
    let billing = project.write("billing.rb", consumer);
    editor.open(&billing, consumer).await;
    assert_eq!(editor.diagnostics(&billing).await, Vec::new());

    project.write("invoice.rb", "class Invoice\nend\n");
    editor
        .watched_file_changed(&invoice, FileChangeType::CHANGED)
        .await;
    editor
        .check(
            &billing,
            "class Billing\n  def charge\n    <err code=\"unresolved-constant\">Invoice::CURRENCY</err>\n  end\nend\n",
        )
        .await;
}

#[tokio::test]
async fn deleted_then_recreated_file_matches_a_fresh_index() {
    let edited = Project::new();
    let mut editor = edited.editor().await;
    let ledger = edited.write("ledger.rb", LEDGER);
    let gateway = edited.write("gateway.rb", GATEWAY);
    editor
        .watched_file_changed(&ledger, FileChangeType::CREATED)
        .await;
    editor
        .watched_file_changed(&gateway, FileChangeType::CREATED)
        .await;
    let original_id = {
        let workspace = editor.workspace_for(&gateway).unwrap();
        let engine = workspace.analysis_engine.read();
        engine
            .view()
            .file_id(Path::new(&gateway))
            .expect("indexed gateway")
    };

    edited.delete("gateway.rb");
    editor
        .watched_file_changed(&gateway, FileChangeType::DELETED)
        .await;
    assert_eq!(
        registered_paths(&editor, &ledger),
        vec![PathBuf::from(&ledger)]
    );

    let without_gateway = Project::new();
    let mut fresh = without_gateway.editor().await;
    let fresh_ledger = without_gateway.write("ledger.rb", LEDGER);
    fresh
        .watched_file_changed(&fresh_ledger, FileChangeType::CREATED)
        .await;
    assert_eq!(
        result_fingerprint(&editor, &ledger),
        result_fingerprint(&fresh, &fresh_ledger),
        "a deleted file must leave no trace in the semantic result"
    );

    edited.write("gateway.rb", RECREATED_GATEWAY);
    editor
        .watched_file_changed(&gateway, FileChangeType::CREATED)
        .await;
    let recreated_id = {
        let workspace = editor.workspace_for(&gateway).unwrap();
        let engine = workspace.analysis_engine.read();
        engine
            .view()
            .file_id(Path::new(&gateway))
            .expect("indexed gateway")
    };
    assert_ne!(original_id, recreated_id, "file ids are never reissued");

    let fresh_gateway = without_gateway.write("gateway.rb", RECREATED_GATEWAY);
    fresh
        .watched_file_changed(&fresh_gateway, FileChangeType::CREATED)
        .await;
    assert_eq!(
        result_fingerprint(&editor, &gateway),
        result_fingerprint(&fresh, &fresh_gateway),
        "a recreated file must match a fresh index of the same sources"
    );

    editor.open(&gateway, RECREATED_GATEWAY).await;
    assert!(
        editor.delivered_diagnostics(&gateway).await.is_empty(),
        "the recreated file must not resurrect the deleted file's diagnostics"
    );
}

#[tokio::test]
async fn a_rehomed_open_document_leaves_its_previous_project() {
    let project = Project::new();
    let mut editor = FakeEditor::new().await;
    let source = project.write("rehomed.rb", "class Rehomed\nend\n");
    editor.open(&source, "class Rehomed\nend\n").await;
    let path = PathBuf::from(&source);
    assert!(editor
        .server()
        .orphan_engine()
        .read()
        .view()
        .file_id(&path)
        .is_some());

    crate::lsp::handlers::notification::handle_did_change_workspace_folders(
        editor.server(),
        tower_lsp::lsp_types::DidChangeWorkspaceFoldersParams {
            event: tower_lsp::lsp_types::WorkspaceFoldersChangeEvent {
                added: vec![tower_lsp::lsp_types::WorkspaceFolder {
                    uri: crate::test::harness::fixture_uri(format!(
                        "{}/",
                        project.root.to_string_lossy()
                    )),
                    name: "rehomed".to_string(),
                }],
                removed: Vec::new(),
            },
        },
    )
    .await;

    assert!(
        editor
            .server()
            .orphan_engine()
            .read()
            .view()
            .file_id(&path)
            .is_none(),
        "the previous owner must forget a rehomed document"
    );
    assert!(is_registered(&editor, &source));
}

fn is_registered(editor: &FakeEditor, filename: &str) -> bool {
    let workspace = editor
        .workspace_for(filename)
        .expect("file has a workspace");
    let engine = workspace.analysis_engine.read();
    engine.view().file_id(Path::new(filename)).is_some()
}

#[tokio::test]
async fn closed_excluded_files_and_deleted_signatures_leave_the_project() {
    let project = Project::new();
    let mut editor = project.editor().await;
    std::fs::create_dir_all(project.root.join("vendor")).expect("create vendor");
    std::fs::create_dir_all(project.root.join("sig")).expect("create sig");

    let vendored = project.write("vendor/tool.rb", "class VendoredTool\nend\n");
    editor.open(&vendored, "class VendoredTool\nend\n").await;
    assert!(is_registered(&editor, &vendored));
    editor.close(&vendored).await;
    assert!(!is_registered(&editor, &vendored));
    assert_eq!(editor.delivered_diagnostics(&vendored).await, Vec::new());

    let signature = project.write(
        "sig/widget.rbs",
        "class Widget\n  def size: () -> Integer\nend\n",
    );
    editor
        .watched_file_changed(&signature, FileChangeType::CREATED)
        .await;
    assert!(is_registered(&editor, &signature));
    project.delete("sig/widget.rbs");
    editor
        .watched_file_changed(&signature, FileChangeType::DELETED)
        .await;
    assert!(!is_registered(&editor, &signature));
}
