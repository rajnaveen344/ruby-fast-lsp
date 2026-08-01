const assert = require('node:assert/strict');
const test = require('node:test');

const {
    runtimeStatusForDocument,
    runtimeStatusItem,
    runtimeStatusPresentation,
    runtimeVersionMarker,
    selectRuntime
} = require('../runtime_selector');

function jruby(version, family, ruby, executable, supportStatus = 'supported') {
    return {
        implementation: 'jruby',
        implementationLabel: 'JRuby',
        family,
        familyLabel: `JRuby ${family} (Ruby ${ruby})`,
        compatibilityLabel: `Ruby ${ruby}`,
        supportStatus,
        engineVersion: version,
        compatibilityVersion: ruby,
        displayName: `JRuby ${version} (Ruby ${ruby})`,
        executable,
        discoverySource: 'rvm',
        javaHome: '/jdk/17'
    };
}

function runtime(implementation, version, family, ruby, executable) {
    const label = implementation === 'mri'
        ? 'MRI'
        : implementation === 'truffleruby'
            ? 'TruffleRuby'
            : implementation;
    return {
        implementation,
        implementationLabel: label,
        family,
        familyLabel: implementation === 'mri'
            ? `MRI ${family}`
            : `${label} ${family} (Ruby ${ruby})`,
        compatibilityLabel: `Ruby ${ruby}`,
        supportStatus: 'supported',
        engineVersion: version,
        compatibilityVersion: ruby,
        displayName: implementation === 'mri'
            ? `MRI ${version}`
            : `${label} ${version} (Ruby ${ruby})`,
        executable,
        discoverySource: 'rvm'
    };
}

function catalog(projects) {
    return {
        projects: projects.map(([root, runtimes]) => ({
            root,
            label: root.split('/').pop(),
            runtimes
        }))
    };
}

function harness(response, selections, additionalOptions = {}) {
    const calls = [];
    const errors = [];
    let applied;
    return {
        calls,
        errors,
        get applied() {
            return applied;
        },
        options: {
            ...additionalOptions,
            client: {
                sendRequest: async (method, params) => {
                    assert.equal(method, 'ruby-fast-lsp/runtime/discover');
                    assert.deepEqual(params, {});
                    return response;
                }
            },
            window: {
                showQuickPick: async (items, options) => {
                    calls.push({ items, options });
                    const selection = selections.shift();
                    if (selection === undefined) {
                        return undefined;
                    }
                    return typeof selection === 'function' ? selection(items) : items[selection];
                },
                showErrorMessage: async message => {
                    errors.push(message);
                }
            },
            applySelection: async selection => {
                applied = selection;
            }
        }
    };
}

test('selects project, JRuby family, exact installation, and confirmation', async () => {
    const fixture = harness(
        catalog([
            ['/repo/admin', [jruby('9.2.21.0', '9.2', '2.5', '/jruby/9.2/bin/jruby')]],
            ['/repo/server', [jruby('9.4.14.0', '9.4', '3.1', '/jruby/9.4/bin/jruby')]]
        ]),
        [0, 1, 0, 0, 0]
    );

    const result = await selectRuntime(fixture.options);
    assert.equal(fixture.calls.length, 5);
    assert.equal(fixture.calls[0].options.title, 'Ruby Fast LSP: Select Project');
    assert.equal(fixture.calls[2].items[0].label, 'JRuby 9.2 (Ruby 2.5)');
    assert.equal(result.projectRoot, '/repo/admin');
    assert.equal(result.runtime.engineVersion, '9.2.21.0');
    assert.deepEqual(result, fixture.applied);
});

test('auto skips family and installation while retaining confirmation', async () => {
    const fixture = harness(
        catalog([
            ['/repo/admin', [jruby('9.2.21.0', '9.2', '2.5', '/jruby/9.2/bin/jruby')]]
        ]),
        [0, 0]
    );
    const result = await selectRuntime(fixture.options);
    assert.deepEqual(result, { projectRoot: '/repo/admin', mode: 'auto' });
    assert.equal(fixture.calls.length, 2);
});

