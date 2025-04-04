#!/bin/bash

# Start the AST server in the background
echo "Starting AST server..."
cargo run --bin ast_server &
SERVER_PID=$!

# Wait for the server to start
echo "Waiting for server to start..."
sleep 2

# Open the HTML page in the default browser
echo "Opening AST visualizer in browser..."
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    open ast_visualizer.html
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux
    xdg-open ast_visualizer.html
elif [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" ]]; then
    # Windows
    start ast_visualizer.html
else
    echo "Please open ast_visualizer.html in your browser manually"
fi

# Wait for user to press Ctrl+C
echo "Press Ctrl+C to stop the server"
trap "kill $SERVER_PID; echo 'Server stopped'; exit 0" INT
wait $SERVER_PID
