#!/bin/bash
# Script that ignores SIGTERM and requires extended stoptime
trap '' SIGTERM
echo "Process started, ignoring SIGTERM"
iteration=0
while true; do
    echo "Stubborn process iteration $((++iteration))"
    sleep 1
    if [ $iteration -gt 10 ]; then
        echo "Finally giving up"
        exit 1
    fi
done
