use crate::capabilities::indexing;
use crate::indexer::file_processor::FileProcessor;
use crate::server::RubyLanguageServer;
use parking_lot::RwLock;
use ruby_analysis::core::{FullyQualifiedName, SourceKind, TextRange};
use ruby_analysis::engine::AnalysisEngine;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::{
    DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse, HoverParams,
    InlayHintParams, Location, PartialResultParams, Position, Range, ReferenceContext,
    ReferenceParams, TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams,
    WorkDoneProgressParams,
};
use tower_lsp::LanguageServer;

use super::assert_elapsed_under_env_budget;

pub(super) async fn run(root: PathBuf) {
    assert!(
        root.is_dir(),
        "Real corpus acceptance root does not exist: {}",
        root.display()
    );

    let files = collect_real_corpus_ruby_files(&root);
    let shape = CorpusShape::from_files(&files);
    eprintln!(
        "real corpus shape: files={} loc={} bytes={} namespace_defs={} method_defs={} rough_call_refs={}",
        shape.files,
        shape.loc,
        shape.bytes,
        shape.namespace_defs,
        shape.method_defs,
        shape.rough_call_refs
    );
    assert!(
        shape.files >= 2_000,
        "expected large-scale Ruby files, got {:?}",
        shape
    );
    assert!(
        shape.method_defs >= 20_000,
        "expected large-scale method defs, got {:?}",
        shape
    );
    assert!(
        shape.rough_call_refs >= 200_000,
        "expected large-scale call/ref density, got {:?}",
        shape
    );

    let server = RubyLanguageServer::default();
    let workspace_uri = tower_lsp::lsp_types::Url::from_file_path(&root).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: real corpus root `{}` is not file-URI convertible. This is a bug because LSP smoke needs local file URIs. Fix: pass a canonical filesystem path.",
            root.display()
        )
    });
    let workspace = server.add_workspace(workspace_uri);

    let index_start = Instant::now();
    index_core_stubs_for_smoke(&server, workspace.analysis_engine.clone());
    index_project_files_for_smoke(&server, workspace.analysis_engine.clone(), &files);
    let index_elapsed = index_start.elapsed();
    let stats = workspace.analysis_engine.read().stats();
    eprintln!(
        "real corpus index: elapsed={:?} files={} methods={} refs={} types={} graph_edges={} diagnostics={}",
        index_elapsed,
        stats.files,
        stats.methods,
        stats.references,
        stats.types,
        stats.graph_edges,
        stats.diagnostics
    );
    print_real_corpus_diagnostic_sample(workspace.analysis_engine.clone());
    assert_elapsed_under_env_budget(
        "SIM_REAL_CORPUS_INDEX_MAX_MS",
        Duration::from_secs(1_200),
        index_elapsed,
        "real corpus index",
    );
    assert!(
        stats.files >= shape.files,
        "expected all project files indexed, shape={:?}, stats={:?}",
        shape,
        stats
    );
    assert!(
        stats.methods >= 15_000,
        "expected substantial real method facts, shape={:?}, stats={:?}",
        shape,
        stats
    );
    assert!(
        stats.references >= 10_000,
        "expected substantial real resolved refs, shape={:?}, stats={:?}",
        shape,
        stats
    );
    assert!(
        stats.types >= 1_000,
        "expected substantial real type facts, shape={:?}, stats={:?}",
        shape,
        stats
    );

    let samples = select_real_method_reference_samples(workspace.analysis_engine.clone(), 12);
    assert!(
        samples.len() >= 8,
        "expected at least 8 real method reference samples after indexing {}, got {} samples, stats={:?}",
        root.display(),
        samples.len(),
        stats
    );
    for sample in &samples {
        open_real_document(&server, &sample.definition.path).await;
        if sample.reference.path != sample.definition.path {
            open_real_document(&server, &sample.reference.path).await;
        }

        let defs = goto_def_locations(&server, &sample.reference).await;
        assert!(
            defs.iter()
                .any(|location| location_start_matches(location, &sample.definition)),
            "expected real goto for {} from {}:{} to include {}:{}, got {:?}",
            sample.label,
            sample.reference.path.display(),
            sample.reference.position.line,
            sample.definition.path.display(),
            sample.definition.position.line,
            defs
        );

        let refs = reference_locations(&server, &sample.reference).await;
        assert!(
            refs.iter()
                .any(|location| location_start_matches(location, &sample.reference))
                && refs.len() >= sample.reference_count.min(3),
            "expected real refs for {} at usage {}:{} to include usage and at least {} refs, got {:?}",
            sample.label,
            sample.reference.path.display(),
            sample.reference.position.line,
            sample.reference_count.min(3),
            refs
        );

        let hover = hover_at(&server, &sample.reference)
            .await
            .unwrap_or_else(|| {
                panic!(
                    "expected real hover for {} at {}:{}",
                    sample.label,
                    sample.reference.path.display(),
                    sample.reference.position.line
                )
            });
        assert!(
            !format!("{hover:?}").trim().is_empty(),
            "expected non-empty real hover for {}, got {:?}",
            sample.label,
            hover
        );
    }

    let hinted =
        assert_real_type_inlay_samples(&server, workspace.analysis_engine.clone(), 3).await;
    eprintln!(
        "real corpus semantic samples: goto/refs/hover={} type_hint_files={}",
        samples.len(),
        hinted
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
}

