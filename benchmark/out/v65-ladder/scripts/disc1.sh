#!/bin/bash
cd "$(dirname "$0")"
./disc.sh sl512 g8 IMGFP_SL512=1
./disc.sh w1 g8 IMGFP_W512=1
./disc.sh sl512-w1 g8 IMGFP_SL512=1 IMGFP_W512=1
echo DONE >> disc/log
