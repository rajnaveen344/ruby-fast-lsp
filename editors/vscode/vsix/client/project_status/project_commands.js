// Project configuration commands: runtime selection, status, and
// .ruby-version persistence, Gemfile and indexing status, linter and
// formatter selection, and per-project require load paths.
const vscode = require('vscode');
const path = require('path');
const fs = require('fs');
const {
    runtimeStatusItem,
    runtimeVersionMarker,
    selectRuntime
} = require('./runtime_selector');
const {
    STATE_KEYS,
    pathsForProject,
    updateLoadPaths,
    updateRuntime,
    validLoadPaths
} = require('../configuration_state');
const {
    indexingStatusQuickPickItems,
    indexingStatusQuickPickPlaceholder,
    indexingStatusRequestParams
} = require('./indexing_status');

async function selectExternalTool(kind, current) {
    const selected = await vscode.window.showQuickPick(
        [
            {
                label: 'Disabled',
                description: 'Do not run an external tool',
                id: 'none',
                picked: current === 'none'
            },
            {
                label: 'RuboCop',
                description: 'Use bundle exec rubocop from the owning Ruby project',
                id: 'rubocop',
                picked: current === 'rubocop'
            },
            {
                label: 'Standard',
                description: 'Use bundle exec standardrb from the owning Ruby project',
                id: 'standard',
                picked: current === 'standard'
            }
        ],
        {
            title: `Ruby Fast LSP: Select ${kind}`,
            placeHolder: `Choose the ${kind.toLowerCase()} for Ruby projects`
        }
    );
    return selected?.id;
}

