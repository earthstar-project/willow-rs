#!/bin/sh
[ -e targets.txt ] || cargo fuzz list > targets.txt
i=1; N="$(cat targets.txt | wc -l)"
while cargo fuzz run "$(head -n$i targets.txt | tail -n1)" "$@"; do
  i=$(( i % N + 1 ))
done