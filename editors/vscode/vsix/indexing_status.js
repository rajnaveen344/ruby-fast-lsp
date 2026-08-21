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
    let receivedAtMs;
    let suspended = false;
    let disposed = false;

    return {
        accept(snapshot, nowMs = Date.now()) {
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
            receivedAtMs = nowMs;
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
            receivedAtMs = undefined;
            suspended = false;
            return true;
        },
        dispose() {
            disposed = true;
            suspended = true;
            aggregate = undefined;
            reuse = undefined;
            projects = [];
            receivedAtMs = undefined;
        },
        snapshot() {
            return {
                sequence,
                aggregate,
                reuse,
                projects,
                receivedAtMs
            };
        }
    };
}

function indexingStatusQuickPickItems(
    snapshot,
    activeProjectRoot,
    runtimeProjects = [],
    nowMs = Date.now()
) {
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
        const elapsed = formatSeconds(liveElapsedMs(project, nowMs, snapshot?.receivedAtMs));
        const progress = project.phase === 'ready' ? '' : indexingProgress(project);
        const active = normalizePath(project.root) === activeRoot;
        const runtime = runtimesByRoot.get(normalizePath(project.root));
        const description = [
            active ? 'active' : undefined,
            `${phase.label}${progress}`,
            elapsed
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

function indexingStatusBarCommand(indexing, snapshot) {
    if (indexing?.phase && indexing.phase !== 'ready') {
        return 'ruby-fast-lsp.indexing.status';
    }
    if (inFlightIndexingProjects(snapshot).length > 0) {
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
            return { icon: '$(clock)', label: 'discovered' };
        case 'queued':
            return { icon: '$(clock)', label: 'queued' };
        case 'resolvingRuntime':
            return { icon: '$(sync~spin)', label: 'runtime' };
        case 'discoveringInputs':
            return { icon: '$(sync~spin)', label: 'inputs' };
        case 'indexingCore':
            return { icon: '$(sync~spin)', label: 'core' };
        case 'indexingProject':
            return { icon: '$(sync~spin)', label: 'indexing' };
        case 'projectNavigationReady':
        case 'indexingDependencies':
            return { icon: '$(sync~spin)', label: 'dependencies' };
        case 'dependencyNavigationReady':
        case 'resolvingSemantics':
            return { icon: '$(sync~spin)', label: 'semantics' };
        case 'publishingDiagnostics':
            return { icon: '$(sync~spin)', label: 'diagnostics' };
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

function indexingFileCounts(project) {
    if (!project || project.completed === undefined || project.completed === null) {
        return '';
    }
    if (project.total === undefined || project.total === null) {
        throw new Error(
            'INVARIANT VIOLATED: indexing status has completed work without a total. '
            + 'This is a bug because project progress needs a stable denominator. '
            + 'Fix: publish both completed and total counters.'
        );
    }
    return `${project.completed}/${project.total}`;
}

function indexingProgress(project) {
    const counts = indexingFileCounts(project);
    return counts ? ` ${counts} files` : '';
}

function indexingBusyPhrase(phase) {
    switch (phase) {
        case undefined:
            return 'Indexing';
        case 'discovered':
            return 'Discovered';
        case 'queued':
            return 'Queued';
        case 'resolvingRuntime':
            return 'Resolving runtime';
        case 'discoveringInputs':
            return 'Discovering inputs';
        case 'indexingCore':
            return 'Indexing core';
        case 'indexingProject':
            return 'Indexing';
        case 'projectNavigationReady':
        case 'indexingDependencies':
            return 'Indexing dependencies';
        case 'dependencyNavigationReady':
        case 'resolvingSemantics':
            return 'Resolving semantics';
        case 'publishingDiagnostics':
            return 'Publishing diagnostics';
        case 'ready':
            return 'Ready';
        case 'failed':
            return 'Failed';
        case 'cancelled':
            return 'Cancelled';
        default:
            return phasePresentation(phase).label;
    }
}

function isTerminalIndexingPhase(phase) {
    return phase === 'ready' || phase === 'failed' || phase === 'cancelled';
}

function liveElapsedMs(indexing, nowMs, receivedAtMs) {
    const snapshotElapsed = Math.max(0, Number(indexing?.elapsedMs || 0));
    if (!indexing || isTerminalIndexingPhase(indexing.phase)) {
        return snapshotElapsed;
    }
    if (!Number.isFinite(nowMs) || !Number.isFinite(receivedAtMs)) {
        return snapshotElapsed;
    }
    return snapshotElapsed + Math.max(0, nowMs - receivedAtMs);
}

function indexingClockShouldRun(snapshot) {
    const projects = Array.isArray(snapshot?.projects) ? snapshot.projects : [];
    return projects.some(project => !isTerminalIndexingPhase(project.phase));
}

function phaseLabel(phase) {
    if (phase === undefined) {
        return 'indexing';
    }
    return phasePresentation(phase).label;
}

function rubyStatusText(icon, message, project) {
    const label = project ? `Ruby (${project})` : 'Ruby';
    return `${icon || '$(ruby)'} ${label}: ${message}`;
}

function lspBusyMessage(phase, counts) {
    if (counts) {
        return `${counts} files`;
    }
    switch (phase) {
        case undefined:
            return 'Indexing';
        case 'discovered':
            return 'Discovered';
        case 'queued':
            return 'Queued';
        case 'resolvingRuntime':
            return 'Runtime';
        case 'discoveringInputs':
            return 'Inputs';
        case 'indexingCore':
            return 'Core';
        case 'indexingProject':
            return 'Indexing';
        case 'projectNavigationReady':
        case 'indexingDependencies':
            return 'Dependencies';
        case 'dependencyNavigationReady':
        case 'resolvingSemantics':
            return 'Semantics';
        case 'publishingDiagnostics':
            return 'Diagnostics';
        default:
            return phasePresentation(phase).label;
    }
}

function lspStatusBarStarting(tooltip = 'Determining the owning project') {
    return {
        text: rubyStatusText('$(sync~spin)', 'Starting'),
        tooltip,
        command: 'ruby-fast-lsp.indexing.status'
    };
}

function lspStatusBarError(tooltip) {
    return {
        text: rubyStatusText('$(warning)', 'Error'),
        tooltip,
        command: 'ruby-fast-lsp.runtime.configure'
    };
}

function lspStatusBarPresentation(status, snapshot, nowMs, receivedAtMs) {
    if (!status) {
        return {
            text: rubyStatusText(undefined, 'No project'),
            tooltip: 'The active document is not owned by a discovered Ruby project',
            command: 'ruby-fast-lsp.runtime.configure'
        };
    }
    const project = projectName(status.root);
    const phase = status.indexing?.phase;
    if (phase === 'failed') {
        return {
            text: rubyStatusText('$(warning)', 'Failed'),
            tooltip: `${project}: ${status.indexing.failure || 'Indexing failed'}`,
            command: 'ruby-fast-lsp.indexing.status'
        };
    }
    if (phase === 'cancelled') {
        return {
            text: rubyStatusText('$(clock)', 'Cancelled'),
            tooltip: `${project}: indexing was cancelled`,
            command: 'ruby-fast-lsp.indexing.status'
        };
    }
    const indexingBar = indexingStatusBarPresentation(
        status,
        snapshot,
        nowMs,
        receivedAtMs
    );
    if (indexingBar) {
        return {
            ...indexingBar,
            command: 'ruby-fast-lsp.indexing.status'
        };
    }
    return {
        text: rubyStatusText(undefined, 'Ready'),
        tooltip: `${project}: indexed and ready`,
        command: 'ruby-fast-lsp.indexing.status'
    };
}

function indexingStatusBarPresentation(status, snapshot, nowMs, receivedAtMs) {
    const inFlight = inFlightIndexingProjects(snapshot, status?.root);
    let lead = leadIndexingProject(inFlight, status?.root);
    let inFlightCount = inFlight.length;
    if (!lead && status && indexingStatusIsInFlight(status)) {
        lead = { ...(status.indexing || { elapsedMs: 0 }), root: status.root };
        inFlightCount = 1;
    }
    if (!lead) {
        return undefined;
    }
    const name = projectName(lead.root);
    const counts = indexingFileCounts(lead);
    const elapsedMs = liveElapsedMs(lead, nowMs, receivedAtMs);
    const message = lspBusyMessage(lead.phase, counts);
    const phrase = indexingBusyPhrase(lead.phase);
    const countLabel = counts ? ` ${counts} files` : '';
    const more = inFlightCount > 1 ? ` · ${inFlightCount} projects indexing` : '';
    const project = indexingStatusProjectPrefix(snapshot, lead);
    return {
        text: rubyStatusText('$(sync~spin)', message, project),
        tooltip: `${name}: ${phrase}${countLabel} — ${formatSeconds(elapsedMs)}${more}`
    };
}

function indexingStatusIsInFlight(status) {
    const phase = status.indexing?.phase;
    if (isTerminalIndexingPhase(phase)) {
        return false;
    }
    if (phase === undefined && status.indexingComplete) {
        return false;
    }
    return Boolean(status.indexing) || status.indexingComplete === false;
}

function inFlightIndexingProjects(snapshot, activeProjectRoot) {
    const projects = Array.isArray(snapshot?.projects) ? [...snapshot.projects] : [];
    const inFlight = projects.filter(project => !isTerminalIndexingPhase(project.phase));
    const activeRoot = normalizePath(activeProjectRoot);
    inFlight.sort((left, right) => {
        const leftRoot = normalizePath(left?.root);
        const rightRoot = normalizePath(right?.root);
        const leftActive = leftRoot === activeRoot;
        const rightActive = rightRoot === activeRoot;
        if (leftActive !== rightActive) {
            return leftActive ? -1 : 1;
        }
        return leftRoot < rightRoot ? -1 : leftRoot > rightRoot ? 1 : 0;
    });
    return inFlight;
}

function leadIndexingProject(inFlight, activeProjectRoot) {
    if (inFlight.length === 0) {
        return undefined;
    }
    const activeRoot = normalizePath(activeProjectRoot);
    const active = inFlight.find(project => normalizePath(project.root) === activeRoot);
    if (active) {
        return active;
    }
    const withCounts = inFlight.find(project => indexingFileCounts(project));
    if (withCounts) {
        return withCounts;
    }
    const running = inFlight.find(project => (
        project.phase !== 'queued' && project.phase !== 'discovered'
    ));
    return running || inFlight[0];
}

function indexingStatusProjectPrefix(snapshot, lead) {
    const projects = Array.isArray(snapshot?.projects) ? snapshot.projects : [];
    if (projects.length <= 1) {
        return undefined;
    }
    return projectName(lead.root);
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
    indexingClockShouldRun,
    indexingProgress,
    indexingStatusBarPresentation,
    lspStatusBarError,
    lspStatusBarPresentation,
    lspStatusBarStarting,
    indexingStatusBarCommand,
    indexingStatusQuickPickPlaceholder,
    indexingStatusQuickPickItems,
    indexingStatusRequestParams,
    liveElapsedMs,
    phaseLabel,
    phasePresentation
};
