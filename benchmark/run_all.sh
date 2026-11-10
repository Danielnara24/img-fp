#!/bin/bash
# Run every installed competitor over a corpus, each at its own defaults,
# into one output directory.
#
#   ./run_all.sh /home/daniel/Documents/IMGS out/default
#
# This is for collecting proposals into the pool, not for timing them. The
# runs are sequential but nothing here enforces a cold cache or a thermal
# cooldown, so do not quote the runtime_seconds it records as benchmark
# numbers -- see TOOLS.md.
set -u

CORPUS="${1:?usage: run_all.sh CORPUS OUTDIR}"
OUT="${2:?usage: run_all.sh CORPUS OUTDIR}"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
R="$HERE/runners"
PY="$REPO/vendor/venv/bin/python"
PY_IDD="$REPO/vendor/venv-imagededup/bin/python"

mkdir -p "$OUT"
failed=()

run() {
  local name="$1"; shift
  if [ -f "$OUT/$name.json" ]; then
    echo "== $name (already present, skipping)"
    return
  fi
  echo "== $name"
  if ! "$@" >"$OUT/$name.log" 2>&1; then
    echo "   FAILED, see $OUT/$name.log"
    failed+=("$name")
  else
    tail -1 "$OUT/$name.log"
  fi
}

# Exact-byte floor: what needs no perceptual matching at all.
echo "== fclones"
"$REPO/vendor/bin/fclones" group "$CORPUS" -f json -o "$OUT/fclones.json" \
  >"$OUT/fclones.log" 2>&1 || failed+=("fclones")

run czkawka        "$PY" "$R/run_czkawka.py"        "$CORPUS" -o "$OUT/czkawka.json"
run pdq            "$PY" "$R/run_pdq.py"            "$CORPUS" -o "$OUT/pdq.json"
run imgdupes_phash "$PY" "$R/run_imgdupes.py"       "$CORPUS" -o "$OUT/imgdupes_phash.json" --method phash -d 4
run findimagedupes "$PY" "$R/run_findimagedupes.py" "$CORPUS" -o "$OUT/findimagedupes.json"
run dupeguru       "$PY" "$R/run_dupeguru.py"       "$CORPUS" -o "$OUT/dupeguru.json"
run difpy          "$PY" "$R/run_difpy.py"          "$CORPUS" -o "$OUT/difpy.json"

run imagededup_phash "$PY_IDD" "$R/run_imagededup.py" "$CORPUS" \
    -o "$OUT/imagededup_phash.json" --method phash
run imagededup_cnn   "$PY_IDD" "$R/run_imagededup.py" "$CORPUS" \
    -o "$OUT/imagededup_cnn.json" --method cnn

# The oracle. Slowest by far on a CPU-only box; caches its embeddings so a
# threshold sweep afterwards is free.
run sscd "$PY_IDD" "$R/run_sscd.py" "$CORPUS" -o "$OUT/sscd.json" \
    --embeddings "$OUT/sscd_embeddings.npz" -b 8 -j 2

echo
if [ ${#failed[@]} -eq 0 ]; then
  echo "all tools completed -> $OUT"
else
  echo "FAILED: ${failed[*]}"
  exit 1
fi
