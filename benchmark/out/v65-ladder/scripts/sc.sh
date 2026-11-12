#!/bin/bash
# sc NAME...: analyse.py summary
cd /home/daniel/Documents/Vscode_repositories/img-fp
for n in "$@"; do vendor/venv/bin/python benchmark/analyse.py --corpus=/home/daniel/Documents/IMGS-ALL2 /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65/out/$n.json > /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65/out/$n.an 2>&1; sed -n 3,5p /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65/out/$n.an; grep -E "cross .*: [0-9,]{3,} pairs" /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65/out/$n.an | head -6; done
