#!/bin/bash

# Parse command line arguments
PORT=${1:-8080}  # Default port is 8080 if not specified

# Start the AST visualizer server with the specified port
echo "Starting Ruby AST Visualizer server on port $PORT..."
PORT=$PORT cargo run -p ast-visualizer &
SERVER_PID=$!

# Wait for the server to start
echo "Waiting for server to start..."
sleep 2

# Get the actual port from the server output (in case the specified port was not available)
ACTUAL_PORT=$(ps -p $SERVER_PID -o command= | grep -o "http://127.0.0.1:[0-9]*" | grep -o "[0-9]*$" || echo $PORT)

# Open the server URL in the default browser
echo "Opening AST visualizer in browser at http://localhost:$ACTUAL_PORT..."
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    open "http://localhost:$ACTUAL_PORT"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux
    xdg-open "http://localhost:$ACTUAL_PORT"
elif [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" ]]; then
    # Windows
    start "http://localhost:$ACTUAL_PORT"
else
    echo "Please open http://localhost:$ACTUAL_PORT in your browser manually"
fi

# Wait for user to press Ctrl+C
echo "Press Ctrl+C to stop the server"
trap "kill $SERVER_PID; echo 'Server stopped'; exit 0" INT
wait $SERVER_PID
