const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');

test('Linux ARM64 npm launcher executes its native package with the original arguments', () => {
  const wrapper = path.resolve(__dirname, '../../npm/ruby-fast-lsp/bin/ruby-fast-lsp');
  const manifest = path.resolve(__dirname, '../../npm/linux-arm64/package.json');
  const binary = path.join(path.dirname(manifest), 'bin/ruby-fast-lsp');
  const calls = [];
  const environment = { SAMPLE_SETTING: 'preserved' };
  const fakeRequire = name => ({
    path,
    fs: {
      existsSync: filename => filename === binary,
      chmodSync: (filename, mode) => calls.push(['chmod', filename, mode]),
    },
    child_process: { execFileSync: (...args) => calls.push(['exec', ...args]) },
  })[name];
  fakeRequire.resolve = name => {
    assert.equal(name, '@ruby-fast/lsp-linux-arm64/package.json');
    return manifest;
  };
  vm.runInNewContext(fs.readFileSync(wrapper, 'utf8'), {
    require: fakeRequire,
    console,
    process: {
      platform: 'linux', arch: 'arm64', argv: ['node', wrapper, '--stdio'],
      env: environment,
      exit: code => { throw new Error(`launcher unexpectedly exited: ${code}`); },
    },
  }, { filename: wrapper });
  assert.equal(calls.length, 2);
  assert.deepEqual(calls[0], ['chmod', binary, 0o755]);
  assert.equal(calls[1][0], 'exec');
  assert.equal(calls[1][1], binary);
  assert.deepEqual(Array.from(calls[1][2]), ['--stdio']);
  assert.equal(calls[1][3].stdio, 'inherit');
  assert.equal(calls[1][3].env, environment);

  const platformPackage = JSON.parse(fs.readFileSync(manifest, 'utf8'));
  assert.equal(platformPackage.name, '@ruby-fast/lsp-linux-arm64');
  assert.deepEqual(platformPackage.os, ['linux']);
  assert.deepEqual(platformPackage.cpu, ['arm64']);
});
