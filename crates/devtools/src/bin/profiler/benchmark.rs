//! Production editor-latency benchmark over the deterministic sample corpus.

use crate::invariant::ExpectInvariant;
use devtools::metrics::{LatencySummary, ProductionBudget, ProductionMeasurements};
use ruby_fast_lsp::features::diagnostics::engine_diagnostics;
use ruby_fast_lsp::features::editing::completion;
use ruby_fast_lsp::features::navigation::{definition, references};
use ruby_fast_lsp::features::presentation::hover;
use ruby_fast_lsp::lsp::lifecycle::indexing;
use ruby_fast_lsp::server::Server;
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::{
    CompletionContext, CompletionResponse, CompletionTriggerKind, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, HoverParams, Position, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams, Url,
    VersionedTextDocumentIdentifier, WorkDoneProgressParams,
};

use crate::reports::bytes_to_mb;

pub(crate) async fn run_production_benchmark(
    server: &Server,
    workspace_path: &std::path::Path,
    cold_indexing: Duration,
    iterations: usize,
) -> anyhow::Result<ProductionMeasurements> {
    let file_path = workspace_path.join("app/controllers/users_controller.rb");
    let original = fs::read_to_string(&file_path).map_err(|error| {
        anyhow::anyhow!(
            "production benchmark requires {} from the deterministic sample corpus: {error}",
            file_path.display()
        )
    })?;
    let uri = Url::from_file_path(&file_path)
        .map_err(|_| anyhow::anyhow!("invalid benchmark file path: {}", file_path.display()))?;

    const COMPLETION_CALL: &str = "@service.list_users";
    const COMPLETION_RECEIVER: &str = "@service.";
    let completion_call_count = original.match_indices(COMPLETION_CALL).count();
    invariant_eq!(
        completion_call_count,
        1,
        what =
            "benchmark corpus contains {completion_call_count} occurrences of {COMPLETION_CALL:?}",
        why = "the completion edit must target one call",
        fix = "keep exactly one benchmark completion call or use a unique marker",
        completion_call_count = completion_call_count,
        COMPLETION_CALL = COMPLETION_CALL,
    );
    let completion_source = original.replacen(COMPLETION_CALL, COMPLETION_RECEIVER, 1);

    indexing::handle_did_open(
        server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: completion_source.clone(),
            },
        },
    )
    .await;

    let completion_position = position_after(&completion_source, COMPLETION_RECEIVER)?;
    let method_position = position_inside(&original, "list_users")?;
    let completion_context = Some(CompletionContext {
        trigger_kind: CompletionTriggerKind::TRIGGER_CHARACTER,
        trigger_character: Some(".".to_string()),
    });

    for _ in 0..5 {
        let _ = completion::find_completion_at_position(
            server,
            uri.clone(),
            completion_position,
            completion_context.clone(),
        )
        .await;
    }

    let mut completion_samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let result = completion::find_completion_at_position(
            server,
            uri.clone(),
            completion_position,
            completion_context.clone(),
        )
        .await;
        completion_samples.push(start.elapsed());
        let completion_labels = match &result {
            CompletionResponse::Array(items) => items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            CompletionResponse::List(list) => list
                .items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
        };
        invariant!(
            completion_labels.contains(&"list_users"),
            what = "benchmark completion lacks list_users; labels: {completion_labels:?}",
            why = "timing a broken query produces misleading evidence",
            fix = "repair the corpus or completion position",
            completion_labels = completion_labels,
        );
    }

    indexing::handle_did_change(
        server,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: original.clone(),
            }],
        },
    )
    .await;

    for _ in 0..5 {
        let _ = definition::find_definition_at_position(server, uri.clone(), method_position).await;
    }

    let hover_params = || HoverParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: method_position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let mut hover_samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let result = hover::handle(server, hover_params())
            .await
            .expect("hover request failed");
        hover_samples.push(start.elapsed());
        invariant!(
            result.is_some(),
            what = "benchmark hover returned no result",
            why = "timing an empty query would produce misleading evidence",
            fix = "repair the deterministic corpus or hover position",
        );
    }

    let mut definition_samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let result =
            definition::find_definition_at_position(server, uri.clone(), method_position).await;
        definition_samples.push(start.elapsed());
        invariant!(
            result
                .as_ref()
                .is_some_and(
                    |response| !definition::definition_locations(response.clone()).is_empty()
                ),
            what = "benchmark definition returned no locations",
            why = "timing an empty query would produce misleading evidence",
            fix = "repair the deterministic corpus or definition position",
        );
    }

    let mut reference_samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let result = references::find_references_at_position(server, &uri, method_position).await;
        reference_samples.push(start.elapsed());
        invariant!(
            result
                .as_ref()
                .is_some_and(|locations| !locations.is_empty()),
            what = "benchmark references returned no locations",
            why = "timing an empty query would produce misleading evidence",
            fix = "repair the deterministic corpus or reference position",
        );
    }

    let writer_wait = tokio::task::block_in_place(|| {
        measure_writer_wait(server, &uri, method_position, iterations)
    });

    let project = server.project_for_uri(&uri);
    let mut diagnostic_samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let _ = project.view(|view| engine_diagnostics(view, &uri));
        diagnostic_samples.push(start.elapsed());
    }

    let variants = [
        format!("{original}\n# benchmark body edit a\n"),
        format!("{original}\n# benchmark body edit b\n"),
    ];
    let mut edit_samples = Vec::with_capacity(iterations);
    for iteration in 0..iterations {
        let start = Instant::now();
        indexing::handle_did_change(
            server,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier {
                    uri: uri.clone(),
                    version: i32::try_from(iteration + 3).expect_invariant(
                        "benchmark iteration count exceeds LSP document versions",
                        "the benchmark cannot represent that many edits",
                        "use fewer than i32::MAX iterations",
                    ),
                },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: variants[iteration % variants.len()].clone(),
                }],
            },
        )
        .await;
        edit_samples.push(start.elapsed());
    }

    let engine_heap_bytes = project.view(|view| view.estimated_memory_stats().total());
    Ok(ProductionMeasurements {
        cold_indexing,
        edit: LatencySummary::from_samples(&edit_samples),
        completion: LatencySummary::from_samples(&completion_samples),
        hover: LatencySummary::from_samples(&hover_samples),
        definition: LatencySummary::from_samples(&definition_samples),
        references: LatencySummary::from_samples(&reference_samples),
        diagnostics: LatencySummary::from_samples(&diagnostic_samples),
        writer_wait,
        engine_heap_bytes,
    })
}

