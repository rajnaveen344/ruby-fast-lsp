#!/bin/bash

set -e

# Create vendor directory if it doesn't exist
mkdir -p vendor

# Remove existing tree-sitter-ruby directory if it exists
if [ -d "vendor/tree-sitter-ruby" ]; then
    echo "Removing existing tree-sitter-ruby directory..."
    rm -rf vendor/tree-sitter-ruby
fi

# Clone the tree-sitter-ruby repository
echo "Cloning tree-sitter-ruby..."
git clone https://github.com/tree-sitter/tree-sitter-ruby.git vendor/tree-sitter-ruby

# Check if the clone was successful
if [ -d "vendor/tree-sitter-ruby" ]; then
    echo "Tree-sitter Ruby grammar cloned successfully!"
    
    # Check if src directory exists and has the parser.c file
    if [ ! -f "vendor/tree-sitter-ruby/src/parser.c" ]; then
        echo "parser.c not found, checking if we need to generate it..."
        
        # Check if there's a grammar.js file
        if [ -f "vendor/tree-sitter-ruby/grammar.js" ]; then
            echo "Found grammar.js, attempting to generate parser.c..."
            
            # Check if tree-sitter CLI is installed
            if command -v tree-sitter &> /dev/null; then
                echo "tree-sitter CLI found, generating parser..."
                cd vendor/tree-sitter-ruby
                tree-sitter generate
                cd ../..
            else
                echo "tree-sitter CLI not found. Installing it with npm..."
                npm install -g tree-sitter-cli
                
                echo "Generating parser with tree-sitter CLI..."
                cd vendor/tree-sitter-ruby
                tree-sitter generate
                cd ../..
            fi
        else
            echo "ERROR: grammar.js not found in tree-sitter-ruby repository."
            exit 1
        fi
    fi
    
    # Final check to ensure parser.c exists
    if [ -f "vendor/tree-sitter-ruby/src/parser.c" ]; then
        echo "parser.c found. Tree-sitter Ruby grammar is ready!"
    else
        echo "ERROR: Failed to generate parser.c"
        exit 1
    fi
else
    echo "ERROR: Failed to clone tree-sitter-ruby repository."
    exit 1
fi
