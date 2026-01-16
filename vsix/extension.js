const vscode = require('vscode');
const path = require('path');
const fs = require('fs');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

// Create output channel for logging
let outputChannel;

let client;

/**
 * Extract zipped stubs to the extension's stubs directory on first run.
 * This ensures go-to-definition shows proper file paths instead of virtual URIs.
 * 
 * Only extracts if:
 * - stubs-zipped/*.zip files exist
 * - corresponding stubs/rubystubsXY directory doesn't exist or is outdated
 */
function extractZippedStubs(extensionPath) {
    const zippedDir = path.join(extensionPath, 'stubs-zipped');
    const stubsDir = path.join(extensionPath, 'stubs');

    if (!fs.existsSync(zippedDir)) {
        return; // No zipped stubs, nothing to do
    }

    const AdmZip = require('adm-zip');
    const zipFiles = fs.readdirSync(zippedDir).filter(f => f.endsWith('.zip'));

    for (const zipFile of zipFiles) {
        const version = zipFile.replace('.zip', ''); // e.g., "rubystubs30"
        const zipPath = path.join(zippedDir, zipFile);
        const extractPath = path.join(stubsDir, version);
        const markerFile = path.join(extractPath, '.extracted');

        // Check if we need to extract
        let needsExtract = false;
        if (!fs.existsSync(extractPath)) {
            needsExtract = true;
        } else if (!fs.existsSync(markerFile)) {
            needsExtract = true;
        } else {
            // Check if zip is newer than extraction
            const zipStat = fs.statSync(zipPath);
            const markerStat = fs.statSync(markerFile);
            if (zipStat.mtime > markerStat.mtime) {
                needsExtract = true;
            }
        }

        if (needsExtract) {
            try {
                if (outputChannel) {
                    outputChannel.appendLine(`[Ruby Fast LSP] Extracting ${zipFile}...`);
                }

                // Clean up old extraction if exists
                if (fs.existsSync(extractPath)) {
                    fs.rmSync(extractPath, { recursive: true });
                }

                // Extract
                const zip = new AdmZip(zipPath);
                zip.extractAllTo(extractPath, true);

                // Write marker file
                fs.writeFileSync(markerFile, new Date().toISOString());

                if (outputChannel) {
                    outputChannel.appendLine(`[Ruby Fast LSP] Extracted ${zipFile} to ${extractPath}`);
                }
            } catch (error) {
                if (outputChannel) {
                    outputChannel.appendLine(`[Ruby Fast LSP] Failed to extract ${zipFile}: ${error.message}`);
                }
            }
        }
    }
}

// Ruby Namespace Tree Data Provider
class RubyNamespaceTreeProvider {
    constructor() {
        this._onDidChangeTreeData = new vscode.EventEmitter();
        this.onDidChangeTreeData = this._onDidChangeTreeData.event;
    }

    refresh() {
        this._onDidChangeTreeData.fire();
    }

    getTreeItem(element) {
        return element;
    }

    async getChildren(element) {
        if (!client) {
            return [];
        }

        // Check if client is ready
        if (client.state !== 2) { // 2 = Running state
            return [];
        }

        try {
            if (!element) {
                // Root level - get namespace tree from LSP server
                const response = await client.sendRequest('ruby/namespaceTree', {
                    uri: vscode.window.activeTextEditor?.document.uri.toString() || ''
                });

                if (response && response.namespaces) {
                    return this.buildTreeItems(response.namespaces);
                }
            } else if (element.children) {
                // Return children of the current element
                return this.buildTreeItems(element.children);
            }
        } catch (error) {
            outputChannel.appendLine(`Ruby Fast LSP Namespace Tree Error: ${error.message}`);
        }

        return [];
    }

