#!/bin/bash
# Worker script for scaling tests
echo "Worker $$  started"
for i in {1..100}; do
    echo "Worker $$ - iteration $i"
    sleep 0.5
done
exit 0
