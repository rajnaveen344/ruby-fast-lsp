// Ruby Projects view wiring: the tree view, its refresh scheduling and
// library-section message, and the rubyIndex.* commands it contributes.
const vscode = require('vscode');
const path = require('path');
const { STATE_KEYS } = require('../configuration_state');
const { RubyIndexProvider } = require('./ruby_index_provider');
const {
    orderRubyIndexProjects,
    namespaceSearchQuickPickItems,
    projectBrowseSections,
    findOwningWorkspaceFolder
} = require('./ruby_index_tree');

function createRubyIndexView({ editorState, editorStatus }) {
    // Register Ruby Index Tree
    const indexProvider = new RubyIndexProvider({
        getIndexingProjects: () => editorStatus.indexingStatusSession.snapshot().projects,
        getRuntimeProjects: () => editorStatus.runtimeProjects || [],
        getShowExternalTypes: () => Boolean(editorState?.showExternalTypes)
    });
    const treeView = vscode.window.createTreeView('rubyIndex', {
        treeDataProvider: indexProvider,
        showCollapseAll: true
    });

    let rubyProjectsRefreshTimer;
    const updateLibrarySectionsMessage = () => {
        treeView.message = editorState.showExternalTypes
            ? undefined
            : 'Standard Library & Gems are hidden — click the library icon in this view’s toolbar to show them.';
    };
    const scheduleRubyProjectsRefresh = () => {
        if (rubyProjectsRefreshTimer) {
            clearTimeout(rubyProjectsRefreshTimer);
        }
        rubyProjectsRefreshTimer = setTimeout(() => {
            indexProvider.refresh();
            updateLibrarySectionsMessage();
        }, 250);
    };
    updateLibrarySectionsMessage();

    return { indexProvider, treeView, updateLibrarySectionsMessage, scheduleRubyProjectsRefresh };
}

