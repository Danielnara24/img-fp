#!/bin/bash
# memwatch.sh PID OUT: RssAnon + swap of PID every 2 s until it exits.
echo -e "t\tanon_swap_kb" > "$2"
while kill -0 "$1" 2>/dev/null; do
    echo -e "$(date +%s)\t$(awk '/^RssAnon|^VmSwap/{s+=$2} END{print s+0}' /proc/$1/status)" >> "$2"
    sleep 2
done
