use crate::analyzer_prism::visitors::index_visitor::IndexVisitor;
use crate::capabilities;
use crate::indexer::index::RubyIndex;
use crate::server::RubyLanguageServer;
use anyhow::Result;
use log::{debug, error, info, warn};
use lsp_types::*;
use ruby_prism::{parse, Visit};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::fs;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tower_lsp::jsonrpc::Result as LspResult;

pub async fn handle_initialize(
    lang_server: &RubyLanguageServer,
    params: InitializeParams,
) -> LspResult<InitializeResult> {
    info!("Initializing Ruby LSP server");

    let workspace_folders = params.workspace_folders;

    if let Some(folder) = workspace_folders.and_then(|folders| folders.first().cloned()) {
        info!(
            "Indexing workspace folder using workspace folder: {:?}",
            folder.uri.as_str()
        );
        let _ = init_workspace(lang_server, folder.uri.clone()).await;
    } else if let Some(root_uri) = params.root_uri {
        info!(
            "Indexing workspace folder using root URI: {:?}",
            root_uri.as_str()
        );
        let _ = init_workspace(lang_server, root_uri.clone()).await;
    } else {
        warn!("No workspace folder or root URI provided. A workspace folder is required to function properly");
    }

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            capabilities::semantic_tokens::get_semantic_tokens_options(),
        )),
        ..ServerCapabilities::default()
    };

    Ok(InitializeResult {
        capabilities,
        ..InitializeResult::default()
    })
}

pub async fn handle_initialized(_: &RubyLanguageServer, _: InitializedParams) {
    info!("Server initialized");
}

pub async fn handle_did_open(lang_server: &RubyLanguageServer, params: DidOpenTextDocumentParams) {
    let start_time = Instant::now();
    // Did open handler started

    let uri = params.text_document.uri.clone();
    let content = params.text_document.text.clone();

    // Process the file for indexing
    let index_ref = lang_server.index();
    let result = process_file_for_indexing(index_ref, uri.clone(), &content);

    if let Err(e) = result {
        error!("Error indexing document: {}", e);
    }

    debug!("[PERF] File indexed in {:?}", start_time.elapsed());
}

pub async fn handle_did_change(
    lang_server: &RubyLanguageServer,
    params: DidChangeTextDocumentParams,
) {
    debug!("Did change: {:?}", params.text_document.uri.as_str());
    let uri = params.text_document.uri.clone();
    let index_ref = lang_server.index();

    for change in params.content_changes {
        let content = change.text.clone();
        let result = process_file_for_indexing(index_ref.clone(), uri.clone(), &content);

        if let Err(e) = result {
            error!("Error re-indexing document: {}", e);
        }
    }
}

pub async fn handle_did_close(
    lang_server: &RubyLanguageServer,
    params: DidCloseTextDocumentParams,
) {
    debug!("Did close: {:?}", params.text_document.uri.as_str());
    let uri = params.text_document.uri.clone();
    let content = std::fs::read_to_string(uri.to_file_path().unwrap()).unwrap();
    let index_ref = lang_server.index();
    let result = process_file_for_indexing(index_ref, uri, &content);

    if let Err(e) = result {
        error!("Error re-indexing document: {}", e);
    }
}

pub async fn handle_shutdown(_: &RubyLanguageServer) -> LspResult<()> {
    info!("Shutting down Ruby LSP server");
    Ok(())
}

pub async fn init_workspace(server: &RubyLanguageServer, folder_uri: Url) -> Result<()> {
    let start_time = Instant::now();
    info!("Indexing workspace folder: {}", folder_uri);

    // Convert URI to filesystem path
    let folder_path = folder_uri
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Failed to convert URI to file path: {}", folder_uri))?;

    // Find all Ruby files in the workspace
    let file_search_start = Instant::now();
    let files = find_ruby_files(&folder_path).await?;
    let file_search_duration = file_search_start.elapsed();
    info!(
        "Found {} Ruby files to index in {:?}",
        files.len(),
        file_search_duration
    );

    // Process files in parallel
    let indexing_start = Instant::now();
    process_files_parallel(server, files).await?;
    let indexing_duration = indexing_start.elapsed();

    let total_duration = start_time.elapsed();
    info!(
        "Workspace indexing completed in {:?} (file search: {:?}, indexing: {:?})",
        total_duration, file_search_duration, indexing_duration
    );
    Ok(())
}

