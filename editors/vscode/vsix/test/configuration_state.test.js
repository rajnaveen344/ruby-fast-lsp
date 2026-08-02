const assert = require('node:assert/strict');
const test = require('node:test');

const {
    DEFAULT_INDEXING,
    DEFAULT_LOAD_PATHS,
    STATE_KEYS,
    pathsForProject,
    readEditorState,
    serverConfiguration,
    updateLoadPaths,
    updateRuntime
} = require('../configuration_state');

function state(initial = {}) {
    const values = new Map(Object.entries(initial));
    return {
        get: key => values.get(key),
        update: async (key, value) => values.set(key, value),
        values
    };
}

function configuration(values = {}) {
    return {
        get: key => values[key]
    };
}

test('migrates valid legacy editor choices into private workspace state', async () => {
    const workspaceState = state();
    const runtime = {
        mode: 'auto',
        projects: [{ root: '/repo', selection: 'auto' }]
    };
    const editorState = readEditorState(workspaceState, configuration({
        runtime,
        linter: 'rubocop',
        formatter: 'standard',
        showExternalTypes: true
    }));

    assert.deepEqual(editorState, {
        runtime,
        linter: 'rubocop',
        formatter: 'standard',
        showExternalTypes: true,
        loadPaths: { default: [], projects: [] }
    });
    await Promise.resolve();
    assert.deepEqual(workspaceState.values.get(STATE_KEYS.runtime), runtime);
    assert.equal(workspaceState.values.get(STATE_KEYS.linter), 'rubocop');
    assert.equal(workspaceState.values.get(STATE_KEYS.formatter), 'standard');
    assert.equal(workspaceState.values.get(STATE_KEYS.showExternalTypes), true);
    assert.deepEqual(workspaceState.values.get(STATE_KEYS.loadPaths), {
        default: [],
        projects: []
    });
});

test('private state wins over obsolete settings and invalid values fail closed', () => {
    const workspaceState = state({
        [STATE_KEYS.linter]: 'standard',
        [STATE_KEYS.formatter]: 'invalid',
        [STATE_KEYS.showExternalTypes]: false
    });
    const editorState = readEditorState(workspaceState, configuration({
        linter: 'rubocop',
        formatter: 'rubocop',
        showExternalTypes: true
    }));

    assert.equal(editorState.linter, 'standard');
    assert.equal(editorState.formatter, 'none');
    assert.equal(editorState.showExternalTypes, false);
});

test('server configuration exposes only product choices and deterministic defaults', () => {
    const editorState = {
        runtime: { mode: 'auto', projects: [] },
        linter: 'rubocop',
        formatter: 'standard',
        showExternalTypes: true,
        loadPaths: {
            default: [],
            projects: [{ root: '/repo/server', paths: ['custom_lib'] }]
        }
    };
    const config = serverConfiguration({
        editorState,
        logLevel: 'debug',
        extensionPath: '/extension',
        extensionPackages: ['/extension/rspec'],
        workspaceTrusted: true
    });

    assert.equal(config.logLevel, 'debug');
    assert.equal(config.linter, 'rubocop');
    assert.equal(config.formatter, 'standard');
    assert.deepEqual(config.linterCommand, []);
    assert.deepEqual(config.formatterCommand, []);
    assert.deepEqual(config.indexing, {
        ...DEFAULT_INDEXING,
        loadPaths: {
            default: [],
            projects: [{ root: '/repo/server', paths: ['custom_lib'] }]
        }
    });
    assert.deepEqual(config.extensionSettings, {});
    assert.equal(config.projectExtensionsEnabled, true);
    assert.equal(config.showExternalTypes, undefined);
    assert.equal(config.stubsPath, undefined);
});

test('legacy flat loadPaths private state migrates into workspace default', async () => {
    const workspaceState = state({
        [STATE_KEYS.loadPaths]: ['custom_lib', 'shared/lib']
    });
    const editorState = readEditorState(workspaceState, configuration({}));
    assert.deepEqual(editorState.loadPaths, {
        default: ['custom_lib', 'shared/lib'],
        projects: []
    });
    await Promise.resolve();
    assert.deepEqual(workspaceState.values.get(STATE_KEYS.loadPaths), {
        default: ['custom_lib', 'shared/lib'],
        projects: []
    });
});

test('loadPaths private state rejects absolute and parent traversal entries', () => {
    const workspaceState = state({
        [STATE_KEYS.loadPaths]: ['/abs', '../escape', 'ok/lib']
    });
    const editorState = readEditorState(workspaceState, configuration({}));
    assert.deepEqual(editorState.loadPaths, DEFAULT_LOAD_PATHS);
});

test('loadPaths updates are isolated by project root', () => {
    const updated = updateLoadPaths(
        {
            default: ['shared'],
            projects: [{ root: '/repo/server', paths: ['custom'] }]
        },
        { projectRoot: '/repo/admin', paths: ['other'] }
    );

    assert.deepEqual(updated, {
        default: ['shared'],
        projects: [
            { root: '/repo/admin', paths: ['other'] },
            { root: '/repo/server', paths: ['custom'] }
        ]
    });
    assert.deepEqual(pathsForProject(updated, '/repo/admin'), ['other']);
    assert.deepEqual(pathsForProject(updated, '/repo/server'), ['custom']);
    assert.deepEqual(pathsForProject(updated, '/repo/web'), ['shared']);
});

test('clearing project loadPaths falls back to workspace default', () => {
    const updated = updateLoadPaths(
        {
            default: ['shared'],
            projects: [{ root: '/repo/server', paths: ['custom'] }]
        },
        { projectRoot: '/repo/server', paths: [] }
    );

    assert.deepEqual(updated, {
        default: ['shared'],
        projects: []
    });
    assert.deepEqual(pathsForProject(updated, '/repo/server'), ['shared']);
});

test('runtime replacement is deterministic and isolated by project root', () => {
    const runtime = {
        mode: 'auto',
        projects: [
            { root: '/repo/server', selection: 'auto' },
            { root: '/repo/admin', selection: 'auto' }
        ]
    };
    const updated = updateRuntime(runtime, {
        projectRoot: '/repo/admin',
        mode: 'explicit',
        runtime: { implementation: 'jruby', engineVersion: '9.2.21.0' }
    });

    assert.deepEqual(updated.projects, [
        {
            root: '/repo/admin',
            selection: { implementation: 'jruby', engineVersion: '9.2.21.0' }
        },
        { root: '/repo/server', selection: 'auto' }
    ]);
});
