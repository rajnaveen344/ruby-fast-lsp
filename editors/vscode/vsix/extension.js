const vscode = require('vscode');
const path = require('path');
const fs = require('fs');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');
const { registerErbHtmlProviders } = require('./erb_html');
const { fileWatcherPatterns } = require('./ruby_file_kinds');
const {
    runtimeStatusForDocument,
    runtimeStatusItem,
    runtimeStatusPresentation,
    runtimeVersionMarker,
    selectRuntime
} = require('./runtime_selector');
const {
    STATE_KEYS,
    pathsForProject,
    readEditorState,
    serverConfiguration,
    updateLoadPaths,
    updateRuntime,
    validLoadPaths
} = require('./configuration_state');
const {
    debugConfiguration,
    minitestInvocation,
    railsViewRelativePaths,
    rspecInvocation
} = require('./test_commands');
const {
    createIndexingStatusSession,
    indexingStatusBarCommand,
    indexingStatusQuickPickItems,
    indexingStatusQuickPickPlaceholder,
    indexingStatusRequestParams
} = require('./indexing_status');
const {
    orderRubyIndexProjects,
    buildWorkspaceProjectForest,
    projectTreePresentation,
    namespaceSearchQuickPickItems,
    projectBrowseSections,
    namespaceChildDescriptors,
    namespaceHasChildren,
    findOwningWorkspaceFolder
} = require('./ruby_index_tree');

// Create output channel for logging
let outputChannel;

