#!/usr/bin/env bash
# Run every tool at its REAL configuration — the one a user gets.
#
# The opposite of run_pool_butterfly.sh. There the settings were deliberately
# over-permissive to build a labelling queue; here each tool runs as shipped,
# because "what does this tool do when you install it and point it at a
# folder" is the question a benchmark is supposed to answer. Where a tool has
# a documented better setting that its defaults do not use, it gets a second
# row rather than a silent upgrade — czkawka's -z/--geometric-invariance are
# worth a factor of seven and the comparison should show both.
#
#     ./run_bench.sh /home/daniel/Documents/IMGS/archive/test out/bench-butterfly
#     ./run_bench.sh /home/daniel/Documents/IMGS               out/bench-full
set -u

ROOT="${1:?usage: run_bench.sh ROOT OUTDIR}"
OUT="${2:?usage: run_bench.sh ROOT OUTDIR}"
R="$(dirname "$0")/runners"
V="$(dirname "$0")/../vendor"
PY="$V/venv/bin/python"
PYT="$V/venv-imagededup/bin/python"     # the venv that has torch
mkdir -p "$OUT"

run() {
  local name="$1"; shift
  if [ -s "$OUT/$name.json" ]; then echo "== $name: present, skipping"; return; fi
  echo "== $name"
  local t0=$SECONDS
  if "$@" -o "$OUT/$name.json" >"$OUT/$name.log" 2>&1; then
    echo "   ok in $((SECONDS-t0))s"
  else
    echo "   FAILED after $((SECONDS-t0))s -- $OUT/$name.log"
  fi
}

# Byte-identical floor. Everything above this should beat it or justify itself.
FCLONES="$V/bin/fclones"
if [ -x "$FCLONES" ] && [ ! -s "$OUT/fclones.json" ]; then
  echo "== fclones"
  "$FCLONES" group --format json "$ROOT" >"$OUT/fclones.json" 2>"$OUT/fclones.log" \
    && echo "   ok" || echo "   FAILED"
fi

run czkawka_default   "$PY"  "$R/run_czkawka.py" "$ROOT"
run czkawka_tuned     "$PY"  "$R/run_czkawka.py" "$ROOT" \
    -z Lanczos3 --geometric-invariance mirror-flip-rotate90
run pdq               "$PY"  "$R/run_pdq.py" "$ROOT" -d 31
run imagededup_phash  "$PYT" "$R/run_imagededup.py" "$ROOT" --method phash -d 10
run imagededup_cnn    "$PYT" "$R/run_imagededup.py" "$ROOT" --method cnn -s 0.9
run imgdupes_phash    "$PY"  "$R/run_imgdupes.py" "$ROOT" --method phash -d 4
run difpy             "$PY"  "$R/run_difpy.py" "$ROOT"
run dupeguru          "$PY"  "$R/run_dupeguru.py" "$ROOT" -t 95
run findimagedupes    "$PY"  "$R/run_findimagedupes.py" "$ROOT" -t 90%
run sscd              "$PYT" "$R/run_sscd.py" "$ROOT" -t 0.6 \
    --embeddings "$OUT/sscd.npz"

echo
echo "== done -> $OUT"
