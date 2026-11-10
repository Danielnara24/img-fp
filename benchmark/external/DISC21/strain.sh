#!/bin/bash
# strain.sh: dev queries --against zips 0 and 1 at --work-size 1024, in one run,
# own cache, correlation 0.5 + dump; downloads and unzips zip 1 first.
set -u
D=/home/daniel/Documents/DISC21; Z=$D/zips; OUT=$D/strain
URL=https://dl.fbaipublicfiles.com/image_similarity_challenge/public
BIN=/home/daniel/Documents/Vscode_repositories/img-fp/target/plain/release/img-fp
mkdir -p "$OUT"; log() { echo "$(date '+%F %T') $*" >> "$OUT/strain.log"; }
echo $$ > "$OUT/script.pid"
if [ ! -d "$D/refs1/images/references" ] || [ "$(ls "$D/refs1/images/references" | wc -l)" -lt 50000 ]; then
    f=$Z/references_1.zip
    want=$(curl -sIL "$URL/references_1.zip" | awk 'tolower($1)=="content-length:"{v=$2} END{gsub("\r","",v); print v}')
    for try in 1 2 3 4 5; do
        curl -sSL --retry 5 --retry-delay 10 -C - -o "$f" "$URL/references_1.zip"
        [ "$(stat -c %s "$f")" = "$want" ] && break; log "download try $try short"; sleep 30
    done
    [ "$(stat -c %s "$f")" = "$want" ] || { log "download FAILED"; exit 1; }
    log "downloaded zip 1"
    rm -rf "$D/refs1"; mkdir -p "$D/refs1"; unzip -q "$f" -d "$D/refs1" || { log "unzip FAILED"; exit 1; }
    rm -f "$f"; log "unzipped zip 1: $(ls "$D/refs1/images/references" | wc -l) files"
fi
used() { df -B1 --output=used "$D" | tail -1; }
base=$(used)
log "run started"
echo 1000 > /proc/self/oom_score_adj
/usr/bin/time -v "$BIN" "$D/queries/images/queries" --against "$D/refs0/images/references" --against "$D/refs1/images/references" \
    --work-size 1024 --min-pixel-correlation 0.5 -t 8 -v --cache "$D/cache/w1024.bin" --log-file "$OUT/run.log" \
    -o "$OUT/report.json" --dump "$OUT/dump.csv" 2> "$OUT/time.txt" &
pid=$!; echo $pid > "$OUT/pid"
echo -e "t\tanon_swap_kb\tdisk_delta_b\tavail_mb" > "$OUT/samples.tsv"
t0=$(date +%s)
while kill -0 $pid 2>/dev/null; do
    kb=$(for p in $pid $(pgrep -P $pid); do awk '/^RssAnon|^VmSwap/{s+=$2} END{print s+0}' /proc/$p/status 2>/dev/null; done | awk '{s+=$1} END{print s+0}')
    av=$(awk '/MemAvailable/{print int($2/1024)}' /proc/meminfo)
    echo -e "$(( $(date +%s) - t0 ))\t${kb:-0}\t$(( $(used) - base ))\t$av" >> "$OUT/samples.tsv"
    sleep 5
done
wait $pid; st=$?; echo "exit $st" >> "$OUT/time.txt"; log "run ended: exit $st"
