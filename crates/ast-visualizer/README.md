# Ruby AST Visualizer

A tool for visualizing Ruby Abstract Syntax Trees (AST) using the Ruby Prism parser.

## Overview

This tool provides a web interface for visualizing the AST of Ruby code. It uses the Ruby Prism parser to parse Ruby code and generate the AST, which is then displayed in an interactive tree view.

## Features

- Two-panel interface: Ruby code editor on the left, AST visualization on the right
- Real-time parsing and visualization of Ruby code
- Interactive tree view of the AST with collapsible nodes
- Syntax highlighting for Ruby code
- Error handling for invalid Ruby code

## Usage

1. Start the server:

```bash
cargo run -p ast-visualizer
```

2. Open a web browser and navigate to:

```
file:///path/to/ruby-fast-lsp/crates/ast-visualizer/static/index.html
```

3. Enter your Ruby code in the left panel
4. Click the "Parse AST" button to generate and visualize the AST

## How It Works

1. The server exposes an HTTP endpoint for parsing Ruby code
2. The web interface sends the Ruby code to the server
3. The server uses the Ruby Prism parser to parse the code and generate the AST
4. The AST is converted to a JSON format and sent back to the web interface
5. The web interface displays the AST in an interactive tree view

## Directory Structure

- `src/main.rs`: The server implementation
- `static/index.html`: The web interface
- `Cargo.toml`: The crate manifest

## Dependencies

- `actix-web`: Web server framework
- `actix-cors`: CORS support for the web server
- `ruby-prism`: Ruby parser
- `serde`: Serialization/deserialization framework
- `serde_json`: JSON support for serde
- `env_logger`: Logging framework
- `log`: Logging facade
