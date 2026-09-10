const assert = require('node:assert/strict');
const { test } = require('node:test');
const { validateResult } = require('../release_checks');

function rustResult(output, { exact = true, status = 0 } = {}) {
  return validateResult({
    command: 'cargo', args: ['test', '--lib', 'selected_test', '--', ...(exact ? ['--exact'] : [])],
    result: { status, signal: null }, output,
  });
}

function nodeResult({ tests = 3, passed = 3, failed = 0, skipped = 0, prefix = '#' } = {}) {
  return validateResult({
    command: 'node', args: ['--test', 'fixture.test.js'], result: { status: 0, signal: null },
    output: [
      `${prefix} tests ${tests}`, `${prefix} suites 0`, `${prefix} pass ${passed}`,
      `${prefix} fail ${failed}`, `${prefix} cancelled 0`, `${prefix} skipped ${skipped}`,
      `${prefix} todo 0`, `${prefix} duration_ms 42`,
    ].join('\n'),
  });
}

test('exact one pass accepted', () => {
  const result = rustResult('test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 14 filtered out; finished in 0.02s\n');
  assert.equal(result.passed, true);
  assert.deepEqual(result.tests, { framework: 'rust', summaries: 1, passed: 1, failed: 0, ignored: 0, measured: 0, filtered_out: 14 });
});

test('exact zero tests rejected', () => {
  const result = rustResult('test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 15 filtered out; finished in 0.00s\n');
  assert.equal(result.passed, false);
  assert.match(result.reason, /exact.*one.*passed/i);
});

test('exact ignored-only rejected', () => {
  const result = rustResult('test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 14 filtered out; finished in 0.00s\n');
  assert.equal(result.passed, false);
  assert.match(result.reason, /ignored/i);
});

test('process nonzero rejected despite pass text', () => {
  const result = rustResult('test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n', { status: 101 });
  assert.equal(result.passed, false);
  assert.match(result.reason, /101/);
});

test('nonexact multi-binary Rust output with positive total accepted', () => {
  const result = rustResult([
    'Running unittests src/lib.rs',
    'test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.20s',
    'Running unittests src/main.rs',
    'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s',
    'Doc-tests fixture',
    'test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s',
  ].join('\n'), { exact: false });
  assert.equal(result.passed, true);
  assert.deepEqual(result.tests, { framework: 'rust', summaries: 3, passed: 15, failed: 0, ignored: 0, measured: 0, filtered_out: 0 });
});

test('an unexpected ignored regression fails the release gate', () => {
  const result = rustResult([
    'test test::integration::new_regression ... ignored, temporarily broken',
    'test result: ok. 12 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.20s',
  ].join('\n'), { exact: false });
  assert.equal(result.passed, false);
  assert.match(result.reason, /unexpected ignored.*new_regression/i);
});

test('ignored counts without individual names cannot hide missing test output', () => {
  const result = rustResult('test result: ok. 12 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.20s', { exact: false });
  assert.equal(result.passed, false);
  assert.match(result.reason, /ignored.*(names|entries|count)/i);
});

test('only explicitly deferred scale and corpus checks are allowed and explained', () => {
  const name = 'test::simulation::tests::generated_project_large_scale_smoke';
  const result = rustResult([
    `test ${name} ... ignored, release gate: explicitly run the large-scale sampled LSP simulation`,
    'test result: ok. 12 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.20s',
  ].join('\n'), { exact: false });
  assert.equal(result.passed, true);
  assert.equal(result.deferred_tests.length, 1);
  assert.equal(result.deferred_tests[0].name, name);
  assert.equal(result.deferred_tests[0].gate, 'large-scale-lsp');
  assert.equal(result.deferred_tests[0].status, 'deferred');
});

test('Node positive pass accepted', () => {
  const result = nodeResult();
  assert.equal(result.passed, true);
  assert.deepEqual(result.tests, { framework: 'node', total: 3, passed: 3, failed: 0, cancelled: 0, skipped: 0, todo: 0 });
});

test('Node zero tests rejected', () => {
  const result = nodeResult({ tests: 0, passed: 0 });
  assert.equal(result.passed, false);
  assert.match(result.reason, /at least one.*pass/i);
});

test('Rust output without a completed result summary is rejected', () => {
  const result = rustResult('running 1 test\ntest selected_test ... ok\n');
  assert.equal(result.passed, false);
  assert.match(result.reason, /missing.*summary/i);
});

test('nonexact Rust suites cannot pass with only ignored tests', () => {
  const result = rustResult('test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s\n', { exact: false });
  assert.equal(result.passed, false);
});

test('failed Rust results are rejected even with a successful process status', () => {
  const result = rustResult('test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s\n', { exact: false });
  assert.equal(result.passed, false);
  assert.equal(result.tests.failed, 1);
});

test('Node skipped or failed tests violate the all-pass policy', () => {
  for (const counts of [{ tests: 3, passed: 2, skipped: 1 }, { tests: 3, passed: 2, failed: 1 }]) {
    const result = nodeResult(counts);
    assert.equal(result.passed, false);
    assert.match(result.reason, /zero.*(failed|skipped)/i);
  }
});

test('Node spec reporter summaries are accepted', () => {
  assert.equal(nodeResult({ prefix: 'ℹ' }).passed, true);
});

test('Node output missing summary counts is rejected', () => {
  const result = validateResult({
    command: 'node', args: ['--test', 'fixture.test.js'], result: { status: 0 },
    output: '# tests 3\n# pass 3\n# fail 0\n',
  });
  assert.equal(result.passed, false);
  assert.match(result.reason, /missing.*summary/i);
});
