// Editor status chrome for the active Ruby document: the left-hand indexing
// status bar item, the Gemfile and runtime language status items, the
// indexing clock, and the governed language-client restart.
const vscode = require('vscode');
const path = require('path');
const fs = require('fs');
const {
    gemfileLanguageStatusPresentation,
    runtimeLanguageStatusPresentation,
    runtimeStatusForDocument
} = require('./runtime_selector');
const {
    createIndexingStatusSession,
    indexingClockShouldRun,
    indexingStatusRequestParams,
    lspStatusBarError,
    lspStatusBarPresentation,
    lspStatusBarStarting
} = require('./indexing_status');

function createEditorStatus({ context, client }) {
    const indexingStatusBarItem = vscode.window.createStatusBarItem(
        vscode.StatusBarAlignment.Left,
        1
    );
    indexingStatusBarItem.command = 'ruby-fast-lsp.indexing.status';
    context.subscriptions.push(indexingStatusBarItem);
    const rubyLanguageSelector = [{ language: 'ruby' }, { language: 'erb' }];
    const gemfileLanguageStatusItem = vscode.languages.createLanguageStatusItem(
        'ruby-fast-lsp.gemfile',
        rubyLanguageSelector
    );
    const runtimeLanguageStatusItem = vscode.languages.createLanguageStatusItem(
        'ruby-fast-lsp.runtime',
        rubyLanguageSelector
    );
    gemfileLanguageStatusItem.name = 'Ruby Fast LSP';
    runtimeLanguageStatusItem.name = 'Ruby Fast LSP';
    context.subscriptions.push(gemfileLanguageStatusItem, runtimeLanguageStatusItem);
    let activeRuntimeProjectRoot;
    let activeRuntimeStatus;
    let runtimeStatusRefreshGeneration = 0;
    const indexingStatusSession = createIndexingStatusSession();
    const INDEXING_CLOCK_INTERVAL_MS = 250;
    let indexingClock;
    let runtimeProjects;
    let clientRestartPromise;
    // Assigned once the Ruby Projects view exists; see setProjectsRefreshScheduler.
    let scheduleRubyProjectsRefresh;
    const acceptIndexingSnapshot = snapshot => indexingStatusSession.accept(snapshot);
    const languageStatusSeverity = severity => {
        switch (severity) {
            case 'information':
                return vscode.LanguageStatusSeverity.Information;
            case 'warning':
                return vscode.LanguageStatusSeverity.Warning;
            case 'error':
                return vscode.LanguageStatusSeverity.Error;
            default:
                throw new Error(
                    `INVARIANT VIOLATED: unknown language status severity '${severity}'. `
                    + 'This is a bug because the editor cannot present an unrecognized status. '
                    + 'Fix: map the presentation severity onto vscode.LanguageStatusSeverity.'
                );
        }
    };
    const languageStatusCommand = command => {
        if (!command) {
            return undefined;
        }
        switch (command) {
            case 'ruby-fast-lsp.openGemfile':
                return { command, title: 'Open Gemfile' };
            case 'ruby-fast-lsp.runtime.configure':
                return { command, title: 'Configure Runtime' };
            case 'ruby-fast-lsp.indexing.status':
                return { command, title: 'Show Indexing Status' };
            default:
                throw new Error(
                    `INVARIANT VIOLATED: unknown language status command '${command}'. `
                    + 'This is a bug because the editor cannot present an unrecognized action. '
                    + 'Fix: map the presentation command onto a contributed editor command.'
                );
        }
    };
    const applyLanguageStatusItem = (item, presentation) => {
        item.name = presentation.name;
        item.text = presentation.text;
        item.detail = presentation.detail;
        item.busy = Boolean(presentation.busy);
        item.severity = languageStatusSeverity(presentation.severity || 'information');
        item.command = languageStatusCommand(presentation.command);
    };
    const paintLanguageStatus = status => {
        const gemfileExists = Boolean(
            status?.root && fs.existsSync(path.join(status.root, 'Gemfile'))
        );
        applyLanguageStatusItem(
            gemfileLanguageStatusItem,
            gemfileLanguageStatusPresentation(status, gemfileExists)
        );
        applyLanguageStatusItem(
            runtimeLanguageStatusItem,
            runtimeLanguageStatusPresentation(status)
        );
    };
    const paintIndexingStatusBar = presentation => {
        indexingStatusBarItem.text = presentation.text;
        indexingStatusBarItem.tooltip = presentation.tooltip;
        indexingStatusBarItem.command = presentation.command;
    };
    const stopIndexingClock = () => {
        if (indexingClock) {
            clearInterval(indexingClock);
            indexingClock = undefined;
        }
    };
    const renderActiveRuntimeStatus = () => {
        if (!activeRuntimeStatus?.root) {
            return;
        }
        const indexingStatus = indexingStatusSession.snapshot();
        const nowMs = Date.now();
        const indexing = indexingStatus.projects.find(
            project => project.root === activeRuntimeStatus.root
        );
        if (indexing) {
            activeRuntimeStatus = { ...activeRuntimeStatus, indexing };
        }
        const presentation = lspStatusBarPresentation(
            activeRuntimeStatus,
            indexingStatus,
            nowMs,
            indexingStatus.receivedAtMs
        );
        if (indexingStatus.aggregate) {
            presentation.tooltip = `${presentation.tooltip}\n\nWorkspace: ${indexingStatus.aggregate.ready} ready, ${indexingStatus.aggregate.active} active, ${indexingStatus.aggregate.queued} queued, ${indexingStatus.aggregate.failed} failed`;
        }
        paintIndexingStatusBar(presentation);
        paintLanguageStatus(activeRuntimeStatus);
    };
    const renderCachedRuntimeStatus = editor => {
        if (!Array.isArray(runtimeProjects)) {
            return false;
        }
        const indexingStatus = indexingStatusSession.snapshot();
        const projects = runtimeProjects.map(project => ({
            ...project,
            indexing: indexingStatus.projects.find(indexing => indexing.root === project.root)
                || project.indexing
        }));
        const status = runtimeStatusForDocument(projects, editor.document.uri.fsPath);
        activeRuntimeProjectRoot = status?.root;
        activeRuntimeStatus = status;
        if (!status) {
            paintIndexingStatusBar(lspStatusBarPresentation(undefined));
            paintLanguageStatus(undefined);
            return true;
        }
        renderActiveRuntimeStatus();
        return true;
    };
    const paintEditorStatus = editor => {
        if (renderCachedRuntimeStatus(editor)) {
            return;
        }
        paintIndexingStatusBar(lspStatusBarStarting());
        const snapshot = indexingStatusSession.snapshot();
        const indexing = runtimeStatusForDocument(
            snapshot.projects,
            editor.document.uri.fsPath
        );
        if (indexing) {
            paintLanguageStatus({ root: indexing.root, indexing });
            return;
        }
        paintLanguageStatus(undefined);
    };
    const syncIndexingClock = () => {
        if (!indexingClockShouldRun(indexingStatusSession.snapshot())) {
            stopIndexingClock();
            return;
        }
        if (indexingClock) {
            return;
        }
        indexingClock = setInterval(() => {
            const editor = vscode.window.activeTextEditor;
            if (editor && ['ruby', 'erb'].includes(editor.document.languageId)) {
                paintEditorStatus(editor);
            }
        }, INDEXING_CLOCK_INTERVAL_MS);
    };
    const syncIndexingChrome = () => {
        const editor = vscode.window.activeTextEditor;
        if (editor && ['ruby', 'erb'].includes(editor.document.languageId)) {
            paintEditorStatus(editor);
            indexingStatusBarItem.show();
        }
        syncIndexingClock();
    };
    const refreshRuntimeStatusBar = async () => {
        const generation = ++runtimeStatusRefreshGeneration;
        const editor = vscode.window.activeTextEditor;
        if (!editor || !['ruby', 'erb'].includes(editor.document.languageId)) {
            activeRuntimeProjectRoot = undefined;
            activeRuntimeStatus = undefined;
            indexingStatusBarItem.hide();
            syncIndexingClock();
            return;
        }
        paintEditorStatus(editor);
        indexingStatusBarItem.show();
        syncIndexingClock();
        if (!client || client.state !== 2) {
            return;
        }
        try {
            const [response, indexingResponse] = await Promise.all([
                client.sendRequest('ruby-fast-lsp/runtime/status', {}),
                client.sendRequest(
                    'ruby-fast-lsp/indexing/status',
                    indexingStatusRequestParams(editor)
                )
            ]);
            if (generation !== runtimeStatusRefreshGeneration) {
                return;
            }
            acceptIndexingSnapshot(indexingResponse);
            runtimeProjects = Array.isArray(response?.projects) ? response.projects : [];
            paintEditorStatus(editor);
            syncIndexingClock();
        } catch (error) {
            if (generation !== runtimeStatusRefreshGeneration) {
                return;
            }
            activeRuntimeProjectRoot = undefined;
            activeRuntimeStatus = undefined;
            paintIndexingStatusBar(lspStatusBarError(
                `Ruby Fast LSP runtime status failed: ${error.message}`
            ));
            paintLanguageStatus(undefined);
        }
    };

    const restartClientWithFreshIndexingStatus = () => {
        if (clientRestartPromise) {
            return clientRestartPromise;
        }
        indexingStatusSession.suspendForRestart();
        runtimeStatusRefreshGeneration += 1;
        runtimeProjects = undefined;
        activeRuntimeProjectRoot = undefined;
        activeRuntimeStatus = undefined;
        stopIndexingClock();
        paintIndexingStatusBar(lspStatusBarStarting('Ruby Fast LSP is restarting'));
        indexingStatusBarItem.show();
        paintLanguageStatus(undefined);
        clientRestartPromise = (async () => {
            try {
                await client.restart();
            } finally {
                indexingStatusSession.completeRestart();
                clientRestartPromise = undefined;
            }
        })();
        return clientRestartPromise;
    };

    const indexingStatusNotification = client.onNotification(
        'ruby-fast-lsp/indexing/statusChanged',
        (snapshot) => {
            if (!acceptIndexingSnapshot(snapshot)) {
                return;
            }
            syncIndexingChrome();
            if (typeof scheduleRubyProjectsRefresh === 'function') {
                scheduleRubyProjectsRefresh();
            }
        }
    );
    paintLanguageStatus(undefined);
    context.subscriptions.push(
        indexingStatusNotification,
        {
            dispose: () => {
                stopIndexingClock();
                indexingStatusSession.dispose();
            }
        }
    );

    return {
        indexingStatusSession,
        acceptIndexingSnapshot,
        refreshRuntimeStatusBar,
        restartClientWithFreshIndexingStatus,
        setProjectsRefreshScheduler: scheduler => {
            scheduleRubyProjectsRefresh = scheduler;
        },
        get activeRuntimeProjectRoot() {
            return activeRuntimeProjectRoot;
        },
        get activeRuntimeStatus() {
            return activeRuntimeStatus;
        },
        get runtimeProjects() {
            return runtimeProjects;
        },
        set runtimeProjects(projects) {
            runtimeProjects = projects;
        }
    };
}

module.exports = { createEditorStatus };
