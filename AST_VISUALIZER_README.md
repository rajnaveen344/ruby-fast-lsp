# Ruby AST Visualizer

This tool helps you visualize the Abstract Syntax Tree (AST) of Ruby code using the Ruby Prism parser. It's designed to help you understand how the AST is formed for a given code snippet.

## Features

- Two-panel interface: Ruby code editor on the left, AST visualization on the right
- Real-time parsing and visualization of Ruby code
- Interactive tree view of the AST with collapsible nodes
- Syntax highlighting for Ruby code
- Error handling for invalid Ruby code

## How to Use

### Starting the Server

1. Make sure you have Rust and Cargo installed
2. Navigate to the project directory
3. Run the AST server:

```bash
cargo run --bin ast_server
```

This will start the server at http://127.0.0.1:8080.

### Using the Visualizer

1. Open `ast_visualizer.html` in your web browser
2. Enter your Ruby code in the left panel
3. Click the "Parse AST" button to generate and visualize the AST
4. Explore the AST by expanding and collapsing nodes in the tree view

### Example

The visualizer comes with a sample Ruby code snippet that defines a `Person` class with `initialize` and `greet` methods. You can modify this code or replace it with your own to see how different Ruby constructs are represented in the AST.

## Understanding the AST

The AST is represented as a tree of nodes, where each node has:

- **Type**: The type of the node (e.g., CLASS, DEF, SEND)
- **Name**: The name associated with the node (e.g., class name, method name)
- **Value**: The value associated with the node (e.g., string content, integer value)
- **Children**: Child nodes that are part of this node
- **Parameters**: Method parameters (for DEF nodes)
- **Receiver**: The receiver of a method call (for SEND nodes)
- **Arguments**: Arguments to a method call (for SEND nodes)

Common node types include:

- **PROGRAM**: The root node of the AST
- **CLASS**: A class definition
- **DEF**: A method definition
- **SEND**: A method call
- **CONST**: A constant reference
- **LVAR**: A local variable reference
- **IVAR**: An instance variable reference
- **STR**: A string literal
- **INT**: An integer literal
- **DSTR**: An interpolated string

## Troubleshooting

- If the server fails to start, make sure port 8080 is available
- If parsing fails, check your Ruby code for syntax errors
- If the visualization doesn't appear, check the browser console for errors

## Technical Details

The visualizer consists of two main components:

1. **Frontend**: An HTML page with JavaScript for the UI and visualization
2. **Backend**: A Rust server that uses the Ruby Prism parser to parse Ruby code and generate the AST

The frontend sends the Ruby code to the backend, which parses it and returns the AST as JSON. The frontend then visualizes the AST as an interactive tree.

## Contributing

Contributions are welcome! Feel free to submit issues or pull requests to improve the visualizer.
