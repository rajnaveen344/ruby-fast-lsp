#!/bin/bash
# Pre-zips Ruby stubs for each version to speed up VSIX packaging
# The LSP server reads directly from these zip files at runtime
#
# Features:
# - Skips zipping if zip file is already up-to-date (newer than source directory)
# - Use --force to re-zip everything

set -e

STUBS_DIR="vsix/stubs"
OUTPUT_DIR="vsix/stubs-zipped"
FORCE=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --force|-f)
            FORCE=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--force]"
            exit 1
            ;;
    esac
done

# Create output directory
mkdir -p "$OUTPUT_DIR"

zipped_count=0
skipped_count=0

echo "Checking Ruby stubs..."

for version_dir in "$STUBS_DIR"/rubystubs*; do
    if [ -d "$version_dir" ]; then
        version_name=$(basename "$version_dir")
        output_file="$OUTPUT_DIR/${version_name}.zip"
        
        # Check if we need to re-zip
        needs_zip=false
        
        if [ "$FORCE" = true ]; then
            needs_zip=true
        elif [ ! -f "$output_file" ]; then
            needs_zip=true
        else
            # Check if any source file is newer than the zip
            newest_source=$(find "$version_dir" -name "*.rb" -newer "$output_file" 2>/dev/null | head -1)
            if [ -n "$newest_source" ]; then
                needs_zip=true
            fi
        fi
        
        if [ "$needs_zip" = true ]; then
            echo "  Zipping $version_name..."
            
            # Create zip with maximum compression, storing files at root level
            (cd "$version_dir" && zip -9 -q "../../stubs-zipped/${version_name}.zip" *.rb)
            
            # Show size info
            original_size=$(du -sk "$version_dir" | cut -f1)
            zip_size=$(du -sk "$output_file" | cut -f1)
            echo "    $version_name: ${original_size}KB -> ${zip_size}KB"
            ((zipped_count++))
        else
            ((skipped_count++))
        fi
    fi
done

echo ""
if [ $zipped_count -eq 0 ] && [ $skipped_count -gt 0 ]; then
    echo "All $skipped_count zip files are up-to-date (use --force to re-zip)"
else
    echo "Done! Zipped: $zipped_count, Skipped: $skipped_count"
fi

echo ""
echo "Total sizes:"
du -sh "$STUBS_DIR"
du -sh "$OUTPUT_DIR"

