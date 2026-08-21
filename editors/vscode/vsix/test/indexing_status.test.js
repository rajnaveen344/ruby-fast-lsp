const assert = require('node:assert/strict');
const test = require('node:test');

const {
    acceptNewerIndexingSnapshot,
    createIndexingStatusSession,
    indexingClockShouldRun,
    indexingStatusBarCommand,
    indexingStatusQuickPickPlaceholder,
    indexingStatusQuickPickItems,
    indexingStatusRequestParams,
    liveElapsedMs,
    lspStatusBarError,
    lspStatusBarPresentation,
    lspStatusBarStarting
} = require('../indexing_status');

test('active editor URI is sent with the authoritative status request', () => {
    const editor = {
        document: {
            uri: {
                toString: () => 'file:///repo/server/lib/app.rb'
            }
        }
    };

    assert.deepEqual(indexingStatusRequestParams(editor), {
        activeDocumentUri: 'file:///repo/server/lib/app.rb'
    });
    assert.deepEqual(indexingStatusRequestParams(undefined), {});
});

test('reordered status responses cannot replace newer project state', () => {
    const newest = acceptNewerIndexingSnapshot(7, {
        sequence: 9,
        aggregate: { ready: 2 },
        projects: [{ root: '/repo/server', phase: 'ready' }]
    });

    assert.deepEqual(newest, {
        sequence: 9,
        aggregate: { ready: 2 },
        projects: [{ root: '/repo/server', phase: 'ready' }]
    });
    assert.equal(acceptNewerIndexingSnapshot(9, {
        sequence: 8,
        aggregate: { ready: 0 },
        projects: [{ root: '/repo/server', phase: 'queued' }]
    }), undefined);
    assert.equal(acceptNewerIndexingSnapshot(9, {
        sequence: 9,
        aggregate: { ready: 0 }
    }), undefined);
});

test('status session rejects old transport events and accepts the restarted server sequence', () => {
    const session = createIndexingStatusSession();
    assert.equal(session.accept({
        sequence: 90,
        aggregate: { ready: 2 },
        projects: [{ root: '/repo/admin', phase: 'ready' }]
    }), true);

    session.suspendForRestart();
    assert.equal(session.accept({
        sequence: 91,
        aggregate: { ready: 0 },
        projects: [{ root: '/repo/admin', phase: 'failed' }]
    }), false);
    session.completeRestart();
    assert.deepEqual(session.snapshot(), {
        sequence: 0,
        aggregate: undefined,
        reuse: undefined,
        projects: [],
        receivedAtMs: undefined
    });
    assert.equal(session.accept({
        sequence: 1,
        aggregate: { ready: 0, active: 1 },
        projects: [{ root: '/repo/admin', phase: 'indexingProject' }]
    }), true);
    assert.equal(session.snapshot().sequence, 1);
});

test('disposed status session permanently rejects delayed notifications', () => {
    const session = createIndexingStatusSession();
    assert.equal(session.accept({
        sequence: 1,
        aggregate: { ready: 1 },
        projects: []
    }), true);
    session.dispose();
    assert.equal(session.accept({
        sequence: 2,
        aggregate: { ready: 0 },
        projects: []
    }), false);
    assert.equal(session.completeRestart(), false);
    assert.equal(session.accept({
        sequence: 1,
        aggregate: { ready: 0 },
        projects: []
    }), false);
});

test('project status details are deterministic and put the active project first', () => {
    const snapshot = {
        sequence: 12,
        aggregate: {
            discovered: 0,
            queued: 0,
            active: 1,
            ready: 1,
            failed: 0,
            cancelled: 0,
            concurrencyLimit: 2
        },
        projects: [
            {
                root: '/repo/admin',
                generation: 2,
                sequence: 9,
                phase: 'ready',
                completed: 80,
                total: 80,
                elapsedMs: 12_400,
                projectNavigationReadyMs: 2_100,
                dependencyNavigationReadyMs: 8_700
            },
            {
                root: '/repo/server',
                generation: 3,
                sequence: 11,
                phase: 'indexingDependencies',
                completed: 41,
                total: 100,
                elapsedMs: 8_400,
                projectNavigationReadyMs: 2_300,
                dependencyNavigationReadyMs: null
            }
        ]
    };

    const items = indexingStatusQuickPickItems(snapshot, '/repo/server');

    assert.equal(items.length, 2);
    assert.deepEqual(items[0], {
        label: '$(sync~spin) server',
        description: 'active · dependencies 41/100 files · 8.4s',
        detail: '/repo/server · generation 3 · project navigation 2.3s · dependency navigation pending',
        project: snapshot.projects[1]
    });
    assert.deepEqual(items[1], {
        label: '$(pass-filled) admin',
        description: 'ready · 12s',
        detail: '/repo/admin · generation 2 · project navigation 2.1s · dependency navigation 8.7s',
        project: snapshot.projects[0]
    });
});

