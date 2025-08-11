const vscode = require('vscode');
const path = require('path');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client;

function getServerPath() {
    const platform = process.platform;
    const arch = process.arch;
    const isWindows = platform === 'win32';
    const extension = isWindows ? '.exe' : '';
    const binaryName = `ruby-fast-lsp${extension}`;

    // Map platform.arch to the correct binary path
    const platformMap = {
        'darwin': {
            'x64': 'macos-x64',
            'arm64': 'macos-arm64'
        },
        'linux': {
            'x64': 'linux-x64',
            'arm64': 'linux-arm64'
        },
        'win32': {
            'x64': 'win32-x64',
            'arm64': 'win32-arm64'
        }
    };

    const platformInfo = platformMap[platform];
    if (!platformInfo) {
        throw new Error(`Unsupported platform: ${platform}`);
    }

    const platformDir = platformInfo[arch];
    if (!platformDir) {
        throw new Error(`Unsupported architecture ${arch} for platform ${platform}`);
    }

    return path.join(__dirname, 'bin', platformDir, binaryName);
}

function activate(context) {
    const config = vscode.workspace.getConfiguration('rubyFastLsp');
    
    const serverOptions = {
        command: getServerPath(),
        args: [],
        transport: TransportKind.stdio
    };

    const clientOptions = {
        documentSelector: [{ scheme: 'file', language: 'ruby' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.rb'),
            configurationSection: 'rubyFastLsp'
        },
        initializationOptions: {
            rubyVersion: config.get('rubyVersion', 'auto'),
            enableCoreStubs: config.get('enableCoreStubs', true),
            stubsPath: config.get('stubsPath', ''),
            versionDetection: config.get('versionDetection', {
                enableRbenv: true,
                enableRvm: true,
                enableChruby: true,
                enableSystemRuby: true
            })
        }
    };

    client = new LanguageClient(
        'ruby-fast-lsp',
        'Ruby Fast LSP',
        serverOptions,
        clientOptions
    );

    // Handle configuration changes
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration(event => {
            if (event.affectsConfiguration('rubyFastLsp')) {
                // Notify the server about configuration changes
                if (client) {
                    const newConfig = vscode.workspace.getConfiguration('rubyFastLsp');
                    client.sendNotification('workspace/didChangeConfiguration', {
                        settings: {
                            rubyFastLsp: {
                                rubyVersion: newConfig.get('rubyVersion', 'auto'),
                                enableCoreStubs: newConfig.get('enableCoreStubs', true),
                                stubsPath: newConfig.get('stubsPath', ''),
                                versionDetection: newConfig.get('versionDetection', {
                                    enableRbenv: true,
                                    enableRvm: true,
                                    enableChruby: true,
                                    enableSystemRuby: true
                                })
                            }
                        }
                    });
                }
            }
        })
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
