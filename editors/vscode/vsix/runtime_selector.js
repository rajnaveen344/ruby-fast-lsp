'use strict';

async function selectRuntime({ window, client, applySelection, preferredProjectRoot }) {
    const catalog = await client.sendRequest('ruby-fast-lsp/runtime/discover', {});
    const projects = Array.isArray(catalog?.projects) ? catalog.projects : [];
    if (projects.length === 0) {
        await window.showErrorMessage('Ruby Fast LSP did not discover any Ruby projects.');
        return undefined;
    }

    let project = projects.find(candidate => candidate.root === preferredProjectRoot) || projects[0];
    if (projects.length > 1 && project.root !== preferredProjectRoot) {
        const selected = await window.showQuickPick(
            projects.map(candidate => ({
                label: candidate.label,
                description: candidate.root,
                project: candidate
            })),
            {
                title: 'Ruby Fast LSP: Select Project',
                placeHolder: 'Choose the Ruby project whose runtime should change'
            }
        );
        if (!selected) {
            return undefined;
        }
        project = selected.project;
    }

    const runtimes = Array.isArray(project.runtimes) ? project.runtimes : [];
    const advertisedImplementations = Array.isArray(project.implementations)
        ? project.implementations
        : runtimes.map(runtime => ({
            id: runtime.implementation,
            label: runtime.implementationLabel
        }));
    const implementations = uniqueBy(
        advertisedImplementations,
        item => item.id
    );
    const implementation = await window.showQuickPick(
        [
            {
                label: 'Auto',
                description: 'Use project files and the active runtime environment',
                id: 'auto'
            },
            ...implementations
        ],
        {
            title: `Ruby Fast LSP: Runtime for ${project.label}`,
            placeHolder: 'Choose a Ruby implementation'
        }
    );
    if (!implementation) {
        return undefined;
    }

    let selection;
    if (implementation.id === 'auto') {
        selection = {
            projectRoot: project.root,
            mode: 'auto'
        };
    } else {
        const implementationRuntimes = runtimes.filter(
            runtime => runtime.implementation === implementation.id
        );
        const families = uniqueBy(
            implementationRuntimes.map(runtime => ({
                id: runtime.family,
                label: runtime.familyLabel,
                description: runtime.supportStatus === 'supported'
                    ? runtime.compatibilityLabel
                    : `${runtime.compatibilityLabel} — unsupported`,
                supportStatus: runtime.supportStatus
            })),
            family => family.id
        );
        if (families.length === 0) {
            await window.showErrorMessage(
                `No ${implementation.label} installations were discovered for ${project.label}.`
            );
            return undefined;
        }
        const family = await window.showQuickPick(families, {
            title: `Ruby Fast LSP: ${implementation.label} Release Family`,
            placeHolder: 'Choose the implementation family and Ruby compatibility series'
        });
        if (!family) {
            return undefined;
        }
        if (family.supportStatus !== 'supported') {
            await window.showErrorMessage(
                `${family.label} is not supported yet. Ruby Fast LSP will not substitute a nearby compatibility model.`
            );
            return undefined;
        }

        const installations = implementationRuntimes
            .filter(runtime => runtime.family === family.id)
            .map(runtime => ({
                label: runtime.displayName,
                description: runtime.executable,
                detail: runtime.javaHome ? `JDK: ${runtime.javaHome}` : undefined,
                runtime
            }));
        if (installations.length === 0) {
            await window.showErrorMessage(
                `No ${family.label} installations were discovered for ${project.label}.`
            );
            return undefined;
        }
        const installation = await window.showQuickPick(installations, {
            title: `Ruby Fast LSP: Exact ${family.label} Installation`,
            placeHolder: 'Choose an installed runtime'
        });
        if (!installation) {
            return undefined;
        }
        selection = {
            projectRoot: project.root,
            mode: 'explicit',
            runtime: canonicalRuntime(installation.runtime)
        };
    }

    const summary = selection.mode === 'auto'
        ? `${project.label} → Auto`
        : `${project.label} → ${installationDisplayName(selection.runtime)} → ${selection.runtime.javaHome || 'No JDK'}`;
    const confirmation = await window.showQuickPick(
        [{ label: 'Apply Runtime', description: summary, selection }],
        {
            title: 'Ruby Fast LSP: Confirm Runtime',
            placeHolder: 'Confirm the effective project, runtime, and JDK'
        }
    );
    if (!confirmation) {
        return undefined;
    }
    await applySelection(confirmation.selection);
    return confirmation.selection;
}

