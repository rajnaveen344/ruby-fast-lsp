use super::super::ruby_gen::{CallSite, SourcePos};
use super::super::{
    large_scale_project, EngineSimulationRunner, LARGE_SCALE_METHOD_DEFS,
    LARGE_SCALE_MIN_GRAPH_EDGES, LARGE_SCALE_RUBY_FILES,
};
use super::assert_elapsed_under_env_budget;
use super::corpus::{
    goto_def_locations, hover_at, inlay_hints_for_path, reference_locations, LspPoint,
};
use crate::{capabilities::indexing, server::RubyLanguageServer};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    time::{Duration, Instant},
};
use tower_lsp::lsp_types::{
    DidOpenTextDocumentParams, Position, TextDocumentIdentifier, TextDocumentItem,
};

pub(super) async fn run_lsp() {
    let seed = std::env::var("SIM_LARGE_SCALE_SEED")
        .ok()
        .map(|value| {
            value.parse::<u64>().unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: SIM_LARGE_SCALE_SEED `{}` is not a u64: {}. This is a bug because scale replay needs a numeric seed. Fix: pass SIM_LARGE_SCALE_SEED=<u64>.",
                    value, err
                )
            })
        })
        .unwrap_or(20_260_524);
    let project = large_scale_project(seed);
    let render = project.render();

    assert_eq!(render.files.len(), LARGE_SCALE_RUBY_FILES);
    assert_eq!(project.enabled_method_count(), LARGE_SCALE_METHOD_DEFS);
    assert!(project.meaningful_edge_count() >= LARGE_SCALE_MIN_GRAPH_EDGES);

    let engine_start = Instant::now();
    let engine_runner = EngineSimulationRunner::start(project.clone());
    let engine_elapsed = engine_start.elapsed();
    let stats = engine_runner.stats();
    eprintln!(
        "large-scale engine index: elapsed={:?} files={} methods={} refs={} types={} graph_edges={}",
        engine_elapsed, stats.files, stats.methods, stats.references, stats.types, stats.graph_edges
    );
    assert_elapsed_under_env_budget(
        "SIM_LARGE_SCALE_ENGINE_MAX_MS",
        Duration::from_secs(120),
        engine_elapsed,
        "large-scale engine index",
    );
    assert!(
        stats.files >= LARGE_SCALE_RUBY_FILES,
        "expected at least {} indexed files, got {:?}",
        LARGE_SCALE_RUBY_FILES,
        stats
    );
    assert!(
        stats.methods >= LARGE_SCALE_METHOD_DEFS,
        "expected at least {} indexed methods, got {:?}",
        LARGE_SCALE_METHOD_DEFS,
        stats
    );

    let samples = sample_call_shapes(&render.map.calls);
    let lsp_start = Instant::now();
    let server = RubyLanguageServer::default();
    let mut opened = BTreeSet::new();
    for call in &samples {
        let def = render.map.defs.get(&call.target).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: generated sample target `{}` has no def. This is a bug because source map should contain every generated target. Fix: inspect large_scale_project.",
                call.target.signature()
            )
        });
        open_once(&server, &render.files, &mut opened, &def.file).await;
    }
    for call in &samples {
        if opened.contains(&call.pos.file) {
            indexing::handle_did_close(
                &server,
                tower_lsp::lsp_types::DidCloseTextDocumentParams {
                    text_document: TextDocumentIdentifier {
                        uri: tower_lsp::lsp_types::Url::parse(&format!(
                            "file:///{}",
                            call.pos.file
                        ))
                        .expect("generated source URI"),
                    },
                },
            )
            .await;
            opened.remove(&call.pos.file);
        }
        open_once(&server, &render.files, &mut opened, &call.pos.file).await;
    }

    for call in &samples {
        let def = render.map.defs.get(&call.target).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: generated sample target `{}` has no def. This is a bug because source map should contain every generated target. Fix: inspect large_scale_project.",
                call.target.signature()
            )
        });

        let defs = goto_def_locations(&server, &point(&call.pos)).await;
        assert!(
            defs.iter()
                .any(|location| location_matches_pos(location, def)),
            "expected scale goto for `{}` shape `{}` from {}:{} to include {}:{}, got {:?}",
            call.target.signature(),
            call.shape_name,
            call.pos.file,
            call.pos.line,
            def.file,
            def.line,
            defs
        );

        let refs = reference_locations(&server, &point(def)).await;
        assert!(
            refs.iter()
                .any(|location| location_matches_pos(location, &call.pos)),
            "expected scale refs for `{}` shape `{}` to include {}:{}, got {:?}",
            call.target.signature(),
            call.shape_name,
            call.pos.file,
            call.pos.line,
            refs
        );

        let hover = hover_at(&server, &point(&call.pos))
            .await
            .unwrap_or_else(|| {
                panic!(
                    "expected scale hover for `{}` shape `{}` at {}:{}",
                    call.target.signature(),
                    call.shape_name,
                    call.pos.file,
                    call.pos.line
                )
            });
        let hover_text = format!("{hover:?}");
        assert!(
            !hover_text.trim().is_empty(),
            "expected non-empty scale hover for `{}` shape `{}`, got {}",
            call.target.signature(),
            call.shape_name,
            hover_text
        );
    }

    let local_call = render
        .map
        .calls
        .iter()
        .find(|call| call.shape_name == "local")
        .expect("scale project must generate a local receiver call");
    let hints = inlay_hints_for_path(&server, &point(&local_call.pos).path).await;
    assert!(
        hints
            .iter()
            .any(|hint| hint.position.line + 1 == local_call.pos.line),
        "expected constructor local receiver type hint before scale call {}:{}, got {:?}",
        local_call.pos.file,
        local_call.pos.line,
        hints
    );
    let lsp_elapsed = lsp_start.elapsed();
    eprintln!(
        "large-scale sampled LSP checks: elapsed={:?} samples={}",
        lsp_elapsed,
        samples.len()
    );
    assert_elapsed_under_env_budget(
        "SIM_LARGE_SCALE_LSP_MAX_MS",
        Duration::from_secs(120),
        lsp_elapsed,
        "large-scale sampled LSP checks",
    );
}

