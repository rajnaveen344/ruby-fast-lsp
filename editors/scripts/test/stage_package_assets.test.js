const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { test } = require('node:test');
const { stagePackageAssets } = require('../stage_package_assets');

const REPOSITORY_ROOT = path.resolve(__dirname, '../../..');
const STUB_SERIES = ['common', '9.0', '9.1', '9.2', '9.3', '9.4', '10.0', '10.1'];
const EXTENSIONS = ['rspec-ruby', 'rails-ruby', 'minitest-ruby', 'sinatra-rust', 'cucumber-rust'];
const WASM_PATH = 'target/wasm32-wasip1/release/fixture.wasm';

function write(root, name, content) {
  const filename = path.join(root, name);
  fs.mkdirSync(path.dirname(filename), { recursive: true });
  fs.writeFileSync(filename, content);
}

function fixture(t) {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'package-assets-'));
  t.after(() => fs.rmSync(temporary, { recursive: true, force: true }));
  const root = path.join(temporary, 'source');
  const destination = path.join(temporary, 'package');
  write(root, 'crates/rbs-parser/rbs_types/core/constants.rbs', 'RUBY_ENGINE: String\n');
  fs.cpSync(path.join(REPOSITORY_ROOT, 'support/jruby/decompiler'),
    path.join(root, 'support/jruby/decompiler'), { recursive: true });
  for (const series of STUB_SERIES) {
    write(root, `support/jruby/stubs/${series}/runtime.rb`, `# JRuby ${series}\n`);
  }
  for (const extension of EXTENSIONS) {
    const wasm = Buffer.from(`\0asm\x01\0\0\0${extension}`);
    const checksum = crypto.createHash('sha256').update(wasm).digest('hex');
    write(root, `extensions/${extension}/${WASM_PATH}`, wasm);
    write(root, `extensions/${extension}/README.md`, `# ${extension}\n`);
    write(root, `extensions/${extension}/extension.toml`,
      `id = "${extension}"\nwasm = "${WASM_PATH}"\nchecksum_sha256 = "${checksum}"\n\n[build]\noutput = "${WASM_PATH}"\n`);
    write(root, `extensions/${extension}/src/lib.rs`, 'source must not ship\n');
    write(root, `extensions/${extension}/target/cache.bin`, 'build cache must not ship\n');
  }
  return { root, destination };
}

function files(root) {
  return fs.readdirSync(root, { withFileTypes: true }).flatMap(entry => {
    if (entry.isDirectory()) {
      return files(path.join(root, entry.name)).map(name => `${entry.name}/${name}`);
    }
    return [entry.name];
  }).sort();
}

function replaceWasmPath(root, value) {
  const manifest = path.join(root, 'extensions/rspec-ruby/extension.toml');
  const content = fs.readFileSync(manifest, 'utf8');
  fs.writeFileSync(manifest, content.replace(`wasm = "${WASM_PATH}"`, `wasm = "${value}"`));
}

test('missing core constants are rejected before touching the package', t => {
  const options = fixture(t);
  fs.rmSync(path.join(options.root, 'crates/rbs-parser/rbs_types/core/constants.rbs'));
  assert.throws(() => stagePackageAssets({ ...options, kind: 'npm' }), /constants\.rbs/);
  assert.equal(fs.existsSync(options.destination), false);
});

test('missing CFR license is rejected before touching the package', t => {
  const options = fixture(t);
  fs.rmSync(path.join(options.root, 'support/jruby/decompiler/LICENSE-CFR'));
  assert.throws(() => stagePackageAssets({ ...options, kind: 'npm' }), /LICENSE-CFR/);
  assert.equal(fs.existsSync(options.destination), false);
});

test('changed CFR bytes fail the pinned checksum without replacing existing assets', t => {
  const options = fixture(t);
  write(options.root, 'support/jruby/decompiler/cfr-0.152.jar', 'wrong binary');
  write(options.destination, 'core-rbs/constants.rbs', 'previous verified constants');
  assert.throws(() => stagePackageAssets({ ...options, kind: 'npm' }), /CFR.*checksum/i);
  assert.equal(fs.readFileSync(path.join(options.destination, 'core-rbs/constants.rbs'), 'utf8'),
    'previous verified constants');
});