/// Concurrent editor reads that hold the project's engine read lock while a
/// writer waits; mixed so each reader thread holds the lock for a different span.
const WRITER_WAIT_READERS: usize = 3;

/// Time to start a project update while reader threads loop
/// over hover, definition, and references. A request that holds its read guard
/// across unrelated work, or reacquires it while a writer is queued, shows up
/// here as writer wait (or as a deadlock).
fn measure_writer_wait(
    server: &Server,
    uri: &Url,
    position: Position,
    iterations: usize,
) -> LatencySummary {
    let project = server.project_for_uri(uri);
    let runtime = tokio::runtime::Handle::current();
    let stop = AtomicBool::new(false);
    let started_readers = AtomicUsize::new(0);
    let mut samples = Vec::with_capacity(iterations);
    std::thread::scope(|scope| {
        for reader in 0..WRITER_WAIT_READERS {
            let runtime = runtime.clone();
            let (stop, started_readers) = (&stop, &started_readers);
            scope.spawn(move || {
                started_readers.fetch_add(1, Ordering::SeqCst);
                while !stop.load(Ordering::SeqCst) {
                    runtime.block_on(read_request(server, uri, position, reader));
                }
            });
        }
        while started_readers.load(Ordering::SeqCst) < WRITER_WAIT_READERS {
            std::thread::yield_now();
        }
        for _ in 0..iterations {
            std::thread::sleep(Duration::from_micros(500));
            let start = Instant::now();
            project.update(|_| samples.push(start.elapsed()));
        }
        stop.store(true, Ordering::SeqCst);
    });
    LatencySummary::from_samples(&samples)
}

async fn read_request(server: &Server, uri: &Url, position: Position, reader: usize) {
    match reader % WRITER_WAIT_READERS {
        0 => {
            let _ = hover::handle(
                server,
                HoverParams {
                    text_document_position_params: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier { uri: uri.clone() },
                        position,
                    },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                },
            )
            .await;
        }
        1 => {
            let _ = definition::find_definition_at_position(server, uri.clone(), position).await;
        }
        _ => {
            let _ = references::find_references_at_position(server, uri, position).await;
        }
    }
}

fn position_after(content: &str, needle: &str) -> anyhow::Result<Position> {
    let offset = content
        .find(needle)
        .map(|start| start + needle.len())
        .ok_or_else(|| anyhow::anyhow!("benchmark corpus is missing {needle:?}"))?;
    Ok(position_at_byte_offset(content, offset))
}

fn position_inside(content: &str, needle: &str) -> anyhow::Result<Position> {
    let offset = content
        .find(needle)
        .map(|start| start + 1)
        .ok_or_else(|| anyhow::anyhow!("benchmark corpus is missing {needle:?}"))?;
    Ok(position_at_byte_offset(content, offset))
}

fn position_at_byte_offset(content: &str, offset: usize) -> Position {
    invariant!(
        content.is_char_boundary(offset),
        what = "benchmark byte offset {offset} is not a UTF-8 character boundary",
        why = "LSP positions must be derived from valid source boundaries",
        fix = "choose a complete source token",
        offset = offset,
    );
    let prefix = &content[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
    let line_start = prefix.rfind('\n').map_or(0, |newline| newline + 1);
    let character = content[line_start..offset].encode_utf16().count();
    Position::new(
        u32::try_from(line).expect_invariant(
            "benchmark line count exceeds u32",
            "LSP positions are u32",
            "use benchmark sources under 4G lines",
        ),
        u32::try_from(character).expect_invariant(
            "benchmark UTF-16 column exceeds u32",
            "LSP positions are u32",
            "use benchmark sources with shorter lines",
        ),
    )
}

pub(crate) fn print_production_measurements(measurements: &ProductionMeasurements) {
    let budget = ProductionBudget::default();
    println!("\n=== PRODUCTION BENCHMARK ===");
    println!(
        "cold_indexing: {:?} (budget {:?})",
        measurements.cold_indexing, budget.cold_indexing
    );
    print_latency("edit", measurements.edit, budget.edit);
    print_latency("completion", measurements.completion, budget.completion);
    print_latency("hover", measurements.hover, budget.hover);
    print_latency("definition", measurements.definition, budget.definition);
    print_latency("references", measurements.references, budget.references);
    print_latency("diagnostics", measurements.diagnostics, budget.diagnostics);
    print_latency("writer_wait", measurements.writer_wait, budget.writer_wait);
    println!(
        "engine_heap: {:.1} MB (budget {:.1} MB)",
        bytes_to_mb(measurements.engine_heap_bytes),
        bytes_to_mb(budget.engine_heap_bytes)
    );
}

fn print_latency(name: &str, summary: LatencySummary, budget: Duration) {
    println!(
        "{name}: n={} min={:?} p50={:?} p95={:?} max={:?} (p95 budget {:?})",
        summary.samples, summary.min, summary.p50, summary.p95, summary.max, budget
    );
}
