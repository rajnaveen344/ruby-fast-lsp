// VS Code entry point: activation wires the language client, the editor
// status chrome, the Ruby Projects view, and the contributed commands.
const vscode = require('vscode');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');
const { session } = require('./session');
const { registerErbHtmlProviders } = require('./erb_html');
const { fileWatcherPatterns } = require('./ruby_file_kinds');
const { readEditorState, serverConfiguration } = require('./configuration_state');
const {
    extractZippedStubs,
    getBundledExtensionPackages,
    getServerPath
} = require('./server_launch');
const { createEditorStatus } = require('./project_status/editor_status');
const { registerProjectCommands } = require('./project_status/project_commands');
const { registerCodeLensCommands } = require('./code_lens/code_lens_commands');
const {
    createRubyIndexView,
    registerExternalTypesToggle,
    registerRubyIndexCommands
} = require('./ruby_index/ruby_index_view');

function activate(context) {
    // Create single output channel for both extension and LSP server logs
    const outputChannel = vscode.window.createOutputChannel('Ruby Fast LSP');
    session.outputChannel = outputChannel;
    context.subscriptions.push(outputChannel);
    registerErbHtmlProviders(vscode, context, (message) => {
        outputChannel.appendLine(`[Ruby Fast LSP] ${message}`);
    });

    // Extract zipped stubs to the extension folder on first run
    // This ensures go-to-definition shows proper file paths
    extractZippedStubs(context.extensionPath);

    const config = vscode.workspace.getConfiguration('rubyFastLsp');
    const editorState = readEditorState(context.workspaceState, config);
    session.editorState = editorState;
    const extensionPackages = getBundledExtensionPackages(context.extensionPath);
    const initializationOptions = serverConfiguration({
        editorState,
        logLevel: config.get('logLevel', 'info'),
        extensionPath: context.extensionPath,
        extensionPackages,
        workspaceTrusted: vscode.workspace.isTrusted
    });

    const serverOptions = {
        command: getServerPath(),
        args: [],
        transport: TransportKind.stdio
    };

    let watchedFileEvents = fileWatcherPatterns(
        initializationOptions.indexing.includedPatterns || []
    ).map((pattern) => vscode.workspace.createFileSystemWatcher(pattern));
    context.subscriptions.push(...watchedFileEvents);

    const clientOptions = {
        documentSelector: [
            { scheme: 'file', language: 'ruby' },
            { scheme: 'file', language: 'erb' }
        ],
        synchronize: {
            fileEvents: watchedFileEvents
        },
        initializationOptions,
        outputChannel: outputChannel
    };

    const client = new LanguageClient(
        'ruby-fast-lsp',
        'Ruby Fast LSP',
        serverOptions,
        clientOptions
    );
    session.client = client;

    const editorStatus = createEditorStatus({ context, client });
    const { refreshRuntimeStatusBar, restartClientWithFreshIndexingStatus } = editorStatus;

    // Handle configuration changes
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration(async event => {
            if (event.affectsConfiguration('rubyFastLsp.logLevel') && client) {
                initializationOptions.logLevel = vscode.workspace
                    .getConfiguration('rubyFastLsp')
                    .get('logLevel', 'info');
                client.sendNotification('workspace/didChangeConfiguration', {
                    settings: { rubyFastLsp: initializationOptions }
                });
            }
        })
    );

    context.subscriptions.push(
        vscode.workspace.onDidGrantWorkspaceTrust(async () => {
            initializationOptions.workspaceTrusted = true;
            if (client) {
                await restartClientWithFreshIndexingStatus();
                await refreshRuntimeStatusBar();
            }
        })
    );

    const view = createRubyIndexView({ editorState, editorStatus });
    const { indexProvider, treeView } = view;
    editorStatus.setProjectsRefreshScheduler(view.scheduleRubyProjectsRefresh);

    const {
        refreshCommand,
        exportCommand,
        gotoDefinitionCommand,
        showLocationsCommand,
        searchCommand
    } = registerRubyIndexCommands({ client, outputChannel, editorState, view });
    const {
        showReferencesCommand,
        runRspecCommand,
        debugRspecCommand,
        runMinitestCommand,
        debugMinitestCommand,
        openRailsViewCommand
    } = registerCodeLensCommands();
    const toggleExternalTypesCommand = registerExternalTypesToggle({ context, editorState, view });
    const {
        selectRuntimeCommand,
        runtimeStatusCommand,
        openGemfileCommand,
        indexingStatusCommand,
        configureRuntimeCommand,
        selectLinterCommand,
        selectFormatterCommand,
        configureLoadPathsCommand
    } = registerProjectCommands({
        context,
        client,
        outputChannel,
        editorState,
        initializationOptions,
        editorStatus
    });

    context.subscriptions.push(treeView, refreshCommand, exportCommand, gotoDefinitionCommand, showLocationsCommand, showReferencesCommand, runRspecCommand, debugRspecCommand, runMinitestCommand, debugMinitestCommand, openRailsViewCommand, searchCommand, toggleExternalTypesCommand, selectRuntimeCommand, runtimeStatusCommand, openGemfileCommand, indexingStatusCommand, configureRuntimeCommand, selectLinterCommand, selectFormatterCommand, configureLoadPathsCommand);

    // Start the client and initialize index tree when ready
    client.start().then(() => {
        indexProvider.refresh();
        void refreshRuntimeStatusBar();
    }).catch(error => {
        outputChannel.appendLine(`[Ruby Index] LSP client failed to start: ${error}`);
    });

    // Auto-refresh when active editor changes
    context.subscriptions.push(
        vscode.window.onDidChangeActiveTextEditor(() => {
            void refreshRuntimeStatusBar();
            if (['ruby', 'erb'].includes(vscode.window.activeTextEditor?.document.languageId)) {
                indexProvider.refresh();
            }
        })
    );

    // Auto-refresh index tree when Ruby files are saved or changed
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument((document) => {
            if (['ruby', 'erb'].includes(document.languageId)) {
                // Debounce the refresh to avoid excessive updates
                setTimeout(() => {
                    indexProvider.refresh();
                }, 500); // 500ms delay to match server-side debouncing
            }
        })
    );

    // Auto-refresh on real-time document changes (as you type)
    let changeTimeout;
    context.subscriptions.push(
        vscode.workspace.onDidChangeTextDocument((event) => {
            if (['ruby', 'erb'].includes(event.document.languageId)) {
                // Clear previous timeout to debounce rapid typing
                if (changeTimeout) {
                    clearTimeout(changeTimeout);
                }
                // Set new timeout for index tree refresh
                changeTimeout = setTimeout(() => {
                    indexProvider.refresh();
                }, 1000); // 1 second delay for typing changes
            }
        })
    );

    // Auto-refresh when Ruby files are opened or closed
    context.subscriptions.push(
        vscode.workspace.onDidOpenTextDocument((document) => {
            if (['ruby', 'erb'].includes(document.languageId)) {
                setTimeout(() => {
                    indexProvider.refresh();
                }, 500);
            }
        })
    );

    context.subscriptions.push(
        vscode.workspace.onDidCloseTextDocument((document) => {
            if (['ruby', 'erb'].includes(document.languageId)) {
                setTimeout(() => {
                    indexProvider.refresh();
                }, 500);
            }
        })
    );
}

function deactivate() {
    if (!session.client) {
        return undefined;
    }
    return session.client.stop();
}

module.exports = { activate, deactivate };
