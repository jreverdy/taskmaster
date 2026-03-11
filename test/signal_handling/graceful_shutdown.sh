#!/bin/bash
# Script that handles SIGTERM gracefully
trap 'echo "Received SIGTERM, shutting down gracefully" && exit 0' SIGTERM
while true; do
    echo "Running..."
    sleep 1
done
