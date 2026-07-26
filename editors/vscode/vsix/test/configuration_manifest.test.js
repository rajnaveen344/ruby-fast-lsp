const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const manifest = JSON.parse(
    fs.readFileSync(path.join(__dirname, '..', 'package.json'), 'utf8')
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
    assert(commands.has('ruby-fast-lsp.linter.select'));
    assert(commands.has('ruby-fast-lsp.formatter.select'));
    assert(commands.has('rubyIndex.toggleExternalTypes'));
});