fn print_real_corpus_diagnostic_sample(analysis_engine: Arc<RwLock<AnalysisEngine>>) {
    if std::env::var("SIM_REAL_CORPUS_DIAGNOSTIC_SAMPLE")
        .ok()
        .as_deref()
        != Some("1")
    {
        return;
    }

    let engine = analysis_engine.read();
    let query = engine.query();
    let mut grouped = BTreeMap::<(String, String), (usize, Vec<String>)>::new();
    for diagnostic in query.all_diagnostic_facts() {
        let key = (diagnostic.code.clone(), diagnostic.message.clone());
        let entry = grouped.entry(key).or_default();
        entry.0 += 1;
        if entry.1.len() < 3 {
            let sample = query
                .file(diagnostic.range.file_id)
                .and_then(|file| {
                    let (line, character) =
                        file.byte_offset_to_line_character(diagnostic.range.start_byte)?;
                    Some(format!(
                        "{}:{}:{}",
                        file.path.display(),
                        line + 1,
                        character + 1
                    ))
                })
                .unwrap_or_else(|| "<unknown>".to_string());
            entry.1.push(sample);
        }
    }

    let mut rows = grouped
        .into_iter()
        .map(|((code, message), (count, samples))| (count, code, message, samples))
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));

    eprintln!("real corpus diagnostics top groups:");
    for (count, code, message, samples) in rows.into_iter().take(12) {
        eprintln!(
            "  count={} code={} message={} samples={}",
            count,
            code,
            message,
            samples.join(", ")
        );
    }
}

#[derive(Debug)]
struct CorpusShape {
    files: usize,
    loc: usize,
    bytes: usize,
    namespace_defs: usize,
    method_defs: usize,
    rough_call_refs: usize,
}

impl CorpusShape {
    fn from_files(files: &[PathBuf]) -> Self {
        let mut shape = Self {
            files: files.len(),
            loc: 0,
            bytes: 0,
            namespace_defs: 0,
            method_defs: 0,
            rough_call_refs: 0,
        };

        for file in files {
            let content = std::fs::read_to_string(file).unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: failed to read real corpus file `{}`: {}. This is a bug because corpus shape requires readable Ruby files. Fix: inspect file permissions.",
                    file.display(),
                    err
                )
            });
            shape.bytes += content.len();
            for line in content.lines() {
                shape.loc += 1;
                let trimmed = line.trim_start();
                if trimmed.starts_with("class ") || trimmed.starts_with("module ") {
                    shape.namespace_defs += 1;
                }
                if trimmed.starts_with("def ") {
                    shape.method_defs += 1;
                }
                if !trimmed.starts_with('#')
                    && (trimmed.contains('.') || trimmed.contains("::") || trimmed.contains('('))
                {
                    shape.rough_call_refs += 1;
                }
            }
        }

        shape
    }
}

