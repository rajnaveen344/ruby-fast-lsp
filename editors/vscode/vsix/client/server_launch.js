// Packaged server assets: the platform binary, bundled extension packages,
// and Ruby stubs extracted from stubs-zipped on first activation. Paths
// resolve against the extension root, one level above this folder.
const path = require('path');
const fs = require('fs');
const { session } = require('./session');

/**
 * Extract zipped stubs to the extension's stubs directory on first run.
 * This ensures go-to-definition shows proper file paths instead of virtual URIs.
 * 
 * Only extracts if:
 * - stubs-zipped/*.zip files exist
 * - corresponding stubs/rubystubsXY directory doesn't exist or is outdated
 */
function extractZippedStubs(extensionPath) {
    const zippedDir = path.join(extensionPath, 'stubs-zipped');
    const stubsDir = path.join(extensionPath, 'stubs');

    if (!fs.existsSync(zippedDir)) {
        return; // No zipped stubs, nothing to do
    }

    const AdmZip = require('adm-zip');
    const zipFiles = fs.readdirSync(zippedDir).filter(f => f.endsWith('.zip'));

    for (const zipFile of zipFiles) {
        const version = zipFile.replace('.zip', ''); // e.g., "rubystubs30"
        const zipPath = path.join(zippedDir, zipFile);
        const extractPath = path.join(stubsDir, version);
        const markerFile = path.join(extractPath, '.extracted');

        // Check if we need to extract
        let needsExtract = false;
        if (!fs.existsSync(extractPath)) {
            needsExtract = true;
        } else if (!fs.existsSync(markerFile)) {
            needsExtract = true;
        } else {
            // Check if zip is newer than extraction
            const zipStat = fs.statSync(zipPath);
            const markerStat = fs.statSync(markerFile);
            if (zipStat.mtime > markerStat.mtime) {
                needsExtract = true;
            }
        }

        if (needsExtract) {
            try {
                if (session.outputChannel) {
                    session.outputChannel.appendLine(`[Ruby Fast LSP] Extracting ${zipFile}...`);
                }

                // Clean up old extraction if exists
                if (fs.existsSync(extractPath)) {
                    fs.rmSync(extractPath, { recursive: true });
                }

                // Extract
                const zip = new AdmZip(zipPath);
                zip.extractAllTo(extractPath, true);

                // Write marker file
                fs.writeFileSync(markerFile, new Date().toISOString());

                if (session.outputChannel) {
                    session.outputChannel.appendLine(`[Ruby Fast LSP] Extracted ${zipFile} to ${extractPath}`);
                }
            } catch (error) {
                if (session.outputChannel) {
                    session.outputChannel.appendLine(`[Ruby Fast LSP] Failed to extract ${zipFile}: ${error.message}`);
                }
            }
        }
    }
}

function getServerPath() {
    const platform = process.platform;
    const arch = process.arch;
    const isWindows = platform === 'win32';
    const extension = isWindows ? '.exe' : '';
    const binaryName = `ruby-fast-lsp${extension}`;

    // Map platform.arch to the correct binary path.
    // Release CI publishes VSIX binaries under VS Code target platform names
    // (darwin-arm64/darwin-x64). Older local packages used macos-* names.
    const platformMap = {
        'darwin': {
            'x64': ['darwin-x64', 'macos-x64'],
            'arm64': ['darwin-arm64', 'macos-arm64']
        },
        'linux': {
            'x64': ['linux-x64'],
            'arm64': ['linux-arm64']
        },
        'win32': {
            'x64': ['win32-x64'],
            'arm64': ['win32-arm64']
        }
    };

    const platformInfo = platformMap[platform];
    if (!platformInfo) {
        throw new Error(`Unsupported platform: ${platform}`);
    }

    const platformDirs = platformInfo[arch];
    if (!platformDirs) {
        throw new Error(`Unsupported architecture ${arch} for platform ${platform}`);
    }

    const candidatePaths = platformDirs.map(platformDir => path.join(__dirname, '..', 'bin', platformDir, binaryName));
    const serverPath = candidatePaths.find(candidatePath => fs.existsSync(candidatePath));
    if (!serverPath) {
        throw new Error(`Ruby Fast LSP binary not found. Tried: ${candidatePaths.join(', ')}`);
    }

    if (!isWindows) {
        fs.chmodSync(serverPath, 0o755);
    }

    return serverPath;
}

function getBundledExtensionPackages(extensionPath) {
    const packages = [];
    for (const packageName of ['rspec-ruby', 'rails-ruby', 'minitest-ruby', 'sinatra-rust', 'cucumber-rust']) {
        const extensionPackage = path.join(extensionPath, 'extensions', packageName);
        if (fs.existsSync(path.join(extensionPackage, 'extension.toml'))) {
            packages.push(extensionPackage);
        }
    }
    return packages;
}

module.exports = { extractZippedStubs, getServerPath, getBundledExtensionPackages };
