#!/usr/bin/env node
'use strict';

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn, spawnSync } = require('node:child_process');
const AdmZip = require(path.resolve(__dirname, '../vscode/vsix/node_modules/adm-zip'));

async function main() {
  const [argument, code = 'code', ...extra] = process.argv.slice(2);
  assert.ok(argument && !extra.length, 'Usage: node smoke_vscode.js <VSIX path> [code executable]');
  const root = path.resolve(__dirname, '../..');
  const artifact = path.resolve(argument);
  const runId = crypto.randomUUID();
  const evidenceRoot = path.resolve(process.env.RUBY_FAST_LSP_EVIDENCE_DIR || path.join(root, 'target/release-evidence'));
  const evidence = path.join(evidenceRoot, `vscode-${new Date().toISOString().replace(/[:.]/g, '-')}-${runId.slice(0, 8)}`);
  fs.mkdirSync(evidence, { recursive: true });
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'ruby-fast-lsp-vscode-'));
  const userData = path.join(temporary, 'user-data');
  const hostResult = path.join(evidence, 'host-results.json');
  const reportPath = path.join(evidence, 'vscode.json');
  const report = { schema_version: 1, run_id: runId, started_at: new Date().toISOString(), artifact, code_executable: code, platform: `${process.platform}-${process.arch}`, status: 'running' };
  const writeReport = () => fs.writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n');
  let launched = false;
  let terminal = false;
  let terminationError;
  try {
    assert.ok(fs.statSync(artifact).isFile(), `VSIX must be a regular file: ${artifact}`);
    const artifactBytes = fs.readFileSync(artifact);
    report.artifact_sha256 = crypto.createHash('sha256').update(artifactBytes).digest('hex');
    const environment = { ...process.env };
    for (const name of ['RUBY_FAST_LSP_EXTENSION_PATHS', 'RUBY_FAST_LSP_EXTENSION_DIRS', 'VSCODE_IPC_HOOK_CLI', 'ELECTRON_RUN_AS_NODE']) delete environment[name];
    const version = spawnSync(code, ['--version'], { encoding: 'utf8', env: environment, timeout: 15000 });
    fs.writeFileSync(path.join(evidence, 'code-version.log'), `${version.stdout || ''}${version.stderr || ''}`);
    assert.ok(version.status === 0 && !version.error, `Cannot read VS Code version: ${version.error?.message || version.stderr || version.status}`);
    report.code_version = version.stdout.trim();
    assert.ok(report.code_version, 'VS Code --version returned no version evidence; fix the executable before running editor checks.');

    const extraction = path.join(temporary, 'archive');
    new AdmZip(artifactBytes).extractAllTo(extraction, true);
    const extensionRoot = path.join(extraction, 'extension');
    const manifest = JSON.parse(fs.readFileSync(path.join(extensionRoot, 'package.json'), 'utf8'));
    assert.ok(manifest.publisher && manifest.name && manifest.main, 'The extracted VSIX must contain its extension identity and entry point.');
    assert.ok(fs.statSync(path.join(extensionRoot, manifest.main)).isFile(), 'The exact VSIX is missing its extension entry point.');
    const platforms = { 'darwin-arm64': 'macos-arm64', 'darwin-x64': 'macos-x64', 'linux-x64': 'linux-x64', 'linux-arm64': 'linux-arm64', 'win32-x64': 'win32-x64' };
    const platform = platforms[report.platform];
    assert.ok(platform, `Unsupported VSIX editor-test platform: ${report.platform}`);
    const binaryName = process.platform === 'win32' ? 'ruby-fast-lsp.exe' : 'ruby-fast-lsp';
    const binary = [report.platform, platform].map(name => path.join(extensionRoot, 'bin', name, binaryName)).find(filename => fs.existsSync(filename));
    assert.ok(binary && fs.statSync(binary).isFile(), `The exact VSIX is missing its ${platform} server binary.`);
    if (process.platform !== 'win32') fs.chmodSync(binary, 0o755);
    report.extension_id = `${manifest.publisher}.${manifest.name}`;
    report.extension_version = manifest.version;

    const fixture = path.join(temporary, 'fixture');
    fs.mkdirSync(fixture);
    fs.mkdirSync(path.join(userData, 'User'), { recursive: true });
    fs.writeFileSync(path.join(userData, 'User/settings.json'), JSON.stringify({
      'files.autoSave': 'off', 'workbench.startupEditor': 'none', 'telemetry.telemetryLevel': 'off',
      'extensions.autoUpdate': false, 'extensions.autoCheckUpdates': false, 'update.mode': 'none',
    }));
    fs.writeFileSync(path.join(fixture, 'service.rb'), 'class Service\n  def value\n    "ready"\n  end\nend\n');
    fs.writeFileSync(path.join(fixture, 'main.rb'), 'require_relative "service"\nService.new.value\n');
    fs.writeFileSync(path.join(fixture, 'view.html.erb'), '<section><%= Service.new.value %></section>\n');
    fs.writeFileSync(path.join(fixture, 'completion.html.erb'), '<sec');

    const args = [
      `--user-data-dir=${userData}`, `--extensions-dir=${path.join(temporary, 'extensions')}`,
      `--extensionDevelopmentPath=${extensionRoot}`, `--extensionTestsPath=${path.join(__dirname, 'vscode_release_test.js')}`,
      '--disable-workspace-trust', '--skip-welcome', '--skip-release-notes', '--new-window', '--wait', '--verbose', fixture,
    ];
    report.arguments = args;
    writeReport();
    const fd = fs.openSync(path.join(evidence, 'code.log'), 'w');
    let outcome;
    try {
      outcome = await new Promise((resolve, reject) => {
        const child = spawn(code, args, {
          env: { ...environment, RUBY_FAST_LSP_VSCODE_RESULT: hostResult, RUBY_FAST_LSP_VSCODE_RUN_ID: runId,
            RUBY_FAST_LSP_VSCODE_FIXTURE: fixture, RUBY_FAST_LSP_VSCODE_EXTENSION: report.extension_id,
            RUBY_FAST_LSP_VSCODE_EXTENSION_ROOT: extensionRoot },
          stdio: ['ignore', fd, fd],
        });
        launched = true;
        let timedOut = false;
        const timer = setTimeout(() => {
          timedOut = true;
          // The macOS CLI launches a detached app through `open`. Target only
          // processes carrying this run's unique isolated directory, never a
          // user's ordinary VS Code instance.
          if (process.platform === 'win32') {
            const killed = spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { encoding: 'utf8', timeout: 10000 });
            if (killed.status !== 0) terminationError = killed.stderr || 'taskkill failed';
          } else {
            const processes = spawnSync('ps', ['-ax', '-o', 'pid=,command='], { encoding: 'utf8', timeout: 5000 });
            if (processes.status !== 0) terminationError = processes.stderr || 'Could not inspect isolated editor processes';
            else for (const line of processes.stdout.split('\n')) {
              const match = line.match(/^\s*(\d+)\s+(.*)$/);
              if (match && match[2].includes(`--user-data-dir=${userData}`)) {
                try { process.kill(Number(match[1]), 'SIGKILL'); }
                catch (error) { if (error.code !== 'ESRCH') terminationError = error.message; }
              }
            }
          }
          child.kill('SIGKILL');
        }, 180000);
        child.once('error', error => { clearTimeout(timer); terminal = true; reject(error); });
        child.once('close', (status, signal) => { clearTimeout(timer); terminal = true; resolve({ exit_code: status, signal, timed_out: timedOut }); });
      });
    } finally {
      fs.closeSync(fd);
    }
    Object.assign(report, outcome);
    assert.ok(!outcome.timed_out, 'Isolated VS Code exceeded the 180-second deadline; inspect code.log and retained editor logs.');
    assert.ok(outcome.exit_code === 0, `Isolated VS Code exited with ${outcome.exit_code} (${outcome.signal || 'no signal'}); inspect code.log.`);
    assert.ok(fs.existsSync(hostResult), 'VS Code exited without extension-host results; no editor workflow can be claimed as passed.');
    const result = JSON.parse(fs.readFileSync(hostResult, 'utf8'));
    assert.equal(result.run_id, runId, 'Extension-host evidence must belong to this exact isolated run.');
    report.checks = result.checks;
    report.unexercised = result.unexercised;
    assert.ok(result.status === 'passed' && result.checks.length > 0 && result.checks.every(check => check.status === 'passed'), 'One or more real editor checks failed; inspect host-results.json.');
    report.status = 'passed';
    console.log(`Isolated VS Code checks passed. Evidence: ${reportPath}`);
  } catch (error) {
    report.status = 'failed';
    report.error = error.stack || error.message;
    if (fs.existsSync(hostResult)) {
      const result = JSON.parse(fs.readFileSync(hostResult, 'utf8'));
      report.checks = result.checks;
      report.unexercised = result.unexercised;
    }
    console.error(`${error.message}\nEvidence: ${reportPath}`);
    process.exitCode = 1;
  } finally {
    const editorLogs = path.join(userData, 'logs');
    if (fs.existsSync(editorLogs)) fs.cpSync(editorLogs, path.join(evidence, 'editor-logs'), { recursive: true });
    if (terminationError) report.termination_error = terminationError;
    if ((!launched || terminal) && !terminationError) fs.rmSync(temporary, { recursive: true, force: true });
    else report.retained_temporary_directory = temporary;
    report.completed_at = new Date().toISOString();
    writeReport();
  }
}

if (require.main === module) main().catch(error => { console.error(error.stack); process.exitCode = 1; });
