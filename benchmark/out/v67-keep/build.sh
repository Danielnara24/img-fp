#!/bin/bash
# A quick build for iterating (no LTO, many codegen units): target/iter. Same target CPU as the
# plain build, so it reads the same cache records; the figures quoted are the plain build's.
cd /home/daniel/Documents/Vscode_repositories/img-fp
RUSTFLAGS="-C target-cpu=x86-64" CARGO_TARGET_DIR=target/iter CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 CARGO_PROFILE_RELEASE_INCREMENTAL=true cargo build --release --bin img-fp 2>&1 | grep -E '^(error|warning: unused)|Finished' -A7
