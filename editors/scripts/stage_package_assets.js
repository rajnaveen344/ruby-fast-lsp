#!/usr/bin/env node
'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

const CFR_SHA256 = 'f686e8f3ded377d7bc87d216a90e9e9512df4156e75b06c655a16648ae8765b2';
const STUB_SERIES = ['common', '9.0', '9.1', '9.2', '9.3', '9.4', '10.0', '10.1'];
const EXTENSIONS = ['rspec-ruby', 'rails-ruby', 'minitest-ruby', 'sinatra-rust', 'cucumber-rust'];

function artifactError(name, reason) {
  return new Error(`Invalid package artifact ${name}: ${reason}. Fix: restore or rebuild the required repository asset before packaging.`);
}

function isContained(root, filename) {
  const relative = path.relative(root, filename);
  return relative !== '..' && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative);
}

function readAsset(root, relative) {
  const filename = path.join(root, relative);
  if (!fs.existsSync(filename) || !fs.lstatSync(filename).isFile()) {
    throw artifactError(relative, 'required regular file is missing');
  }
  if (!isContained(root, fs.realpathSync(filename))) {
    throw artifactError(relative, 'source path must remain contained in the repository root');
  }
  return fs.readFileSync(filename);
}

function readDirectory(root, relative) {
  const directory = path.join(root, relative);
  if (!fs.existsSync(directory) || !fs.lstatSync(directory).isDirectory()) {
    throw artifactError(relative, 'required directory is missing');
  }
  if (!isContained(root, fs.realpathSync(directory))) {
    throw artifactError(relative, 'source directory must remain contained in the repository root');
  }
  return fs.readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))
    .flatMap(entry => {
      const source = path.join(relative, entry.name);
      if (entry.isDirectory()) {
        return readDirectory(root, source).map(file => ({ ...file, name: path.join(entry.name, file.name) }));
      }
      return [{ name: entry.name, bytes: readAsset(root, source) }];
    });
}

function verifyChecksum(name, bytes, expected) {
  const actual = crypto.createHash('sha256').update(bytes).digest('hex');
  if (actual !== expected) {
    throw artifactError(name, `SHA-256 checksum mismatch: expected ${expected}, got ${actual}`);
  }
}

// The checked-in manifests use single-line, unescaped double-quoted literals.
// Read only these two top-level fields; this is deliberately not a TOML parser.
function manifestLiteral(manifest, name, field) {
  const header = manifest.split(/^\s*\[/m, 1)[0];
  const lines = header.split(/\r?\n/).filter(line => new RegExp(`^\\s*${field}\\s*=`).test(line));
  const literal = lines.length === 1 && lines[0].match(new RegExp(`^\\s*${field}\\s*=\\s*"([^"\\\\\\r\\n]*)"\\s*(?:#.*)?$`));
  if (!literal) {
    throw artifactError(name, `${field} must be exactly one top-level unescaped string literal`);
  }
  return literal[1];
}

function extensionAssets(root, extension) {
  const base = path.join('extensions', extension);
  const manifestName = path.join(base, 'extension.toml');
  const manifest = readAsset(root, manifestName);
  const wasm = manifestLiteral(manifest.toString('utf8'), manifestName, 'wasm');
  const checksum = manifestLiteral(manifest.toString('utf8'), manifestName, 'checksum_sha256');
  if (!wasm || path.posix.isAbsolute(wasm) || path.win32.parse(wasm).root
      || wasm.includes('\\') || wasm.includes('\0') || wasm.split('/').some(part => !part || part === '..' || part === '.')) {
    throw artifactError(manifestName, 'wasm must be a contained relative path without traversal');
  }
  if (!/^[a-f0-9]{64}$/.test(checksum)) {
    throw artifactError(manifestName, 'checksum_sha256 must contain exactly 64 lowercase hexadecimal digits');
  }
  const wasmName = path.join(base, wasm);
  const wasmBytes = readAsset(root, wasmName);
  if (!isContained(fs.realpathSync(path.join(root, base)), fs.realpathSync(path.join(root, wasmName)))) {
    throw artifactError(wasmName, 'wasm must remain contained in its extension package');
  }
  verifyChecksum(wasmName, wasmBytes, checksum);
  return [
    { name: manifestName, bytes: manifest },
    { name: path.join(base, 'README.md'), bytes: readAsset(root, path.join(base, 'README.md')) },
    { name: wasmName, bytes: wasmBytes },
  ];
}

function stagePackageAssets({ root, destination, kind }) {
  if (kind !== 'npm' && kind !== 'vsix') {
    throw new Error(`Invalid package kind ${JSON.stringify(kind)}. Fix: choose npm or vsix.`);
  }
  const sourceRoot = fs.realpathSync(root);
  const destinationRoot = fs.existsSync(destination) ? fs.realpathSync(destination) : path.resolve(destination);
  if (destinationRoot === sourceRoot) {
    throw new Error('Invalid package destination: the repository root cannot be overwritten. Fix: choose a separate staging directory.');
  }

  const assets = [{
    name: 'core-rbs/constants.rbs',
    bytes: readAsset(sourceRoot, 'crates/rbs-parser/rbs_types/core/constants.rbs'),
  }];
  const decompiler = readDirectory(sourceRoot, 'support/jruby/decompiler');
  // Validate required files explicitly, even if other files exist in this directory.
  readAsset(sourceRoot, 'support/jruby/decompiler/LICENSE-CFR');
  const cfr = decompiler.find(file => file.name === 'cfr-0.152.jar');
  if (!cfr) {
    throw artifactError('CFR cfr-0.152.jar', 'required pinned decompiler is missing');
  }
  verifyChecksum('CFR cfr-0.152.jar', cfr.bytes, CFR_SHA256);
  assets.push(...decompiler.map(file => ({ ...file, name: path.join('jruby-decompiler', file.name) })));

  const directories = ['core-rbs', 'jruby-decompiler'];
  if (kind === 'vsix') {
    for (const series of STUB_SERIES) {
      const source = path.join('support/jruby/stubs', series);
      readAsset(sourceRoot, path.join(source, 'runtime.rb'));
      assets.push(...readDirectory(sourceRoot, source)
        .map(file => ({ ...file, name: path.join('jruby-stubs', series, file.name) })));
    }
    for (const extension of EXTENSIONS) {
      assets.push(...extensionAssets(sourceRoot, extension));
    }
    directories.push('jruby-stubs', 'extensions');
  }

  // Validate and retain the exact bytes first, so invalid inputs cannot erase an
  // existing staged package or change between checksum verification and copying.
  fs.mkdirSync(destinationRoot, { recursive: true });
  for (const directory of directories) {
    fs.rmSync(path.join(destinationRoot, directory), { recursive: true, force: true });
  }
  for (const asset of assets) {
    const filename = path.join(destinationRoot, asset.name);
    fs.mkdirSync(path.dirname(filename), { recursive: true });
    fs.writeFileSync(filename, asset.bytes);
  }
}

module.exports = { stagePackageAssets };

if (require.main === module) {
  const [kind, destination, ...extra] = process.argv.slice(2);
  if (!destination || extra.length) {
    console.error('Usage: node stage_package_assets.js <npm|vsix> DEST');
    process.exitCode = 1;
  } else {
    try {
      stagePackageAssets({ root: path.resolve(__dirname, '../..'), destination, kind });
      console.log(`Validated and staged ${kind} runtime assets in ${path.resolve(destination)}`);
    } catch (error) {
      console.error(error.message);
      process.exitCode = 1;
    }
  }
}
