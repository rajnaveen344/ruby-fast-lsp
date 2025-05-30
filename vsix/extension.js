const vscode = require('vscode');
const path = require('path');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client;

function getServerPath() {
    const platform = process.platform;
    const extension = platform === 'win32' ? '.exe' : '';

    if (platform === 'darwin') {
        return path.join(__dirname, 'bin', 'macos', 'ruby-fast-lsp');
    } else if (platform === 'linux') {
        return path.join(__dirname, 'bin', 'linux', 'ruby-fast-lsp');
    } else if (platform === 'win32') {
        return path.join(__dirname, 'bin', 'win32', 'ruby-fast-lsp.exe');
    }

    throw new Error(`Unsupported platform: ${platform}`);
}

function activate(context) {
    const serverOptions = {
        command: getServerPath(),
        args: [],
        transport: TransportKind.stdio
    };

    const clientOptions = {
        documentSelector: [{ scheme: 'file', language: 'ruby' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.rb')
        }
    };

    client = new LanguageClient(
        'ruby-fast-lsp',
        'Ruby Fast LSP',
        serverOptions,
        clientOptions
    );

    client.start();
}

function deactivate() {
    if (!client) {
        return undefined;
    }
    return client.stop();
}

module.exports = { activate, deactivate };
