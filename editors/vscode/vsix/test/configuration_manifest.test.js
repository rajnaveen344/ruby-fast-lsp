const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const manifest = JSON.parse(
    fs.readFileSync(path.join(__dirname, '..', 'package.json'), 'utf8')
);
const extensionSource = fs.readFileSync(
    path.join(__dirname, '..', 'extension.js'),
    'utf8'
);

test('the Settings page exposes only log level', () => {
    assert.deepEqual(
        Object.keys(manifest.contributes.configuration.properties),
        ['rubyFastLsp.logLevel']
    );
});

test('editor-managed product choices have commands instead of JSON settings', () => {
    const commands = new Set(
        manifest.contributes.commands.map(command => command.command)
    );
    assert(commands.has('ruby-fast-lsp.runtime.select'));
    assert(commands.has('ruby-fast-lsp.runtime.configure'));
    assert(commands.has('ruby-fast-lsp.indexing.status'));
    assert(commands.has('ruby-fast-lsp.linter.select'));
    assert(commands.has('ruby-fast-lsp.formatter.select'));
    assert(commands.has('rubyIndex.toggleExternalTypes'));
});

test('one right-hand status item renders structured indexing state', () => {
    const statusItems = extensionSource.match(/createStatusBarItem\(/g) || [];
    assert.equal(statusItems.length, 1);
    assert.match(extensionSource, /StatusBarAlignment\.Right/);
    assert.match(extensionSource, /ruby-fast-lsp\/indexing\/statusChanged/);
    assert.doesNotMatch(extensionSource, /onNotification\(['"]\\$\/progress/);
    assert.doesNotMatch(extensionSource, /statusBarItem\.hide/);
});

test('every language-client restart resets the authoritative indexing session', () => {
    const directRestarts = extensionSource.match(/await client\.restart\(\)/g) || [];
    const governedRestarts = extensionSource.match(
        /await restartClientWithFreshIndexingStatus\(\)/g
    ) || [];

    assert.equal(
        directRestarts.length,
        1,
        'client.restart must exist only inside the governed restart helper'
    );
    assert.equal(governedRestarts.length, 3);
    assert.match(extensionSource, /indexingStatusSession\.suspendForRestart\(\)/);
    assert.match(extensionSource, /indexingStatusSession\.completeRestart\(\)/);
    assert.match(extensionSource, /indexingStatusSession\.dispose\(\)/);
});
