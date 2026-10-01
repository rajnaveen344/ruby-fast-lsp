// Tree data provider for the Ruby Projects view: projects the server namespace
// tree into project, library, namespace, mixin, and includer tree items.
const vscode = require('vscode');
const { session } = require('../session');
const {
    orderRubyIndexProjects,
    buildWorkspaceProjectForest,
    projectTreePresentation,
    projectBrowseSections,
    namespaceChildDescriptors,
    namespaceHasChildren
} = require('./ruby_index_tree');

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
            || (() => Boolean(session.editorState?.showExternalTypes));
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
        return session.client.sendRequest('ruby/namespaceTree', {
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
        if (!session.client || session.client.state !== 2) {
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
            session.outputChannel.appendLine(`Ruby Fast LSP Index Error: ${error.message}`);
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

module.exports = { RubyIndexProvider };
