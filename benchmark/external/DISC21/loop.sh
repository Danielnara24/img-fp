#!/bin/bash
# loop.sh FIRST LAST: for each reference zip N in FIRST..LAST, download it
# (a downloader runs ahead, at most AHEAD zips on disk unprocessed), unzip it
# to refsN, delete the zip, run shard.sh N, keep shardN's report and dump,
# and delete refsN's images. Progress in loop.log; one line per step.
set -u
FIRST=$1; LAST=$2; AHEAD=2
D=/home/daniel/Documents/DISC21; Z=$D/zips
URL=https://dl.fbaipublicfiles.com/image_similarity_challenge/public
log() { echo "$(date '+%F %T') $*" >> "$D/loop.log"; }

fetch() {  # fetch N: download references_N.zip whole, marked .done when complete
    local n=$1 f=$Z/references_$n.zip want got
    want=$(curl -sIL "$URL/references_$n.zip" | awk 'tolower($1)=="content-length:"{v=$2} END{gsub("\r","",v); print v}')
    for try in 1 2 3 4 5; do
        curl -sSL --retry 5 --retry-delay 10 -C - -o "$f" "$URL/references_$n.zip"
        got=$(stat -c %s "$f" 2>/dev/null || echo 0)
        [ -n "$want" ] && [ "$got" = "$want" ] && { touch "$f.done"; log "downloaded $n ($got bytes)"; return 0; }
        log "download $n try $try: $got of $want bytes"; sleep 30
    done
    log "download $n FAILED"; return 1
}

downloader() {
    for n in $(seq "$FIRST" "$LAST"); do
        # wait while AHEAD zips are on disk, downloaded and not yet taken
        while [ "$(ls "$Z"/references_*.zip.done 2>/dev/null | wc -l)" -ge "$AHEAD" ]; do sleep 20; done
        [ -e "$Z/references_$n.zip.done" ] || fetch "$n" || return 1
    done
}

downloader &
dl=$!
echo $$ > "$D/loop.pid"
log "loop $FIRST..$LAST started (pid $$, downloader $dl)"

for n in $(seq "$FIRST" "$LAST"); do
    while [ ! -e "$Z/references_$n.zip.done" ]; do
        kill -0 $dl 2>/dev/null || [ -e "$Z/references_$n.zip.done" ] || { log "downloader gone before $n; stopping"; exit 1; }
        sleep 20
    done
    rm -rf "$D/refs$n"; mkdir -p "$D/refs$n"
    if ! unzip -q "$Z/references_$n.zip" -d "$D/refs$n"; then
        log "unzip $n FAILED; zip kept, stopping"; exit 1
    fi
    cnt=$(ls "$D/refs$n/images/references" | wc -l)
    log "unzipped $n: $cnt files"
    rm -f "$Z/references_$n.zip" "$Z/references_$n.zip.done"
    ls "$D/refs$n/images/references" | sed 's/\..*//' > "$D/refs$n.ids"
    log "shard $n started"
    "$D/shard.sh" "$n"
    st=$(tail -1 "$D/shard$n/time.txt")
    log "shard $n ended: $st, $(grep Elapsed "$D/shard$n/time.txt" | awk '{print $NF}')"
    case "$st" in
        "exit 0"|"exit 2") rm -rf "$D/refs$n/images"; log "deleted refs$n images" ;;
        *) log "shard $n did not finish; images kept, stopping"; kill $dl 2>/dev/null; exit 1 ;;
    esac
done
log "loop done"
