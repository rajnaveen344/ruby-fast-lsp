const vscode = require('vscode');
const path = require('path');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

// Create output channel for logging
let outputChannel;

let client;

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
            
            item.tooltip = `${ns.kind}: ${ns.name}`;
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
    // Create output channel for extension logs
    outputChannel = vscode.window.createOutputChannel('Ruby Fast LSP Namespace Tree');
    context.subscriptions.push(outputChannel);
    
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
        }
    };

    client = new LanguageClient(
        'ruby-fast-lsp',
        'Ruby Fast LSP',
        serverOptions,
        clientOptions
    );

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
                                stubsPath: newConfig.get('stubsPath', '')
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

    context.subscriptions.push(treeView, refreshCommand);

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
}

function deactivate() {
    if (!client) {
        return undefined;
    }
    return client.stop();
}

module.exports = { activate, deactivate };