    buildTreeItems(namespaces) {
        return namespaces.map(ns => {
            const hasChildren = ns.children && ns.children.length > 0;
            const item = new vscode.TreeItem(
                ns.name,
                hasChildren ? vscode.TreeItemCollapsibleState.Collapsed : vscode.TreeItemCollapsibleState.None
            );

            // Build detailed tooltip with mixin information
            let tooltip = `${ns.kind}: ${ns.fqn}`;
            if (ns.superclass) {
                tooltip += `\nSuperclass: ${ns.superclass}`;
            }
            if (ns.includes && ns.includes.length > 0) {
                tooltip += `\nIncludes: ${ns.includes.join(', ')}`;
            }
            if (ns.prepends && ns.prepends.length > 0) {
                tooltip += `\nPrepends: ${ns.prepends.join(', ')}`;
            }
            if (ns.extends && ns.extends.length > 0) {
                tooltip += `\nExtends: ${ns.extends.join(', ')}`;
            }

            item.tooltip = tooltip;
            item.description = ns.kind;
            item.children = ns.children || [];

            // Set icon based on kind
            if (ns.kind === 'Class') {
                item.iconPath = new vscode.ThemeIcon('symbol-class');
            } else if (ns.kind === 'Module') {
                item.iconPath = new vscode.ThemeIcon('symbol-module');
            }

            // Add location information for navigation
            if (ns.location && ns.location.range && ns.location.range.start && ns.location.range.end) {
                item.command = {
                    command: 'vscode.open',
                    title: 'Open',
                    arguments: [
                        vscode.Uri.parse(ns.location.uri),
                        {
                            selection: new vscode.Range(
                                ns.location.range.start.line,
                                ns.location.range.start.character,
                                ns.location.range.end.line,
                                ns.location.range.end.character
                            )
                        }
                    ]
                };
            }

            return item;
        });
    }
}

function getServerPath() {
    const platform = process.platform;
    const arch = process.arch;
    const isWindows = platform === 'win32';
    const extension = isWindows ? '.exe' : '';
    const binaryName = `ruby-fast-lsp${extension}`;

    // Map platform.arch to the correct binary path
    const platformMap = {
        'darwin': {
            'x64': 'macos-x64',
            'arm64': 'macos-arm64'
        },
        'linux': {
            'x64': 'linux-x64',
            'arm64': 'linux-arm64'
        },
        'win32': {
            'x64': 'win32-x64',
            'arm64': 'win32-arm64'
        }
    };

    const platformInfo = platformMap[platform];
    if (!platformInfo) {
        throw new Error(`Unsupported platform: ${platform}`);
    }

    const platformDir = platformInfo[arch];
    if (!platformDir) {
        throw new Error(`Unsupported architecture ${arch} for platform ${platform}`);
    }

    return path.join(__dirname, 'bin', platformDir, binaryName);
}

