#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..');
const cargo = fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8');
const packageSection = cargo.match(/^\[package\]\s*$([\s\S]*?)(?=^\[|\z)/m);
if (!packageSection) {
    throw new Error('INVARIANT VIOLATED: Cargo.toml has no [package] section. Fix the root manifest.');
}
const cargoVersion = packageSection[1].match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
if (!cargoVersion) {
    throw new Error('INVARIANT VIOLATED: root Cargo package has no literal version. Fix Cargo.toml.');
}

const manifests = [
    'editors/vscode/vsix/package.json',
    'editors/npm/ruby-fast-lsp/package.json',
    'editors/npm/darwin-arm64/package.json',
    'editors/npm/darwin-x64/package.json',
    'editors/npm/linux-x64/package.json',
    'editors/npm/win32-x64/package.json'
];
const errors = [];
for (const relative of manifests) {
    const manifest = JSON.parse(fs.readFileSync(path.join(root, relative), 'utf8'));
    if (manifest.version !== cargoVersion) {
        errors.push(`${relative}: expected ${cargoVersion}, found ${manifest.version}`);
    }
}

const npmRoot = JSON.parse(
    fs.readFileSync(path.join(root, 'editors/npm/ruby-fast-lsp/package.json'), 'utf8')
);
for (const [name, version] of Object.entries(npmRoot.optionalDependencies ?? {})) {
    if (version !== cargoVersion) {
        errors.push(`editors/npm/ruby-fast-lsp/package.json: ${name} expected ${cargoVersion}, found ${version}`);
    }
}

const vsixLock = JSON.parse(
    fs.readFileSync(path.join(root, 'editors/vscode/vsix/package-lock.json'), 'utf8')
);
if (vsixLock.version !== cargoVersion || vsixLock.packages?.['']?.version !== cargoVersion) {
    errors.push('editors/vscode/vsix/package-lock.json: root versions do not match Cargo');
}

if (errors.length > 0) {
    process.stderr.write(`Package version mismatch:\n${errors.map(error => `- ${error}`).join('\n')}\n`);
    process.exit(1);
}

process.stdout.write(`All distribution manifests match ${cargoVersion}.\n`);