pub(super) fn run_engine() {
    let seed = std::env::var("SIM_LARGE_SCALE_SEED")
        .ok()
        .map(|value| {
            value.parse::<u64>().unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: SIM_LARGE_SCALE_SEED `{}` is not a u64: {}. This is a bug because scale replay needs a numeric seed. Fix: pass SIM_LARGE_SCALE_SEED=<u64>.",
                    value, err
                )
            })
        })
        .unwrap_or(20_260_524);
    let project = large_scale_project(seed);
    let render = project.render();
    eprintln!(
        "large-scale all-edge shape: files={} methods={} edges={} calls={} const_refs={}",
        render.files.len(),
        project.enabled_method_count(),
        project.meaningful_edge_count(),
        render.map.calls.len(),
        render.map.constant_refs.len()
    );

    let engine_start = Instant::now();
    let runner = EngineSimulationRunner::start(project);
    let stats = runner.stats();
    let engine_elapsed = engine_start.elapsed();
    eprintln!(
        "large-scale all-edge index stats: elapsed={:?} files={} methods={} refs={} types={} graph_edges={}",
        engine_elapsed, stats.files, stats.methods, stats.references, stats.types, stats.graph_edges
    );
    assert_elapsed_under_env_budget(
        "SIM_LARGE_SCALE_ALL_EDGES_INDEX_MAX_MS",
        Duration::from_secs(120),
        engine_elapsed,
        "large-scale all-edge engine index",
    );
    assert_eq!(render.files.len(), LARGE_SCALE_RUBY_FILES);
    assert_eq!(stats.methods, LARGE_SCALE_METHOD_DEFS);

    let check_start = Instant::now();
    runner.check_definitions();
    runner.check_references();
    runner.check_types();
    let check_elapsed = check_start.elapsed();
    eprintln!(
        "large-scale all-edge oracle checks: elapsed={:?}",
        check_elapsed
    );
    assert_elapsed_under_env_budget(
        "SIM_LARGE_SCALE_ALL_EDGES_CHECK_MAX_MS",
        Duration::from_secs(300),
        check_elapsed,
        "large-scale all-edge oracle checks",
    );
}

fn sample_call_shapes(calls: &[CallSite]) -> Vec<&CallSite> {
    let required = [
        "bare",
        "local",
        "ivar",
        "class",
        "constructor",
        "one-hop",
        "receiver-local",
        "super",
    ];
    required
        .iter()
        .map(|shape| {
            calls
                .iter()
                .find(|call| {
                    call.shape_name == *shape
                        && call.definition_support.is_supported()
                        && call.reference_support.is_supported()
                        && call.hover_support.is_supported()
                })
                .unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: scale project did not generate supported `{}` call. This is a bug because scale smoke must cover every agent navigation call shape. Fix: inspect large_scale_project.",
                        shape
                    )
                })
        })
        .collect()
}

async fn open_once(
    server: &RubyLanguageServer,
    files: &std::collections::BTreeMap<String, String>,
    opened: &mut BTreeSet<String>,
    file: &str,
) {
    if opened.contains(file) {
        return;
    }
    let content = files.get(file).unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: scale smoke tried to open missing file `{}`. This is a bug because source map positions must point at rendered files. Fix: inspect render_project.",
            file
        )
    });
    indexing::handle_did_open(
        server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: tower_lsp::lsp_types::Url::parse(&format!("file:///{file}"))
                    .expect("generated source URI"),
                language_id: "ruby".into(),
                version: 1,
                text: content.clone(),
            },
        },
    )
    .await;
    opened.insert(file.to_string());
}

fn location_matches_pos(location: &tower_lsp::lsp_types::Location, pos: &SourcePos) -> bool {
    location.uri.path().ends_with(&pos.file) && location.range.start.line == pos.line
}

fn point(pos: &SourcePos) -> LspPoint {
    LspPoint {
        path: PathBuf::from(format!("/{}", pos.file)),
        position: Position::new(pos.line, pos.character),
    }
}
