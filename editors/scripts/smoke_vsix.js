#!/usr/bin/env node

const fs = require('fs');
const crypto = require('crypto');
const os = require('os');
const path = require('path');
const { spawn } = require('child_process');
const AdmZip = require(path.resolve(__dirname, '../vscode/vsix/node_modules/adm-zip'));
const {
    runPackagedJrubyNavigationSmoke
} = require('./smoke_jruby_navigation');

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
const minitestPackage = path.join(extensionRoot, 'extensions', 'minitest-ruby');
const sinatraPackage = path.join(extensionRoot, 'extensions', 'sinatra-rust');
const cucumberPackage = path.join(extensionRoot, 'extensions', 'cucumber-rust');
const cfrJar = path.join(extensionRoot, 'jruby-decompiler', 'cfr-0.152.jar');
for (const required of [
    binary,
    path.join(extensionRoot, 'erb_html.js'),
    path.join(extensionRoot, 'ruby_file_kinds.js'),
    path.join(extensionRoot, 'ruby_file_kinds.json'),
    path.join(extensionRoot, 'runtime_selector.js'),
    path.join(extensionRoot, 'configuration_state.js'),
    path.join(extensionRoot, 'core-rbs', 'constants.rbs'),
    cfrJar,
    path.join(extensionRoot, 'jruby-decompiler', 'LICENSE-CFR'),
    path.join(extensionRoot, 'jruby-decompiler', 'README.md'),
    ...['common', '9.0', '9.1', '9.2', '9.3', '9.4', '10.0', '10.1'].map(
        series => path.join(extensionRoot, 'jruby-stubs', series, 'runtime.rb')
    ),
    path.join(rspecPackage, 'extension.toml'),
    path.join(railsPackage, 'extension.toml'),
    path.join(railsPackage, 'target', 'wasm32-wasip1', 'release', 'ruby_fast_lsp_rails_extension.wasm'),
    path.join(minitestPackage, 'extension.toml'),
    path.join(minitestPackage, 'target', 'wasm32-wasip1', 'release', 'ruby_fast_lsp_minitest_extension.wasm'),
    path.join(sinatraPackage, 'extension.toml'),
    path.join(cucumberPackage, 'extension.toml')
]) {
    if (!fs.existsSync(required)) {
        fs.rmSync(temp, { recursive: true, force: true });
        throw new Error(`Packaged VSIX is missing required path: ${required}`);
    }
}
const cfrSha256 = crypto.createHash('sha256').update(fs.readFileSync(cfrJar)).digest('hex');
if (cfrSha256 !== 'f686e8f3ded377d7bc87d216a90e9e9512df4156e75b06c655a16648ae8765b2') {
    fs.rmSync(temp, { recursive: true, force: true });
    throw new Error(`Packaged CFR checksum mismatch: ${cfrSha256}`);
}
if (process.platform !== 'win32') fs.chmodSync(binary, 0o755);

const { createErbHtmlDocument } = require(path.join(extensionRoot, 'erb_html.js'));
const fileKinds = require(path.join(extensionRoot, 'ruby_file_kinds.js'));
const packagedManifest = require(path.join(extensionRoot, 'package.json'));
const packagedSettings = Object.keys(packagedManifest.contributes.configuration.properties);
const packagedCommands = new Set(
    packagedManifest.contributes.commands.map(command => command.command)
);
if (JSON.stringify(packagedSettings) !== JSON.stringify(['rubyFastLsp.logLevel']) ||
    !packagedCommands.has('ruby-fast-lsp.runtime.configure')) {
    fs.rmSync(temp, { recursive: true, force: true });
    throw new Error('Packaged VSIX runtime controls or minimal Settings surface drifted');
}
const packagedRuby = packagedManifest.contributes.languages.find(language => language.id === 'ruby');
const packagedErbLanguage = packagedManifest.contributes.languages.find(language => language.id === 'erb');
if (JSON.stringify(packagedRuby?.extensions) !== JSON.stringify(fileKinds.RUBY_EXTENSIONS) ||
    JSON.stringify(packagedRuby?.filenames) !== JSON.stringify(fileKinds.RUBY_FILENAMES) ||
    JSON.stringify(packagedErbLanguage?.extensions) !== JSON.stringify(fileKinds.ERB_EXTENSIONS)) {
    fs.rmSync(temp, { recursive: true, force: true });
    throw new Error('Packaged VSIX Ruby/ERB language declarations drifted from canonical file kinds');
}
const packagedErb = createErbHtmlDocument(
    'file:///app/views/users/show.html.erb',
    1,
    '<main><section cl></section><%= User.name %></main>'
);
if (!packagedErb.complete({ line: 0, character: 17 }).items.some(item => item.label === 'class')) {
    fs.rmSync(temp, { recursive: true, force: true });
    throw new Error('Packaged ERB HTML service did not return host-language completion');
}
if (packagedErb.complete({ line: 0, character: 38 }).items.length !== 0) {
    fs.rmSync(temp, { recursive: true, force: true });
    throw new Error('Packaged ERB HTML service leaked completion into a Ruby region');
}

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
let navigationStarted = false;
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
        process.stdout.write(`VSIX initialized Ruby Fast LSP with bundled RSpec, Rails, Minitest, Sinatra, and Cucumber, packaged ERB HTML features, and verified JRuby implementation navigation on ${platformKey}.\n`);
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
    const minitest = statuses.find(status => status.id === 'minitest-ruby');
    if (!minitest || minitest.status !== 'loaded') {
        return finish(new Error(`Bundled Minitest extension did not load: ${JSON.stringify(statuses)}`));
    }
    const sinatra = statuses.find(status => status.id === 'sinatra-rust');
    if (!sinatra || sinatra.status !== 'loaded') {
        return finish(new Error(`Bundled Sinatra extension did not load: ${JSON.stringify(statuses)}`));
    }
    const cucumber = statuses.find(status => status.id === 'cucumber-rust');
    if (!cucumber || cucumber.status !== 'loaded') {
        return finish(new Error(`Bundled Cucumber extension did not load: ${JSON.stringify(statuses)}`));
    }
    navigationStarted = true;
    if (timer) {
        clearTimeout(timer);
        timer = undefined;
    }
    runPackagedJrubyNavigationSmoke({
        command: binary,
        label: 'Packaged VSIX Ruby Fast LSP'
    }).then(() => finish()).catch(error => finish(error));
}

child.stderr.on('data', chunk => { stderr += chunk.toString(); });
child.on('error', error => finish(error));
child.on('exit', code => {
    if (!settled && !navigationStarted) {
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
            extensionPackages: [rspecPackage, railsPackage, minitestPackage, sinatraPackage, cucumberPackage],
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