function activate(context) {
    // Create single output channel for both extension and LSP server logs
    outputChannel = vscode.window.createOutputChannel('Ruby Fast LSP');
    context.subscriptions.push(outputChannel);

    // Extract zipped stubs to the extension folder on first run
    // This ensures go-to-definition shows proper file paths
    extractZippedStubs(context.extensionPath);

    const config = vscode.workspace.getConfiguration('rubyFastLsp');

    const serverOptions = {
        command: getServerPath(),
        args: [],
        transport: TransportKind.stdio
    };

    const clientOptions = {
        documentSelector: [{ scheme: 'file', language: 'ruby' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.rb'),
            configurationSection: 'rubyFastLsp'
        },
        initializationOptions: {
            rubyVersion: config.get('rubyVersion', 'auto'),
            stubsPath: config.get('stubsPath', ''),
            extensionPath: context.extensionPath
        },
        outputChannel: outputChannel
    };

    client = new LanguageClient(
        'ruby-fast-lsp',
        'Ruby Fast LSP',
        serverOptions,
        clientOptions
    );

    // Create status bar item for indexing progress
    const statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
    context.subscriptions.push(statusBarItem);

    // Listen for progress notifications from the LSP server
    client.onNotification('$/progress', (params) => {
        if (params.token === 'indexing') {
            const value = params.value;
            if (value.kind === 'begin') {
                statusBarItem.text = `$(sync~spin) ${value.title}: ${value.message || 'Starting...'}`;
                statusBarItem.show();
                outputChannel.appendLine(`[Ruby Fast LSP] ${value.title}: ${value.message || 'Starting...'}`);
            } else if (value.kind === 'report') {
                const message = value.message || 'Processing...';
                const percentage = value.percentage !== undefined ? ` (${value.percentage}%)` : '';
                statusBarItem.text = `$(sync~spin) ${message}${percentage}`;
                outputChannel.appendLine(`[Ruby Fast LSP] ${message}${percentage}`);
            } else if (value.kind === 'end') {
                statusBarItem.text = `$(check) Ruby Fast LSP: ${value.message || 'Ready'}`;
                outputChannel.appendLine(`[Ruby Fast LSP] ${value.message || 'Ready'}`);
                // Hide the status bar after a brief delay
                setTimeout(() => {
                    statusBarItem.hide();
                }, 3000);
            }
        }
    });

    // Handle configuration changes
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration(event => {
            if (event.affectsConfiguration('rubyFastLsp')) {
                // Notify the server about configuration changes
                if (client) {
                    const newConfig = vscode.workspace.getConfiguration('rubyFastLsp');
                    client.sendNotification('workspace/didChangeConfiguration', {
                        settings: {
                            rubyFastLsp: {
                                rubyVersion: newConfig.get('rubyVersion', 'auto'),
                                stubsPath: newConfig.get('stubsPath', ''),
                                logLevel: newConfig.get('logLevel', 'info')
                            }
                        }
                    });
                }
            }
        })
    );

    // Register Ruby Namespace Tree
    const namespaceTreeProvider = new RubyNamespaceTreeProvider();
    const treeView = vscode.window.createTreeView('rubyNamespaceTree', {
        treeDataProvider: namespaceTreeProvider,
        showCollapseAll: true
    });

    // Register refresh command
    const refreshCommand = vscode.commands.registerCommand('rubyNamespaceTree.refresh', () => {
        namespaceTreeProvider.refresh();
    });

    // Register wrapper command for showReferences to handle LSP JSON serialization
    const showReferencesCommand = vscode.commands.registerCommand('ruby-fast-lsp.showReferences',
        (uriStr, position, locations) => {
            // Convert JSON arguments to proper VS Code types
            const uri = vscode.Uri.parse(uriStr);
            const pos = new vscode.Position(position.line, position.character);
            const locs = locations.map(loc => new vscode.Location(
                vscode.Uri.parse(loc.uri),
                new vscode.Range(
                    new vscode.Position(loc.range.start.line, loc.range.start.character),
                    new vscode.Position(loc.range.end.line, loc.range.end.character)
                )
            ));

            // Call the built-in showReferences command with proper types
            return vscode.commands.executeCommand('editor.action.showReferences', uri, pos, locs);
        }
    );

    context.subscriptions.push(treeView, refreshCommand, showReferencesCommand);

    // Start the client and initialize namespace tree when ready
    client.start().then(() => {
        // Auto-refresh namespace tree when client is ready
        setTimeout(() => {
            namespaceTreeProvider.refresh();
        }, 1000); // Small delay to ensure everything is settled
    }).catch(error => {
        outputChannel.appendLine(`[NAMESPACE_TREE_EXT] LSP client failed to start: ${error}`);
    });

    // Auto-refresh when active editor changes
    context.subscriptions.push(
        vscode.window.onDidChangeActiveTextEditor(() => {
            if (vscode.window.activeTextEditor?.document.languageId === 'ruby') {
                namespaceTreeProvider.refresh();
            }
        })
    );

    // Auto-refresh namespace tree when Ruby files are saved or changed
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument((document) => {
            if (document.languageId === 'ruby') {
                // Debounce the refresh to avoid excessive updates
                setTimeout(() => {
                    namespaceTreeProvider.refresh();
                }, 500); // 500ms delay to match server-side debouncing
            }
        })
    );

    // Auto-refresh on real-time document changes (as you type)
    let changeTimeout;
    context.subscriptions.push(
        vscode.workspace.onDidChangeTextDocument((event) => {
            if (event.document.languageId === 'ruby') {
                // Clear previous timeout to debounce rapid typing
                if (changeTimeout) {
                    clearTimeout(changeTimeout);
                }
                // Set new timeout for namespace tree refresh
                changeTimeout = setTimeout(() => {
                    namespaceTreeProvider.refresh();
                }, 1000); // 1 second delay for typing changes
            }
        })
    );

    // Auto-refresh when Ruby files are opened or closed
    context.subscriptions.push(
        vscode.workspace.onDidOpenTextDocument((document) => {
            if (document.languageId === 'ruby') {
                setTimeout(() => {
                    namespaceTreeProvider.refresh();
                }, 500);
            }
        })
    );

    context.subscriptions.push(
        vscode.workspace.onDidCloseTextDocument((document) => {
            if (document.languageId === 'ruby') {
                setTimeout(() => {
                    namespaceTreeProvider.refresh();
                }, 500);
            }
        })
    );
}

function deactivate() {
    if (!client) {
        return undefined;
    }
    return client.stop();
}

module.exports = { activate, deactivate };