test('npm stages exact core and CFR bytes and preserves the platform binary', t => {
  const options = fixture(t);
  write(options.destination, 'bin/ruby-fast-lsp', 'platform binary');
  write(options.destination, 'core-rbs/stale.rbs', 'obsolete');
  write(options.destination, 'jruby-decompiler/obsolete.jar', 'obsolete');
  stagePackageAssets({ ...options, kind: 'npm' });
  const decompilerSource = path.join(options.root, 'support/jruby/decompiler');
  const decompilerFiles = files(decompilerSource);
  assert.deepEqual(files(options.destination), [
    'bin/ruby-fast-lsp', 'core-rbs/constants.rbs',
    ...decompilerFiles.map(name => `jruby-decompiler/${name}`),
  ].sort());
  assert.deepEqual(fs.readFileSync(path.join(options.destination, 'core-rbs/constants.rbs')),
    fs.readFileSync(path.join(options.root, 'crates/rbs-parser/rbs_types/core/constants.rbs')));
  for (const name of decompilerFiles) {
    assert.deepEqual(fs.readFileSync(path.join(options.destination, 'jruby-decompiler', name)),
      fs.readFileSync(path.join(decompilerSource, name)), name);
  }
  assert.equal(fs.readFileSync(path.join(options.destination, 'bin/ruby-fast-lsp'), 'utf8'), 'platform binary');
});

test('VSIX includes all required overlays and only manifest, README and declared Wasm per extension', t => {
  const options = fixture(t);
  write(options.destination, 'extensions/obsolete/extension.toml', 'obsolete');
  write(options.destination, 'jruby-stubs/obsolete/runtime.rb', 'obsolete');
  stagePackageAssets({ ...options, kind: 'vsix' });
  assert.deepEqual(files(path.join(options.destination, 'jruby-stubs')),
    STUB_SERIES.map(series => `${series}/runtime.rb`).sort());
  assert.deepEqual(files(path.join(options.destination, 'extensions')),
    EXTENSIONS.flatMap(extension => [
      `${extension}/extension.toml`, `${extension}/README.md`, `${extension}/${WASM_PATH}`,
    ]).sort());
  for (const series of STUB_SERIES) {
    assert.equal(fs.readFileSync(path.join(options.destination, 'jruby-stubs', series, 'runtime.rb'), 'utf8'),
      `# JRuby ${series}\n`);
  }
  for (const extension of EXTENSIONS) {
    for (const name of ['extension.toml', 'README.md', WASM_PATH]) {
      assert.deepEqual(fs.readFileSync(path.join(options.destination, 'extensions', extension, name)),
        fs.readFileSync(path.join(options.root, 'extensions', extension, name)));
    }
  }
  assert.ok(fs.existsSync(path.join(options.destination, 'core-rbs/constants.rbs')));
  assert.ok(fs.existsSync(path.join(options.destination, 'jruby-decompiler/LICENSE-CFR')));
});

for (const wasmPath of ['../escape.wasm', 'target/../../escape.wasm', '/absolute.wasm', 'C:/absolute.wasm']) {
  test(`manifest Wasm path ${JSON.stringify(wasmPath)} is rejected`, t => {
    const options = fixture(t);
    replaceWasmPath(options.root, wasmPath);
    assert.throws(() => stagePackageAssets({ ...options, kind: 'vsix' }), /wasm.*(relative|contain|traversal)/i);
    assert.equal(fs.existsSync(options.destination), false);
  });
}

test('changed extension Wasm fails its manifest checksum before staging', t => {
  const options = fixture(t);
  write(options.root, `extensions/rspec-ruby/${WASM_PATH}`, 'changed wasm');
  assert.throws(() => stagePackageAssets({ ...options, kind: 'vsix' }), /rspec-ruby.*checksum/i);
  assert.equal(fs.existsSync(options.destination), false);
});

test('a missing JRuby series fails rather than creating a partial VSIX', t => {
  const options = fixture(t);
  fs.rmSync(path.join(options.root, 'support/jruby/stubs/10.1/runtime.rb'));
  assert.throws(() => stagePackageAssets({ ...options, kind: 'vsix' }), /10\.1.*runtime\.rb/);
  assert.equal(fs.existsSync(options.destination), false);
});

test('unknown package kind is rejected', t => {
  const options = fixture(t);
  assert.throws(() => stagePackageAssets({ ...options, kind: 'unknown' }), /kind.*npm.*vsix/i);
  assert.equal(fs.existsSync(options.destination), false);
});

test('the repository root cannot be used as the staging destination', t => {
  const { root } = fixture(t);
  assert.throws(() => stagePackageAssets({ root, destination: root, kind: 'vsix' }), /destination.*root/i);
  assert.ok(fs.existsSync(path.join(root, 'extensions/rspec-ruby/extension.toml')));
});