#[derive(Debug, Clone)]
pub(super) struct LspPoint {
    pub(super) path: PathBuf,
    pub(super) position: Position,
}

#[derive(Debug, Clone)]
struct RealMethodSample {
    label: String,
    definition: LspPoint,
    reference: LspPoint,
    reference_count: usize,
}

fn collect_real_corpus_ruby_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_real_corpus_ruby_files_into(root, &mut files);
    files.sort();
    files
}

fn collect_real_corpus_ruby_files_into(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: failed to read corpus dir `{}`: {}. This is a bug because corpus smoke needs readable dirs. Fix: inspect path/permissions.",
            dir.display(),
            err
        )
    });
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if matches!(name, ".git" | "vendor" | ".bundle" | "tmp" | "log") {
                continue;
            }
            collect_real_corpus_ruby_files_into(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rb") {
            files.push(path);
        }
    }
}

fn index_project_files_for_smoke(
    server: &RubyLanguageServer,
    analysis_engine: Arc<RwLock<AnalysisEngine>>,
    files: &[PathBuf],
) {
    index_files_for_smoke(server, analysis_engine, files, SourceKind::Project);
}

fn index_core_stubs_for_smoke(
    server: &RubyLanguageServer,
    analysis_engine: Arc<RwLock<AnalysisEngine>>,
) {
    let stubs_dir = core_stubs_dir_for_smoke().expect(
        "INVARIANT VIOLATED: core stubs are unavailable for real corpus acceptance. This is a bug because the semantic smoke requires its language inputs. Fix: build or restore the packaged core stubs before running corpus validation."
    );
    let files = collect_real_corpus_ruby_files(&stubs_dir);
    assert!(
        !files.is_empty(),
        "INVARIANT VIOLATED: real corpus core stub directory is empty. This is a bug because corpus validation requires indexed language inputs. Fix: build or restore the packaged core stubs before running corpus validation."
    );
    eprintln!(
        "real corpus smoke: indexing {} core stub files from {}",
        files.len(),
        stubs_dir.display()
    );
    index_files_for_smoke(server, analysis_engine, &files, SourceKind::Stub);
}

fn core_stubs_dir_for_smoke() -> Option<PathBuf> {
    let stubs_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("editors")
        .join("vscode")
        .join("vsix")
        .join("stubs");
    [
        "rubystubs33",
        "rubystubs34",
        "rubystubs32",
        "rubystubs31",
        "rubystubs30",
    ]
    .into_iter()
    .map(|name| stubs_root.join(name))
    .find(|path| path.is_dir())
}

fn index_files_for_smoke(
    server: &RubyLanguageServer,
    analysis_engine: Arc<RwLock<AnalysisEngine>>,
    files: &[PathBuf],
    source_kind: SourceKind,
) {
    let processor = FileProcessor::with_extension_registry(server.extensions.registry().clone());
    for file in files {
        let content = std::fs::read_to_string(file).unwrap_or_else(|err| {
            panic!(
                "INVARIANT VIOLATED: failed to read real corpus file `{}` during indexing: {}. This is a bug because indexed files must be readable. Fix: inspect file permissions.",
                file.display(),
                err
            )
        });
        let uri = tower_lsp::lsp_types::Url::from_file_path(file).unwrap_or_else(|_| {
            panic!(
                "INVARIANT VIOLATED: real corpus file `{}` is not file-URI convertible. This is a bug because LSP indexing requires file URIs. Fix: inspect path.",
                file.display()
            )
        });
        processor
            .collect_file_facts_as_deferred_resolution_in_engine(
                &uri, &content, analysis_engine.clone(), source_kind,
            )
            .unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: failed to index real corpus file `{}`: {}. This is a bug because smoke indexing should tolerate valid Ruby project files. Fix: inspect parser/fact collector failure.",
                    file.display(),
                    err
                )
            });
    }
    analysis_engine.write().resolve();
}

