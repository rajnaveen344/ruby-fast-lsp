const assert = require('node:assert/strict');
const test = require('node:test');

const packageManifest = require('../package.json');
const {
    ERB_EXTENSIONS,
    RUBY_EXTENSIONS,
    RUBY_FILENAMES,
    fileWatcherPatterns
} = require('../ruby_file_kinds');

test('VS Code manifest advertises the complete common Ruby and ERB file set', () => {
    const ruby = packageManifest.contributes.languages.find((language) => language.id === 'ruby');
    const erb = packageManifest.contributes.languages.find((language) => language.id === 'erb');

    assert.deepEqual(ruby.extensions, RUBY_EXTENSIONS);
    assert.deepEqual(ruby.filenames, RUBY_FILENAMES);
    assert.deepEqual(erb.extensions, ERB_EXTENSIONS);
});

test('file watcher patterns cover extension and conventional filename changes', () => {
    const patterns = fileWatcherPatterns(['bin/*']);
    assert(patterns.some((pattern) => pattern.includes('thor')));
    assert(patterns.some((pattern) => pattern.includes('jbuilder')));
    assert(patterns.some((pattern) => pattern.includes('Thorfile')));
    assert(patterns.some((pattern) => pattern.includes('Fastfile')));
    assert(patterns.some((pattern) => pattern.includes('rhtml')));
    assert(patterns.includes('bin/*'));
});
