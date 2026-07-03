#!/usr/bin/env bash
# Copy the latest compiled corpus artifact into the crate's vendored data dir.
#
# The Rust compiler in `trickydata-inputs/` emits a self-contained
# `trickydata.trickydata` artifact. We vendor that single file into `data/` so
# the crate embeds a working corpus (via `include_bytes!`) with no network or
# filesystem access at runtime. Run this before cutting a release, after
# recompiling the corpus.
#
# Usage: scripts/sync-data.sh [SOURCE_DIR]
#   SOURCE_DIR defaults to ../trickydata-inputs relative to the crate.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate_root="$(dirname "$here")"
src="${1:-$crate_root/../trickydata-inputs}"
destinations=("$crate_root/data" "$crate_root/trickydata-macros/data")

for name in trickydata.trickydata; do
    if [[ ! -f "$src/$name" ]]; then
        echo "error: $src/$name not found" >&2
        exit 1
    fi
    for dest in "${destinations[@]}"; do
        mkdir -p "$dest"
        cp "$src/$name" "$dest/$name"
        echo "copied $src/$name -> $dest/$name"
    done
done