fn select_real_method_reference_samples(
    analysis_engine: Arc<RwLock<AnalysisEngine>>,
    limit: usize,
) -> Vec<RealMethodSample> {
    let selection_start = Instant::now();
    let engine = analysis_engine.read();
    let query = engine.query();
    let mut methods = engine.all_method_facts();
    let method_candidates = methods.len();
    methods.sort_by_key(|method| {
        (
            method.range.file_id,
            method.range.start_byte,
            method.fqn.to_string(),
        )
    });

    let mut samples = Vec::new();
    let mut seen_labels = BTreeSet::new();
    let mut seen_reference_files = BTreeSet::new();
    let mut skipped_definitions = 0;
    let mut reference_queries = 0;

    'methods: for method in methods {
        let FullyQualifiedName::Method(_, ruby_method) = &method.fqn else {
            continue;
        };
        if matches!(ruby_method.to_string().as_str(), "new" | "initialize") {
            continue;
        }
        let label = method.fqn.to_string();
        if seen_labels.contains(&label) {
            continue;
        }
        let Some(definition) = lsp_point_for_range(&engine, method.range) else {
            skipped_definitions += 1;
            continue;
        };
        if !is_preferred_real_sample_path(&definition.path) {
            skipped_definitions += 1;
            continue;
        }
        reference_queries += 1;
        let mut refs = query.method_reference_ranges(&method.owner, ruby_method);
        refs.sort_by_key(|range| (range.file_id, range.start_byte, range.end_byte));
        let reference_count = refs.len();
        for reference_range in refs {
            if reference_range == method.range {
                continue;
            }
            let Some(reference) = lsp_point_for_range(&engine, reference_range) else {
                continue;
            };
            if !is_preferred_real_sample_path(&reference.path) {
                continue;
            }
            let reference_file = reference.path.clone();
            if !seen_reference_files.insert(reference_file) && samples.len() < limit / 2 {
                continue;
            }
            seen_labels.insert(label.clone());
            samples.push(RealMethodSample {
                label: label.clone(),
                definition: definition.clone(),
                reference,
                reference_count,
            });
            if samples.len() >= limit {
                break 'methods;
            }
            break;
        }
    }

    eprintln!(
        "real corpus sample selection: elapsed={:?} candidates={} skipped_definitions={} reference_queries={} samples={}",
        selection_start.elapsed(), method_candidates, skipped_definitions, reference_queries, samples.len()
    );
    samples
}

fn is_preferred_real_sample_path(path: &Path) -> bool {
    let path = path.to_string_lossy();
    !path.contains("/.ai-docs/")
        && !path.contains("/spec/")
        && !path.contains("/editors/vscode/vsix/stubs/")
}

fn lsp_point_for_range(
    engine: &ruby_analysis::engine::AnalysisEngine,
    range: TextRange,
) -> Option<LspPoint> {
    let file = engine.file(range.file_id)?;
    let (line, character) = file.byte_offset_to_line_character(range.start_byte)?;
    Some(LspPoint {
        path: file.path.clone(),
        position: Position::new(line, character),
    })
}

async fn open_real_document(server: &RubyLanguageServer, path: &Path) {
    let uri = tower_lsp::lsp_types::Url::from_file_path(path).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: real document `{}` is not file-URI convertible. This is a bug because didOpen requires a valid file URI. Fix: inspect path.",
            path.display()
        )
    });
    if server.documents.read().contains_key(&uri) {
        return;
    }
    let content = std::fs::read_to_string(path).unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: failed to open real document `{}`: {}. This is a bug because LSP smoke samples must be readable. Fix: inspect file permissions.",
            path.display(),
            err
        )
    });
    indexing::handle_did_open(
        server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri,
                language_id: "ruby".to_string(),
                version: 1,
                text: content,
            },
        },
    )
    .await;
}

