'use strict';

function indexingStatusRequestParams(editor) {
    const uri = editor?.document?.uri;
    if (!uri || typeof uri.toString !== 'function') {
        return {};
    }
    return { activeDocumentUri: uri.toString() };
}

function acceptNewerIndexingSnapshot(currentSequence, snapshot) {
    if (!Number.isSafeInteger(currentSequence) || currentSequence < 0) {
        throw new Error('current indexing status sequence must be a non-negative safe integer');
    }
    if (!Number.isSafeInteger(snapshot?.sequence)
        || snapshot.sequence <= currentSequence) {
        return undefined;
    }
    const accepted = {
        sequence: snapshot.sequence,
        aggregate: snapshot.aggregate,
        projects: Array.isArray(snapshot.projects) ? snapshot.projects : []
    };
    if (snapshot.reuse !== undefined) {
        accepted.reuse = snapshot.reuse;
    }
    return accepted;
}

function createIndexingStatusSession() {
    let sequence = 0;
    let aggregate;
    let reuse;
    let projects = [];
    let suspended = false;
    let disposed = false;

    return {
        accept(snapshot) {
            if (suspended || disposed) {
                return false;
            }
            const accepted = acceptNewerIndexingSnapshot(sequence, snapshot);
            if (!accepted) {
                return false;
            }
            sequence = accepted.sequence;
            aggregate = accepted.aggregate;
            reuse = accepted.reuse;
            projects = accepted.projects;
            return true;
        },
        suspendForRestart() {
            if (disposed) {
                throw new Error(
                    'INVARIANT VIOLATED: disposed indexing status session was restarted. '
                    + 'This is a bug because disposed editor state cannot own a new server transport. '
                    + 'Fix: create a new extension activation session.'
                );
            }
            if (suspended) {
                throw new Error(
                    'INVARIANT VIOLATED: indexing status session was suspended twice. '
                    + 'This is a bug because concurrent client restarts need one shared owner. '
                    + 'Fix: coalesce restart callers before suspending status publication.'
                );
            }
            suspended = true;
        },
        completeRestart() {
            if (disposed) {
                return false;
            }
            if (!suspended) {
                throw new Error(
                    'INVARIANT VIOLATED: indexing status restart completed without suspension. '
                    + 'This is a bug because old-server notifications could cross the sequence reset. '
                    + 'Fix: suspend the status session before restarting the language client.'
                );
            }
            sequence = 0;
            aggregate = undefined;
            reuse = undefined;
            projects = [];
            suspended = false;
            return true;
        },
        dispose() {
            disposed = true;
            suspended = true;
            aggregate = undefined;
            reuse = undefined;
            projects = [];
        },
        snapshot() {
            return {
                sequence,
                aggregate,
                reuse,
                projects
            };
        }
    };
}

function indexingStatusQuickPickItems(snapshot, activeProjectRoot, runtimeProjects = []) {
    const activeRoot = normalizePath(activeProjectRoot);
    const runtimesByRoot = new Map(
        runtimeProjects.map(runtime => [normalizePath(runtime?.root), runtime])
    );
    const projects = Array.isArray(snapshot?.projects) ? [...snapshot.projects] : [];
    projects.sort((left, right) => {
        const leftRoot = normalizePath(left?.root);
        const rightRoot = normalizePath(right?.root);
        const leftActive = leftRoot === activeRoot;
        const rightActive = rightRoot === activeRoot;
        if (leftActive !== rightActive) {
            return leftActive ? -1 : 1;
        }
        return leftRoot < rightRoot ? -1 : leftRoot > rightRoot ? 1 : 0;
    });

    return projects.map(project => {
        const phase = phasePresentation(project.phase);
        const elapsed = formatSeconds(project.elapsedMs);
        const progress = project.phase === 'ready' ? '' : indexingProgress(project);
        const target = phase.targetSeconds === undefined
            ? ''
            : ` / ${phase.targetSeconds}s`;
        const active = normalizePath(project.root) === activeRoot;
        const runtime = runtimesByRoot.get(normalizePath(project.root));
        const description = [
            active ? 'active' : undefined,
            `${phase.label}${progress}`,
            `${elapsed}${target}`
        ].filter(Boolean).join(' · ');
        const details = [
            String(project.root),
            runtimePresentation(runtime),
            runtime?.javaHome ? `JDK ${runtime.javaHome}` : undefined,
            runtime?.classpathFingerprintSha256
                ? `classpath ${runtime.classpathFingerprintSha256.slice(0, 12)}`
                : undefined,
            `generation ${project.generation}`,
            navigationMilestone('project navigation', project.projectNavigationReadyMs),
            navigationMilestone('dependency navigation', project.dependencyNavigationReadyMs),
            project.failure || undefined
        ].filter(Boolean);
        return {
            label: `${phase.icon} ${projectName(project.root)}`,
            description,
            detail: details.join(' · '),
            project
        };
    });
}