let client;
let editorState;

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
    constructor(options = {}) {
        this._onDidChangeTreeData = new vscode.EventEmitter();
        this.onDidChangeTreeData = this._onDidChangeTreeData.event;
        this._cachedNamespaces = [];
        this._fqnToItem = new Map();
        this._getIndexingProjects = options.getIndexingProjects || (() => []);
        this._getRuntimeProjects = options.getRuntimeProjects || (() => []);
        this._getShowExternalTypes = options.getShowExternalTypes
            || (() => Boolean(editorState?.showExternalTypes));
    }

    refresh() {
        this._cachedNamespaces = [];
        this._fqnToItem.clear();
        this._onDidChangeTreeData.fire();
    }

    _flattenNamespaces(namespaces, result = []) {
        for (const ns of namespaces) {
            result.push(ns);
            if (ns.modules && ns.modules.length > 0) {
                this._flattenNamespaces(ns.modules, result);
            }
            if (ns.classes && ns.classes.length > 0) {
                this._flattenNamespaces(ns.classes, result);
            }
        }
        return result;
    }

    getAllNamespaces() {
        return this._cachedNamespaces;
    }

    getItemByFqn(fqn) {
        return this._fqnToItem.get(fqn);
    }

    getTreeItem(element) {
        return element;
    }

    _resolveProjects() {
        const indexingProjects = this._getIndexingProjects();
        if (Array.isArray(indexingProjects) && indexingProjects.length > 0) {
            return indexingProjects;
        }
        const runtimeProjects = this._getRuntimeProjects();
        if (Array.isArray(runtimeProjects) && runtimeProjects.length > 0) {
            return runtimeProjects;
        }
        const folders = vscode.workspace.workspaceFolders || [];
        return folders.map((folder) => ({ root: folder.uri.fsPath }));
    }

    _projectRequestUri(rootPath) {
        const normalized = rootPath.replace(/[\\/]+$/, '');
        return vscode.Uri.file(normalized).toString();
    }

    async _fetchNamespaceTree(uri) {
        return client.sendRequest('ruby/namespaceTree', {
            uri: uri || '',
            show_external_types: this._getShowExternalTypes()
        });
    }

    getParent(element) {
        if (!element) {
            return null;
        }
        if (element.nodeType === 'workspace' || element.nodeType === 'pathFolder') {
            return element.parentItem || null;
        }
        if (element.nodeType === 'project') {
            return element.parentItem || null;
        }
        if (element.nodeType === 'librarySection') {
            return element.projectItem || null;
        }
        if (element.nodeType === 'libraryPackage') {
            return element.libraryItem || element.projectItem || null;
        }
        if (element.nodeType === 'mixinSection'
            || element.nodeType === 'includedBySection'
            || element.nodeType === 'singleton'
            || element.nodeType === 'mixin'
            || element.nodeType === 'includer') {
            return element.parentItem || null;
        }
        if (!element.namespaceData || element.nodeType !== 'namespace') {
            return null;
        }

        const fqn = element.namespaceData.fqn;
        if (!fqn || !fqn.includes('::')) {
            return element.packageItem || element.libraryItem || element.projectItem || null;
        }

        const parts = fqn.split('::');
        parts.pop();
        const parentFqn = parts.join('::');
        let parentItem = this._fqnToItem.get(parentFqn);
        if (parentItem) {
            return parentItem;
        }

        const parentNs = this._cachedNamespaces.find(ns => ns.fqn === parentFqn);
        if (parentNs) {
            parentItem = this._buildSingleTreeItem(
                parentNs,
                element.projectItem,
                element.libraryItem,
                element.packageItem
            );
            this._fqnToItem.set(parentFqn, parentItem);
            return parentItem;
        }

        return element.packageItem || element.libraryItem || element.projectItem || null;
    }

    _buildSingleTreeItem(ns, projectItem, libraryItem = null, packageItem = null) {
        const item = new vscode.TreeItem(
            ns.name,
            namespaceHasChildren(ns)
                ? vscode.TreeItemCollapsibleState.Collapsed
                : vscode.TreeItemCollapsibleState.None
        );
        const locations = ns.locations || [];
        item.description = locations.length > 1
            ? `${ns.kind} (${locations.length} locations)`
            : ns.kind;
        item.namespaceData = ns;
        item.nodeType = 'namespace';
        item.projectItem = projectItem;
        item.libraryItem = libraryItem;
        item.packageItem = packageItem;
        if (ns.kind === 'Class') {
            item.iconPath = new vscode.ThemeIcon('symbol-class');
        } else if (ns.kind === 'Module') {
            item.iconPath = new vscode.ThemeIcon('symbol-module');
        }
        return item;
    }

    _buildForestItems(nodes, parentItem) {
        const activePath = vscode.window.activeTextEditor?.document?.uri?.fsPath;
        return (nodes || []).map((node) => {
            if (node.kind === 'project') {
                const presentation = projectTreePresentation(node.project, activePath);
                const item = new vscode.TreeItem(
                    node.label,
                    vscode.TreeItemCollapsibleState.Collapsed
                );
                item.nodeType = 'project';
                item.projectRoot = node.project.root;
                item.description = presentation.description;
                item.iconPath = new vscode.ThemeIcon(presentation.iconId);
                item.tooltip = presentation.tooltip;
                item.contextValue = presentation.active
                    ? 'rubyIndexProjectActive'
                    : 'rubyIndexProject';
                item.parentItem = parentItem;
                return item;
            }

            const item = new vscode.TreeItem(
                node.label,
                vscode.TreeItemCollapsibleState.Collapsed
            );
            item.nodeType = node.kind;
            item.folderPath = node.path;
            item.forestChildren = node.children || [];
            item.description = node.kind === 'workspace' ? 'workspace' : 'folder';
            item.iconPath = new vscode.ThemeIcon(
                node.kind === 'workspace' ? 'root-folder' : 'folder'
            );
            item.tooltip = node.path;
            item.contextValue = node.kind === 'workspace'
                ? 'rubyIndexWorkspace'
                : 'rubyIndexPathFolder';
            item.parentItem = parentItem;
            return item;
        });
    }

    _buildLibrarySectionItems(sections, projectItem) {
        return (sections || []).map((section) => {
            const item = new vscode.TreeItem(
                section.label,
                vscode.TreeItemCollapsibleState.Collapsed
            );
            item.nodeType = 'librarySection';
            item.librarySectionId = section.id;
            item.projectItem = projectItem;
            item.libraryNamespaces = section.namespaces;
            item.libraryPackages = section.packages || [];
            item.iconPath = new vscode.ThemeIcon(section.icon);
            item.description = section.description;
            item.contextValue = 'rubyIndexLibrarySection';
            return item;
        });
    }

    _buildLibraryPackageItems(packages, projectItem, libraryItem) {
        return (packages || []).map((packageInfo) => {
            const item = new vscode.TreeItem(
                packageInfo.label,
                vscode.TreeItemCollapsibleState.Collapsed
            );
            item.nodeType = 'libraryPackage';
            item.projectItem = projectItem;
            item.libraryItem = libraryItem;
            item.libraryNamespaces = packageInfo.namespaces;
            item.iconPath = new vscode.ThemeIcon('package');
            item.description = 'gem';
            item.tooltip = `${packageInfo.name} ${packageInfo.version}`.trim();
            item.contextValue = 'rubyIndexLibraryPackage';
            return item;
        });
    }

    async getChildren(element) {
        if (!client || client.state !== 2) {
            return [];
        }

        try {
            if (!element) {
                const projects = orderRubyIndexProjects(this._resolveProjects());
                if (projects.length === 0) {
                    const response = await this._fetchNamespaceTree(
                        vscode.window.activeTextEditor?.document.uri.toString() || ''
                    );
                    const sections = projectBrowseSections(
                        response,
                        this._getShowExternalTypes()
                    );
                    this._cachedNamespaces = this._flattenNamespaces([
                        ...sections.projectNamespaces,
                        ...sections.libraryNamespaces
                    ]);
                    // Libraries first (JRE / Maven analogues), then project types.
                    return [
                        ...this._buildLibrarySectionItems(sections.librarySections, null),
                        ...this.buildTreeItems(sections.projectNamespaces, null)
                    ];
                }

                const workspaceFolders = (vscode.workspace.workspaceFolders || [])
                    .map((folder) => folder.uri.fsPath);
                const forest = buildWorkspaceProjectForest(projects, workspaceFolders);
                return this._buildForestItems(forest, null);
            }

            if (element.nodeType === 'workspace' || element.nodeType === 'pathFolder') {
                return this._buildForestItems(element.forestChildren || [], element);
            }

            if (element.nodeType === 'project') {
                const response = await this._fetchNamespaceTree(
                    this._projectRequestUri(element.projectRoot)
                );
                const sections = projectBrowseSections(
                    response,
                    this._getShowExternalTypes()
                );
                const flattened = this._flattenNamespaces([
                    ...sections.projectNamespaces,
                    ...sections.libraryNamespaces
                ]);
                const byFqn = new Map(this._cachedNamespaces.map((ns) => [ns.fqn, ns]));
                for (const ns of flattened) {
                    byFqn.set(ns.fqn, ns);
                }
                this._cachedNamespaces = [...byFqn.values()].map((ns) => ({
                    ...ns,
                    projectRoot: ns.projectRoot || element.projectRoot
                }));

                // Libraries first under each project, matching Java Projects JRE/Maven.
                return [
                    ...this._buildLibrarySectionItems(sections.librarySections, element),
                    ...this.buildTreeItems(sections.projectNamespaces, element)
                ];
            }

            if (element.nodeType === 'librarySection') {
                return [
                    ...this._buildLibraryPackageItems(
                        element.libraryPackages || [],
                        element.projectItem,
                        element
                    ),
                    ...this.buildTreeItems(
                        element.libraryNamespaces || [],
                        element.projectItem,
                        element
                    )
                ];
            }

            if (element.nodeType === 'libraryPackage') {
                return this.buildTreeItems(
                    element.libraryNamespaces || [],
                    element.projectItem,
                    element.libraryItem,
                    element
                );
            }

            if (element.nodeType === 'includedBySection') {
                return element.includers.map((inc) => {
                    const item = this.buildIncluderItem(
                        inc.name,
                        inc.locations || [],
                        inc.via_modules || []
                    );
                    item.parentItem = element;
                    return item;
                });
            }
            if (element.nodeType === 'includer') {
                if (element.viaModules && element.viaModules.length > 0) {
                    return element.viaModules.map((viaModule) => {
                        const item = this.buildViaModuleItem(viaModule);
                        item.parentItem = element;
                        return item;
                    });
                }
                return [];
            }
            if (element.nodeType === 'mixinSection') {
                const useClassIcon = element.mixinLabel === 'Superclass';
                return element.mixins.map((m) => {
                    let item;
                    if (typeof m === 'object' && m.name) {
                        item = this.buildMixinItem(m.name, useClassIcon, m.locations || []);
                    } else {
                        item = this.buildMixinItem(m, useClassIcon, []);
                    }
                    item.parentItem = element;
                    return item;
                });
            }
            if (element.nodeType === 'mixin') {
                return [];
            }
            if (element.nodeType === 'namespace' && element.namespaceData) {
                return this.buildNamespaceChildren(element);
            }
            if (element.nodeType === 'singleton' && element.namespaceData) {
                const ns = element.namespaceData;
                const children = [];
                if (ns.includes && ns.includes.length > 0) {
                    const section = this.buildMixinSectionItem('Includes', 'plug', ns.includes);
                    section.parentItem = element;
                    children.push(section);
                }
                if (ns.prepends && ns.prepends.length > 0) {
                    const section = this.buildMixinSectionItem('Prepends', 'pinned', ns.prepends);
                    section.parentItem = element;
                    children.push(section);
                }
                return children;
            }
        } catch (error) {
            outputChannel.appendLine(`Ruby Fast LSP Index Error: ${error.message}`);
        }

        return [];
    }

    buildNamespaceChildren(element) {
        const children = [];
        for (const descriptor of namespaceChildDescriptors(element.namespaceData)) {
            if (descriptor.kind === 'namespace') {
                children.push(...this.buildTreeItems(
                    [descriptor.namespace],
                    element.projectItem,
                    element.libraryItem,
                    element.packageItem
                ));
            } else if (descriptor.kind === 'mixinSection') {
                const section = this.buildMixinSectionItem(
                    descriptor.label,
                    descriptor.icon,
                    descriptor.mixins
                );
                section.parentItem = element;
                children.push(section);
            } else if (descriptor.kind === 'singleton') {
                const singleton = this.buildSingletonClassItem(descriptor.namespace);
                singleton.parentItem = element;
                children.push(singleton);
            } else if (descriptor.kind === 'includedBySection') {
                const section = this.buildIncludedBySectionItem(
                    descriptor.label,
                    descriptor.icon,
                    descriptor.includers
                );
                section.parentItem = element;
                children.push(section);
            }
        }
        return children;
    }

    buildTreeItems(namespaces, projectItem = null, libraryItem = null, packageItem = null) {
        return namespaces.map(ns => {
            const item = new vscode.TreeItem(
                ns.name,
                namespaceHasChildren(ns)
                    ? vscode.TreeItemCollapsibleState.Collapsed
                    : vscode.TreeItemCollapsibleState.None
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
            item.projectItem = projectItem;
            item.libraryItem = libraryItem;
            item.packageItem = packageItem;

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

    // Map platform.arch to the correct binary path.
    // Release CI publishes VSIX binaries under VS Code target platform names
    // (darwin-arm64/darwin-x64). Older local packages used macos-* names.
    const platformMap = {
        'darwin': {
            'x64': ['darwin-x64', 'macos-x64'],
            'arm64': ['darwin-arm64', 'macos-arm64']
        },
        'linux': {
            'x64': ['linux-x64'],
            'arm64': ['linux-arm64']
        },
        'win32': {
            'x64': ['win32-x64'],
            'arm64': ['win32-arm64']
        }
    };

    const platformInfo = platformMap[platform];
    if (!platformInfo) {
        throw new Error(`Unsupported platform: ${platform}`);
    }

    const platformDirs = platformInfo[arch];
    if (!platformDirs) {
        throw new Error(`Unsupported architecture ${arch} for platform ${platform}`);
    }

    const candidatePaths = platformDirs.map(platformDir => path.join(__dirname, 'bin', platformDir, binaryName));
    const serverPath = candidatePaths.find(candidatePath => fs.existsSync(candidatePath));
    if (!serverPath) {
        throw new Error(`Ruby Fast LSP binary not found. Tried: ${candidatePaths.join(', ')}`);
    }

    if (!isWindows) {
        fs.chmodSync(serverPath, 0o755);
    }

    return serverPath;
}

function getBundledExtensionPackages(extensionPath) {
    const packages = [];
    for (const packageName of ['rspec-ruby', 'rails-ruby', 'minitest-ruby', 'sinatra-rust', 'cucumber-rust']) {
        const extensionPackage = path.join(extensionPath, 'extensions', packageName);
        if (fs.existsSync(path.join(extensionPackage, 'extension.toml'))) {
            packages.push(extensionPackage);
        }
    }
    return packages;
}

function testWorkingDirectory(uriString) {
    const uri = vscode.Uri.parse(uriString);
    return vscode.workspace.getWorkspaceFolder(uri)?.uri.fsPath || path.dirname(uri.fsPath);
}

function runTestInTerminal(name, invocation, cwd) {
    const execution = new vscode.ProcessExecution(
        invocation.argv[0],
        invocation.argv.slice(1),
        { cwd }
    );
    const task = new vscode.Task(
        { type: 'ruby-fast-lsp-test', target: name },
        vscode.TaskScope.Workspace,
        name,
        'Ruby Fast LSP',
        execution
    );
    task.presentationOptions = {
        reveal: vscode.TaskRevealKind.Always,
        panel: vscode.TaskPanelKind.Dedicated,
        clear: true
    };
    return vscode.tasks.executeTask(task);
}

function debugTest(name, invocation, cwd) {
    return vscode.debug.startDebugging(undefined, debugConfiguration(name, invocation, cwd));
}

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

function activate(context) {
    // Create single output channel for both extension and LSP server logs
    outputChannel = vscode.window.createOutputChannel('Ruby Fast LSP');
    context.subscriptions.push(outputChannel);
    registerErbHtmlProviders(vscode, context, (message) => {
        outputChannel.appendLine(`[Ruby Fast LSP] ${message}`);
    });

    // Extract zipped stubs to the extension folder on first run
    // This ensures go-to-definition shows proper file paths
    extractZippedStubs(context.extensionPath);

    const config = vscode.workspace.getConfiguration('rubyFastLsp');
    editorState = readEditorState(context.workspaceState, config);
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

    client = new LanguageClient(
        'ruby-fast-lsp',
        'Ruby Fast LSP',
        serverOptions,
        clientOptions
    );

    const runtimeStatusBarItem = vscode.window.createStatusBarItem(
        vscode.StatusBarAlignment.Right,
        100
    );
    runtimeStatusBarItem.command = 'ruby-fast-lsp.runtime.configure';
    context.subscriptions.push(runtimeStatusBarItem);
    let activeRuntimeProjectRoot;
    let activeRuntimeStatus;
    let runtimeStatusRefreshGeneration = 0;
    const indexingStatusSession = createIndexingStatusSession();
    let runtimeProjects;
    let clientRestartPromise;
    const acceptIndexingSnapshot = snapshot => indexingStatusSession.accept(snapshot);
    const renderActiveRuntimeStatus = () => {
        if (!activeRuntimeStatus?.root) {
            return;
        }
        const indexingStatus = indexingStatusSession.snapshot();
        const indexing = indexingStatus.projects.find(
            project => project.root === activeRuntimeStatus.root
        );
        if (indexing) {
            activeRuntimeStatus = { ...activeRuntimeStatus, indexing };
        }
        const presentation = runtimeStatusPresentation(activeRuntimeStatus);
        runtimeStatusBarItem.text = presentation.text;
        runtimeStatusBarItem.command = indexingStatusBarCommand(
            activeRuntimeStatus.indexing
        );
        runtimeStatusBarItem.tooltip = indexingStatus.aggregate
            ? `${presentation.tooltip}\n\nWorkspace: ${indexingStatus.aggregate.ready} ready, ${indexingStatus.aggregate.active} active, ${indexingStatus.aggregate.queued} queued, ${indexingStatus.aggregate.failed} failed`
            : presentation.tooltip;
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
            runtimeStatusBarItem.text = '$(ruby) No Ruby project';
            runtimeStatusBarItem.tooltip = 'The active document is not owned by a discovered Ruby project';
            runtimeStatusBarItem.command = 'ruby-fast-lsp.runtime.configure';
            return true;
        }
        renderActiveRuntimeStatus();
        return true;
    };
    const refreshRuntimeStatusBar = async () => {
        const generation = ++runtimeStatusRefreshGeneration;
        const editor = vscode.window.activeTextEditor;
        if (!editor || !['ruby', 'erb'].includes(editor.document.languageId)) {
            activeRuntimeProjectRoot = undefined;
            activeRuntimeStatus = undefined;
            runtimeStatusBarItem.hide();
            return;
        }
        if (!renderCachedRuntimeStatus(editor)) {
            runtimeStatusBarItem.text = '$(sync~spin) Ruby Fast LSP';
            runtimeStatusBarItem.tooltip = 'Ruby Fast LSP is determining the owning project';
        }
        runtimeStatusBarItem.show();
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
            renderCachedRuntimeStatus(editor);
        } catch (error) {
            if (generation !== runtimeStatusRefreshGeneration) {
                return;
            }
            activeRuntimeProjectRoot = undefined;
            activeRuntimeStatus = undefined;
            runtimeStatusBarItem.text = '$(warning) Runtime unavailable';
            runtimeStatusBarItem.tooltip = `Ruby Fast LSP runtime status failed: ${error.message}`;
            runtimeStatusBarItem.command = 'ruby-fast-lsp.runtime.configure';
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
        runtimeStatusBarItem.text = '$(sync~spin) Ruby Fast LSP';
        runtimeStatusBarItem.tooltip = 'Ruby Fast LSP is restarting';
        runtimeStatusBarItem.show();
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
            const editor = vscode.window.activeTextEditor;
            if (editor && ['ruby', 'erb'].includes(editor.document.languageId)) {
                renderCachedRuntimeStatus(editor);
            }
            if (typeof scheduleRubyProjectsRefresh === 'function') {
                scheduleRubyProjectsRefresh();
            }
        }
    );
    context.subscriptions.push(
        indexingStatusNotification,
        { dispose: () => indexingStatusSession.dispose() }
    );

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

    // Register Ruby Index Tree
    const indexProvider = new RubyIndexProvider({
        getIndexingProjects: () => indexingStatusSession.snapshot().projects,
        getRuntimeProjects: () => runtimeProjects || [],
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

    const runRspecCommand = vscode.commands.registerCommand('ruby-fast-lsp.rspec.run',
        (uriStr, _line, target) => {
            try {
                runTestInTerminal('RSpec', rspecInvocation(target), testWorkingDirectory(uriStr));
            } catch (error) {
                vscode.window.showErrorMessage(`Unable to run RSpec: ${error.message}`);
            }
        }
    );

    const debugRspecCommand = vscode.commands.registerCommand('ruby-fast-lsp.rspec.debug',
        (uriStr, _line, target) => {
            try {
                return debugTest('Debug RSpec', rspecInvocation(target), testWorkingDirectory(uriStr));
            } catch (error) {
                vscode.window.showErrorMessage(`Unable to debug RSpec: ${error.message}`);
                return undefined;
            }
        }
    );

    const minitestTarget = (uriStr, line, testName) => {
        const cwd = testWorkingDirectory(uriStr);
        const rails = path.join(cwd, 'bin', process.platform === 'win32' ? 'rails.bat' : 'rails');
        return {
            cwd,
            invocation: minitestInvocation(uriStr, line, testName, fs.existsSync(rails) ? rails : null)
        };
    };

    const runMinitestCommand = vscode.commands.registerCommand('ruby-fast-lsp.minitest.run',
        (uriStr, line, testName) => {
            try {
                const target = minitestTarget(uriStr, line, testName);
                runTestInTerminal('Minitest', target.invocation, target.cwd);
            } catch (error) {
                vscode.window.showErrorMessage(`Unable to run Minitest: ${error.message}`);
            }
        }
    );

    const debugMinitestCommand = vscode.commands.registerCommand('ruby-fast-lsp.minitest.debug',
        (uriStr, line, testName) => {
            try {
                const target = minitestTarget(uriStr, line, testName);
                return debugTest('Debug Minitest', target.invocation, target.cwd);
            } catch (error) {
                vscode.window.showErrorMessage(`Unable to debug Minitest: ${error.message}`);
                return undefined;
            }
        }
    );

    const openRailsViewCommand = vscode.commands.registerCommand('ruby-fast-lsp.rails.openView',
        async (controllerUriString, controller, action) => {
            try {
                const controllerUri = vscode.Uri.parse(controllerUriString);
                const workspace = vscode.workspace.getWorkspaceFolder(controllerUri);
                if (!workspace) {
                    throw new Error('the controller is not inside an open workspace');
                }
                for (const relative of railsViewRelativePaths(controller, action)) {
                    const candidate = vscode.Uri.file(path.join(workspace.uri.fsPath, relative));
                    if (fs.existsSync(candidate.fsPath)) {
                        const document = await vscode.workspace.openTextDocument(candidate);
                        return vscode.window.showTextDocument(document);
                    }
                }
                vscode.window.showInformationMessage(
                    `No view found for ${controller}#${action}`
                );
                return undefined;
            } catch (error) {
                vscode.window.showErrorMessage(`Unable to open Rails view: ${error.message}`);
                return undefined;
            }
        }
    );

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

    const selectRuntimeCommand = vscode.commands.registerCommand(
        'ruby-fast-lsp.runtime.select',
        async () => selectRuntime({
            window: vscode.window,
            client,
            preferredProjectRoot: activeRuntimeProjectRoot,
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
            runtimeProjects = Array.isArray(runtimeResponse?.projects)
                ? runtimeResponse.projects
                : [];
            const snapshot = indexingStatusSession.snapshot();
            const items = indexingStatusQuickPickItems(
                snapshot,
                activeRuntimeProjectRoot,
                runtimeProjects
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
            const project = activeRuntimeStatus?.root
                ? path.basename(activeRuntimeStatus.root)
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
            if (runtimeVersionMarker(activeRuntimeStatus || {})) {
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
                await saveRuntimeMarker(activeRuntimeStatus);
            } else if (action.id === 'auto') {
                if (!activeRuntimeProjectRoot) {
                    await vscode.window.showErrorMessage(
                        'The active document is not owned by a discovered Ruby project.'
                    );
                    return;
                }
                await applyAutoRuntime(activeRuntimeProjectRoot);
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
            if (!activeRuntimeProjectRoot) {
                await vscode.window.showErrorMessage(
                    'The active document is not owned by a discovered Ruby project.'
                );
                return;
            }
            const projectLabel = path.basename(activeRuntimeProjectRoot);
            const currentPaths = pathsForProject(editorState.loadPaths, activeRuntimeProjectRoot);
            const input = await vscode.window.showInputBox({
                title: `Require Load Paths — ${projectLabel}`,
                prompt: `Editing ${activeRuntimeProjectRoot} · comma-separated project-relative dirs (restart required)`,
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
                projectRoot: activeRuntimeProjectRoot,
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

    context.subscriptions.push(treeView, refreshCommand, exportCommand, gotoDefinitionCommand, showLocationsCommand, showReferencesCommand, runRspecCommand, debugRspecCommand, runMinitestCommand, debugMinitestCommand, openRailsViewCommand, searchCommand, toggleExternalTypesCommand, selectRuntimeCommand, runtimeStatusCommand, indexingStatusCommand, configureRuntimeCommand, selectLinterCommand, selectFormatterCommand, configureLoadPathsCommand);

    // Start the client and initialize index tree when ready
    client.start().then(() => {
        // Auto-refresh index tree when client is ready
        setTimeout(() => {
            indexProvider.refresh();
            void refreshRuntimeStatusBar();
        }, 1000); // Small delay to ensure everything is settled
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
    if (!client) {
        return undefined;
    }
    return client.stop();
}

module.exports = { activate, deactivate };