pub(super) async fn goto_def_locations(
    server: &RubyLanguageServer,
    point: &LspPoint,
) -> Vec<Location> {
    let uri = tower_lsp::lsp_types::Url::from_file_path(&point.path).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: goto point path `{}` is not file-URI convertible. This is a bug because LSP queries need file URIs. Fix: inspect sample selection.",
            point.path.display()
        )
    });
    let params = GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position: point.position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    match server.goto_definition(params).await {
        Ok(Some(GotoDefinitionResponse::Scalar(location))) => vec![location],
        Ok(Some(GotoDefinitionResponse::Array(locations))) => locations,
        Ok(Some(GotoDefinitionResponse::Link(links))) => links
            .into_iter()
            .map(|link| Location {
                uri: link.target_uri,
                range: link.target_range,
            })
            .collect(),
        Ok(None) => Vec::new(),
        Err(err) => panic!(
            "INVARIANT VIOLATED: real corpus goto request failed at `{}`:{}:{}: {}. This is a bug because LSP requests should return JSON-RPC success. Fix: inspect goto handler.",
            point.path.display(),
            point.position.line,
            point.position.character,
            err
        ),
    }
}

pub(super) async fn reference_locations(
    server: &RubyLanguageServer,
    point: &LspPoint,
) -> Vec<Location> {
    let uri = tower_lsp::lsp_types::Url::from_file_path(&point.path).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: reference point path `{}` is not file-URI convertible. This is a bug because LSP queries need file URIs. Fix: inspect sample selection.",
            point.path.display()
        )
    });
    let params = ReferenceParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position: point.position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: ReferenceContext {
            include_declaration: true,
        },
    };
    server
        .references(params)
        .await
        .unwrap_or_else(|err| {
            panic!(
                "INVARIANT VIOLATED: real corpus references request failed at `{}`:{}:{}: {}. This is a bug because LSP requests should return JSON-RPC success. Fix: inspect references handler.",
                point.path.display(),
                point.position.line,
                point.position.character,
                err
            )
        })
        .unwrap_or_default()
}

pub(super) async fn hover_at(
    server: &RubyLanguageServer,
    point: &LspPoint,
) -> Option<tower_lsp::lsp_types::Hover> {
    let uri = tower_lsp::lsp_types::Url::from_file_path(&point.path).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: hover point path `{}` is not file-URI convertible. This is a bug because LSP queries need file URIs. Fix: inspect sample selection.",
            point.path.display()
        )
    });
    let params = HoverParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position: point.position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    server.hover(params).await.unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: real corpus hover request failed at `{}`:{}:{}: {}. This is a bug because LSP requests should return JSON-RPC success. Fix: inspect hover handler.",
            point.path.display(),
            point.position.line,
            point.position.character,
            err
        )
    })
}

async fn assert_real_type_inlay_samples(
    server: &RubyLanguageServer,
    analysis_engine: Arc<RwLock<AnalysisEngine>>,
    limit: usize,
) -> Vec<PathBuf> {
    let candidates = {
        let engine = analysis_engine.read();
        let mut counts = BTreeMap::new();
        for fact in engine.query().all_type_facts() {
            *counts.entry(fact.range.file_id).or_insert(0usize) += 1;
        }
        let mut candidates = counts
            .into_iter()
            .filter_map(|(file_id, count)| {
                engine.file(file_id).map(|file| (count, file.path.clone()))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        candidates
    };

    let mut samples = Vec::new();
    for (_count, path) in candidates.into_iter().take(50) {
        open_real_document(server, &path).await;
        let hints = inlay_hints_for_path(server, &path).await;
        if !hints.is_empty() {
            samples.push(path);
            if samples.len() >= limit {
                return samples;
            }
        }
    }

    assert!(
        !samples.is_empty(),
        "expected at least one real corpus file to produce type inlay hints from indexed type facts"
    );
    samples
}

pub(super) async fn inlay_hints_for_path(
    server: &RubyLanguageServer,
    path: &Path,
) -> Vec<tower_lsp::lsp_types::InlayHint> {
    let uri = tower_lsp::lsp_types::Url::from_file_path(path).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: inlay hint path `{}` is not file-URI convertible. This is a bug because LSP queries need file URIs. Fix: inspect sample selection.",
            path.display()
        )
    });
    let document = server.get_doc(&uri).expect(
        "INVARIANT VIOLATED: an inlay hint sample has no open document. This is a bug because campaign observations must query delivered source. Fix: open the sample before querying hints.",
    );
    let line_count = document.content.lines().count() as u32;
    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri },
        range: Range::new(Position::new(0, 0), Position::new(line_count, 0)),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    server
        .inlay_hint(params)
        .await
        .unwrap_or_else(|err| {
            panic!(
                "INVARIANT VIOLATED: real corpus inlay hint request failed for `{}`: {}. This is a bug because LSP requests should return JSON-RPC success. Fix: inspect inlay hint handler.",
                path.display(),
                err
            )
        })
        .unwrap_or_default()
}