test('failed project details preserve the authoritative failure', () => {
    const failed = {
        root: 'C:\\repo\\web',
        generation: 4,
        sequence: 13,
        phase: 'failed',
        completed: null,
        total: null,
        elapsedMs: 1_250,
        projectNavigationReadyMs: null,
        dependencyNavigationReadyMs: null,
        failure: 'runtime marker did not resolve'
    };

    assert.deepEqual(indexingStatusQuickPickItems({
        sequence: 14,
        aggregate: {
            discovered: 0,
            queued: 0,
            active: 0,
            ready: 0,
            failed: 1,
            cancelled: 0,
            concurrencyLimit: 2
        },
        projects: [failed]
    }, 'C:\\repo\\web'), [{
        label: '$(error) web',
        description: 'active · failed · 1.3s',
        detail: 'C:\\repo\\web · generation 4 · project navigation pending · dependency navigation pending · runtime marker did not resolve',
        project: failed
    }]);
});

test('status bar opens indexing details only while project indexing needs attention', () => {
    assert.equal(
        indexingStatusBarCommand({ phase: 'indexingProject' }),
        'ruby-fast-lsp.indexing.status'
    );
    assert.equal(
        indexingStatusBarCommand({ phase: 'failed' }),
        'ruby-fast-lsp.indexing.status'
    );
    assert.equal(
        indexingStatusBarCommand({ phase: 'ready' }),
        'ruby-fast-lsp.runtime.configure'
    );
    assert.equal(
        indexingStatusBarCommand({ phase: 'ready' }, {
            projects: [{ root: '/repo/server', phase: 'indexingProject' }]
        }),
        'ruby-fast-lsp.indexing.status'
    );
    assert.equal(
        indexingStatusBarCommand(undefined),
        'ruby-fast-lsp.runtime.configure'
    );
});

test('left status bar always uses a short Ruby: label', () => {
    const texts = {
        ready: lspStatusBarPresentation({
            root: '/repo/admin',
            indexingComplete: true
        }).text,
        noProject: lspStatusBarPresentation(undefined).text,
        starting: lspStatusBarStarting().text,
        error: lspStatusBarError('runtime failed').text,
        discovered: lspStatusBarPresentation({
            root: '/repo/admin',
            indexing: { phase: 'discovered', elapsedMs: 0 }
        }).text,
        queued: lspStatusBarPresentation({
            root: '/repo/admin',
            indexing: { phase: 'queued', elapsedMs: 0 }
        }).text,
        runtime: lspStatusBarPresentation({
            root: '/repo/admin',
            indexing: { phase: 'resolvingRuntime', elapsedMs: 0 }
        }).text,
        inputs: lspStatusBarPresentation({
            root: '/repo/admin',
            indexing: { phase: 'discoveringInputs', elapsedMs: 0 }
        }).text,
        core: lspStatusBarPresentation({
            root: '/repo/admin',
            indexing: { phase: 'indexingCore', elapsedMs: 0 }
        }).text,
        indexing: lspStatusBarPresentation({
            root: '/repo/admin',
            indexing: { phase: 'indexingProject', elapsedMs: 0 }
        }).text,
        files: lspStatusBarPresentation({
            root: '/repo/admin',
            indexing: {
                phase: 'indexingProject',
                completed: 120,
                total: 300,
                elapsedMs: 0
            }
        }).text,
        dependencies: lspStatusBarPresentation({
            root: '/repo/admin',
            indexing: { phase: 'indexingDependencies', elapsedMs: 0 }
        }).text,
        semantics: lspStatusBarPresentation({
            root: '/repo/admin',
            indexing: { phase: 'resolvingSemantics', elapsedMs: 0 }
        }).text,
        diagnostics: lspStatusBarPresentation({
            root: '/repo/admin',
            indexing: { phase: 'publishingDiagnostics', elapsedMs: 0 }
        }).text,
        failed: lspStatusBarPresentation({
            root: '/repo/admin',
            indexing: { phase: 'failed', elapsedMs: 0, failure: 'boom' }
        }).text,
        cancelled: lspStatusBarPresentation({
            root: '/repo/admin',
            indexing: { phase: 'cancelled', elapsedMs: 0 }
        }).text
    };

    assert.deepEqual(texts, {
        ready: '$(ruby) Ruby: Ready',
        noProject: '$(ruby) Ruby: No project',
        starting: '$(sync~spin) Ruby: Starting',
        error: '$(warning) Ruby: Error',
        discovered: '$(sync~spin) Ruby: Discovered',
        queued: '$(sync~spin) Ruby: Queued',
        runtime: '$(sync~spin) Ruby: Runtime',
        inputs: '$(sync~spin) Ruby: Inputs',
        core: '$(sync~spin) Ruby: Core',
        indexing: '$(sync~spin) Ruby: Indexing',
        files: '$(sync~spin) Ruby: 120/300 files',
        dependencies: '$(sync~spin) Ruby: Dependencies',
        semantics: '$(sync~spin) Ruby: Semantics',
        diagnostics: '$(sync~spin) Ruby: Diagnostics',
        failed: '$(warning) Ruby: Failed',
        cancelled: '$(clock) Ruby: Cancelled'
    });
});

