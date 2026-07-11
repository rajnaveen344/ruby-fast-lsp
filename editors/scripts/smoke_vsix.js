#!/usr/bin/env node

const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawn } = require('child_process');
const AdmZip = require(path.resolve(__dirname, '../vscode/vsix/node_modules/adm-zip'));

const platformKey = `${process.platform}-${process.arch}`;
const vsixPlatforms = {
    'darwin-arm64': 'macos-arm64',
    'darwin-x64': 'macos-x64',
    'linux-x64': 'linux-x64',
    'win32-x64': 'win32-x64'
};
const platform = vsixPlatforms[platformKey];
if (!platform) {
    throw new Error(`Unsupported VSIX smoke-test platform: ${platformKey}`);
}

const artifactArgument = process.argv[2];
if (!artifactArgument) {
    throw new Error('Usage: smoke_vsix.js /path/to/ruby-fast-lsp-VERSION.vsix');
}
const artifact = path.resolve(artifactArgument);
if (!fs.existsSync(artifact)) {
    throw new Error(`VSIX artifact does not exist: ${artifact}`);
}

const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'ruby-fast-lsp-vsix-'));
try {
    new AdmZip(artifact).extractAllTo(temp, true);
} catch (error) {
    fs.rmSync(temp, { recursive: true, force: true });
    throw new Error(`Failed to extract VSIX: ${error.message}`);
}

const extensionRoot = path.join(temp, 'extension');
const binaryName = process.platform === 'win32' ? 'ruby-fast-lsp.exe' : 'ruby-fast-lsp';
const binary = path.join(extensionRoot, 'bin', platform, binaryName);
const rspecPackage = path.join(extensionRoot, 'extensions', 'rspec-ruby');
const railsPackage = path.join(extensionRoot, 'extensions', 'rails-ruby');
for (const required of [
    binary,
    path.join(rspecPackage, 'extension.toml'),
    path.join(railsPackage, 'extension.toml')
]) {
    if (!fs.existsSync(required)) {
        fs.rmSync(temp, { recursive: true, force: true });
        throw new Error(`Packaged VSIX is missing required path: ${required}`);
    }
}
if (process.platform !== 'win32') fs.chmodSync(binary, 0o755);

const childEnv = { ...process.env, RUST_LOG: 'error' };
delete childEnv.RUBY_FAST_LSP_EXTENSION_PATHS;
delete childEnv.RUBY_FAST_LSP_EXTENSION_DIRS;

const child = spawn(binary, [], {
    cwd: temp,
    env: childEnv,
    stdio: ['pipe', 'pipe', 'pipe']
});
let stdout = Buffer.alloc(0);
let stderr = '';
let settled = false;
let initialized = false;
let timer;

function frame(message) {
    const body = JSON.stringify(message);
    return `Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`;
}

function finish(error) {
    if (settled) return;
    settled = true;
    if (timer) clearTimeout(timer);
    child.kill();
    fs.rmSync(temp, { recursive: true, force: true });
    if (error) {
        process.stderr.write(`${error.message}\n${stderr}`);
        process.exitCode = 1;
    } else {
        process.stdout.write(`VSIX initialized Ruby Fast LSP with bundled RSpec and Rails on ${platformKey}.\n`);
    }
}

function handleResponse(response) {
    if (response.id === 1) {
        if (!response.result?.capabilities) {
            return finish(new Error(`Unexpected initialize response: ${JSON.stringify(response)}`));
        }
        initialized = true;
        child.stdin.write(frame({ jsonrpc: '2.0', method: 'initialized', params: {} }));
        child.stdin.write(frame({
            jsonrpc: '2.0',
            id: 2,
            method: 'ruby-fast-lsp/extensions/status',
            params: {}
        }));
        return;
    }
    if (response.id !== 2) return;
    const statuses = response.result?.extensions;
    if (!Array.isArray(statuses)) {
        return finish(new Error(`Unexpected extension status response: ${JSON.stringify(response)}`));
    }
    const rspec = statuses.find(status => status.id === 'rspec-ruby');
    if (!rspec || rspec.status !== 'loaded') {
        return finish(new Error(`Bundled RSpec extension did not load: ${JSON.stringify(statuses)}`));
    }
    const rails = statuses.find(status => status.id === 'rails-ruby');
    if (!rails || rails.status !== 'loaded') {
        return finish(new Error(`Bundled Rails extension did not load: ${JSON.stringify(statuses)}`));
    }
    finish();
}

child.stderr.on('data', chunk => { stderr += chunk.toString(); });
child.on('error', error => finish(error));
child.on('exit', code => {
    if (!settled) {
        finish(new Error(`Packaged Ruby Fast LSP exited before ${initialized ? 'extension status' : 'initialize'} response with status ${code}`));
    }
});
child.stdout.on('data', chunk => {
    stdout = Buffer.concat([stdout, chunk]);
    while (true) {
        const headerEnd = stdout.indexOf('\r\n\r\n');
        if (headerEnd < 0) return;
        const header = stdout.subarray(0, headerEnd).toString();
        const length = Number(header.match(/Content-Length:\s*(\d+)/i)?.[1]);
        if (!Number.isInteger(length)) return finish(new Error('LSP response omitted Content-Length'));
        const bodyStart = headerEnd + 4;
        if (stdout.length < bodyStart + length) return;
        const response = JSON.parse(stdout.subarray(bodyStart, bodyStart + length).toString());
        stdout = stdout.subarray(bodyStart + length);
        handleResponse(response);
        if (settled) return;
    }
});

child.stdin.write(frame({
    jsonrpc: '2.0',
    id: 1,
    method: 'initialize',
    params: {
        capabilities: {},
        initializationOptions: {
            extensionPath: extensionRoot,
            extensionPackages: [rspecPackage, railsPackage],
            extensionDirs: [],
            extensionSettings: {},
            workspaceTrusted: false,
            projectExtensionsEnabled: false
        }
    }
}));

timer = setTimeout(() => {
    finish(new Error(`Timed out waiting for packaged ${initialized ? 'extension status' : 'initialize'} response`));
}, 15000);
