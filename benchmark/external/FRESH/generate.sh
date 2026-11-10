#!/bin/bash
# Fresh held-out corpus (session v56): INRIA Holidays photos as seeds, groups split by hash
# into dev / test, 12 transforms each. Then each half combined into ~/Documents/HOLI-{DEV,TEST}.
cd /home/daniel/Documents/Vscode_repositories/img-fp/benchmark/corpus || exit 1
PY=../../vendor/venv/bin/python; V=/home/daniel/Documents/FRESH
ONLY=jpeg_mid,scale_small,crop_off_centre,crop_half,crop_slight,rotate_small,flip_h,greyscale,watermark_corner,embed_small,collage_cell,phone_screenshot
echo "=== generate $(date)"
for c in dev-a dev-b test-a test-b; do
  nice -n 19 $PY make_variants.py --corpus $V/parts/$c --only $ONLY > $V/gen-$c.log 2>&1 &
done
wait
tail -n 5 $V/gen-*.log
$PY make_combined.py --src $V/parts --parts dev-a,dev-b --out /home/daniel/Documents/HOLI-DEV --clean
$PY make_combined.py --src $V/parts --parts test-a,test-b --out /home/daniel/Documents/HOLI-TEST --clean
echo "=== done $(date)"
