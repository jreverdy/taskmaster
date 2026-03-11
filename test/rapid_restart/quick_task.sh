#!/bin/bash
# Quick task that completes rapidly
echo "Starting rapid task $$"
for i in {1..3}; do
    echo "Quick iteration $i"
    sleep 0.2
done
echo "Task complete"
exit 0
