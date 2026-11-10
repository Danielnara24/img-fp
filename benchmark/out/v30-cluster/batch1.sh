set -e
A=./acc.py
for ws in 384 512 640 1024; do
  python3 $A elong${ws}_base elong $ws 0 0 elong_base$ws
  python3 $A elong${ws}_base_demote elong $ws 0 1 elong_base$ws
  python3 $A elong${ws}_areaW_demote elong $ws w 1 elong_areaW$ws
  python3 $A elong${ws}_area_demote elong $ws 1 1 elong_area$ws
done
for c in imgs imgs2 imgs3 imgs4; do
  python3 $A ${c}512_base $c 512 0 0 ${c}_base512
  python3 $A ${c}512_base_demote $c 512 0 1 ${c}_base512
  python3 $A ${c}512_areaW_demote $c 512 w 1 ${c}_areaW512
  python3 $A ${c}512_area $c 512 1 0 ${c}_area512
  python3 $A ${c}512_area_demote $c 512 1 1 ${c}_area512
done
for ws in 640 384; do
  python3 $A all${ws}_base all $ws 0 0 base$ws
  python3 $A all${ws}_base_demote all $ws 0 1 base$ws
  python3 $A all${ws}_areaW_demote all $ws w 1 areaW$ws
  python3 $A all${ws}_area all $ws 1 0 area$ws
  python3 $A all${ws}_area_demote all $ws 1 1 area$ws
  rm -f caches/*$ws.bin
done
