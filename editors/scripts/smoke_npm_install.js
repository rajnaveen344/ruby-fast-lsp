#!/usr/bin/env node
'use strict';

// Exercise npm's actual tarball/install path on every native release target.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const root = path.resolve(__dirname, '../..');
const platform = `${process.platform}-${process.arch}`;
if (!['darwin-arm64', 'darwin-x64', 'linux-x64', 'win32-x64'].includes(platform)) {
  throw new Error(`No published npm package for ${platform}`);
}
const nodeDirectory = path.dirname(process.execPath);
const candidates = [
  process.env.npm_execpath,
  path.join(nodeDirectory, 'node_modules/npm/bin/npm-cli.js'),
  path.resolve(nodeDirectory, '../lib/node_modules/npm/bin/npm-cli.js'),
];
for (const directory of (process.env.PATH || '').split(path.delimiter)) {
  const npm = path.join(directory, 'npm');
  if (fs.existsSync(npm) && fs.statSync(npm).isFile()) candidates.push(fs.realpathSync(npm));
}
const npmCli = candidates.find(candidate => candidate && candidate.endsWith('.js') && fs.existsSync(candidate));
if (!npmCli) throw new Error('npm CLI JavaScript entry point not found alongside Node; install Node with npm.');
const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'ruby-fast-lsp-installed-'));
function npm(args, cwd = temp) {
  return execFileSync(process.execPath, [npmCli, ...args], { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] });
}
try {
  execFileSync(process.execPath, [path.join(root, 'editors/check_package_versions.js')], { stdio: 'inherit' });
  const packages = [platform, 'ruby-fast-lsp'].map(name => {
    const packed = JSON.parse(npm(['pack', '--json', '--pack-destination', temp], path.join(root, 'editors/npm', name)));
    if (packed.length !== 1 || !packed[0].filename) throw new Error(`npm pack produced no unique artifact for ${name}`);
    return path.join(temp, packed[0].filename);
  });
  fs.writeFileSync(path.join(temp, 'package.json'), JSON.stringify({ name: 'release-install-smoke', version: '1.0.0', private: true }));
  npm(['install', '--ignore-scripts', '--no-audit', '--no-fund', ...packages]);
  // Use the installed JavaScript entry directly: Windows .cmd shims cannot be
  // executed by Node as JavaScript and must not require an implicit shell.
  const wrapper = path.join(temp, 'node_modules/@ruby-fast/lsp/bin/ruby-fast-lsp');
  execFileSync(process.execPath, [path.join(__dirname, 'smoke_npm.js'), wrapper, path.join(temp, 'node_modules')], { stdio: 'inherit' });
} finally {
  fs.rmSync(temp, { recursive: true, force: true });
}
