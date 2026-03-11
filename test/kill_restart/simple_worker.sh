#!/bin/bash

# Simple worker process for kill-and-restart testing
# This process should be able to be killed and restarted

counter=0
while true; do
    counter=$((counter + 1))
    echo "Cycle $counter at $(date '+%H:%M:%S')" >&2
    sleep 1
done
