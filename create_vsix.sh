#!/bin/bash
set -e

# Configuration
EXTENSION_NAME="ruby-fast-lsp"
EXTENSION_VERSION=$(grep -m 1 "version" Cargo.toml | cut -d '"' -f 2)
EXTENSION_DIR="./vsix"
TARGET_DIR="./target"
REBUILD_LSP=false
SKIP_BUILDS=false
SELECTED_PLATFORMS=""

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
    
    binary_path="./target/${target}/release/${binary_name}"
    
    # Check if we need to build
    if [ ! -f "$binary_path" ] || [ "$REBUILD_LSP" = true ]; then
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
            
            if ! cross build --release --target "$target"; then
                echo "Failed to cross-compile for $platform ($target)"
                return 1
            fi
        else
            # For native builds, use cargo directly
            if ! cargo build --release --target "$target"; then
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

# Navigate to extension directory and package
cd "$EXTENSION_DIR"
echo "Installing dependencies..."
npm install

echo "Packaging extension..."
vsce package

# Move the VSIX file to the target directory
mv *.vsix "../$TARGET_DIR/"

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
echo "  ./create_vsix.sh --platforms macos-x64,macos-arm64,linux-x64,linux-arm64,win32-x64,win32-arm64"
echo "  or"
echo "  ./create_vsix.sh --platforms all"
