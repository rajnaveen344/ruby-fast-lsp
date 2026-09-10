#!/usr/bin/env node
'use strict';

// The same commands run locally and in CI. Output stays in durable per-command
// logs; a missing optional corpus is reported explicitly, never as a pass.
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const root = path.resolve(__dirname, '../..');

// These are separate checks, never quarantined regressions. All other ignored
// tests (including doctests) fail the gate until they are made executable.
const deferredRustChecks = new Map([
  ['test::simulation::tests::generated_project_large_scale_smoke', { gate: 'large-scale-lsp', reason: 'release gate: explicitly run the large-scale sampled LSP simulation' }],
  ['test::simulation::tests::generated_project_large_scale_engine_checks_all_edges', { gate: 'large-scale-engine', reason: 'release gate: explicitly run the large-scale all-edge engine simulation' }],
  ['test::simulation::tests::generated_project_real_corpus_smoke', { gate: 'real-corpus', reason: 'requires an explicitly selected read-only real corpus via SIM_REAL_CORPUS_ROOT' }],
]);

// A successful process is insufficient evidence: an exact Rust filter can
// silently select no tests. Keep this validator independent of process and I/O.
function validateResult({ command, args, result, output }) {
  let tests;
  let reason;
  let deferred_tests;
  if (command === 'cargo' && args[0] === 'test') {
    const summaries = output.match(/^test result:[^\r\n]*/gm) || [];
    const counts = summaries.map(line => line.match(/^test result: (ok|FAILED)\.\s+(\d+) passed;\s+(\d+) failed;\s+(\d+) ignored;\s+(\d+) measured;\s+(\d+) filtered out;/));
    if (!counts.length || counts.some(count => !count)) {
      reason = 'Missing or invalid Rust test result summary. Fix: require completed test output before accepting this gate.';
    } else {
      tests = { framework: 'rust', summaries: counts.length, passed: 0, failed: 0, ignored: 0, measured: 0, filtered_out: 0 };
      for (const count of counts) {
        for (const [index, field] of ['passed', 'failed', 'ignored', 'measured', 'filtered_out'].entries()) {
          tests[field] += Number(count[index + 2]);
        }
      }
      if (tests.failed || counts.some(count => count[1] !== 'ok')) {
        reason = 'Rust summaries must report zero failed tests and successful results.';
      } else if (args.includes('--exact') && (tests.passed !== 1 || tests.ignored !== 0)) {
        reason = 'An exact Rust gate must prove exactly one passed test and zero failed or ignored tests. Fix: check the exact filter and ignored-test flags.';
      } else if (tests.passed === 0) {
        reason = 'A Rust suite must prove at least one passed test. Fix: check the suite selection.';
      }
      if (!reason) {
        const entries = [...output.matchAll(/^test (.+?) \.\.\. ignored(?:, ([^\r\n]+))?\r?$/gm)];
        if (entries.length !== tests.ignored || new Set(entries.map(entry => entry[1])).size !== entries.length) {
          reason = 'Ignored test entries must match the summary count and have unique names. Fix: retain complete test output.';
        } else if (entries.length) {
          deferred_tests = [];
          for (const [, name, explanation] of entries) {
            const deferred = deferredRustChecks.get(name);
            if (!deferred || explanation !== deferred.reason) {
              reason = `Unexpected ignored test or reason: ${name}. Fix: run the regression normally; only the documented separate scale/corpus gates may be deferred.`;
              break;
            }
            deferred_tests.push({ name, ...deferred, status: 'deferred' });
          }
        }
      }
    }
  } else if (command === 'node' && args.includes('--test')) {
    tests = { framework: 'node' };
    for (const [label, field] of [
      ['tests', 'total'], ['pass', 'passed'], ['fail', 'failed'],
      ['cancelled', 'cancelled'], ['skipped', 'skipped'], ['todo', 'todo'],
    ]) {
      // Node uses TAP summaries on older runtimes and spec summaries on newer ones.
      const counts = [...output.matchAll(new RegExp(`^(?:#|ℹ)\\s+${label}\\s+(\\d+)\\s*$`, 'gm'))];
      if (counts.length !== 1) {
        reason = `Missing or ambiguous Node test summary count ${label}. Fix: retain the complete test-runner summary.`;
        tests = undefined;
        break;
      }
      tests[field] = Number(counts[0][1]);
    }
    if (tests) {
      if (tests.failed || tests.cancelled || tests.skipped || tests.todo) {
        reason = 'Node gates require zero failed, cancelled, skipped, or todo tests.';
      } else if (tests.passed === 0) {
        reason = 'A Node gate must prove at least one passed test. Fix: check the test file selection.';
      } else if (tests.total !== tests.passed) {
        reason = 'Node summary totals must match the passed count. Fix: retain an unambiguous completed test summary.';
      }
    }
  }
  if (result.status !== 0 || result.error || result.signal) {
    reason = `Process did not complete successfully: exit ${result.status}, signal ${result.signal || 'none'}${result.error ? `, ${result.error.message}` : ''}.`;
  }
  return { passed: !reason, reason, tests, deferred_tests };
}

