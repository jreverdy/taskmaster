#!/bin/bash

# This script simulates a slow startup process
# It takes 3 seconds to complete initialization before it's "ready"

echo "Starting initialization..."
sleep 3
echo "Initialization complete, running main loop..."

# Run forever, printing status
while true; do
    sleep 1
    echo "Process running at $(date '+%H:%M:%S')"
done
