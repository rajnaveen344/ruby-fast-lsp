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

// Ruby Index Tree Data Provider
class RubyIndexProvider {
    constructor() {
        this._onDidChangeTreeData = new vscode.EventEmitter();
        this.onDidChangeTreeData = this._onDidChangeTreeData.event;
        // Cache for namespace data (used for search)
        this._cachedNamespaces = [];
        // Map from FQN to TreeItem (for reveal)
        this._fqnToItem = new Map();
    }

    refresh() {
        // Clear cache on refresh
        this._cachedNamespaces = [];
        this._fqnToItem.clear();
        this._onDidChangeTreeData.fire();
    }

    // Flatten all namespaces recursively for search
    _flattenNamespaces(namespaces, result = []) {
        for (const ns of namespaces) {
            result.push(ns);
            if (ns.children && ns.children.length > 0) {
                this._flattenNamespaces(ns.children, result);
            }
        }
        return result;
    }

    // Get all namespaces for search (uses cache)
    getAllNamespaces() {
        return this._cachedNamespaces;
    }

    // Get TreeItem by FQN (for reveal)
    getItemByFqn(fqn) {
        return this._fqnToItem.get(fqn);
    }

    getTreeItem(element) {
        return element;
    }

    // Required for TreeView.reveal() to work with nested items
    getParent(element) {
        if (!element || !element.namespaceData || element.nodeType !== 'namespace') {
            return null;
        }

        const fqn = element.namespaceData.fqn;
        if (!fqn || !fqn.includes('::')) {
            return null; // Root level item
        }

        // Get parent FQN (e.g., "Foo::Bar::Baz" -> "Foo::Bar")
        const parts = fqn.split('::');
        parts.pop();
        const parentFqn = parts.join('::');

        // Return cached parent item if exists
        let parentItem = this._fqnToItem.get(parentFqn);
        if (parentItem) {
            return parentItem;
        }

        // Build parent item from cached namespace data
        const parentNs = this._cachedNamespaces.find(ns => ns.fqn === parentFqn);
        if (parentNs) {
            parentItem = this._buildSingleTreeItem(parentNs);
            this._fqnToItem.set(parentFqn, parentItem);
            return parentItem;
        }

        return null;
    }