function registerRubyIndexCommands({ client, outputChannel, editorState, view }) {
    const { indexProvider, treeView, updateLibrarySectionsMessage } = view;

    // Register refresh command
    const refreshCommand = vscode.commands.registerCommand('rubyIndex.refresh', () => {
        indexProvider.refresh();
        updateLibrarySectionsMessage();
    });

    // Register export command to download inheritance graph as JSON
    const exportCommand = vscode.commands.registerCommand('rubyIndex.export', async () => {
        if (!client || client.state !== 2) {
            vscode.window.showWarningMessage('Ruby Fast LSP is not ready yet. Please wait for indexing to complete.');
            return;
        }

        try {
            outputChannel.appendLine('[Ruby Fast LSP] Exporting inheritance graph as JSON...');
            const response = await client.sendRequest('ruby/exportGraph', {});

            if (response && response.nodes) {
                // Create a new document with the JSON content
                const doc = await vscode.workspace.openTextDocument({
                    content: JSON.stringify(response, null, 2),
                    language: 'json'
                });
                await vscode.window.showTextDocument(doc);
                outputChannel.appendLine(`[Ruby Fast LSP] Graph export complete: ${response.node_count} nodes`);
            } else {
                vscode.window.showWarningMessage('No graph data available to export.');
            }
        } catch (error) {
            outputChannel.appendLine(`[Ruby Fast LSP] Failed to export graph: ${error.message}`);
            vscode.window.showErrorMessage(`Failed to export graph: ${error.message}`);
        }
    });

    // Register goto definition command for tree items
    const gotoDefinitionCommand = vscode.commands.registerCommand('rubyIndex.gotoDefinition', async (fqn) => {
        if (!client || client.state !== 2) {
            vscode.window.showWarningMessage('Ruby Fast LSP is not ready yet. Please wait for indexing to complete.');
            return;
        }

        try {
            // Use the debug/lookup endpoint to find the definition location
            const response = await client.sendRequest('ruby-fast-lsp/debug/lookup', { fqn });

            if (response && response.found && response.entries && response.entries.length > 0) {
                // Get the first entry's location
                const entry = response.entries[0];
                // Location format: "file:///path/to/file.rb:line:col" (0-indexed)
                // Match the URI and the trailing :line:col
                const locationMatch = entry.location.match(/^(.+):(\d+):(\d+)$/);

                if (locationMatch) {
                    const uri = locationMatch[1];
                    const line = parseInt(locationMatch[2]);
                    const col = parseInt(locationMatch[3]);

                    const doc = await vscode.workspace.openTextDocument(vscode.Uri.parse(uri));
                    const editor = await vscode.window.showTextDocument(doc);
                    const position = new vscode.Position(line, col);
                    editor.selection = new vscode.Selection(position, position);
                    editor.revealRange(new vscode.Range(position, position), vscode.TextEditorRevealType.InCenter);
                }
            } else {
                vscode.window.showWarningMessage(`Definition not found for: ${fqn}`);
            }
        } catch (error) {
            outputChannel.appendLine(`[Ruby Fast LSP] Failed to goto definition: ${error.message}`);
        }
    });

    // Register show locations command for items with multiple definitions/call sites
    const showLocationsCommand = vscode.commands.registerCommand('rubyIndex.showLocations', async (name, locations) => {
        if (!locations || locations.length === 0) {
            vscode.window.showWarningMessage(`No locations found for: ${name}`);
            return;
        }

        // Build quick pick items with file path info
        const items = locations.map((loc) => {
            const uri = vscode.Uri.parse(loc.uri);
            const fileName = path.basename(uri.fsPath);
            const relativePath = vscode.workspace.asRelativePath(uri);
            return {
                label: `${fileName}:${(loc.line || 0) + 1}`,
                description: relativePath,
                detail: `Line ${(loc.line || 0) + 1}, Column ${(loc.character || 0) + 1}`,
                location: loc
            };
        });

        const selected = await vscode.window.showQuickPick(items, {
            placeHolder: `Select a location for "${name}" (${locations.length} found)`,
            matchOnDescription: true
        });

        if (selected) {
            const loc = selected.location;
            const doc = await vscode.workspace.openTextDocument(vscode.Uri.parse(loc.uri));
            const editor = await vscode.window.showTextDocument(doc);
            const position = new vscode.Position(loc.line || 0, loc.character || 0);
            editor.selection = new vscode.Selection(position, position);
            editor.revealRange(new vscode.Range(position, position), vscode.TextEditorRevealType.InCenter);
        }
    });

    // Ctrl+P-style Go to Class/Module for Ruby Projects (Cmd/Ctrl+Shift+R).
    const searchCommand = vscode.commands.registerCommand('rubyIndex.search', async () => {
        if (!client || client.state !== 2) {
            vscode.window.showWarningMessage('Ruby Fast LSP is not ready yet. Please wait for indexing to complete.');
            return;
        }

        const activeUri = vscode.window.activeTextEditor?.document?.uri;
        const activePath = activeUri?.fsPath;
        const workspaceFolders = (vscode.workspace.workspaceFolders || [])
            .map((folder) => folder.uri.fsPath);
        const projects = orderRubyIndexProjects(indexProvider._resolveProjects());
        const activeProjectRoot = activePath
            ? (findOwningWorkspaceFolder(activePath, projects.map((project) => project.root))
                || findOwningWorkspaceFolder(activePath, workspaceFolders))
            : null;

        const byFqn = new Map();
        const remember = (namespaces, projectRoot) => {
            for (const ns of indexProvider._flattenNamespaces(namespaces)) {
                byFqn.set(ns.fqn, { ...ns, projectRoot: projectRoot || ns.projectRoot });
            }
        };

        for (const ns of indexProvider.getAllNamespaces()) {
            remember([ns], ns.projectRoot);
        }

        const requestUri = activeUri?.toString()
            || (activeProjectRoot
                ? vscode.Uri.file(activeProjectRoot).toString()
                : (projects[0] ? vscode.Uri.file(projects[0].root).toString() : ''));

        try {
            const response = await client.sendRequest('ruby/namespaceTree', {
                uri: requestUri,
                show_external_types: editorState.showExternalTypes
            });
            if (response && (response.modules || response.classes || response.libraries)) {
                const sections = projectBrowseSections(
                    response,
                    editorState.showExternalTypes
                );
                const projectRoot = activeProjectRoot || projects[0]?.root;
                remember([
                    ...sections.projectNamespaces,
                    ...sections.libraryNamespaces
                ], projectRoot);
                indexProvider._cachedNamespaces = [...byFqn.values()];
            }
        } catch (error) {
            vscode.window.showErrorMessage(`Failed to fetch namespaces: ${error.message}`);
            return;
        }

        const namespaces = [...byFqn.values()];
        if (namespaces.length === 0) {
            vscode.window.showInformationMessage('No namespaces found in Ruby Projects.');
            return;
        }

        const items = namespaceSearchQuickPickItems(namespaces, {
            activeProjectRoot
        });

        const selected = await vscode.window.showQuickPick(items, {
            title: 'Go to Class/Module in Ruby Projects',
            placeHolder: 'Type a class or module name (like Ctrl+P)',
            matchOnDescription: true,
            matchOnDetail: true
        });

        if (!selected) {
            return;
        }

        let item = indexProvider.getItemByFqn(selected.fqn);
        if (!item) {
            item = indexProvider._buildSingleTreeItem(
                selected.namespaceData,
                null,
                null,
                null
            );
            indexProvider._fqnToItem.set(selected.fqn, item);
        }

        try {
            await treeView.reveal(item, { select: true, focus: true, expand: 3 });
        } catch (error) {
            outputChannel.appendLine(`[Ruby Index] Failed to reveal item: ${error.message}`);
            const locations = selected.namespaceData?.locations || [];
            if (locations.length > 0) {
                const loc = locations[0];
                const doc = await vscode.workspace.openTextDocument(vscode.Uri.parse(loc.uri));
                const editor = await vscode.window.showTextDocument(doc);
                const position = new vscode.Position(loc.line || 0, loc.character || 0);
                editor.selection = new vscode.Selection(position, position);
                editor.revealRange(
                    new vscode.Range(position, position),
                    vscode.TextEditorRevealType.InCenter
                );
            } else {
                vscode.window.showInformationMessage(`Found: ${selected.fqn}`);
            }
        }
    });

    return { refreshCommand, exportCommand, gotoDefinitionCommand, showLocationsCommand, searchCommand };
}