async fn process_files_parallel(server: &RubyLanguageServer, files: Vec<PathBuf>) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }

    let setup_start = Instant::now();

    // Get the number of logical cores for optimal parallelism
    let num_cores = std::thread::available_parallelism()
        .unwrap_or(NonZeroUsize::new(4).unwrap())
        .get();

    // Create a semaphore to limit concurrent tasks
    // Use 75% of available cores to avoid overwhelming the system
    let max_concurrent = std::cmp::max(1, num_cores * 3 / 4);
    let semaphore = Arc::new(Semaphore::new(max_concurrent));

    debug!(
        "Parallelizing indexing with {} concurrent workers for {} files",
        max_concurrent,
        files.len()
    );

    // Create a shared server reference
    let index_ref = server.index();
    let setup_duration = setup_start.elapsed();
    debug!("Parallel indexing setup completed in {:?}", setup_duration);

    // Create a set to track all spawned tasks
    let mut tasks = JoinSet::new();

    // Spawn tasks for each file
    let spawn_start = Instant::now();
    let files_count = files.len();
    for file_path in files {
        let semaphore_clone = semaphore.clone();
        let index_clone = index_ref.clone();
        let file_path_clone = file_path.clone();

        // Spawn a task for each file
        tasks.spawn(async move {
            // Acquire a permit from the semaphore
            let _permit = semaphore_clone.acquire().await.unwrap();

            // Read the file content
            let content = match fs::read_to_string(&file_path_clone).await {
                Ok(content) => content,
                Err(e) => {
                    error!("Error reading file {}: {}", file_path_clone.display(), e);
                    return Err(anyhow::anyhow!("Failed to read file: {}", e));
                }
            };

            // Create a file URI
            let uri = match Url::from_file_path(&file_path_clone) {
                Ok(uri) => uri,
                Err(_) => {
                    return Err(anyhow::anyhow!("Failed to convert file path to URI"));
                }
            };

            // Process the file
            process_file_for_indexing(index_clone, uri, &content).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to process file {}: {}",
                    file_path_clone.display(),
                    e
                )
            })
        });
    }
    let spawn_duration = spawn_start.elapsed();
    debug!(
        "Spawned {} indexing tasks in {:?}",
        files_count, spawn_duration
    );

    // Wait for all tasks to complete
    let wait_start = Instant::now();
    let mut completed_tasks = 0;
    let mut failed_tasks = 0;

    while let Some(result) = tasks.join_next().await {
        completed_tasks += 1;
        if let Err(e) = result {
            failed_tasks += 1;
            error!("Task panicked: {}", e);
        } else if let Err(e) = result.unwrap() {
            failed_tasks += 1;
            error!("{}", e);
        }

        // Log progress periodically
        if completed_tasks % 100 == 0 || completed_tasks == files_count {
            debug!(
                "Indexed {}/{} files ({} failed)",
                completed_tasks, files_count, failed_tasks
            );
        }
    }

    let wait_duration = wait_start.elapsed();
    let total_duration = setup_start.elapsed();

    debug!(
        "Parallel indexing completed in {:?} (setup: {:?}, spawn: {:?}, processing: {:?})",
        total_duration, setup_duration, spawn_duration, wait_duration
    );
    info!(
        "Successfully indexed {} files ({} failed)",
        files_count - failed_tasks,
        failed_tasks
    );

    Ok(())
}

async fn find_ruby_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut ruby_files = Vec::new();
    let mut dirs_to_process = vec![dir.to_path_buf()];

    while let Some(current_dir) = dirs_to_process.pop() {
        let mut entries = match fs::read_dir(&current_dir).await {
            Ok(entries) => entries,
            Err(e) => {
                debug!("Error reading directory {}: {}", current_dir.display(), e);
                continue;
            }
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();

            if path.is_dir() {
                dirs_to_process.push(path);
            } else if let Some(ext) = path.extension() {
                if ext == "rb" {
                    ruby_files.push(path);
                }
            }
        }
    }

    Ok(ruby_files)
}

fn process_file_for_indexing(
    index: Arc<std::sync::Mutex<RubyIndex>>,
    uri: Url,
    content: &str,
) -> Result<(), String> {
    // Remove any existing entries for this URI
    index.lock().unwrap().remove_entries_for_uri(&uri);

    // Parse the file
    let parse_result = parse(content.as_bytes());
    let node = parse_result.node();

    // Create a visitor and process the AST
    let mut visitor = IndexVisitor::new(index, uri.clone(), content.to_string());
    visitor.visit(&node);

    debug!("Processed file: {}", uri);
    Ok(())
}
