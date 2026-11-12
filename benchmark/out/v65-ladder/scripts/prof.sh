#!/bin/bash
cd /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65
for m in "" "--fast"; do n=head${m:+-fast}
rm -f /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/d/c.db.matched
/usr/bin/time -v /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/v65/img-fp-head /home/daniel/Documents/IMGS-ALL2 -r -x '*' -t 8 -v --no-last --cache /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/d/c.db $m -o /dev/null 2> $n.err
done
rm -f /tmp/claude-1000/-home-daniel-Documents-Vscode-repositories-img-fp/79c88b7d-101f-408e-a430-7e7a78976241/scratchpad/d/c.db.matched
echo done > prof.done
