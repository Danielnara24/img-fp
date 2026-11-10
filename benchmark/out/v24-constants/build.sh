#!/bin/bash
# Builds one plain x86-64 binary per value of each swept constant, read at
# build time from IMGFP_THUMB / IMGFP_GRID / IMGFP_BLOCK / IMGFP_FLAT.
cd /home/daniel/Documents/Vscode_repositories/img-fp
O=benchmark/out/v24-constants
export RUSTFLAGS="-C target-cpu=x86-64"
one() { # name VAR=value...
  local name=$1; shift
  [ -x "$O/bin/img-fp-$name" ] && return
  env "$@" cargo build --release --target-dir target/plain 2>&1 | tail -1
  cp target/plain/release/img-fp "$O/bin/img-fp-$name"
  echo "built $name"
}
one def
for v in 64 96 192 256; do one thumb$v IMGFP_THUMB=$v; done
for v in 32 40 56 64 80; do one grid$v IMGFP_GRID=$v; done
for v in 4 6 12 16; do one block$v IMGFP_BLOCK=$v; done
for v in 0 1 2 3 6 8 12; do one flat$v IMGFP_FLAT=$v; done
echo ALL BUILT