function indexingStatusBarCommand(indexing) {
    if (indexing?.phase && indexing.phase !== 'ready') {
        return 'ruby-fast-lsp.indexing.status';
    }
    return 'ruby-fast-lsp.runtime.configure';
}

function indexingStatusQuickPickPlaceholder(snapshot) {
    const aggregate = snapshot?.aggregate || {};
    const projects = Array.isArray(snapshot?.projects) ? snapshot.projects.length : 0;
    const active = nonNegativeInteger(aggregate.active);
    const concurrencyLimit = nonNegativeInteger(aggregate.concurrencyLimit);
    const details = [
        `${projects} ${projects === 1 ? 'project' : 'projects'}`,
        `${nonNegativeInteger(aggregate.ready)} ready`,
        `${active} active`,
        `${nonNegativeInteger(aggregate.queued)} queued`,
        `${nonNegativeInteger(aggregate.failed)} failed`,
        `workers ${active}/${concurrencyLimit}`
    ];
    const reuse = snapshot?.reuse;
    appendPersistentReuse(
        details,
        'gems',
        reuse?.persistentGemProducts
    );
    appendPersistentReuse(
        details,
        'Java',
        reuse?.persistentJavaArtifacts
    );
    appendPersistentReuse(
        details,
        'extensions',
        reuse?.persistentCompiledWasm
    );
    appendSingleFlightReuse(
        details,
        'classpath files',
        reuse?.classpathFileSingleFlight
    );
    appendSingleFlightReuse(
        details,
        'Java metadata',
        reuse?.javaArtifactSingleFlight
    );
    const joinedFlights = nonNegativeInteger(reuse?.gemSingleFlight?.joinedFlights);
    if (joinedFlights > 0) {
        details.push(`shared gem work ${joinedFlights}`);
    }
    const corruptions = nonNegativeInteger(reuse?.persistentGemProducts?.corruptions)
        + nonNegativeInteger(reuse?.persistentJavaArtifacts?.corruptions)
        + nonNegativeInteger(reuse?.persistentCompiledWasm?.corruptions);
    if (corruptions > 0) {
        details.push(`cache rebuilds ${corruptions}`);
    }
    const failures = nonNegativeInteger(reuse?.gemSingleFlight?.failures)
        + nonNegativeInteger(reuse?.classpathFileSingleFlight?.failures)
        + nonNegativeInteger(reuse?.javaArtifactSingleFlight?.failures);
    if (failures > 0) {
        details.push(`shared failures ${failures}`);
    }
    return details.join(' · ');
}

function appendSingleFlightReuse(details, label, counters) {
    const lookups = nonNegativeInteger(counters?.lookups);
    if (lookups === 0) {
        return;
    }
    const reused = nonNegativeInteger(counters?.hits)
        + nonNegativeInteger(counters?.joinedFlights);
    details.push(`reused ${label} ${reused}/${lookups}`);
}

function appendPersistentReuse(details, label, counters) {
    const lookups = nonNegativeInteger(counters?.lookups);
    if (lookups === 0) {
        return;
    }
    details.push(`cache ${label} ${nonNegativeInteger(counters?.hits)}/${lookups}`);
}

