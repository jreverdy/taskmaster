#!/bin/bash
# Long-running task that needs extended stoptime
trap 'echo "Cleaning up..." && sleep 2 && exit 0' SIGTERM
echo "Starting long cleanup task"
for i in {1..30}; do
    echo "Working... $i/30"
    sleep 0.5
done
exit 0
