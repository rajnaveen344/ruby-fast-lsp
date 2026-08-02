'use strict';

/**
 * Pure helpers for the hierarchical Ruby Projects tree (Java Projects–like).
 * Keep presentation policy here so Node tests can assert shape without LSP.
 */

const LIBRARY_SECTION_PRESENTATION = Object.freeze({
    runtime: Object.freeze({
        label: 'Ruby Standard Library',
        description: 'core / stdlib / runtime',
        icon: 'library'
    }),
    gems: Object.freeze({
        label: 'Gems',
        description: 'Bundler / RubyGems',
        icon: 'package'
    }),
    excluded: Object.freeze({
        label: 'Excluded',
        description: 'non-project workspace sources',
        icon: 'folder-library'
    })
});

function projectRootLabel(rootPath) {
    if (typeof rootPath !== 'string' || rootPath.length === 0) {
        throw new Error('project root path must be a non-empty string');
    }
    const normalized = rootPath.replace(/[\\/]+$/, '');
    const parts = normalized.split(/[\\/]/).filter(Boolean);
    return parts.length === 0 ? normalized : parts[parts.length - 1];
}

function normalizeRootPath(rootPath) {
    return String(rootPath || '')
        .replace(/\\/g, '/')
        .replace(/\/+$/, '');
}

/**
 * Order Gemfile-owned project roots stably by path.
 * Do not promote the active document's owner — jumping roots is disorienting.
 */
function orderRubyIndexProjects(projects, _activeDocumentPath) {
    const list = Array.isArray(projects)
        ? projects
            .filter((project) => typeof project?.root === 'string' && project.root.length > 0)
            .map((project) => ({
                root: project.root,
                label: projectRootLabel(project.root),
                phase: project.phase
            }))
        : [];
    list.sort((left, right) => left.root.localeCompare(right.root));
    return list;
}

function findOwningWorkspaceFolder(projectRoot, workspaceFolders) {
    const root = normalizeRootPath(projectRoot);
    let owner = null;
    for (const folder of workspaceFolders) {
        const normalized = normalizeRootPath(folder);
        if (root === normalized || root.startsWith(`${normalized}/`)) {
            if (!owner || normalized.length > owner.length) {
                owner = normalized;
            }
        }
    }
    return owner;
}

function sortForestChildren(nodes) {
    nodes.sort((left, right) => left.path.localeCompare(right.path));
    for (const node of nodes) {
        if (Array.isArray(node.children) && node.children.length > 0) {
            sortForestChildren(node.children);
        }
    }
}

function ensurePathFolder(parent, absolutePath, label) {
    let child = parent.children.find((node) => node.path === absolutePath);
    if (!child) {
        child = {
            kind: 'pathFolder',
            path: absolutePath,
            label,
            project: null,
            children: []
        };
        parent.children.push(child);
    }
    return child;
}

/**
 * Nest Gemfile projects under workspace folders by relative path.
 *
 * Presentation only — does not merge engines. Intermediate path segments become
 * folder chrome; Gemfile roots remain project leaves (or a collapsed single
 * project when the workspace folder itself is the only Gemfile root).
 */
function buildWorkspaceProjectForest(projects, workspaceFolderPaths) {
    const ordered = orderRubyIndexProjects(projects);
    const folders = [...new Set(
        (Array.isArray(workspaceFolderPaths) ? workspaceFolderPaths : [])
            .filter((folder) => typeof folder === 'string' && folder.length > 0)
            .map(normalizeRootPath)
    )].sort((left, right) => left.localeCompare(right));

    const workspaces = new Map();
    for (const folder of folders) {
        workspaces.set(folder, {
            kind: 'workspace',
            path: folder,
            label: projectRootLabel(folder),
            project: null,
            children: []
        });
    }

    const orphans = [];
    for (const project of ordered) {
        const root = normalizeRootPath(project.root);
        const owner = findOwningWorkspaceFolder(root, folders);
        if (!owner) {
            orphans.push({
                kind: 'project',
                path: root,
                label: project.label,
                project,
                children: []
            });
            continue;
        }

        const workspace = workspaces.get(owner);
        if (root === owner) {
            workspace.project = project;
            continue;
        }

        const relative = root.slice(owner.length + 1);
        const segments = relative.split('/').filter(Boolean);
        let current = workspace;
        let currentPath = owner;
        for (let index = 0; index < segments.length; index += 1) {
            const segment = segments[index];
            currentPath = `${currentPath}/${segment}`;
            const isLast = index === segments.length - 1;
            if (isLast) {
                const existing = current.children.find((node) => node.path === currentPath);
                if (existing) {
                    existing.kind = 'project';
                    existing.project = project;
                    existing.label = segment;
                } else {
                    current.children.push({
                        kind: 'project',
                        path: currentPath,
                        label: segment,
                        project,
                        children: []
                    });
                }
            } else {
                current = ensurePathFolder(current, currentPath, segment);
            }
        }
    }

    const roots = [];
    for (const workspace of workspaces.values()) {
        if (workspace.project) {
            workspace.children.unshift({
                kind: 'project',
                path: workspace.path,
                label: workspace.label,
                project: workspace.project,
                children: []
            });
        }
        sortForestChildren(workspace.children);
        if (workspace.children.length === 0) {
            continue;
        }
        if (
            workspace.children.length === 1
            && workspace.children[0].kind === 'project'
            && workspace.children[0].path === workspace.path
        ) {
            roots.push(workspace.children[0]);
            continue;
        }
        // One workspace folder: nest by relative path at the tree root — no
        // redundant container row for the folder itself.
        if (folders.length === 1) {
            roots.push(...workspace.children);
            continue;
        }
        roots.push(workspace);
    }

    if (folders.length === 1) {
        roots.sort((left, right) => left.path.localeCompare(right.path));
    }
    sortForestChildren(orphans);
    roots.push(...orphans);
    return roots;
}