test('single-project and multi-project ready use the same left status bar', () => {
    const single = lspStatusBarPresentation({
        root: '/repo/admin',
        indexingComplete: true,
        indexing: { phase: 'ready', elapsedMs: 12_000 }
    }, {
        projects: [{ root: '/repo/admin', phase: 'ready', elapsedMs: 12_000 }]
    });
    const multiAllReady = lspStatusBarPresentation({
        root: '/repo/admin',
        indexingComplete: true,
        indexing: { phase: 'ready', elapsedMs: 12_000 }
    }, {
        projects: [
            { root: '/repo/admin', phase: 'ready', elapsedMs: 12_000 },
            { root: '/repo/server', phase: 'ready', elapsedMs: 18_000 }
        ]
    });

    assert.equal(single.text, '$(ruby) Ruby: Ready');
    assert.equal(multiAllReady.text, single.text);
});

test('in-progress elapsed keeps ticking from the last accepted snapshot', () => {
    assert.equal(liveElapsedMs({
        phase: 'indexingProject',
        elapsedMs: 3200
    }, 10_000, 10_000), 3200);
    assert.equal(liveElapsedMs({
        phase: 'indexingProject',
        elapsedMs: 3200
    }, 11_250, 10_000), 4450);
    assert.equal(liveElapsedMs({
        phase: 'ready',
        elapsedMs: 12_400
    }, 20_000, 10_000), 12_400);
    assert.equal(liveElapsedMs({
        phase: 'failed',
        elapsedMs: 19_000
    }, 20_000, 10_000), 19_000);
    assert.equal(liveElapsedMs({
        phase: 'indexingProject',
        elapsedMs: 3200
    }, 11_250), 3200);
});

test('status session records when the client accepted a snapshot', () => {
    const session = createIndexingStatusSession();
    assert.equal(session.accept({
        sequence: 1,
        aggregate: { active: 1 },
        projects: [{ root: '/repo/admin', phase: 'indexingProject', elapsedMs: 1000 }]
    }, 50_000), true);
    assert.equal(session.snapshot().receivedAtMs, 50_000);
    assert.equal(session.accept({
        sequence: 1,
        aggregate: { active: 0 },
        projects: [{ root: '/repo/admin', phase: 'ready' }]
    }, 60_000), false);
    assert.equal(session.snapshot().receivedAtMs, 50_000);
});

test('single-project busy omits the project prefix', () => {
    assert.equal(lspStatusBarPresentation({
        root: '/repo/admin',
        indexing: {
            phase: 'indexingProject',
            completed: 2048,
            total: 2658,
            elapsedMs: 12_000
        }
    }, {
        projects: [{
            root: '/repo/admin',
            phase: 'indexingProject',
            completed: 2048,
            total: 2658,
            elapsedMs: 12_000
        }]
    }).text, '$(sync~spin) Ruby: 2048/2658 files');
});

test('multi-project busy names the lead project on the left status bar', () => {
    assert.equal(lspStatusBarPresentation({
        root: '/repo/server',
        indexing: {
            phase: 'indexingProject',
            completed: 2048,
            total: 2658,
            elapsedMs: 12_000
        }
    }, {
        projects: [
            { root: '/repo/admin', phase: 'ready', elapsedMs: 11_000 },
            {
                root: '/repo/server',
                phase: 'indexingProject',
                completed: 2048,
                total: 2658,
                elapsedMs: 12_000
            },
            { root: '/repo/web', phase: 'queued', elapsedMs: 400 }
        ]
    }).text, '$(sync~spin) Ruby (server): 2048/2658 files');
});

