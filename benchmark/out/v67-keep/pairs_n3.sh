#!/bin/bash
cd /home/daniel/Documents/Vscode_repositories/img-fp
vendor/venv/bin/python benchmark/pairs.py run benchmark/out/v67-keep/n3/img-fp benchmark/out/v67-keep/n3/pairs --cache benchmark/out/v67-keep/cache/c.db > benchmark/out/v67-keep/n3/pairs.log 2>&1
echo done >> benchmark/out/v67-keep/n3/pairs.log