/**
 * Compact indexing-phase text for Ruby Projects tree rows.
 */
function projectPhaseDescription(phase) {
    switch (phase) {
        case undefined:
        case null:
        case '':
            return 'Ruby project';
        case 'discovered':
            return 'discovered';
        case 'queued':
            return 'queued';
        case 'resolvingRuntime':
            return 'runtime…';
        case 'discoveringInputs':
            return 'inputs…';
        case 'indexingCore':
            return 'core…';
        case 'indexingProject':
            return 'indexing…';
        case 'projectNavigationReady':
        case 'indexingDependencies':
            return 'dependencies…';
        case 'dependencyNavigationReady':
        case 'resolvingSemantics':
            return 'semantics…';
        case 'publishingDiagnostics':
            return 'diagnostics…';
        case 'ready':
            return 'ready';
        case 'failed':
            return 'failed';
        case 'cancelled':
            return 'cancelled';
        default:
            return String(phase);
    }
}

function isActiveProjectRoot(projectRoot, activeDocumentPath) {
    if (typeof projectRoot !== 'string' || projectRoot.length === 0) {
        return false;
    }
    if (typeof activeDocumentPath !== 'string' || activeDocumentPath.length === 0) {
        return false;
    }
    const root = normalizeRootPath(projectRoot);
    const active = normalizeRootPath(activeDocumentPath);
    return active === root || active.startsWith(`${root}/`);
}

/**
 * Tree icon / description / tooltip for a Gemfile project row.
 */
function projectTreePresentation(project, activeDocumentPath) {
    const phase = project?.phase;
    const active = isActiveProjectRoot(project?.root, activeDocumentPath);
    const phaseText = projectPhaseDescription(phase);
    let iconId = 'file-code';
    if (phase === 'failed') {
        iconId = 'error';
    } else if (phase && phase !== 'ready' && phase !== 'cancelled') {
        iconId = 'sync~spin';
    } else if (active) {
        iconId = 'target';
    }
    return {
        active,
        iconId,
        description: active ? `${phaseText} · active` : phaseText,
        tooltip: active
            ? `${project.root}\n${phaseText}\nActive editor project`
            : `${project.root}\n${phaseText}`
    };
}

/**
 * Ctrl+P-style QuickPick rows for namespace search.
 * Active-project matches sort first; label is the FQN (path-like).
 */
function namespaceSearchQuickPickItems(namespaces, options = {}) {
    const activeProjectRoot = options.activeProjectRoot
        ? normalizeRootPath(options.activeProjectRoot)
        : null;
    const list = Array.isArray(namespaces) ? [...namespaces] : [];
    list.sort((left, right) => {
        const leftActive = activeProjectRoot
            && typeof left.projectRoot === 'string'
            && normalizeRootPath(left.projectRoot) === activeProjectRoot
            ? 0
            : 1;
        const rightActive = activeProjectRoot
            && typeof right.projectRoot === 'string'
            && normalizeRootPath(right.projectRoot) === activeProjectRoot
            ? 0
            : 1;
        if (leftActive !== rightActive) {
            return leftActive - rightActive;
        }
        return String(left.fqn || '').localeCompare(String(right.fqn || ''));
    });
    return list.map((ns) => {
        const projectLabel = typeof ns.projectRoot === 'string' && ns.projectRoot.length > 0
            ? projectRootLabel(ns.projectRoot)
            : '';
        return {
            label: ns.fqn || ns.name,
            description: [ns.kind, projectLabel].filter(Boolean).join(' · '),
            detail: ns.projectRoot || undefined,
            fqn: ns.fqn,
            namespaceData: ns,
            projectRoot: ns.projectRoot
        };
    });
}

function combineNamespaceRoots(modules, classes) {
    return [...(modules || []), ...(classes || [])];
}

function librarySectionPresentation(sectionId) {
    const presentation = LIBRARY_SECTION_PRESENTATION[sectionId];
    if (!presentation) {
        throw new Error(
            `INVARIANT VIOLATED: unknown library section id ${JSON.stringify(sectionId)}. `
            + 'This is a bug because namespaceTree libraries must use runtime|gems|excluded. '
            + 'Fix: align engine LibrarySectionId serde with adapter presentation.'
        );
    }
    return presentation;
}