function runtimePresentation(runtime) {
    if (!runtime) {
        return undefined;
    }
    const mode = runtime.mode === 'auto'
        ? 'Auto'
        : runtime.mode === 'explicit'
            ? 'Explicit'
            : runtime.mode === 'legacy'
                ? 'Legacy'
                : String(runtime.mode || 'Runtime');
    if (!runtime.implementation) {
        return `runtime ${mode} (unresolved)`;
    }
    const implementation = runtime.implementation === 'jruby'
        ? 'JRuby'
        : runtime.implementation === 'truffleruby'
            ? 'TruffleRuby'
            : runtime.implementation === 'mri'
                ? 'MRI'
                : String(runtime.implementation);
    const engine = runtime.engineVersion
        ? `${implementation} ${runtime.engineVersion}`
        : implementation;
    const compatibility = runtime.compatibilityVersion
        && runtime.implementation !== 'mri'
        ? ` (Ruby ${runtime.compatibilityVersion})`
        : '';
    return `runtime ${mode} → ${engine}${compatibility}`;
}

function phasePresentation(phase) {
    switch (phase) {
        case 'discovered':
            return { icon: '$(clock)', label: 'discovered', targetSeconds: 5 };
        case 'queued':
            return { icon: '$(clock)', label: 'queued', targetSeconds: 5 };
        case 'resolvingRuntime':
            return { icon: '$(sync~spin)', label: 'runtime', targetSeconds: 5 };
        case 'discoveringInputs':
            return { icon: '$(sync~spin)', label: 'inputs', targetSeconds: 5 };
        case 'indexingCore':
            return { icon: '$(sync~spin)', label: 'core', targetSeconds: 5 };
        case 'indexingProject':
            return { icon: '$(sync~spin)', label: 'project', targetSeconds: 5 };
        case 'projectNavigationReady':
        case 'indexingDependencies':
            return { icon: '$(sync~spin)', label: 'dependencies', targetSeconds: 15 };
        case 'dependencyNavigationReady':
        case 'resolvingSemantics':
            return { icon: '$(sync~spin)', label: 'semantics', targetSeconds: 15 };
        case 'publishingDiagnostics':
            return { icon: '$(sync~spin)', label: 'diagnostics', targetSeconds: 15 };
        case 'ready':
            return { icon: '$(pass-filled)', label: 'ready' };
        case 'failed':
            return { icon: '$(error)', label: 'failed' };
        case 'cancelled':
            return { icon: '$(circle-slash)', label: 'cancelled' };
        default:
            throw new Error(
                `INVARIANT VIOLATED: unknown indexing phase '${phase}'. `
                + 'This is a bug because the editor cannot present an unrecognized server phase. '
                + 'Fix: add the server phase to the authoritative indexing status presentation.'
            );
    }
}

function indexingProgress(project) {
    if (project.completed === undefined || project.completed === null) {
        return '';
    }
    if (project.total === undefined || project.total === null) {
        throw new Error(
            'INVARIANT VIOLATED: indexing status has completed work without a total. '
            + 'This is a bug because project progress needs a stable denominator. '
            + 'Fix: publish both completed and total counters.'
        );
    }
    return ` ${project.completed}/${project.total}`;
}

function navigationMilestone(label, milliseconds) {
    return milliseconds === undefined || milliseconds === null
        ? `${label} pending`
        : `${label} ${formatSeconds(milliseconds)}`;
}

function formatSeconds(milliseconds) {
    const seconds = Math.max(0, Number(milliseconds || 0) / 1000);
    return seconds < 10 ? `${seconds.toFixed(1)}s` : `${Math.round(seconds)}s`;
}

function projectName(root) {
    const segments = normalizePath(root).split('/').filter(Boolean);
    return segments.at(-1) || 'Ruby project';
}

function normalizePath(value) {
    return String(value || '').replaceAll('\\', '/').replace(/\/$/, '');
}

function nonNegativeInteger(value) {
    return Number.isSafeInteger(value) && value >= 0 ? value : 0;
}

module.exports = {
    acceptNewerIndexingSnapshot,
    createIndexingStatusSession,
    indexingStatusBarCommand,
    indexingStatusQuickPickPlaceholder,
    indexingStatusQuickPickItems,
    indexingStatusRequestParams
};
