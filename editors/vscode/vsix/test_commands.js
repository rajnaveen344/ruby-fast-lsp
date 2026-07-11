const { fileURLToPath } = require('node:url');

function filePath(uri) {
    const parsed = new URL(uri);
    if (parsed.protocol !== 'file:') {
        throw new Error(`Test command requires a file URI, got ${parsed.protocol}`);
    }
    return fileURLToPath(parsed);
}

function rspecInvocation(target) {
    const separator = target.lastIndexOf(':');
    if (separator < 0) {
        throw new Error('RSpec target must end with a one-based line number');
    }
    const path = filePath(target.slice(0, separator));
    const line = target.slice(separator + 1);
    if (!/^[1-9][0-9]*$/.test(line)) {
        throw new Error(`RSpec target has an invalid line number: ${line}`);
    }
    const exactTarget = `${path}:${line}`;
    return {
        argv: ['bundle', 'exec', 'rspec', exactTarget],
        debug: { script: 'bundle', args: ['exec', 'rspec', exactTarget] }
    };
}

function minitestInvocation(uri, line, testName, railsCommand) {
    if (!/^[1-9][0-9]*$/.test(line)) {
        throw new Error(`Minitest target has an invalid line number: ${line}`);
    }
    const path = filePath(uri);
    if (railsCommand) {
        return {
            argv: [railsCommand, 'test', `${path}:${line}`],
            debug: { script: railsCommand, args: ['test', `${path}:${line}`] }
        };
    }
    const args = ['exec', 'ruby', '-Itest', path];
    if (testName) args.push('--name', testName);
    return {
        argv: ['bundle', ...args],
        debug: { script: 'bundle', args }
    };
}

function debugConfiguration(name, invocation, cwd) {
    return {
        type: 'rdbg',
        request: 'launch',
        name,
        script: invocation.debug.script,
        args: invocation.debug.args,
        cwd
    };
}

module.exports = { debugConfiguration, minitestInvocation, rspecInvocation };
