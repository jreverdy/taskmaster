#!/bin/bash
# Service that should only start when requested
echo "Service started: $$"
sleep 2
while true; do
    echo "Service alive"
    sleep 5
done
