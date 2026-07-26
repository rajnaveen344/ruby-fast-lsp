'use strict';

const DEFAULT_RUNTIME = Object.freeze({ mode: 'auto', projects: [] });
const DEFAULT_JRUBY = Object.freeze({ mode: 'auto', projects: [] });
const DEFAULT_INDEXING = Object.freeze({
    projectRoots: [],
    excludedPatterns: [],
    includedPatterns: [],
    excludedGems: [],
    includedGems: []
});

const STATE_KEYS = Object.freeze({
    runtime: 'rubyFastLsp.runtime',
    linter: 'rubyFastLsp.linter',
    formatter: 'rubyFastLsp.formatter',
    showExternalTypes: 'rubyFastLsp.showExternalTypes'
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
            false,
            value => typeof value === 'boolean'
        )
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
        indexing: DEFAULT_INDEXING
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

function validRuntime(value) {
    return value !== null
        && typeof value === 'object'
        && value.mode === 'auto'
        && Array.isArray(value.projects);
}

function validTool(value) {
    return value === 'none' || value === 'rubocop' || value === 'standard';
}

module.exports = {
    DEFAULT_INDEXING,
    STATE_KEYS,
    readEditorState,
    serverConfiguration,
    updateRuntime,
    validTool
};
