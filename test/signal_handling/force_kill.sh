#!/bin/bash
# Script that ignores signals and needs SIGKILL
trap '' SIGTERM
while true; do
    echo "Running (ignoring SIGTERM)..."
    sleep 1
done