fn location_start_matches(location: &Location, point: &LspPoint) -> bool {
    location
        .uri
        .to_file_path()
        .ok()
        .as_deref()
        .is_some_and(|path| path == point.path)
        && location.range.start == point.position
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn corpus_smoke_helpers_preserve_owning_engine() {
        let directory =
            tempfile::tempdir().expect("synthetic corpus temp directory must be writable");
        let base = directory
            .path()
            .canonicalize()
            .expect("synthetic corpus directory must have a canonical path");
        let root = base.join("project");
        std::fs::create_dir(&root).expect("synthetic project directory must be writable");
        let stub = base.join("core.rb");
        let service = root.join("service.rb");
        let caller = root.join("call.rb");
        std::fs::write(&stub, "class Object; end\n").expect("synthetic stub must be writable");
        std::fs::write(&service, "class Service\n  def value\n    1\n  end\nend\n")
            .expect("synthetic service must be writable");
        std::fs::write(&caller, "Service.new.value\n").expect("synthetic caller must be writable");

        let server = RubyLanguageServer::default();
        let orphan_sources = server
            .orphan_engine()
            .read()
            .files()
            .map(|file| (file.path.clone(), file.kind))
            .collect::<BTreeMap<_, _>>();
        let workspace = server.add_workspace(
            tower_lsp::lsp_types::Url::from_file_path(&root)
                .expect("synthetic project must have a file URI"),
        );
        index_files_for_smoke(
            &server,
            workspace.analysis_engine.clone(),
            &[stub.clone()],
            SourceKind::Stub,
        );
        index_project_files_for_smoke(
            &server,
            workspace.analysis_engine.clone(),
            &[service.clone(), caller.clone()],
        );
        {
            let engine = workspace.analysis_engine.read();
            for (path, kind) in [
                (&stub, SourceKind::Stub),
                (&service, SourceKind::Project),
                (&caller, SourceKind::Project),
            ] {
                let file_id = engine.file_id(path).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: corpus smoke owning engine is missing {kind:?} file `{}`. This is a bug because corpus inputs must share one isolated engine. Fix: pass the owning engine through collection and observation helpers.",
                    path.display()
                )
            });
                assert_eq!(
                    engine
                        .file(file_id)
                        .expect("registered file must exist")
                        .kind,
                    kind
                );
            }
            assert!(
                engine.stats().references > 0,
                "owning engine must resolve project references"
            );
        }
        assert_eq!(
            server
                .orphan_engine()
                .read()
                .files()
                .map(|file| (file.path.clone(), file.kind))
                .collect::<BTreeMap<_, _>>(),
            orphan_sources,
            "corpus collection must preserve the orphan engine's original source inventory"
        );

        open_real_document(&server, &caller).await;
        let point = LspPoint {
            path: caller,
            position: Position::new(0, 12),
        };
        let locations = goto_def_locations(&server, &point).await;
        assert!(
            locations.iter().any(|location| {
                location.uri.to_file_path().ok().as_ref() == Some(&service)
                    && location.range.start.line == 1
            }),
            "public goto must route to the owning engine's service method: {locations:?}"
        );
        assert_eq!(
            server
                .orphan_engine()
                .read()
                .files()
                .map(|file| (file.path.clone(), file.kind))
                .collect::<BTreeMap<_, _>>(),
            orphan_sources,
            "public project queries must preserve the orphan engine's original source inventory"
        );
    }
}