    // Build a single tree item from namespace data (used by getParent)
    _buildSingleTreeItem(ns) {
        const hasChildren = ns.children && ns.children.length > 0;
        const hasSuperclass = ns.superclass && ns.superclass.name && !ns.superclass.name.includes('(not found)');
        const hasIncludes = ns.includes && ns.includes.length > 0;
        const hasPrepends = ns.prepends && ns.prepends.length > 0;
        const hasSingletonClass = ns.singleton_class != null;
        const hasIncludedBy = ns.included_by && ns.included_by.length > 0;
        const hasMixins = hasSuperclass || hasIncludes || hasPrepends || hasSingletonClass || hasIncludedBy;
        const hasAnyChildren = hasChildren || hasMixins;

        const item = new vscode.TreeItem(
            ns.name,
            hasAnyChildren ? vscode.TreeItemCollapsibleState.Collapsed : vscode.TreeItemCollapsibleState.None
        );

        const locations = ns.locations || [];
        if (locations.length > 1) {
            item.description = `${ns.kind} (${locations.length} locations)`;
        } else {
            item.description = ns.kind;
        }

        item.namespaceData = ns;
        item.nodeType = 'namespace';

        if (ns.kind === 'Class') {
            item.iconPath = new vscode.ThemeIcon('symbol-class');
        } else if (ns.kind === 'Module') {
            item.iconPath = new vscode.ThemeIcon('symbol-module');
        }

        return item;
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
                const config = vscode.workspace.getConfiguration('rubyFastLsp');
                const showExternalTypes = config.get('showExternalTypes', false);
                const response = await client.sendRequest('ruby/namespaceTree', {
                    uri: vscode.window.activeTextEditor?.document.uri.toString() || '',
                    show_external_types: showExternalTypes
                });

                if (response && response.namespaces) {
                    // Cache flattened namespaces for search
                    this._cachedNamespaces = this._flattenNamespaces(response.namespaces);
                    return this.buildTreeItems(response.namespaces);
                }
            } else if (element.nodeType === 'includedBySection') {
                // Return individual class items (all includers are classes)
                return element.includers.map(inc => this.buildIncluderItem(inc.name, inc.locations || [], inc.via_modules || []));
            } else if (element.nodeType === 'includer') {
                // Return via modules as children (intermediate modules in the include chain)
                // viaModules is array of ViaModuleInfo objects { name, call_location }
                if (element.viaModules && element.viaModules.length > 0) {
                    return element.viaModules.map(viaModule => this.buildViaModuleItem(viaModule));
                }
                return [];
            } else if (element.nodeType === 'mixinSection') {
                // Return individual mixin items
                // mixins are MixinInfo objects (with name and locations)
                const useClassIcon = element.mixinLabel === 'Superclass';
                return element.mixins.map(m => {
                    // Handle MixinInfo objects
                    if (typeof m === 'object' && m.name) {
                        return this.buildMixinItem(m.name, useClassIcon, m.locations || []);
                    } else {
                        return this.buildMixinItem(m, useClassIcon, []);
                    }
                });
            } else if (element.nodeType === 'mixin') {
                // Mixin items have no children
                return [];
            } else if (element.nodeType === 'namespace' && element.namespaceData) {
                // Build children: mixin sections + singleton class + nested namespaces
                const ns = element.namespaceData;
                const children = [];

                // Add superclass section (superclass is now a MixinInfo object with name and location)
                if (ns.superclass && ns.superclass.name && !ns.superclass.name.includes('(not found)')) {
                    children.push(this.buildMixinSectionItem('Superclass', 'arrow-up', [ns.superclass]));
                }

                // Add includes section
                if (ns.includes && ns.includes.length > 0) {
                    children.push(this.buildMixinSectionItem('Includes', 'plug', ns.includes));
                }

                // Add prepends section
                if (ns.prepends && ns.prepends.length > 0) {
                    children.push(this.buildMixinSectionItem('Prepends', 'pinned', ns.prepends));
                }

                // Add singleton class as a child node (contains extends as includes)
                if (ns.singleton_class) {
                    children.push(this.buildSingletonClassItem(ns.singleton_class));
                }

                // Add included_by section for modules (classes that include this module, directly or transitively)
                if (ns.included_by && ns.included_by.length > 0) {
                    children.push(this.buildIncludedBySectionItem('Included By Classes', 'references', ns.included_by));
                }

                // Add nested namespace children
                if (ns.children && ns.children.length > 0) {
                    children.push(...this.buildTreeItems(ns.children));
                }

                return children;
            } else if (element.nodeType === 'singleton' && element.namespaceData) {
                // Build children for singleton class: its includes (which are the extends)
                const ns = element.namespaceData;
                const children = [];

                if (ns.includes && ns.includes.length > 0) {
                    children.push(this.buildMixinSectionItem('Includes', 'plug', ns.includes));
                }

                if (ns.prepends && ns.prepends.length > 0) {
                    children.push(this.buildMixinSectionItem('Prepends', 'pinned', ns.prepends));
                }

                return children;
            }
        } catch (error) {
            outputChannel.appendLine(`Ruby Fast LSP Index Error: ${error.message}`);
        }

        return [];
    }

    buildTreeItems(namespaces) {
        return namespaces.map(ns => {
            const hasChildren = ns.children && ns.children.length > 0;
            // superclass is now a MixinInfo object with name and location fields
            const hasSuperclass = ns.superclass && ns.superclass.name && !ns.superclass.name.includes('(not found)');
            const hasIncludes = ns.includes && ns.includes.length > 0;
            const hasPrepends = ns.prepends && ns.prepends.length > 0;
            const hasSingletonClass = ns.singleton_class != null;
            const hasIncludedBy = ns.included_by && ns.included_by.length > 0;
            const hasMixins = hasSuperclass || hasIncludes || hasPrepends || hasSingletonClass || hasIncludedBy;
            const hasAnyChildren = hasChildren || hasMixins;

            const item = new vscode.TreeItem(
                ns.name,
                hasAnyChildren ? vscode.TreeItemCollapsibleState.Collapsed : vscode.TreeItemCollapsibleState.None
            );

            // Show location count in description if multiple
            const locations = ns.locations || [];
            if (locations.length > 1) {
                item.description = `${ns.kind} (${locations.length} locations)`;
            } else {
                item.description = ns.kind;
            }

            // Store namespace data for building mixin children
            item.namespaceData = ns;
            item.nodeType = 'namespace';

            // Set icon based on kind
            if (ns.kind === 'Class') {
                item.iconPath = new vscode.ThemeIcon('symbol-class');
            } else if (ns.kind === 'Module') {
                item.iconPath = new vscode.ThemeIcon('symbol-module');
            }

            // Add location information for navigation
            if (locations.length === 1) {
                // Single location - open directly
                const loc = locations[0];
                item.command = {
                    command: 'vscode.open',
                    title: 'Open',
                    arguments: [
                        vscode.Uri.parse(loc.uri),
                        {
                            selection: new vscode.Range(
                                loc.line || 0,
                                loc.character || 0,
                                loc.line || 0,
                                loc.character || 0
                            )
                        }
                    ]
                };
            } else if (locations.length > 1) {
                // Multiple locations - show picker
                item.command = {
                    command: 'rubyIndex.showLocations',
                    title: 'Show Locations',
                    arguments: [ns.fqn, locations]
                };
            }

            // Store in FQN map for reveal
            this._fqnToItem.set(ns.fqn, item);

            return item;
        });
    }

    buildSingletonClassItem(singletonClass) {
        const hasIncludes = singletonClass.includes && singletonClass.includes.length > 0;
        const hasPrepends = singletonClass.prepends && singletonClass.prepends.length > 0;
        const hasChildren = hasIncludes || hasPrepends;

        const item = new vscode.TreeItem(
            singletonClass.name,
            hasChildren ? vscode.TreeItemCollapsibleState.Collapsed : vscode.TreeItemCollapsibleState.None
        );

        item.iconPath = new vscode.ThemeIcon('symbol-class');
        item.description = 'Singleton';
        item.nodeType = 'singleton';
        item.namespaceData = singletonClass;

        return item;
    }

    buildMixinSectionItem(label, icon, mixins) {
        const item = new vscode.TreeItem(
            `${label} (${mixins.length})`,
            vscode.TreeItemCollapsibleState.Collapsed
        );
        item.iconPath = new vscode.ThemeIcon(icon);
        item.nodeType = 'mixinSection';
        item.mixins = mixins;
        item.mixinLabel = label;
        return item;
    }

    buildMixinItem(name, useClassIcon = false, locations = []) {
        const item = new vscode.TreeItem(
            name,
            vscode.TreeItemCollapsibleState.None
        );
        item.iconPath = new vscode.ThemeIcon(useClassIcon ? 'symbol-class' : 'symbol-interface');
        item.nodeType = 'mixin';

        // Show location count if multiple
        if (locations && locations.length > 1) {
            item.description = `(${locations.length} locations)`;
        }

        // If we have call site locations, use them for navigation
        // Otherwise fall back to looking up the definition
        if (locations.length === 1) {
            // Single location - open directly
            const loc = locations[0];
            item.command = {
                command: 'vscode.open',
                title: 'Go to Call Site',
                arguments: [
                    vscode.Uri.parse(loc.uri),
                    {
                        selection: new vscode.Range(
                            loc.line || 0,
                            loc.character || 0,
                            loc.line || 0,
                            loc.character || 0
                        )
                    }
                ]
            };
        } else if (locations.length > 1) {
            // Multiple locations - use custom command to show picker
            item.command = {
                command: 'rubyIndex.showLocations',
                title: 'Show Locations',
                arguments: [name, locations]
            };
        } else {
            // Fall back to definition lookup (for items without call site location)
            item.command = {
                command: 'rubyIndex.gotoDefinition',
                title: 'Go to Definition',
                arguments: [name]
            };
        }
        return item;
    }

    buildIncludedBySectionItem(label, icon, includers) {
        const item = new vscode.TreeItem(
            `${label} (${includers.length})`,
            vscode.TreeItemCollapsibleState.Collapsed
        );
        item.iconPath = new vscode.ThemeIcon(icon);
        item.nodeType = 'includedBySection';
        item.includers = includers;
        return item;
    }

    buildIncluderItem(name, locations = [], viaModules = []) {
        // Collapsible if there are intermediate modules in the include chain
        const hasViaModules = viaModules && viaModules.length > 0;
        const item = new vscode.TreeItem(
            name,
            hasViaModules ? vscode.TreeItemCollapsibleState.Collapsed : vscode.TreeItemCollapsibleState.None
        );
        // All includers are classes (we traverse through modules to find classes)
        item.iconPath = new vscode.ThemeIcon('symbol-class');

        // Show description: via module count and/or location count
        const descriptions = [];
        if (hasViaModules) {
            descriptions.push(`via ${viaModules.length} module${viaModules.length > 1 ? 's' : ''}`);
        }
        if (locations && locations.length > 1) {
            descriptions.push(`${locations.length} locations`);
        }
        if (descriptions.length > 0) {
            item.description = `(${descriptions.join(', ')})`;
        }

        item.nodeType = 'includer';
        item.viaModules = viaModules;

        // Navigate to definition using locations
        if (locations && locations.length === 1) {
            const loc = locations[0];
            item.command = {
                command: 'vscode.open',
                title: 'Go to Definition',
                arguments: [
                    vscode.Uri.parse(loc.uri),
                    {
                        selection: new vscode.Range(
                            loc.line || 0,
                            loc.character || 0,
                            loc.line || 0,
                            loc.character || 0
                        )
                    }
                ]
            };
        } else if (locations && locations.length > 1) {
            item.command = {
                command: 'rubyIndex.showLocations',
                title: 'Show Locations',
                arguments: [name, locations]
            };
        } else {
            // Fall back to lookup
            item.command = {
                command: 'rubyIndex.gotoDefinition',
                title: 'Go to Definition',
                arguments: [name]
            };
        }
        return item;
    }

    buildViaModuleItem(viaModuleInfo) {
        // viaModuleInfo is { name: string, call_location?: LocationInfo }
        const moduleName = typeof viaModuleInfo === 'string' ? viaModuleInfo : viaModuleInfo.name;
        const callLocation = typeof viaModuleInfo === 'object' ? viaModuleInfo.call_location : null;

        const item = new vscode.TreeItem(
            moduleName,
            vscode.TreeItemCollapsibleState.None
        );
        item.iconPath = new vscode.ThemeIcon('symbol-module');
        item.description = 'via';
        item.nodeType = 'viaModule';

        // Navigate to the include/prepend call site if available, otherwise fall back to module definition
        if (callLocation) {
            item.command = {
                command: 'vscode.open',
                title: 'Go to Include Call',
                arguments: [
                    vscode.Uri.parse(callLocation.uri),
                    {
                        selection: new vscode.Range(
                            callLocation.line || 0,
                            callLocation.character || 0,
                            callLocation.line || 0,
                            callLocation.character || 0
                        )
                    }
                ]
            };
        } else {
            // Fall back to module definition
            item.command = {
                command: 'rubyIndex.gotoDefinition',
                title: 'Go to Definition',
                arguments: [moduleName]
            };
        }
        return item;
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

                // Refresh tree if showExternalTypes changed
                if (event.affectsConfiguration('rubyFastLsp.showExternalTypes')) {
                    indexProvider.refresh();
                }
            }
        })
    );

    // Register Ruby Index Tree
    const indexProvider = new RubyIndexProvider();
    const treeView = vscode.window.createTreeView('rubyIndex', {
        treeDataProvider: indexProvider,
        showCollapseAll: true
    });

    // Register refresh command
    const refreshCommand = vscode.commands.registerCommand('rubyIndex.refresh', () => {
        indexProvider.refresh();
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

    // Register search command to find and reveal namespaces in tree
    const searchCommand = vscode.commands.registerCommand('rubyIndex.search', async () => {
        if (!client || client.state !== 2) {
            vscode.window.showWarningMessage('Ruby Fast LSP is not ready yet. Please wait for indexing to complete.');
            return;
        }

        // Get all namespaces from cache
        let namespaces = indexProvider.getAllNamespaces();

        // If cache is empty, trigger a refresh and wait for data
        if (namespaces.length === 0) {
            // Fetch fresh data
            try {
                const config = vscode.workspace.getConfiguration('rubyFastLsp');
                const showExternalTypes = config.get('showExternalTypes', false);
                const response = await client.sendRequest('ruby/namespaceTree', {
                    uri: vscode.window.activeTextEditor?.document.uri.toString() || '',
                    show_external_types: showExternalTypes
                });
                if (response && response.namespaces) {
                    namespaces = indexProvider._flattenNamespaces(response.namespaces);
                }
            } catch (error) {
                vscode.window.showErrorMessage(`Failed to fetch namespaces: ${error.message}`);
                return;
            }
        }

        if (namespaces.length === 0) {
            vscode.window.showInformationMessage('No namespaces found in the Ruby Index.');
            return;
        }

        // Build QuickPick items from namespaces
        const items = namespaces.map(ns => {
            const icon = ns.kind === 'Class' ? '$(symbol-class)' : '$(symbol-module)';
            return {
                label: `${icon} ${ns.name}`,
                description: ns.fqn !== ns.name ? ns.fqn : '',
                detail: ns.kind,
                fqn: ns.fqn,
                namespaceData: ns
            };
        });

        const selected = await vscode.window.showQuickPick(items, {
            placeHolder: 'Search for a class or module...',
            matchOnDescription: true,  // Match on FQN
            matchOnDetail: false
        });

        if (selected) {
            // Try to reveal the item in the tree
            // First, ensure the tree has built the item
            let item = indexProvider.getItemByFqn(selected.fqn);

            if (!item) {
                // Item not in cache yet (tree not expanded), build it
                item = indexProvider._buildSingleTreeItem(selected.namespaceData);
                indexProvider._fqnToItem.set(selected.fqn, item);
            }

            // Reveal the item in the tree (expand parents if needed)
            try {
                await treeView.reveal(item, { select: true, focus: true, expand: true });
            } catch (error) {
                outputChannel.appendLine(`[Ruby Index] Failed to reveal item: ${error.message}`);
                // Fallback: just show a message
                vscode.window.showInformationMessage(`Found: ${selected.fqn}`);
            }
        }
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

    // Register toggle external types command
    const toggleExternalTypesCommand = vscode.commands.registerCommand('rubyIndex.toggleExternalTypes', async () => {
        const config = vscode.workspace.getConfiguration('rubyFastLsp');
        const currentValue = config.get('showExternalTypes', false);
        await config.update('showExternalTypes', !currentValue, vscode.ConfigurationTarget.Workspace);
        const newValue = !currentValue;
        vscode.window.showInformationMessage(
            newValue
                ? 'Ruby Index: Now showing external types (core, stdlib, gems)'
                : 'Ruby Index: Now showing only project types'
        );
        indexProvider.refresh();
    });

    context.subscriptions.push(treeView, refreshCommand, exportCommand, gotoDefinitionCommand, showLocationsCommand, showReferencesCommand, searchCommand, toggleExternalTypesCommand);

    // Start the client and initialize index tree when ready
    client.start().then(() => {
        // Auto-refresh index tree when client is ready
        setTimeout(() => {
            indexProvider.refresh();
        }, 1000); // Small delay to ensure everything is settled
    }).catch(error => {
        outputChannel.appendLine(`[Ruby Index] LSP client failed to start: ${error}`);
    });

    // Auto-refresh when active editor changes
    context.subscriptions.push(
        vscode.window.onDidChangeActiveTextEditor(() => {
            if (vscode.window.activeTextEditor?.document.languageId === 'ruby') {
                indexProvider.refresh();
            }
        })
    );

    // Auto-refresh index tree when Ruby files are saved or changed
    context.subscriptions.push(
        vscode.workspace.onDidSaveTextDocument((document) => {
            if (document.languageId === 'ruby') {
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
            if (event.document.languageId === 'ruby') {
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
            if (document.languageId === 'ruby') {
                setTimeout(() => {
                    indexProvider.refresh();
                }, 500);
            }
        })
    );

    context.subscriptions.push(
        vscode.workspace.onDidCloseTextDocument((document) => {
            if (document.languageId === 'ruby') {
                setTimeout(() => {
                    indexProvider.refresh();
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
