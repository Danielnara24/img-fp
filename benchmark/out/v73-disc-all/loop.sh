#!/bin/bash
# v73: DISC21's whole dev set (50,000 queries against the 1,000,000 references,
# 20 shards of 50,000) at the new defaults (correlation 0.65, points 5), no
# bar passed. Each shard: download references_N.zip if refsN is not
# unpacked (one ahead), unzip, run the queries --against it on one cache
# (a copy of cache57; --prune-cache keeps only the queries and this shard's
# references, so it does not grow by 5 GB a shard), keep the report, delete
# the shard's images (refs0 and refs1 were already unpacked and are kept).
# Resumes: a shard with a report is done. (The first pass wrote `-o X.json.part`,
# which img-fp took for the text layout: shardN.txt, kept, cannot be scored.)
set -u
O=/home/daniel/Documents/Vscode_repositories/img-fp/benchmark/out/v73-disc-all
D=/home/daniel/Documents/DISC21; Z=$D/zips; C=$O/cache/c.db
URL=https://dl.fbaipublicfiles.com/image_similarity_challenge/public
echo 1000 > /proc/self/oom_score_adj
log() { echo "$(date '+%F %T') $*" >> "$O/loop.log"; }
free_gb() { df -BG --output=avail "$D" | tail -1 | tr -dc 0-9; }
need() { [ -d "$D/refs$1/images/references" ] && [ "$(ls "$D/refs$1/images/references" | wc -l)" = 50000 ]; }
fetch() {
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
  for n in $(seq 0 19); do
    [ -s "$O/shard$n.json" ] && continue; need $n && continue
    while [ "$(ls "$Z"/references_*.zip.done 2>/dev/null | wc -l)" -ge 1 ]; do sleep 20; done
    [ -e "$Z/references_$n.zip.done" ] || fetch "$n" || return 1
  done
}
mkdir -p "$O/cache"
if [ ! -e "$C" ]; then
  cp "$D/cache57/c.db" "$D/cache57/c.db.pack.1" "$O/cache/" && touch "$O/cache/c.db.lock"; log "cache copied"
fi
downloader & dl=$!
echo $$ > "$O/loop.pid"; log "loop started (pid $$, downloader $dl)"
for n in $(seq 0 19); do
  [ -s "$O/shard$n.json" ] && continue
  if ! need $n; then
    while [ ! -e "$Z/references_$n.zip.done" ]; do
      kill -0 $dl 2>/dev/null || [ -e "$Z/references_$n.zip.done" ] || { log "downloader gone before $n; stopping"; exit 1; }
      sleep 20
    done
    [ "$(free_gb)" -lt 15 ] && { log "only $(free_gb) GB free before unzipping $n; stopping"; kill $dl; exit 1; }
    rm -rf "$D/refs$n"; mkdir -p "$D/refs$n"
    unzip -q "$Z/references_$n.zip" -d "$D/refs$n" || { log "unzip $n FAILED; stopping"; kill $dl; exit 1; }
    rm -f "$Z/references_$n.zip" "$Z/references_$n.zip.done"
    log "unzipped $n: $(ls "$D/refs$n/images/references" | wc -l) files, $(free_gb) GB free"
  fi
  rm -f "$C.matched"
  /usr/bin/time -v "$O/img-fp" "$D/queries/images/queries" --against "$D/refs$n/images/references" \
      -t 8 -v --no-last --cache "$C" --prune-cache --format json -o "$O/shard$n.json" 2> "$O/shard$n.err"
  rc=$?; rm -f "$C.matched"
  case $rc in
    0|2) python3 -c "import json,sys; json.load(open(sys.argv[1]))['pairs']" "$O/shard$n.json" || { log "shard $n report is not JSON; stopping"; kill $dl 2>/dev/null; exit 1; }
         log "shard $n done: $(grep -E 'User time|Elapsed' "$O/shard$n.err" | awk '{print $NF}' | tr '\n' ' ')$(free_gb) GB free"
         [ $n -ge 2 ] && rm -rf "$D/refs$n/images" ;;
    *) log "shard $n exit $rc; stopping"; kill $dl 2>/dev/null; exit 1 ;;
  esac
done
log "loop done"
