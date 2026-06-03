#!/bin/bash

for i in $(seq 1 100000)
do
    echo "[STDOUT] Message $i Lorem ipsum dolor sit amet consectetur adipiscing elit."

    echo "[STDERR] Error $i Lorem ipsum dolor sit amet consectetur adipiscing elit." >&2
done