// Commands invoked by server code lenses: the references peek, RSpec and
// Minitest run/debug, and opening the Rails view for a controller action.
const vscode = require('vscode');
const path = require('path');
const fs = require('fs');
const {
    debugConfiguration,
    minitestInvocation,
    railsViewRelativePaths,
    rspecInvocation
} = require('./test_commands');

function testWorkingDirectory(uriString) {
    const uri = vscode.Uri.parse(uriString);
    return vscode.workspace.getWorkspaceFolder(uri)?.uri.fsPath || path.dirname(uri.fsPath);
}

function runTestInTerminal(name, invocation, cwd) {
    const execution = new vscode.ProcessExecution(
        invocation.argv[0],
        invocation.argv.slice(1),
        { cwd }
    );
    const task = new vscode.Task(
        { type: 'ruby-fast-lsp-test', target: name },
        vscode.TaskScope.Workspace,
        name,
        'Ruby Fast LSP',
        execution
    );
    task.presentationOptions = {
        reveal: vscode.TaskRevealKind.Always,
        panel: vscode.TaskPanelKind.Dedicated,
        clear: true
    };
    return vscode.tasks.executeTask(task);
}

function debugTest(name, invocation, cwd) {
    return vscode.debug.startDebugging(undefined, debugConfiguration(name, invocation, cwd));
}

function registerCodeLensCommands() {
    // Register wrapper command for showReferences to handle LSP JSON serialization
    const showReferencesCommand = vscode.commands.registerCommand('ruby-fast-lsp.showReferences',
        (uriStr, position, locations) => {
            // Convert JSON arguments to proper VS Code types
            const uri = vscode.Uri.parse(uriStr);
            const pos = new vscode.Position(position.line, position.character);
            const locs = locations.map(loc => new vscode.Location(
                vscode.Uri.parse(loc.uri),
                new vscode.Range(
                    new vscode.Position(loc.range.start.line, loc.range.start.character),
                    new vscode.Position(loc.range.end.line, loc.range.end.character)
                )
            ));

            // Call the built-in showReferences command with proper types
            return vscode.commands.executeCommand('editor.action.showReferences', uri, pos, locs);
        }
    );

    const runRspecCommand = vscode.commands.registerCommand('ruby-fast-lsp.rspec.run',
        (uriStr, _line, target) => {
            try {
                runTestInTerminal('RSpec', rspecInvocation(target), testWorkingDirectory(uriStr));
            } catch (error) {
                vscode.window.showErrorMessage(`Unable to run RSpec: ${error.message}`);
            }
        }
    );

    const debugRspecCommand = vscode.commands.registerCommand('ruby-fast-lsp.rspec.debug',
        (uriStr, _line, target) => {
            try {
                return debugTest('Debug RSpec', rspecInvocation(target), testWorkingDirectory(uriStr));
            } catch (error) {
                vscode.window.showErrorMessage(`Unable to debug RSpec: ${error.message}`);
                return undefined;
            }
        }
    );

    const minitestTarget = (uriStr, line, testName) => {
        const cwd = testWorkingDirectory(uriStr);
        const rails = path.join(cwd, 'bin', process.platform === 'win32' ? 'rails.bat' : 'rails');
        return {
            cwd,
            invocation: minitestInvocation(uriStr, line, testName, fs.existsSync(rails) ? rails : null)
        };
    };

    const runMinitestCommand = vscode.commands.registerCommand('ruby-fast-lsp.minitest.run',
        (uriStr, line, testName) => {
            try {
                const target = minitestTarget(uriStr, line, testName);
                runTestInTerminal('Minitest', target.invocation, target.cwd);
            } catch (error) {
                vscode.window.showErrorMessage(`Unable to run Minitest: ${error.message}`);
            }
        }
    );

    const debugMinitestCommand = vscode.commands.registerCommand('ruby-fast-lsp.minitest.debug',
        (uriStr, line, testName) => {
            try {
                const target = minitestTarget(uriStr, line, testName);
                return debugTest('Debug Minitest', target.invocation, target.cwd);
            } catch (error) {
                vscode.window.showErrorMessage(`Unable to debug Minitest: ${error.message}`);
                return undefined;
            }
        }
    );

    const openRailsViewCommand = vscode.commands.registerCommand('ruby-fast-lsp.rails.openView',
        async (controllerUriString, controller, action) => {
            try {
                const controllerUri = vscode.Uri.parse(controllerUriString);
                const workspace = vscode.workspace.getWorkspaceFolder(controllerUri);
                if (!workspace) {
                    throw new Error('the controller is not inside an open workspace');
                }
                for (const relative of railsViewRelativePaths(controller, action)) {
                    const candidate = vscode.Uri.file(path.join(workspace.uri.fsPath, relative));
                    if (fs.existsSync(candidate.fsPath)) {
                        const document = await vscode.workspace.openTextDocument(candidate);
                        return vscode.window.showTextDocument(document);
                    }
                }
                vscode.window.showInformationMessage(
                    `No view found for ${controller}#${action}`
                );
                return undefined;
            } catch (error) {
                vscode.window.showErrorMessage(`Unable to open Rails view: ${error.message}`);
                return undefined;
            }
        }
    );

    return {
        showReferencesCommand,
        runRspecCommand,
        debugRspecCommand,
        runMinitestCommand,
        debugMinitestCommand,
        openRailsViewCommand
    };
}

module.exports = { registerCodeLensCommands };