function canonicalRuntime(runtime) {
    return {
        implementation: runtime.implementation,
        family: runtime.family,
        engineVersion: runtime.engineVersion,
        compatibilityVersion: runtime.compatibilityVersion,
        executable: runtime.executable,
        discoverySource: runtime.discoverySource,
        javaHome: runtime.javaHome
    };
}

function installationDisplayName(runtime) {
    if (runtime.implementation === 'jruby') {
        return `JRuby ${runtime.engineVersion} (Ruby ${runtime.compatibilityVersion})`;
    }
    if (runtime.implementation === 'truffleruby') {
        return `TruffleRuby ${runtime.engineVersion} (Ruby ${runtime.compatibilityVersion})`;
    }
    return `MRI ${runtime.engineVersion}`;
}

function runtimeStatusItem(status) {
    const project = status.root ? status.root.split(/[\\/]/).filter(Boolean).pop() : 'Ruby project';
    const implementation = status.implementation === 'jruby'
        ? `JRuby ${status.engineVersion} (Ruby ${status.compatibilityVersion})`
        : status.implementation === 'truffleruby'
            ? `TruffleRuby ${status.engineVersion} (Ruby ${status.compatibilityVersion})`
            : status.implementation === 'mri'
                ? `MRI ${status.engineVersion || status.compatibilityVersion}`
                : 'Auto';
    const details = [];
    if (status.executable) details.push(status.executable);
    if (status.javaHome) details.push(`JDK ${status.javaHome}`);
    if (status.stubOverlay) details.push(`overlay ${status.stubOverlay}`);
    if (status.classpathFingerprintSha256) {
        details.push(`classpath ${status.classpathFingerprintSha256.slice(0, 12)}`);
    }
    details.push(status.indexingComplete ? 'ready' : 'indexing');
    return {
        label: `${project} → ${implementation}`,
        description: details.join(' · '),
        detail: status.root
    };
}

function runtimeStatusForDocument(projects, documentPath) {
    const normalizedDocument = normalizePath(documentPath);
    return projects
        .filter(status => {
            const root = normalizePath(status.root);
            return normalizedDocument === root || normalizedDocument.startsWith(`${root}/`);
        })
        .sort((left, right) => normalizePath(right.root).length - normalizePath(left.root).length)[0];
}

function runtimeStatusPresentation(status) {
    const project = status.root
        ? normalizePath(status.root).split('/').filter(Boolean).pop()
        : 'Ruby project';
    const ready = status.indexingComplete ? 'ready' : 'indexing';
    const icon = status.indexingComplete ? '$(ruby)' : '$(sync~spin)';
    const runtime = shortRuntimeIdentity(status);
    if (status.mode === 'auto') {
        return {
            text: `${icon} ${runtime === 'Auto' ? 'Auto' : `Auto: ${runtime}`}`,
            tooltip: `${project}: ${runtime === 'Auto' ? 'Auto runtime detection' : `Auto → ${runtime}`} — ${ready}`
        };
    }
    return {
        text: `${icon} ${runtime}`,
        tooltip: `${project}: ${detailedRuntimeIdentity(status)} — ${ready}`
    };
}

function runtimeVersionMarker(status) {
    if (!status.engineVersion) {
        return undefined;
    }
    if (status.implementation === 'jruby') {
        return `jruby-${status.engineVersion}`;
    }
    if (status.implementation === 'truffleruby') {
        return `truffleruby-${status.engineVersion}`;
    }
    if (status.implementation === 'mri') {
        return status.engineVersion;
    }
    return undefined;
}

function shortRuntimeIdentity(status) {
    const version = status.engineVersion || status.compatibilityVersion;
    if (status.implementation === 'jruby') {
        return version ? `JRuby ${version}` : 'JRuby';
    }
    if (status.implementation === 'truffleruby') {
        return version ? `TruffleRuby ${version}` : 'TruffleRuby';
    }
    if (status.implementation === 'mri') {
        return version ? `MRI ${version}` : 'MRI';
    }
    return 'Auto';
}

function detailedRuntimeIdentity(status) {
    const identity = shortRuntimeIdentity(status);
    if (status.implementation !== 'mri' && status.compatibilityVersion) {
        return `${identity} (Ruby ${status.compatibilityVersion})`;
    }
    return identity;
}

function normalizePath(value) {
    return String(value || '').replaceAll('\\', '/').replace(/\/$/, '');
}

function uniqueBy(values, key) {
    const seen = new Set();
    return values.filter(value => {
        const identity = key(value);
        if (seen.has(identity)) {
            return false;
        }
        seen.add(identity);
        return true;
    });
}

module.exports = {
    runtimeStatusForDocument,
    runtimeStatusItem,
    runtimeStatusPresentation,
    runtimeVersionMarker,
    selectRuntime
};
