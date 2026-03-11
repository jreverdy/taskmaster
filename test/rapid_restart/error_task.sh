#!/bin/bash
# Task that sometimes fails and needs retries
RANDOM_FAIL=$((RANDOM % 3))
if [ $RANDOM_FAIL -eq 0 ]; then
    echo "Random failure occurred"
    exit 1
else
    echo "Task succeeded with exit code $RANDOM_FAIL"
    exit $RANDOM_FAIL
fi
