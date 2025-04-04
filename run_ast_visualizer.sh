#!/bin/bash

# Start the AST visualizer server
echo "Starting Ruby AST Visualizer server..."
cargo run -p ast-visualizer &
SERVER_PID=$!

# Wait for the server to start
echo "Waiting for server to start..."
sleep 2

# Open the HTML page in the default browser
echo "Opening AST visualizer in browser..."
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    open "file://$(pwd)/crates/ast-visualizer/static/index.html"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux
    xdg-open "file://$(pwd)/crates/ast-visualizer/static/index.html"
elif [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" ]]; then
    # Windows
    start "file://$(pwd)/crates/ast-visualizer/static/index.html"
else
    echo "Please open crates/ast-visualizer/static/index.html in your browser manually"
fi

# Wait for user to press Ctrl+C
echo "Press Ctrl+C to stop the server"
trap "kill $SERVER_PID; echo 'Server stopped'; exit 0" INT
wait $SERVER_PID