function main() {
  const mode = process.argv[2];
  if (!['correctness', 'simulation'].includes(mode)) {
    throw new Error('Usage: node editors/scripts/release_checks.js <correctness|simulation>');
  }
  const evidence = path.resolve(process.env.RUBY_FAST_LSP_EVIDENCE_DIR || path.join(root, 'target/release-evidence'));
  fs.mkdirSync(evidence, { recursive: true });
  const summary = { schema_version: 1, mode, started_at: new Date().toISOString(), checks: [] };
  const commands = mode === 'correctness' ? [
    ['source-layout-tests', 'python3', ['-B', '-m', 'unittest', 'discover', '-s', 'support/structure', '-p', 'test_*.py']],
    ['source-layout', 'python3', ['-B', 'support/structure/check.py']],
    ['versions', 'node', ['editors/check_package_versions.js']],
    ['workspace', 'cargo', ['test', '--locked', '--workspace']],
    ['editor', 'node', ['--test', ...fs.readdirSync(path.join(root, 'editors/vscode/vsix/test')).filter(f => f.endsWith('.test.js')).sort().map(f => `editors/vscode/vsix/test/${f}`)]],
    ['packaging', 'node', ['--test', ...fs.readdirSync(path.join(root, 'editors/scripts/test')).filter(f => f.endsWith('.test.js')).sort().map(f => `editors/scripts/test/${f}`)]],
    // Already enforced by the workspace run; repeat explicitly to retain each JSON report.
    ['inference-acceptance', 'cargo', ['test', '--locked', '-p', 'ruby-fast-lsp', '--lib', 'test::inference_scorecard::report_m0_scorecard', '--', '--exact', '--nocapture']],
    ['real-project-precision', 'cargo', ['test', '--locked', '-p', 'ruby-fast-lsp', '--lib', 'test::real_project_precision::report_real_project_precision', '--', '--exact', '--nocapture']],
  ] : [
    ['ruby-oracle-controls', 'python3', ['support/simulation/run_oracle_controls.py']],
    ['simulation', 'cargo', ['test', '--locked', '--release', '-p', 'ruby-fast-lsp', '--lib', 'test::simulation', '--', '--nocapture']],
    ['large-scale-lsp', 'cargo', ['test', '--locked', '--release', '-p', 'ruby-fast-lsp', '--lib', 'test::simulation::tests::generated_project_large_scale_smoke', '--', '--ignored', '--exact', '--nocapture']],
    ['large-scale-engine', 'cargo', ['test', '--locked', '--release', '-p', 'ruby-fast-lsp', '--lib', 'test::simulation::tests::generated_project_large_scale_engine_checks_all_edges', '--', '--ignored', '--exact', '--nocapture']],
    ['performance', 'cargo', ['run', '--locked', '--release', '--bin', 'profiler', '--', '--benchmark-iterations', '100', '--check-budgets']],
  ];
  if (mode === 'simulation' && process.env.SIM_REAL_CORPUS_ROOT) {
    commands.push(['real-corpus', 'cargo', ['test', '--locked', '--release', '-p', 'ruby-fast-lsp', '--lib', 'test::simulation::tests::generated_project_real_corpus_smoke', '--', '--ignored', '--exact', '--nocapture']]);
  }
  let failed = false;
  for (const [name, command, args] of commands) {
    const logfile = path.join(evidence, `${name}.log`);
    const fd = fs.openSync(logfile, 'w');
    const started = Date.now();
    console.log(`Running ${name}; log ${logfile}`);
    let result;
    try {
      result = spawnSync(command, args, {
        cwd: root, stdio: ['ignore', fd, fd], timeout: 30 * 60 * 1000,
        env: { ...process.env, RUST_LOG: 'error' },
      });
    } finally {
      fs.closeSync(fd);
    }
    const validation = validateResult({ command, args, result, output: fs.readFileSync(logfile, 'utf8') });
    const passed = validation.passed;
    summary.checks.push({ name, command: [command, ...args], status: passed ? 'passed' : 'failed', exit_code: result.status, signal: result.signal, error: result.error?.message, validation_error: validation.reason, tests: validation.tests, deferred_tests: validation.deferred_tests, elapsed_ms: Date.now() - started, log: path.basename(logfile) });
    fs.writeFileSync(path.join(evidence, `${mode}.json`), JSON.stringify(summary, null, 2) + '\n');
    if (!passed) { failed = true; break; }
  }
  if (mode === 'simulation' && !process.env.SIM_REAL_CORPUS_ROOT) {
    summary.checks.push({ name: 'real-corpus', status: 'not_run', reason: 'No explicit SIM_REAL_CORPUS_ROOT; synthetic scale and reviewed precision are separate checks.' });
  }
  summary.completed_at = new Date().toISOString();
  summary.required_checks_passed = !failed;
  summary.required_checks_not_run = commands.slice(summary.checks.filter(c => c.status !== 'not_run').length).map(([name]) => name);
  fs.writeFileSync(path.join(evidence, `${mode}.json`), JSON.stringify(summary, null, 2) + '\n');
  process.exitCode = failed ? 1 : 0;
}

module.exports = { validateResult };

if (require.main === module) {
  main();
}
