#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Configuration
EXTENSION_NAME="ruby-fast-lsp"
EXTENSION_VERSION=$(grep -m 1 "version" "$ROOT_DIR/Cargo.toml" | cut -d '"' -f 2)
EXTENSION_DIR="$SCRIPT_DIR/vsix"
TARGET_DIR="$ROOT_DIR/target"
REBUILD_LSP=false
SKIP_BUILDS=false
SELECTED_PLATFORMS=""

node "$ROOT_DIR/editors/check_package_versions.js"

# Define target platforms and architectures as arrays
ALL_PLATFORMS=("macos-x64" "macos-arm64" "linux-x64" "win32-x64")
ALL_TARGETS=("x86_64-apple-darwin" "aarch64-apple-darwin" "x86_64-unknown-linux-gnu" "x86_64-pc-windows-gnu")

# Determine current platform
CURRENT_PLATFORM=""
if [ "$(uname)" == "Darwin" ]; then
    if [ "$(uname -m)" == "x86_64" ]; then
        CURRENT_PLATFORM="macos-x64"
    else
        CURRENT_PLATFORM="macos-arm64"
    fi
elif [ "$(uname)" == "Linux" ]; then
    if [ "$(uname -m)" == "x86_64" ]; then
        CURRENT_PLATFORM="linux-x64"
    fi
else
    # Assuming Windows
    if [ "$(uname -m)" == "x86_64" ]; then
        CURRENT_PLATFORM="win32-x64"
    fi
fi

# Parse command-line arguments
while [[ $# -gt 0 ]]; do
    key="$1"
    case $key in
        --rebuild)
            REBUILD_LSP=true
            shift
            ;;
        --skip-builds)
            SKIP_BUILDS=true
            shift
            ;;
        --platforms)
            SELECTED_PLATFORMS="$2"
            shift
            shift
            ;;
        --current-platform-only)
            SELECTED_PLATFORMS="$CURRENT_PLATFORM"
            shift
            ;;
        --help)
            echo "Usage: $0 [options]"
            echo "Options:"
            echo "  --rebuild               Force rebuild of the LSP binary"
            echo "  --skip-builds           Skip building binaries (use existing ones)"
            echo "  --platforms LIST        Comma-separated list of platforms to build for"
            echo "                          Available: macos-x64,macos-arm64,linux-x64,linux-arm64,win32-x64,win32-arm64"
            echo "  --current-platform-only Build only for the current platform ($CURRENT_PLATFORM)"
            echo "  --help                  Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $key"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# If no platforms specified, use current platform only
if [ -z "$SELECTED_PLATFORMS" ]; then
    SELECTED_PLATFORMS="$CURRENT_PLATFORM"
    echo "No platforms specified, building for current platform ($CURRENT_PLATFORM) only"
    echo "To build for all platforms, use: $0 --platforms all"
fi

# Determine which platforms to build for
PLATFORMS=()
TARGETS=()

if [ "$SELECTED_PLATFORMS" == "all" ]; then
    PLATFORMS=(${ALL_PLATFORMS[@]})
    TARGETS=(${ALL_TARGETS[@]})
else
    IFS=',' read -ra PLATFORM_LIST <<< "$SELECTED_PLATFORMS"
    for platform in "${PLATFORM_LIST[@]}"; do
        found=false
        for i in "${!ALL_PLATFORMS[@]}"; do
            if [ "${ALL_PLATFORMS[$i]}" == "$platform" ]; then
                PLATFORMS+=("$platform")
                TARGETS+=("${ALL_TARGETS[$i]}")
                found=true
                break
            fi
        done
        if [ "$found" == "false" ]; then
            echo "Warning: Unknown platform '$platform', skipping"
        fi
    done
fi

