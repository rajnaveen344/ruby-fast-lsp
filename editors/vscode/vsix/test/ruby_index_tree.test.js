'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const {
    projectRootLabel,
    orderRubyIndexProjects,
    buildWorkspaceProjectForest,
    projectTreePresentation,
    namespaceSearchQuickPickItems,
    projectBrowseSections,
    librarySectionsFromResponse,
    namespaceChildDescriptors,
    namespaceHasChildren
} = require('../ruby_index_tree');

test('project root label uses the final path component', () => {
    assert.equal(projectRootLabel('/Users/naveenraj/goshposh/server'), 'server');
    assert.equal(projectRootLabel('/Users/naveenraj/goshposh/server/'), 'server');
    assert.equal(projectRootLabel('C:\\repo\\admin'), 'admin');
});

test('project roots stay in stable path order', () => {
    const ordered = orderRubyIndexProjects(
        [
            { root: '/repo/admin', phase: 'ready' },
            { root: '/repo/server', phase: 'ready' },
            { root: '/repo/worker', phase: 'indexingProject' }
        ],
        '/repo/server/lib/platform/prospect_posts.rb'
    );
    assert.deepEqual(ordered.map((project) => project.root), [
        '/repo/admin',
        '/repo/server',
        '/repo/worker'
    ]);
    assert.equal(ordered[1].label, 'server');
});

test('single workspace nests projects at tree root without folder wrapper', () => {
    const forest = buildWorkspaceProjectForest(
        [
            { root: '/Users/naveenraj/goshposh/admin', phase: 'ready' },
            { root: '/Users/naveenraj/goshposh/server', phase: 'ready' },
            { root: '/Users/naveenraj/goshposh/devops/tools/capistrano', phase: 'ready' },
            { root: '/Users/naveenraj/goshposh/devops/tools/service-flags', phase: 'ready' },
            { root: '/Users/naveenraj/goshposh/pm-loggers-pm_logger', phase: 'ready' }
        ],
        ['/Users/naveenraj/goshposh']
    );

    assert.deepEqual(forest.map((node) => `${node.kind}:${node.label}`), [
        'project:admin',
        'pathFolder:devops',
        'project:pm-loggers-pm_logger',
        'project:server'
    ]);
    const devops = forest.find((node) => node.label === 'devops');
    assert.equal(devops.children[0].label, 'tools');
    assert.deepEqual(
        devops.children[0].children.map((node) => node.label),
        ['capistrano', 'service-flags']
    );
});

test('multi-workspace keeps each folder as a container', () => {
    const forest = buildWorkspaceProjectForest(
        [
            { root: '/ws/a/server', phase: 'ready' },
            { root: '/ws/b/admin', phase: 'ready' }
        ],
        ['/ws/a', '/ws/b']
    );
    assert.deepEqual(forest.map((node) => `${node.kind}:${node.label}`), [
        'workspace:a',
        'workspace:b'
    ]);
    assert.equal(forest[0].children[0].label, 'server');
    assert.equal(forest[1].children[0].label, 'admin');
});

test('single workspace Gemfile project collapses to one project root', () => {
    const forest = buildWorkspaceProjectForest(
        [{ root: '/repo/server', phase: 'ready' }],
        ['/repo/server']
    );
    assert.equal(forest.length, 1);
    assert.equal(forest[0].kind, 'project');
    assert.equal(forest[0].path, '/repo/server');
});

test('projects outside workspace folders remain top-level orphans', () => {
    const forest = buildWorkspaceProjectForest(
        [
            { root: '/repo/server', phase: 'ready' },
            { root: '/other/orphan', phase: 'ready' }
        ],
        ['/repo']
    );
    assert.equal(forest[0].kind, 'project');
    assert.equal(forest[0].label, 'server');
    assert.equal(forest[1].kind, 'project');
    assert.equal(forest[1].label, 'orphan');
});

test('active project gets a target icon and active description', () => {
    const presentation = projectTreePresentation(
        { root: '/repo/server', phase: 'ready' },
        '/repo/server/app/models/user.rb'
    );
    assert.equal(presentation.active, true);
    assert.equal(presentation.iconId, 'target');
    assert.equal(presentation.description, 'ready · active');

    const indexing = projectTreePresentation(
        { root: '/repo/admin', phase: 'indexingProject' },
        '/repo/server/app.rb'
    );
    assert.equal(indexing.active, false);
    assert.equal(indexing.iconId, 'sync~spin');
    assert.equal(indexing.description, 'indexing…');
});