function registerExternalTypesToggle({ context, editorState, view }) {
    const { indexProvider, updateLibrarySectionsMessage } = view;

    const syncLibrarySectionsContext = () => {
        void vscode.commands.executeCommand(
            'setContext',
            'rubyIndex.showLibrarySections',
            Boolean(editorState?.showExternalTypes)
        );
    };
    syncLibrarySectionsContext();

    // View title (top-right) library icon toggles Standard Library & Gems sections.
    const toggleExternalTypesCommand = vscode.commands.registerCommand('rubyIndex.toggleExternalTypes', async () => {
        editorState.showExternalTypes = !editorState.showExternalTypes;
        await context.workspaceState.update(
            STATE_KEYS.showExternalTypes,
            editorState.showExternalTypes
        );
        syncLibrarySectionsContext();
        updateLibrarySectionsMessage();
        vscode.window.showInformationMessage(
            editorState.showExternalTypes
                ? 'Ruby Projects: Showing Ruby Standard Library & Gems'
                : 'Ruby Projects: Hiding Ruby Standard Library & Gems — use the library toolbar icon to show them again'
        );
        indexProvider.refresh();
    });

    return toggleExternalTypesCommand;
}

module.exports = {
    createRubyIndexView,
    registerRubyIndexCommands,
    registerExternalTypesToggle
};
