#!/bin/bash
# Base (0.76.0 + default 0.8), vocab (+ contiguous leaves), new (+ per-axis
# bound): IMGS-ALL and IMGS-ALL2 cold for base then cached for the others,
# then DISC21 refs0 cached, vocab and base alternated for the quantise timing.
F=/home/daniel/Documents/DISC21/fixtest; D=/home/daniel/Documents
one() { # tag bin corpus cache extra...
    local tag=$1 bin=$2 corpus=$3 cache=$4; shift 4
    echo "$(date +%T) start $tag"
    /usr/bin/time -v "$F/img-fp-$bin" "$corpus" -t 8 -v --no-last --cache "$cache" \
        --log-file "$F/$tag.log" -o "$F/$tag.json" "$@" 2> "$F/$tag.time"
    echo "$(date +%T) end $tag exit $?"
}
one all-base base $D/IMGS-ALL $F/all.bin -r -x '*'
one all-vocab vocab $D/IMGS-ALL $F/all.bin -r -x '*'
one all-new new $D/IMGS-ALL $F/all.bin -r -x '*'
one all2-base base $D/IMGS-ALL2 $F/all2.bin -r -x '*'
one all2-new new $D/IMGS-ALL2 $F/all2.bin -r -x '*'
R=$D/DISC21/refs0/images/references; C=$D/DISC21/cache/cache.bin
one disc-vocab vocab $R $C
one disc-base base $R $C
one disc-new new $R $C
echo "$(date +%T) all done"