test('sibling background work uses the lead project prefix', () => {
    assert.deepEqual(lspStatusBarPresentation({
        root: '/repo/admin',
        indexingComplete: true,
        indexing: { phase: 'ready', elapsedMs: 11_000 }
    }, {
        projects: [
            { root: '/repo/admin', phase: 'ready', elapsedMs: 11_000 },
            {
                root: '/repo/server',
                phase: 'indexingProject',
                completed: 0,
                total: 10,
                elapsedMs: 47_500
            },
            { root: '/repo/web', phase: 'queued', elapsedMs: 400 }
        ]
    }), {
        text: '$(sync~spin) Ruby (server): 0/10 files',
        tooltip: 'server: Indexing 0/10 files — 48s · 2 projects indexing',
        command: 'ruby-fast-lsp.indexing.status'
    });
});

test('the local indexing clock runs only while a generation is in flight', () => {
    assert.equal(indexingClockShouldRun({
        projects: [{ phase: 'indexingProject' }]
    }), true);
    assert.equal(indexingClockShouldRun({
        projects: [{ phase: 'ready' }, { phase: 'queued' }]
    }), true);
    assert.equal(indexingClockShouldRun({
        projects: [{ phase: 'ready' }, { phase: 'failed' }]
    }), false);
    assert.equal(indexingClockShouldRun({ projects: [] }), false);
});

test('project status picker summarizes bounded scheduler state', () => {
    assert.equal(indexingStatusQuickPickPlaceholder({
        aggregate: {
            discovered: 1,
            queued: 2,
            active: 2,
            ready: 3,
            failed: 1,
            cancelled: 0,
            concurrencyLimit: 2
        },
        projects: new Array(7).fill({})
    }), '7 projects · 3 ready · 2 active · 2 queued · 1 failed · workers 2/2');
});

test('project status details join authoritative runtime identity without inferring it', () => {
    const project = {
        root: '/repo/admin',
        generation: 4,
        phase: 'ready',
        elapsedMs: 11_000,
        projectNavigationReadyMs: 2_000,
        dependencyNavigationReadyMs: 8_000
    };
    const runtime = {
        root: '/repo/admin',
        mode: 'auto',
        implementation: 'jruby',
        engineVersion: '9.2.21.0',
        compatibilityVersion: '2.6.8',
        javaHome: '/jdk/17',
        classpathFingerprintSha256: '0123456789abcdef'
    };

    assert.deepEqual(
        indexingStatusQuickPickItems(
            { projects: [project] },
            '/repo/admin',
            [runtime]
        ),
        [{
            label: '$(pass-filled) admin',
            description: 'active · ready · 11s',
            detail: '/repo/admin · runtime Auto → JRuby 9.2.21.0 (Ruby 2.6.8) · JDK /jdk/17 · classpath 0123456789ab · generation 4 · project navigation 2.0s · dependency navigation 8.0s',
            project
        }]
    );
});

test('project status picker reports process cache and single-flight reuse evidence', () => {
    assert.equal(indexingStatusQuickPickPlaceholder({
        aggregate: {
            active: 0,
            ready: 2,
            queued: 0,
            failed: 0,
            concurrencyLimit: 2
        },
        reuse: {
            persistentGemProducts: {
                lookups: 604,
                hits: 604,
                producers: 0,
                corruptions: 0
            },
            persistentJavaArtifacts: {
                lookups: 358,
                hits: 357,
                producers: 1,
                corruptions: 0
            },
            persistentCompiledWasm: {
                lookups: 5,
                hits: 5,
                producers: 0,
                corruptions: 0
            },
            gemSingleFlight: {
                lookups: 604,
                hits: 0,
                joinedFlights: 12,
                producers: 592,
                failures: 0
            },
            classpathFileSingleFlight: {
                lookups: 12,
                hits: 5,
                joinedFlights: 2,
                producers: 5,
                failures: 0
            },
            javaArtifactSingleFlight: {
                lookups: 358,
                hits: 190,
                joinedFlights: 0,
                producers: 168,
                failures: 0
            }
        },
        projects: [{}, {}]
    }), '2 projects · 2 ready · 0 active · 0 queued · 0 failed · workers 0/2 · cache gems 604/604 · cache Java 357/358 · cache extensions 5/5 · reused classpath files 7/12 · reused Java metadata 190/358 · shared gem work 12');
});