test('namespace search QuickPick is Ctrl+P style with active project first', () => {
    const items = namespaceSearchQuickPickItems([
        { fqn: 'Admin::User', name: 'User', kind: 'Class', projectRoot: '/repo/admin' },
        {
            fqn: 'GoshPosh::Platform::API::ProspectPosts',
            name: 'ProspectPosts',
            kind: 'Class',
            projectRoot: '/repo/server'
        },
        { fqn: 'GoshPosh::App', name: 'App', kind: 'Module', projectRoot: '/repo/server' }
    ], { activeProjectRoot: '/repo/server' });

    assert.equal(items[0].label, 'GoshPosh::App');
    assert.equal(items[1].label, 'GoshPosh::Platform::API::ProspectPosts');
    assert.equal(items[2].label, 'Admin::User');
    assert.equal(items[0].description, 'Module · server');
});

test('project browse sections split runtime and gems like JRE vs Maven', () => {
    const sections = projectBrowseSections({
        modules: [{ fqn: 'GoshPosh', modules: [{ fqn: 'GoshPosh::Platform' }] }],
        classes: [{ fqn: 'User' }],
        libraries: [
            {
                id: 'runtime',
                modules: [],
                classes: [{ fqn: 'String' }],
                packages: []
            },
            {
                id: 'gems',
                modules: [],
                classes: [],
                packages: [
                    {
                        name: 'activesupport',
                        version: '7.1.0',
                        modules: [],
                        classes: [{ fqn: 'String' }]
                    },
                    {
                        name: 'auth',
                        version: '1.0.0',
                        modules: [{ fqn: 'Auth' }],
                        classes: []
                    }
                ]
            }
        ],
        external_modules: [{ fqn: 'Auth' }],
        external_classes: [{ fqn: 'String' }]
    }, true);

    assert.deepEqual(sections.projectNamespaces.map((ns) => ns.fqn), [
        'GoshPosh',
        'User'
    ]);
    assert.deepEqual(sections.librarySections.map((section) => section.label), [
        'Ruby Standard Library',
        'Gems'
    ]);
    assert.deepEqual(sections.librarySections[0].namespaces.map((ns) => ns.fqn), ['String']);
    assert.deepEqual(
        sections.librarySections[1].packages.map((packageInfo) => packageInfo.label),
        ['activesupport 7.1.0', 'auth 1.0.0']
    );
    assert.deepEqual(
        sections.librarySections[1].packages[0].namespaces.map((ns) => ns.fqn),
        ['String']
    );
    assert.equal(sections.hasLibraries, true);

    const hidden = projectBrowseSections({
        modules: [{ fqn: 'GoshPosh' }],
        classes: [],
        libraries: [
            {
                id: 'gems',
                modules: [{ fqn: 'Auth' }],
                classes: [],
                packages: []
            }
        ]
    }, false);
    assert.equal(hidden.hasLibraries, false);
    assert.deepEqual(hidden.librarySections, []);
    assert.deepEqual(hidden.libraryNamespaces, []);
});

test('legacy flat external payload becomes one libraries section', () => {
    const sections = librarySectionsFromResponse({
        external_modules: [{ fqn: 'Auth' }],
        external_classes: [{ fqn: 'String' }]
    }, true);
    assert.equal(sections.length, 1);
    assert.equal(sections[0].label, 'Libraries');
    assert.deepEqual(sections[0].namespaces.map((ns) => ns.fqn), ['Auth', 'String']);
});

test('namespace children list nested types before mixin chrome', () => {
    const children = namespaceChildDescriptors({
        fqn: 'GoshPosh',
        modules: [{ fqn: 'GoshPosh::Platform', name: 'Platform' }],
        classes: [{ fqn: 'GoshPosh::App', name: 'App' }],
        superclass: { name: 'Object', locations: [] },
        includes: [{ name: 'Kernel', locations: [] }],
        prepends: [{ name: 'Pre', locations: [] }],
        singleton_class: { name: '#<Class:GoshPosh>', includes: [] },
        included_by: [{ name: 'Host', locations: [], via_modules: [] }]
    });

    assert.deepEqual(children.map((child) => child.kind), [
        'namespace',
        'namespace',
        'mixinSection',
        'mixinSection',
        'mixinSection',
        'singleton',
        'includedBySection'
    ]);
    assert.equal(children[0].namespace.fqn, 'GoshPosh::Platform');
    assert.equal(children[1].namespace.fqn, 'GoshPosh::App');
    assert.equal(children[2].label, 'Superclass');
    assert.equal(children[3].label, 'Includes');
    assert.equal(children[6].label, 'Included By');
    assert.equal(namespaceHasChildren({ modules: [{ fqn: 'A' }] }), true);
    assert.equal(namespaceHasChildren({ modules: [], classes: [] }), false);
});
