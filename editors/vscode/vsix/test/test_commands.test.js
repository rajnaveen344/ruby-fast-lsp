const assert = require('node:assert/strict');
const test = require('node:test');

const {
    debugConfiguration,
    minitestInvocation,
    rspecInvocation
} = require('../test_commands');

test('RSpec run and debug share an exact file-line target', () => {
    const invocation = rspecInvocation('file:///repo/spec/user_spec.rb:12');

    assert.deepEqual(invocation.argv, ['bundle', 'exec', 'rspec', '/repo/spec/user_spec.rb:12']);
    assert.equal(invocation.debug.script, 'bundle');
    assert.deepEqual(invocation.debug.args, ['exec', 'rspec', '/repo/spec/user_spec.rb:12']);
});

test('Rails Minitest uses the line-aware Rails runner', () => {
    const invocation = minitestInvocation(
        'file:///repo/test/models/user_test.rb',
        '18',
        'test_valid',
        '/repo/bin/rails'
    );

    assert.deepEqual(invocation.argv, ['/repo/bin/rails', 'test', '/repo/test/models/user_test.rb:18']);
    assert.equal(invocation.debug.script, '/repo/bin/rails');
    assert.deepEqual(invocation.debug.args, ['test', '/repo/test/models/user_test.rb:18']);
});

test('plain Minitest uses the exact method filter', () => {
    const invocation = minitestInvocation(
        'file:///repo/test/user_test.rb',
        '7',
        'test_valid',
        null
    );

    assert.deepEqual(invocation.argv, [
        'bundle', 'exec', 'ruby', '-Itest', '/repo/test/user_test.rb', '--name', 'test_valid'
    ]);
    assert.equal(invocation.debug.script, 'bundle');
    assert.deepEqual(invocation.debug.args, [
        'exec', 'ruby', '-Itest', '/repo/test/user_test.rb', '--name', 'test_valid'
    ]);
});

test('a plain Minitest class lens runs its complete file', () => {
    const invocation = minitestInvocation(
        'file:///repo/test/user_test.rb',
        '1',
        '',
        null
    );

    assert.deepEqual(invocation.argv, [
        'bundle', 'exec', 'ruby', '-Itest', '/repo/test/user_test.rb'
    ]);
    assert.deepEqual(invocation.debug.args, [
        'exec', 'ruby', '-Itest', '/repo/test/user_test.rb'
    ]);
});

test('debug commands launch an rdbg configuration instead of a notification', () => {
    assert.deepEqual(
        debugConfiguration('Debug RSpec', rspecInvocation('file:///repo/spec/user_spec.rb:12'), '/repo'),
        {
            type: 'rdbg',
            request: 'launch',
            name: 'Debug RSpec',
            script: 'bundle',
            args: ['exec', 'rspec', '/repo/spec/user_spec.rb:12'],
            cwd: '/repo'
        }
    );
});