function registerProjectCommands({
    context,
    client,
    outputChannel,
    editorState,
    initializationOptions,
    editorStatus
}) {
    const {
        acceptIndexingSnapshot,
        indexingStatusSession,
        refreshRuntimeStatusBar,
        restartClientWithFreshIndexingStatus
    } = editorStatus;

    const selectRuntimeCommand = vscode.commands.registerCommand(
        'ruby-fast-lsp.runtime.select',
        async () => selectRuntime({
            window: vscode.window,
            client,
            preferredProjectRoot: editorStatus.activeRuntimeProjectRoot,
            applySelection: async selection => {
                editorState.runtime = updateRuntime(editorState.runtime, selection);
                await context.workspaceState.update(STATE_KEYS.runtime, editorState.runtime);
                initializationOptions.runtime = editorState.runtime;
                await restartClientWithFreshIndexingStatus();
                await refreshRuntimeStatusBar();
            }
        })
    );
    const runtimeStatusCommand = vscode.commands.registerCommand(
        'ruby-fast-lsp.runtime.status',
        async () => {
            const status = await client.sendRequest('ruby-fast-lsp/runtime/status', {});
            const projects = Array.isArray(status?.projects) ? status.projects : [];
            if (projects.length === 0) {
                await vscode.window.showInformationMessage(
                    'Ruby Fast LSP has no registered Ruby projects.'
                );
                return;
            }
            await vscode.window.showQuickPick(projects.map(runtimeStatusItem), {
                title: 'Ruby Fast LSP: Effective Runtime Status',
                placeHolder: 'Project, runtime, JDK, overlay, classpath, and indexing state'
            });
        }
    );
    const openGemfileCommand = vscode.commands.registerCommand(
        'ruby-fast-lsp.openGemfile',
        async () => {
            if (!editorStatus.activeRuntimeProjectRoot) {
                await vscode.window.showInformationMessage(
                    'The active document is not owned by a discovered Ruby project.'
                );
                return;
            }
            const gemfile = vscode.Uri.file(path.join(editorStatus.activeRuntimeProjectRoot, 'Gemfile'));
            if (!fs.existsSync(gemfile.fsPath)) {
                await vscode.window.showInformationMessage(
                    'This Ruby project has no Gemfile.'
                );
                return;
            }
            const document = await vscode.workspace.openTextDocument(gemfile);
            return vscode.window.showTextDocument(document);
        }
    );
    const indexingStatusCommand = vscode.commands.registerCommand(
        'ruby-fast-lsp.indexing.status',
        async () => {
            if (!client || client.state !== 2) {
                await vscode.window.showWarningMessage(
                    'Ruby Fast LSP is not ready to report project indexing status.'
                );
                return;
            }
            const [response, runtimeResponse] = await Promise.all([
                client.sendRequest(
                    'ruby-fast-lsp/indexing/status',
                    indexingStatusRequestParams(vscode.window.activeTextEditor)
                ),
                client.sendRequest('ruby-fast-lsp/runtime/status', {})
            ]);
            acceptIndexingSnapshot(response);
            editorStatus.runtimeProjects = Array.isArray(runtimeResponse?.projects)
                ? runtimeResponse.projects
                : [];
            const snapshot = indexingStatusSession.snapshot();
            const items = indexingStatusQuickPickItems(
                snapshot,
                editorStatus.activeRuntimeProjectRoot,
                editorStatus.runtimeProjects
            );
            if (items.length === 0) {
                await vscode.window.showInformationMessage(
                    'Ruby Fast LSP has no registered Ruby projects.'
                );
                return;
            }
            await vscode.window.showQuickPick(items, {
                title: 'Ruby Fast LSP: Project Indexing Status',
                placeHolder: indexingStatusQuickPickPlaceholder(snapshot),
                matchOnDescription: true,
                matchOnDetail: true
            });
        }
    );

    const applyAutoRuntime = async projectRoot => {
        editorState.runtime = updateRuntime(editorState.runtime, {
            projectRoot,
            mode: 'auto'
        });
        await context.workspaceState.update(STATE_KEYS.runtime, editorState.runtime);
        initializationOptions.runtime = editorState.runtime;
        await restartClientWithFreshIndexingStatus();
        await refreshRuntimeStatusBar();
    };

    const saveRuntimeMarker = async status => {
        if (!vscode.workspace.isTrusted) {
            await vscode.window.showErrorMessage(
                'Trust this workspace before writing its .ruby-version file.'
            );
            return;
        }
        const marker = runtimeVersionMarker(status);
        if (!marker || !status.root) {
            await vscode.window.showErrorMessage(
                'The active project has no exact effective runtime to save.'
            );
            return;
        }
        const markerUri = vscode.Uri.file(path.join(status.root, '.ruby-version'));
        let existing;
        try {
            existing = Buffer.from(await vscode.workspace.fs.readFile(markerUri))
                .toString('utf8')
                .trim();
        } catch (error) {
            if (error?.code !== 'FileNotFound') {
                outputChannel.appendLine(
                    `[Ruby Fast LSP] Unable to read ${markerUri.fsPath}: ${error.message}`
                );
            }
        }
        if (existing !== marker) {
            const confirmation = await vscode.window.showWarningMessage(
                `Save ${marker} to ${markerUri.fsPath}?`,
                {
                    modal: true,
                    detail: existing
                        ? `This replaces the current value: ${existing}`
                        : 'This creates a project-owned runtime marker that can be shared with the repository.'
                },
                'Save Runtime'
            );
            if (confirmation !== 'Save Runtime') {
                return;
            }
            await vscode.workspace.fs.writeFile(markerUri, Buffer.from(`${marker}\n`, 'utf8'));
        }
        await applyAutoRuntime(status.root);
        await vscode.window.showInformationMessage(
            `Saved ${marker} for ${path.basename(status.root)} and switched it to Auto.`
        );
    };

    const configureRuntimeCommand = vscode.commands.registerCommand(
        'ruby-fast-lsp.runtime.configure',
        async () => {
            const project = editorStatus.activeRuntimeStatus?.root
                ? path.basename(editorStatus.activeRuntimeStatus.root)
                : 'active Ruby project';
            const actions = [
                {
                    label: '$(settings-gear) Change Runtime…',
                    description: `Select an exact runtime for ${project}`,
                    id: 'change'
                },
                {
                    label: '$(refresh) Use Auto Detection',
                    description: `Resolve ${project} from its project markers and environment`,
                    id: 'auto'
                },
                {
                    label: '$(list-flat) Show All Project Runtimes',
                    description: 'Inspect runtime, JDK, overlay, and classpath status',
                    id: 'status'
                },
                {
                    label: '$(server-process) Show Project Indexing',
                    description: 'Inspect authoritative phase, progress, timing, and failures',
                    id: 'indexing'
                },
                {
                    label: '$(checklist) Select Linter…',
                    description: 'Disabled, RuboCop, or Standard',
                    id: 'linter'
                },
                {
                    label: '$(wand) Select Formatter…',
                    description: 'Disabled, RuboCop, or Standard',
                    id: 'formatter'
                }
            ];
            if (runtimeVersionMarker(editorStatus.activeRuntimeStatus || {})) {
                actions.splice(1, 0, {
                    label: '$(save) Save Runtime to .ruby-version',
                    description: 'Persist the exact runtime in the owning project',
                    id: 'save'
                });
            }
            const action = await vscode.window.showQuickPick(actions, {
                title: `Ruby Fast LSP: Configure ${project}`,
                placeHolder: 'Choose a project runtime action'
            });
            if (!action) {
                return;
            }
            if (action.id === 'change') {
                await vscode.commands.executeCommand('ruby-fast-lsp.runtime.select');
            } else if (action.id === 'save') {
                await saveRuntimeMarker(editorStatus.activeRuntimeStatus);
            } else if (action.id === 'auto') {
                if (!editorStatus.activeRuntimeProjectRoot) {
                    await vscode.window.showErrorMessage(
                        'The active document is not owned by a discovered Ruby project.'
                    );
                    return;
                }
                await applyAutoRuntime(editorStatus.activeRuntimeProjectRoot);
            } else if (action.id === 'status') {
                await vscode.commands.executeCommand('ruby-fast-lsp.runtime.status');
            } else if (action.id === 'indexing') {
                await vscode.commands.executeCommand('ruby-fast-lsp.indexing.status');
            } else if (action.id === 'linter') {
                await vscode.commands.executeCommand('ruby-fast-lsp.linter.select');
            } else if (action.id === 'formatter') {
                await vscode.commands.executeCommand('ruby-fast-lsp.formatter.select');
            }
        }
    );

    const selectLinterCommand = vscode.commands.registerCommand(
        'ruby-fast-lsp.linter.select',
        async () => {
            const selected = await selectExternalTool('Linter', editorState.linter);
            if (selected === undefined) {
                return;
            }
            editorState.linter = selected;
            await context.workspaceState.update(STATE_KEYS.linter, selected);
            initializationOptions.linter = selected;
            client.sendNotification('workspace/didChangeConfiguration', {
                settings: { rubyFastLsp: initializationOptions }
            });
        }
    );
    const selectFormatterCommand = vscode.commands.registerCommand(
        'ruby-fast-lsp.formatter.select',
        async () => {
            const selected = await selectExternalTool('Formatter', editorState.formatter);
            if (selected === undefined) {
                return;
            }
            editorState.formatter = selected;
            await context.workspaceState.update(STATE_KEYS.formatter, selected);
            initializationOptions.formatter = selected;
            client.sendNotification('workspace/didChangeConfiguration', {
                settings: { rubyFastLsp: initializationOptions }
            });
        }
    );
    const configureLoadPathsCommand = vscode.commands.registerCommand(
        'ruby-fast-lsp.indexing.configureLoadPaths',
        async () => {
            if (!editorStatus.activeRuntimeProjectRoot) {
                await vscode.window.showErrorMessage(
                    'The active document is not owned by a discovered Ruby project.'
                );
                return;
            }
            const projectLabel = path.basename(editorStatus.activeRuntimeProjectRoot);
            const currentPaths = pathsForProject(editorState.loadPaths, editorStatus.activeRuntimeProjectRoot);
            const input = await vscode.window.showInputBox({
                title: `Require Load Paths — ${projectLabel}`,
                prompt: `Editing ${editorStatus.activeRuntimeProjectRoot} · comma-separated project-relative dirs (restart required)`,
                value: currentPaths.join(', '),
                placeHolder: 'custom_lib, shared/lib'
            });
            if (input === undefined) {
                return;
            }
            const paths = input
                .split(',')
                .map(entry => entry.trim())
                .filter(entry => entry.length > 0);
            if (!validLoadPaths(paths)) {
                vscode.window.showErrorMessage(
                    'Load paths must be project-relative paths without parent traversal.'
                );
                return;
            }
            editorState.loadPaths = updateLoadPaths(editorState.loadPaths, {
                projectRoot: editorStatus.activeRuntimeProjectRoot,
                paths
            });
            await context.workspaceState.update(STATE_KEYS.loadPaths, editorState.loadPaths);
            initializationOptions.indexing = {
                ...initializationOptions.indexing,
                loadPaths: {
                    default: [...(editorState.loadPaths.default || [])],
                    projects: (editorState.loadPaths.projects || []).map(project => ({
                        root: project.root,
                        paths: [...project.paths]
                    }))
                }
            };
            const restart = await vscode.window.showInformationMessage(
                `Require load paths updated for ${projectLabel}. Restart Ruby Fast LSP to apply.`,
                'Restart'
            );
            if (restart === 'Restart') {
                await restartClientWithFreshIndexingStatus();
            }
        }
    );

    return {
        selectRuntimeCommand,
        runtimeStatusCommand,
        openGemfileCommand,
        indexingStatusCommand,
        configureRuntimeCommand,
        selectLinterCommand,
        selectFormatterCommand,
        configureLoadPathsCommand
    };
}

module.exports = { registerProjectCommands };
