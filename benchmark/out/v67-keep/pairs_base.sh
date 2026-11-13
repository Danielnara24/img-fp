#!/bin/bash
cd /home/daniel/Documents/Vscode_repositories/img-fp
vendor/venv/bin/python benchmark/pairs.py run benchmark/out/v67-keep/base/img-fp benchmark/out/v67-keep/base/pairs --cache benchmark/out/v67-keep/cache/c.db > benchmark/out/v67-keep/base/pairs.log 2>&1
echo done >> benchmark/out/v67-keep/base/pairs.log