function gemPackageLabel(packageInfo) {
    const name = packageInfo?.name;
    const version = packageInfo?.version;
    if (typeof name !== 'string' || name.length === 0) {
        throw new Error(
            'INVARIANT VIOLATED: gem package name is missing. '
            + 'This is a bug because namespaceTree packages must carry Bundler identity. '
            + 'Fix: align engine LibraryPackageTree with the adapter.'
        );
    }
    if (typeof version === 'string' && version.length > 0) {
        return `${name} ${version}`;
    }
    return name;
}

/**
 * Prefer structured `libraries` sections; fall back to flat external_* for older payloads.
 */
function librarySectionsFromResponse(response, showExternalTypes) {
    if (!showExternalTypes) {
        return [];
    }

    if (Array.isArray(response?.libraries) && response.libraries.length > 0) {
        return response.libraries
            .map((section) => {
                const packages = Array.isArray(section.packages)
                    ? section.packages
                        .map((packageInfo) => {
                            const namespaces = combineNamespaceRoots(
                                packageInfo.modules,
                                packageInfo.classes
                            );
                            if (namespaces.length === 0) {
                                return null;
                            }
                            return {
                                name: packageInfo.name,
                                version: packageInfo.version,
                                label: gemPackageLabel(packageInfo),
                                namespaces
                            };
                        })
                        .filter(Boolean)
                    : [];
                const namespaces = combineNamespaceRoots(section.modules, section.classes);
                if (namespaces.length === 0 && packages.length === 0) {
                    return null;
                }
                const presentation = librarySectionPresentation(section.id);
                return {
                    id: section.id,
                    label: presentation.label,
                    description: presentation.description,
                    icon: presentation.icon,
                    namespaces,
                    packages
                };
            })
            .filter(Boolean);
    }

    const legacyNamespaces = combineNamespaceRoots(
        response?.external_modules,
        response?.external_classes
    );
    if (legacyNamespaces.length === 0) {
        return [];
    }
    const presentation = librarySectionPresentation('runtime');
    return [{
        id: 'runtime',
        label: 'Libraries',
        description: presentation.description,
        icon: presentation.icon,
        namespaces: legacyNamespaces,
        packages: []
    }];
}

/**
 * Split a namespaceTree response into project browse roots and library sections.
 */
function projectBrowseSections(response, showExternalTypes) {
    const projectNamespaces = combineNamespaceRoots(
        response?.modules,
        response?.classes
    );
    const librarySections = librarySectionsFromResponse(response, showExternalTypes);
    const libraryNamespaces = librarySections.flatMap((section) => [
        ...section.namespaces,
        ...section.packages.flatMap((packageInfo) => packageInfo.namespaces)
    ]);
    return {
        projectNamespaces,
        librarySections,
        libraryNamespaces,
        hasLibraries: librarySections.length > 0
    };
}

/**
 * Child descriptors for a namespace node: nested types first, mixin chrome last.
 */
function namespaceChildDescriptors(ns) {
    if (!ns || typeof ns !== 'object') {
        return [];
    }
    const children = [];
    for (const child of ns.modules || []) {
        children.push({ kind: 'namespace', namespace: child });
    }
    for (const child of ns.classes || []) {
        children.push({ kind: 'namespace', namespace: child });
    }

    const superclass = ns.superclass;
    if (superclass?.name && !String(superclass.name).includes('(not found)')) {
        children.push({
            kind: 'mixinSection',
            label: 'Superclass',
            icon: 'arrow-up',
            mixins: [superclass]
        });
    }
    if (ns.includes?.length > 0) {
        children.push({
            kind: 'mixinSection',
            label: 'Includes',
            icon: 'plug',
            mixins: ns.includes
        });
    }
    if (ns.prepends?.length > 0) {
        children.push({
            kind: 'mixinSection',
            label: 'Prepends',
            icon: 'pinned',
            mixins: ns.prepends
        });
    }
    if (ns.singleton_class) {
        children.push({ kind: 'singleton', namespace: ns.singleton_class });
    }
    if (ns.included_by?.length > 0) {
        children.push({
            kind: 'includedBySection',
            label: 'Included By',
            icon: 'references',
            includers: ns.included_by
        });
    }
    return children;
}

function namespaceHasChildren(ns) {
    return namespaceChildDescriptors(ns).length > 0;
}

module.exports = {
    projectRootLabel,
    orderRubyIndexProjects,
    buildWorkspaceProjectForest,
    findOwningWorkspaceFolder,
    projectPhaseDescription,
    isActiveProjectRoot,
    projectTreePresentation,
    namespaceSearchQuickPickItems,
    projectBrowseSections,
    librarySectionsFromResponse,
    librarySectionPresentation,
    gemPackageLabel,
    namespaceChildDescriptors,
    namespaceHasChildren,
    combineNamespaceRoots,
    normalizeRootPath,
    LIBRARY_SECTION_PRESENTATION
};