if [ ${#PLATFORMS[@]} -eq 0 ]; then
    echo "Error: No valid platforms selected"
    exit 1
fi

echo "Building for platforms: ${PLATFORMS[*]}"

# Ensure directories exist
mkdir -p "$TARGET_DIR"

# Only create directories for platforms we're building for
for platform in "${PLATFORMS[@]}"; do
    mkdir -p "$EXTENSION_DIR/bin/$platform"
done

# Function to check if we can build for a target
can_build_for_target() {
    local platform=$1
    local target=$2
    
    # Always allow building for current platform
    if [ "$platform" == "$CURRENT_PLATFORM" ]; then
        return 0
    fi
    
    # Check for cross-compilation tools based on target
    if [[ $target == *-linux-* ]]; then
        # Check for Linux cross-compilation tools
        if ! command -v "${target}-gcc" &> /dev/null; then
            echo "Warning: Cross-compiler for $target not found"
            echo "To build for Linux targets on macOS, you need to install cross-compilation tools:"
            echo "  brew install FiloSottile/musl-cross/musl-cross"
            return 1
        fi
    elif [[ $target == *-windows-* ]]; then
        # Check for Windows cross-compilation tools
        if ! command -v "${target}-gcc" &> /dev/null && ! rustup target list | grep -q "$target (installed)"; then
            echo "Warning: Windows cross-compilation may require additional setup"
            echo "See: https://rust-lang.github.io/rustup/cross-compilation.html"
        fi
    fi
    
    return 0
}

# Function to build binary for a specific target
build_for_target() {
    local platform=$1
    local target=$2
    local binary_name="ruby-fast-lsp"
    local binary_path=""
    
    # Check if we can build for this target
    if ! can_build_for_target "$platform" "$target"; then
        echo "Skipping build for $platform ($target) due to missing dependencies"
        return 1
    fi
    
    # Add .exe extension for Windows targets
    if [[ $platform == win32-* ]]; then
        binary_name="${binary_name}.exe"
    fi
    
    binary_path="$ROOT_DIR/target/${target}/release/${binary_name}"
    
    # Native packages must always be built from the current checkout. Reusing a
    # target-specific artifact can silently package an older server even after
    # `cargo build --release` refreshed the ordinary native target directory.
    # Cross-platform release assembly may reuse intentionally prebuilt artifacts.
    if [ "$platform" == "$CURRENT_PLATFORM" ] || [ ! -f "$binary_path" ] || [ "$REBUILD_LSP" = true ]; then
        if [ "$SKIP_BUILDS" = true ]; then
            echo "Skipping build for $platform ($target)"
            return 0
        fi
        
        echo "Building for $platform ($target)..."
        
        # Use cross for building
        if [ "$platform" != "$CURRENT_PLATFORM" ]; then
            echo "Cross-compiling from $CURRENT_PLATFORM to $platform using cross..."
            if ! command -v cross &> /dev/null; then
                echo "Error: 'cross' command not found. Install it with:"
                echo "  cargo install cross --git https://github.com/cross-rs/cross"
                return 1
            fi
            
            if ! cross build --release --target "$target" --manifest-path "$ROOT_DIR/Cargo.toml"; then
                echo "Failed to cross-compile for $platform ($target)"
                return 1
            fi
        else
            # For native builds, use cargo directly
            if ! cargo build --release --target "$target" --manifest-path "$ROOT_DIR/Cargo.toml"; then
                echo "Failed to build for $platform ($target)"
                return 1
            fi
        fi
    else
        echo "Using existing binary for $platform ($target)"
    fi
    
    # Copy binary to extension directory
    echo "Copying binary to extension directory for $platform"
    if [ -f "$binary_path" ]; then
        cp "$binary_path" "$EXTENSION_DIR/bin/$platform/"
        return 0
    else
        echo "Binary not found at $binary_path"
        return 1
    fi
}

# Build for selected targets
if [ "$SKIP_BUILDS" = false ]; then
    echo "Building binaries for selected platforms and architectures..."
    built_platforms=()
    
    for i in $(seq 0 $((${#PLATFORMS[@]} - 1))); do
        if build_for_target "${PLATFORMS[$i]}" "${TARGETS[$i]}"; then
            built_platforms+=("${PLATFORMS[$i]}")
        fi
    done
    
    if [ ${#built_platforms[@]} -eq 0 ]; then
        echo "Error: Failed to build for platforms $SELECTED_PLATFORMS"
        exit 1
    fi
    
    echo "Successfully built for: ${built_platforms[*]}"
fi

# Pre-zip stubs for faster VSIX packaging
echo "Pre-zipping Ruby stubs..."
STUBS_ZIP_DIR="$EXTENSION_DIR/stubs-zipped"

if [ -d "$EXTENSION_DIR/stubs" ]; then
    ZIP_STUBS_SCRIPT="$SCRIPT_DIR/../scripts/zip_vscode_stubs.sh"
    if [ ! -x "$ZIP_STUBS_SCRIPT" ]; then
        echo "Error: missing executable VS Code stub zipper at $ZIP_STUBS_SCRIPT"
        exit 1
    fi
    "$ZIP_STUBS_SCRIPT"

    # Verify zipped stubs
    if [ -d "$STUBS_ZIP_DIR" ]; then
        zip_count=$(find "$STUBS_ZIP_DIR" -maxdepth 1 -name "*.zip" | wc -l)
        echo "Created $zip_count pre-zipped stub archives in $STUBS_ZIP_DIR"
    fi
else
    echo "Warning: Stubs directory not found at $EXTENSION_DIR/stubs"
    echo "Stubs will not be included in the VSIX package"
fi

# Stage the embedded core-RBS proof source as a real navigation target. The
# server also embeds these bytes for standalone checking, but packaged LSP
# definition locations must resolve to an actual immutable file.
echo "Bundling core runtime RBS..."
CORE_RBS_SOURCE="$ROOT_DIR/crates/rbs-parser/rbs_types/core/constants.rbs"
CORE_RBS_TARGET="$EXTENSION_DIR/core-rbs"
if [ ! -f "$CORE_RBS_SOURCE" ]; then
    echo "Error: missing core runtime RBS at $CORE_RBS_SOURCE"
    exit 1
fi
rm -rf "$CORE_RBS_TARGET"
mkdir -p "$CORE_RBS_TARGET"
cp "$CORE_RBS_SOURCE" "$CORE_RBS_TARGET/constants.rbs"

# Stage the runtime-owned JRuby delta assets. These remain uncompressed because
# the server indexes only the selected series and needs stable navigation URIs
# inside the installed extension.
echo "Bundling JRuby runtime stubs..."
JRUBY_STUB_SOURCE="$ROOT_DIR/support/jruby/stubs"
JRUBY_STUB_TARGET="$EXTENSION_DIR/jruby-stubs"
if [ ! -d "$JRUBY_STUB_SOURCE" ]; then
    echo "Error: missing JRuby runtime stubs at $JRUBY_STUB_SOURCE"
    exit 1
fi
rm -rf "$JRUBY_STUB_TARGET"
cp -R "$JRUBY_STUB_SOURCE" "$JRUBY_STUB_TARGET"
for jruby_stub_series in common 9.0 9.1 9.2 9.3 9.4 10.0 10.1; do
    if [ ! -f "$JRUBY_STUB_TARGET/$jruby_stub_series/runtime.rb" ]; then
        echo "Error: JRuby $jruby_stub_series runtime overlay was not staged"
        exit 1
    fi
done

# Stage the checksum-pinned implementation-navigation decompiler. The server
# resolves this path relative to its packaged binary and verifies the SHA-256
# again before every accepted process.
echo "Bundling JRuby Java decompiler..."
JRUBY_DECOMPILER_SOURCE="$ROOT_DIR/support/jruby/decompiler"
JRUBY_DECOMPILER_TARGET="$EXTENSION_DIR/jruby-decompiler"
if [ ! -f "$JRUBY_DECOMPILER_SOURCE/cfr-0.152.jar" ] || [ ! -f "$JRUBY_DECOMPILER_SOURCE/LICENSE-CFR" ]; then
    echo "Error: missing pinned CFR decompiler artifact or license"
    exit 1
fi
rm -rf "$JRUBY_DECOMPILER_TARGET"
cp -R "$JRUBY_DECOMPILER_SOURCE" "$JRUBY_DECOMPILER_TARGET"
CFR_EXPECTED_SHA256="f686e8f3ded377d7bc87d216a90e9e9512df4156e75b06c655a16648ae8765b2"
CFR_ACTUAL_SHA256=$(shasum -a 256 "$JRUBY_DECOMPILER_TARGET/cfr-0.152.jar" | awk '{print $1}')
if [ "$CFR_ACTUAL_SHA256" != "$CFR_EXPECTED_SHA256" ]; then
    echo "Error: staged CFR checksum mismatch: expected $CFR_EXPECTED_SHA256, got $CFR_ACTUAL_SHA256"
    exit 1
fi

# Bundle core server-loaded extension packages.
echo "Bundling Ruby Fast LSP extensions..."
rm -rf "$EXTENSION_DIR/extensions"
if [ "$SKIP_BUILDS" = false ]; then
    "$ROOT_DIR/extensions/sinatra-rust/build-and-test.sh"
    "$ROOT_DIR/extensions/cucumber-rust/build-and-test.sh"
    "$ROOT_DIR/extensions/minitest-ruby/build-and-test.sh"
    "$ROOT_DIR/extensions/rails-ruby/build-and-test.sh"
fi
mkdir -p "$EXTENSION_DIR/extensions/rspec-ruby/target/wasm32-wasip1/release"
cp "$ROOT_DIR/extensions/rspec-ruby/extension.toml" "$EXTENSION_DIR/extensions/rspec-ruby/"
cp "$ROOT_DIR/extensions/rspec-ruby/README.md" "$EXTENSION_DIR/extensions/rspec-ruby/"
cp \
    "$ROOT_DIR/extensions/rspec-ruby/target/wasm32-wasip1/release/rspec-ruby.wasm" \
    "$EXTENSION_DIR/extensions/rspec-ruby/target/wasm32-wasip1/release/"
mkdir -p "$EXTENSION_DIR/extensions/rails-ruby/target/wasm32-wasip1/release"
cp "$ROOT_DIR/extensions/rails-ruby/extension.toml" "$EXTENSION_DIR/extensions/rails-ruby/"
cp "$ROOT_DIR/extensions/rails-ruby/README.md" "$EXTENSION_DIR/extensions/rails-ruby/"
cp \
    "$ROOT_DIR/extensions/rails-ruby/target/wasm32-wasip1/release/ruby_fast_lsp_rails_extension.wasm" \
    "$EXTENSION_DIR/extensions/rails-ruby/target/wasm32-wasip1/release/"
mkdir -p "$EXTENSION_DIR/extensions/minitest-ruby/target/wasm32-wasip1/release"
cp "$ROOT_DIR/extensions/minitest-ruby/extension.toml" "$EXTENSION_DIR/extensions/minitest-ruby/"
cp "$ROOT_DIR/extensions/minitest-ruby/README.md" "$EXTENSION_DIR/extensions/minitest-ruby/"
cp \
    "$ROOT_DIR/extensions/minitest-ruby/target/wasm32-wasip1/release/ruby_fast_lsp_minitest_extension.wasm" \
    "$EXTENSION_DIR/extensions/minitest-ruby/target/wasm32-wasip1/release/"
mkdir -p "$EXTENSION_DIR/extensions/sinatra-rust/target/wasm32-wasip1/release"
cp "$ROOT_DIR/extensions/sinatra-rust/extension.toml" "$EXTENSION_DIR/extensions/sinatra-rust/"
cp "$ROOT_DIR/extensions/sinatra-rust/README.md" "$EXTENSION_DIR/extensions/sinatra-rust/"
cp \
    "$ROOT_DIR/extensions/sinatra-rust/target/wasm32-wasip1/release/ruby_fast_lsp_sinatra_extension.wasm" \
    "$EXTENSION_DIR/extensions/sinatra-rust/target/wasm32-wasip1/release/"
mkdir -p "$EXTENSION_DIR/extensions/cucumber-rust/target/wasm32-wasip1/release"
cp "$ROOT_DIR/extensions/cucumber-rust/extension.toml" "$EXTENSION_DIR/extensions/cucumber-rust/"
cp "$ROOT_DIR/extensions/cucumber-rust/README.md" "$EXTENSION_DIR/extensions/cucumber-rust/"
cp \
    "$ROOT_DIR/extensions/cucumber-rust/target/wasm32-wasip1/release/ruby_fast_lsp_cucumber_extension.wasm" \
    "$EXTENSION_DIR/extensions/cucumber-rust/target/wasm32-wasip1/release/"

# Navigate to extension directory and package
cd "$EXTENSION_DIR"
echo "Installing dependencies..."
npm install

echo "Packaging extension..."
vsce package

EXPECTED_VSIX="$EXTENSION_NAME-$EXTENSION_VERSION.vsix"
if [ ! -f "$EXPECTED_VSIX" ]; then
    echo "Error: expected versioned artifact $EXPECTED_VSIX was not produced"
    exit 1
fi

for platform in "${PLATFORMS[@]}"; do
    if [ "$platform" == "$CURRENT_PLATFORM" ]; then
        echo "Smoke testing packaged binary and bundled extensions..."
        node "$ROOT_DIR/editors/scripts/smoke_vsix.js" "$EXTENSION_DIR/$EXPECTED_VSIX"
        break
    fi
done

# Move the VSIX file to the target directory
mkdir -p "$TARGET_DIR"
mv "$EXPECTED_VSIX" "$TARGET_DIR/"

echo "VSIX package created successfully!"
echo "You can find the VSIX file in the target directory of your project."
echo "To install the extension, run: code --install-extension $TARGET_DIR/$EXTENSION_NAME-$EXTENSION_VERSION.vsix"
echo "The extension now includes binaries for the following platforms:"
for platform in "${built_platforms[@]}"; do
    case $platform in
        macos-x64)
            echo "  - macOS (Intel/x64)"
            ;;
        macos-arm64)
            echo "  - macOS (Apple Silicon/ARM64)"
            ;;
        linux-x64)
            echo "  - Linux (x64)"
            ;;
        linux-arm64)
            echo "  - Linux (ARM64)"
            ;;
        win32-x64)
            echo "  - Windows (x64)"
            ;;
        win32-arm64)
            echo "  - Windows (ARM64)"
            ;;
    esac
done

echo ""
echo "To build for additional platforms, use the --platforms option:"
echo "  ./editors/vscode/create_vsix.sh --platforms macos-x64,macos-arm64,linux-x64,linux-arm64,win32-x64,win32-arm64"
echo "  or"
echo "  ./editors/vscode/create_vsix.sh --platforms all"
