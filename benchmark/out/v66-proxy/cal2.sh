#!/bin/bash
cd "$(dirname "$0")"
while kill -0 1600741 2>/dev/null; do sleep 20; done
mkdir -p ch1; ./one.sh p-h1 h1 ch1/c.db proxy
echo DONE2 >> log
