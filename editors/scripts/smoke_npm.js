#!/usr/bin/env node

const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawn } = require('child_process');

const root = path.resolve(__dirname, '../..');
const platformKey = `${process.platform}-${process.arch}`;
const packageDirs = {
    'darwin-arm64': 'darwin-arm64',
    'darwin-x64': 'darwin-x64',
    'linux-x64': 'linux-x64',
    'win32-x64': 'win32-x64'
};
const packageDir = packageDirs[platformKey];
if (!packageDir) {
    throw new Error(`Unsupported npm smoke-test platform: ${platformKey}`);
}

const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'ruby-fast-lsp-npm-'));
const wrapperArgument = process.argv[2];
let wrapper;
let moduleRoot;
if (wrapperArgument) {
    wrapper = path.resolve(wrapperArgument);
    moduleRoot = path.resolve(path.dirname(wrapper), '..');
} else {
    const scope = path.join(temp, 'node_modules', '@ruby-fast');
    fs.mkdirSync(scope, { recursive: true });
    fs.symlinkSync(path.join(root, 'editors/npm', packageDir), path.join(scope, `lsp-${packageDir}`), 'dir');
    wrapper = path.join(root, 'editors/npm/ruby-fast-lsp/bin/ruby-fast-lsp');
    moduleRoot = path.join(temp, 'node_modules');
}
const child = spawn(process.execPath, [wrapper, '--stdio'], {
    cwd: temp,
    env: { ...process.env, NODE_PATH: moduleRoot },
    stdio: ['pipe', 'pipe', 'pipe']
});
let stdout = Buffer.alloc(0);
let stderr = '';
let settled = false;
let timer;

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
        process.stdout.write(`npm wrapper initialized Ruby Fast LSP on ${platformKey}.\n`);
    }
}

child.stderr.on('data', chunk => { stderr += chunk.toString(); });
child.on('error', error => finish(error));
child.on('exit', code => {
    if (!settled) finish(new Error(`npm wrapper exited before initialize response with status ${code}`));
});
child.stdout.on('data', chunk => {
    stdout = Buffer.concat([stdout, chunk]);
    const headerEnd = stdout.indexOf('\r\n\r\n');
    if (headerEnd < 0) return;
    const header = stdout.subarray(0, headerEnd).toString();
    const length = Number(header.match(/Content-Length:\s*(\d+)/i)?.[1]);
    if (!Number.isInteger(length)) return finish(new Error('LSP response omitted Content-Length'));
    const bodyStart = headerEnd + 4;
    if (stdout.length < bodyStart + length) return;
    const response = JSON.parse(stdout.subarray(bodyStart, bodyStart + length).toString());
    if (response.id !== 1 || !response.result?.capabilities) {
        return finish(new Error(`Unexpected initialize response: ${JSON.stringify(response)}`));
    }
    finish();
});

const request = JSON.stringify({
    jsonrpc: '2.0',
    id: 1,
    method: 'initialize',
    params: { capabilities: {} }
});
child.stdin.write(`Content-Length: ${Buffer.byteLength(request)}\r\n\r\n${request}`);

timer = setTimeout(() => {
    finish(new Error('Timed out waiting for npm wrapper initialize response'));
}, 10000);
