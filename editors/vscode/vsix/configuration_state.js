'use strict';

const DEFAULT_RUNTIME = Object.freeze({ mode: 'auto', projects: [] });
const DEFAULT_JRUBY = Object.freeze({ mode: 'auto', projects: [] });
const DEFAULT_LOAD_PATHS = Object.freeze({ default: [], projects: [] });
const DEFAULT_INDEXING = Object.freeze({
    projectRoots: [],
    excludedPatterns: [],
    includedPatterns: [],
    excludedGems: [],
    includedGems: [],
    loadPaths: { ...DEFAULT_LOAD_PATHS, projects: [] }
});

const STATE_KEYS = Object.freeze({
    runtime: 'rubyFastLsp.runtime',
    linter: 'rubyFastLsp.linter',
    formatter: 'rubyFastLsp.formatter',
    // New key defaults on (JRE/Gems visible). Old showExternalTypes defaulted off.
    showExternalTypes: 'rubyFastLsp.showLibrarySections',
    loadPaths: 'rubyFastLsp.indexing.loadPaths'
});

function readEditorState(workspaceState, legacyConfiguration) {
    return {
        runtime: readMigrated(
            workspaceState,
            legacyConfiguration,
            STATE_KEYS.runtime,
            'runtime',
            DEFAULT_RUNTIME,
            validRuntime
        ),
        linter: readMigrated(
            workspaceState,
            legacyConfiguration,
            STATE_KEYS.linter,
            'linter',
            'none',
            validTool
        ),
        formatter: readMigrated(
            workspaceState,
            legacyConfiguration,
            STATE_KEYS.formatter,
            'formatter',
            'none',
            validTool
        ),
        showExternalTypes: readMigrated(
            workspaceState,
            legacyConfiguration,
            STATE_KEYS.showExternalTypes,
            'showExternalTypes',
            true,
            value => typeof value === 'boolean'
        ),
        loadPaths: readLoadPaths(workspaceState, legacyConfiguration)
    };
}

function readMigrated(workspaceState, configuration, stateKey, legacyKey, fallback, validate) {
    const stored = workspaceState.get(stateKey);
    if (stored !== undefined) {
        return validate(stored) ? stored : fallback;
    }

    const legacy = configuration.get(legacyKey);
    const value = legacy !== undefined && validate(legacy) ? legacy : fallback;
    void workspaceState.update(stateKey, value);
    return value;
}

function readLoadPaths(workspaceState, configuration) {
    const stored = workspaceState.get(STATE_KEYS.loadPaths);
    if (stored !== undefined) {
        const migrated = migrateLoadPaths(stored);
        if (!migrated) {
            return { default: [], projects: [] };
        }
        if (Array.isArray(stored)) {
            void workspaceState.update(STATE_KEYS.loadPaths, migrated);
        }
        return migrated;
    }

    const legacy = configuration.get('loadPaths');
    const migrated = migrateLoadPaths(legacy);
    const value = migrated || { default: [], projects: [] };
    void workspaceState.update(STATE_KEYS.loadPaths, value);
    return value;
}

function migrateLoadPaths(value) {
    if (Array.isArray(value)) {
        // Legacy flat list becomes the workspace default so umbrellas keep
        // prior behavior until projects specialize their own entries.
        return validPathList(value)
            ? { default: [...value], projects: [] }
            : null;
    }
    return validLoadPaths(value) ? normalizeLoadPaths(value) : null;
}

function normalizeLoadPaths(value) {
    const projects = Array.isArray(value.projects)
        ? value.projects
            .map(project => ({
                root: project.root,
                paths: [...project.paths]
            }))
            .sort((left, right) => left.root.localeCompare(right.root))
        : [];
    return {
        default: Array.isArray(value.default) ? [...value.default] : [],
        projects
    };
}

function serverConfiguration({
    editorState,
    logLevel,
    extensionPath,
    extensionPackages,
    workspaceTrusted
}) {
    return {
        rubyVersion: 'auto',
        runtime: editorState.runtime,
        jruby: DEFAULT_JRUBY,
        extensionPath,
        extensionPackages,
        extensionDirs: [],
        extensionSettings: {},
        workspaceTrusted,
        projectExtensionsEnabled: true,
        logLevel,
        linter: editorState.linter,
        linterCommand: [],
        formatter: editorState.formatter,
        formatterCommand: [],
        indexing: {
            ...DEFAULT_INDEXING,
            loadPaths: normalizeLoadPaths(editorState.loadPaths || DEFAULT_LOAD_PATHS)
        }
    };
}

function updateRuntime(runtimeConfig, selection) {
    const projects = Array.isArray(runtimeConfig.projects)
        ? runtimeConfig.projects.filter(project => project.root !== selection.projectRoot)
        : [];
    projects.push({
        root: selection.projectRoot,
        selection: selection.mode === 'auto' ? 'auto' : selection.runtime
    });
    projects.sort((left, right) => left.root.localeCompare(right.root));
    return { mode: 'auto', projects };
}

function updateLoadPaths(loadPathsConfig, selection) {
    const current = normalizeLoadPaths(loadPathsConfig || DEFAULT_LOAD_PATHS);
    const projects = current.projects.filter(project => project.root !== selection.projectRoot);
    if (Array.isArray(selection.paths) && selection.paths.length > 0) {
        projects.push({
            root: selection.projectRoot,
            paths: [...selection.paths]
        });
    }
    projects.sort((left, right) => left.root.localeCompare(right.root));
    return {
        default: [...current.default],
        projects
    };
}

function pathsForProject(loadPathsConfig, projectRoot) {
    const current = normalizeLoadPaths(loadPathsConfig || DEFAULT_LOAD_PATHS);
    const match = current.projects.find(project => project.root === projectRoot);
    return match ? [...match.paths] : [...current.default];
}

function validRuntime(value) {
    return value !== null
        && typeof value === 'object'
        && value.mode === 'auto'
        && Array.isArray(value.projects);
}

function validTool(value) {
    return value === 'none' || value === 'rubocop' || value === 'standard';
}

function validPathList(value) {
    return Array.isArray(value)
        && value.every(entry => typeof entry === 'string'
            && entry.length > 0
            && !entry.startsWith('/')
            && !entry.includes('..'));
}

function validLoadPaths(value) {
    if (Array.isArray(value)) {
        return validPathList(value);
    }
    return value !== null
        && typeof value === 'object'
        && validPathList(value.default || [])
        && Array.isArray(value.projects)
        && value.projects.every(project => project !== null
            && typeof project === 'object'
            && typeof project.root === 'string'
            && project.root.length > 0
            && validPathList(project.paths || []));
}

module.exports = {
    DEFAULT_INDEXING,
    DEFAULT_LOAD_PATHS,
    STATE_KEYS,
    pathsForProject,
    readEditorState,
    serverConfiguration,
    updateLoadPaths,
    updateRuntime,
    validLoadPaths,
    validTool
};