test('status-bar selection targets the active project without another project prompt', async () => {
    const fixture = harness(
        catalog([
            ['/repo/admin', [jruby('9.2.21.0', '9.2', '2.5', '/jruby/9.2/bin/jruby')]],
            ['/repo/server', [runtime('mri', '3.3.11', '3.3', '3.3', '/ruby/3.3/bin/ruby')]]
        ]),
        [1, 0, 0, 0],
        { preferredProjectRoot: '/repo/server' }
    );

    const result = await selectRuntime(fixture.options);
    assert.equal(result.projectRoot, '/repo/server');
    assert.equal(result.runtime.engineVersion, '3.3.11');
    assert.equal(fixture.calls[0].options.title, 'Ruby Fast LSP: Runtime for server');
    assert.equal(fixture.calls.length, 4);
});

test('cancellation at any QuickPick level applies nothing', async () => {
    const fixture = harness(
        catalog([
            ['/repo/admin', [jruby('9.2.21.0', '9.2', '2.5', '/jruby/9.2/bin/jruby')]]
        ]),
        [1, undefined]
    );
    assert.equal(await selectRuntime(fixture.options), undefined);
    assert.equal(fixture.applied, undefined);
});

test('unsupported future JRuby family is visible and fails closed', async () => {
    const fixture = harness(
        catalog([
            ['/repo/admin', [
                jruby('10.2.0.0', '10.2', '4.1', '/jruby/10.2/bin/jruby', 'unsupported')
            ]]
        ]),
        [1, 0]
    );
    assert.equal(await selectRuntime(fixture.options), undefined);
    assert.equal(fixture.applied, undefined);
    assert.match(fixture.errors[0], /will not substitute a nearby compatibility model/);
});

test('empty project and installation catalogs produce actionable errors', async () => {
    const empty = harness(catalog([]), []);
    assert.equal(await selectRuntime(empty.options), undefined);
    assert.match(empty.errors[0], /did not discover any Ruby projects/);

    const noInstallCatalog = catalog([['/repo/admin', []]]);
    noInstallCatalog.projects[0].implementations = [{ id: 'jruby', label: 'JRuby' }];
    const noInstall = harness(noInstallCatalog, [1]);
    assert.equal(await selectRuntime(noInstall.options), undefined);
    assert.match(noInstall.errors[0], /No .* installations were discovered/);
});

test('formats server-owned JRuby runtime status without reconstructing identity', () => {
    const item = runtimeStatusItem({
        root: '/repo/admin',
        mode: 'explicit',
        implementation: 'jruby',
        family: '9.2',
        engineVersion: '9.2.21.0',
        compatibilityVersion: '2.5',
        executable: '/runtimes/jruby-9.2.21.0/bin/jruby',
        javaHome: '/jdk/17',
        stubOverlay: '9.2',
        classpathFingerprintSha256: 'abcdef0123456789',
        indexingComplete: true
    });

    assert.equal(item.label, 'admin → JRuby 9.2.21.0 (Ruby 2.5)');
    assert.match(item.description, /JDK \/jdk\/17/);
    assert.match(item.description, /overlay 9\.2/);
    assert.match(item.description, /classpath abcdef012345/);
    assert.match(item.description, /ready/);
});

test('selects the deepest owning project status for the active document', () => {
    const projects = [
        { root: '/repo', implementation: 'mri', engineVersion: '3.3.11' },
        { root: '/repo/admin', implementation: 'jruby', engineVersion: '9.2.21.0' },
        { root: '/repo/server', implementation: 'mri', engineVersion: '3.2.8' }
    ];

    assert.deepEqual(
        runtimeStatusForDocument(projects, '/repo/admin/lib/app.rb'),
        projects[1]
    );
    assert.deepEqual(
        runtimeStatusForDocument(projects, '/repo/server/app.rb'),
        projects[2]
    );
    assert.deepEqual(
        runtimeStatusForDocument(projects, '/repo/administrator/app.rb'),
        projects[0],
        'a sibling prefix must not be mistaken for the nested admin project'
    );
    assert.equal(runtimeStatusForDocument(projects, '/outside/app.rb'), undefined);
});

test('renders exact runtime identity and indexing state for the status bar', () => {
    assert.deepEqual(runtimeStatusPresentation({
        root: '/repo/admin',
        mode: 'explicit',
        implementation: 'jruby',
        engineVersion: '9.2.21.0',
        compatibilityVersion: '2.5',
        indexingComplete: true
    }), {
        text: '$(ruby) JRuby 9.2.21.0',
        tooltip: 'admin: JRuby 9.2.21.0 (Ruby 2.5) — ready'
    });

    assert.deepEqual(runtimeStatusPresentation({
        root: '/repo/server',
        mode: 'auto',
        implementation: 'mri',
        engineVersion: '3.3.11',
        compatibilityVersion: '3.3',
        indexingComplete: false
    }), {
        text: '$(sync~spin) server: indexing 0.0s / 5s',
        tooltip: 'server: indexing — 0.0s / 5s'
    });
});

