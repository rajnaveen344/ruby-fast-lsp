#!/bin/bash

# Start the AST visualizer server
echo "Starting Ruby AST Visualizer server..."
cargo run -p ast-visualizer &
SERVER_PID=$!

# Wait for the server to start
echo "Waiting for server to start..."
sleep 2

# Open the server URL in the default browser
echo "Opening AST visualizer in browser..."
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    open "http://localhost:3000"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux
    xdg-open "http://localhost:3000"
elif [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" ]]; then
    # Windows
    start "http://localhost:3000"
else
    echo "Please open http://localhost:3000 in your browser manually"
fi

# Wait for user to press Ctrl+C
echo "Press Ctrl+C to stop the server"
trap "kill $SERVER_PID; echo 'Server stopped'; exit 0" INT
wait $SERVER_PID
