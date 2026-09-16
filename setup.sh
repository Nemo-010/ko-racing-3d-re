#!/bin/sh
# K.O. Racing 3D - asset setup.
#
# The game archive is the property of its authors and is NOT redistributed in
# this repository.  This script downloads it and unpacks only the pieces the
# tools and the Rust port read:
#
#   x/                      raw JAR contents (decompilation input)
#   assets/                 all resources at their original paths (Python tools)
#   rust/kora/assets/       the data/data.* pack plus lists/ (Rust port)
#
# Usage:  ./setup.sh
set -eu

URL="${KORA_JAR_URL:-https://kazam.pages.dev/public/sep.16.26/KORa_17_612805.jar}"
ROOT="$(cd "$(dirname "$0")" && pwd)"
JAR="$ROOT/KORa_17_612805.jar"
X="$ROOT/x"

command -v unzip >/dev/null 2>&1 || { echo "setup: unzip is required" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "setup: python3 is required" >&2; exit 1; }

if [ ! -f "$JAR" ]; then
    echo "setup: fetching $URL"
    curl -fsSL -o "$JAR" "$URL"
fi

echo "setup: unpacking the JAR into x/"
mkdir -p "$X"
(cd "$X" && unzip -oq "$JAR")

echo "setup: extracting every resource into assets/"
(cd "$ROOT/tools" && python3 -m kora unpack ../x ../assets)

echo "setup: preparing the Rust port's asset directory"
mkdir -p "$ROOT/rust/kora/assets/lists" "$ROOT/rust/kora/assets/sounds"
cp "$X"/data "$X"/data.* "$ROOT/rust/kora/assets/"
cp "$X"/lists/* "$ROOT/rust/kora/assets/lists/"
# The only sound in the game is a JAR resource, not a pack entry.
cp "$X"/sounds/theme.mid "$ROOT/rust/kora/assets/sounds/"

echo "setup: done"
echo "  Python tools: python3 -m kora info x/"
echo "  Rust port:    cd rust/kora && cargo run --release"