test('renders authoritative project phases, deadlines, and terminal failures', () => {
    assert.deepEqual(runtimeStatusPresentation({
        root: '/repo/admin',
        mode: 'explicit',
        implementation: 'jruby',
        engineVersion: '9.2.21.0',
        compatibilityVersion: '2.5',
        indexing: {
            generation: 3,
            sequence: 9,
            phase: 'indexingProject',
            completed: 120,
            total: 300,
            elapsedMs: 3200
        }
    }), {
        text: '$(sync~spin) admin: project 3.2s / 5s',
        tooltip: 'admin: project 120/300 — 3.2s / 5s'
    });

    assert.deepEqual(runtimeStatusPresentation({
        root: '/repo/admin',
        indexing: {
            generation: 3,
            sequence: 10,
            phase: 'indexingDependencies',
            elapsedMs: 18100
        }
    }), {
        text: '$(warning) admin: slow indexing · 18s',
        tooltip: 'admin: dependencies — 18s / 15s (target exceeded)'
    });

    assert.deepEqual(runtimeStatusPresentation({
        root: '/repo/admin',
        indexing: {
            generation: 3,
            sequence: 11,
            phase: 'failed',
            elapsedMs: 19000,
            failure: 'Gemfile.lock could not be read'
        }
    }), {
        text: '$(warning) admin: indexing failed',
        tooltip: 'admin: Gemfile.lock could not be read'
    });
});

test('serializes exact effective runtimes into standard project markers', () => {
    assert.equal(runtimeVersionMarker({
        implementation: 'jruby',
        engineVersion: '9.2.21.0'
    }), 'jruby-9.2.21.0');
    assert.equal(runtimeVersionMarker({
        implementation: 'truffleruby',
        engineVersion: '24.1.2'
    }), 'truffleruby-24.1.2');
    assert.equal(runtimeVersionMarker({
        implementation: 'mri',
        engineVersion: '3.3.11'
    }), '3.3.11');
    assert.equal(runtimeVersionMarker({ implementation: 'jruby' }), undefined);
});

test('every supported JRuby family remains a distinct compatibility-labelled selector level', async () => {
    const versions = [
        ['9.0.5.0', '9.0', '2.2'],
        ['9.1.17.0', '9.1', '2.3'],
        ['9.2.21.0', '9.2', '2.5'],
        ['9.3.15.0', '9.3', '2.6'],
        ['9.4.14.0', '9.4', '3.1'],
        ['10.0.6.0', '10.0', '3.4'],
        ['10.1.0.0', '10.1', '4.0']
    ];
    const runtimes = versions.map(([version, family, ruby]) =>
        jruby(version, family, ruby, `/jruby/${version}/bin/jruby`)
    );

    for (let index = 0; index < versions.length; index += 1) {
        const fixture = harness(catalog([['/repo/admin', runtimes]]), [1, index, 0, 0]);
        const result = await selectRuntime(fixture.options);
        const [version, family, ruby] = versions[index];
        assert.equal(result.runtime.engineVersion, version);
        assert.equal(result.runtime.family, family);
        assert.equal(result.runtime.compatibilityVersion, ruby);
        assert.equal(
            fixture.calls[1].items[index].label,
            `JRuby ${family} (Ruby ${ruby})`
        );
    }
});

test('MRI and TruffleRuby selectors preserve implementation-specific labels', async () => {
    const runtimes = [
        runtime('mri', '3.3.11', '3.3', '3.3', '/ruby/3.3/bin/ruby'),
        runtime('truffleruby', '24.1.2', '24.1', '3.3', '/truffle/24.1/bin/ruby')
    ];
    const mri = harness(catalog([['/repo/admin', runtimes]]), [1, 0, 0, 0]);
    const mriResult = await selectRuntime(mri.options);
    assert.equal(mri.calls[1].items[0].label, 'MRI 3.3');
    assert.equal(mriResult.runtime.implementation, 'mri');

    const truffle = harness(catalog([['/repo/admin', runtimes]]), [2, 0, 0, 0]);
    const truffleResult = await selectRuntime(truffle.options);
    assert.equal(
        truffle.calls[1].items[0].label,
        'TruffleRuby 24.1 (Ruby 3.3)'
    );
    assert.equal(truffleResult.runtime.implementation, 'truffleruby');
});
