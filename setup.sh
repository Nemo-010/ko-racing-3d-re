#!/bin/sh
# K.O. Racing 3D - asset setup.
#
# The game archive is the property of its authors and is NOT redistributed in
# this repository.  This script downloads it and unpacks only the pieces the
# tools and the Rust port read:
#
#   x/                      raw JAR contents (decompilation input)
#   assets/                 every resource at its original path, plus the loose
#                           JAR directories the game reads (Python tools)
#   rust/kora/assets/       the same tree, for the Rust port
#
# The port reads an extracted tree, not the packed archive, so swapping an asset
# is a matter of editing the file and running again.  It still understands the
# packed form - give it `KORA_ASSETS=x` to read the archive itself.
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

# The game also reads a few directories straight out of the JAR rather than out
# of its archive: the tile and detail lists, the interface text, and the theme.
echo "setup: gathering the loose JAR directories"
for loose in lists ui sounds; do
    [ -d "$X/$loose" ] || continue
    mkdir -p "$ROOT/assets/$loose"
    cp "$X"/"$loose"/* "$ROOT/assets/$loose/"
done

# The port reads the same tree, from `assets` in its working directory.  The
# Wavefront files `kora obj` writes and the extraction manifest are tool output
# the game never reads, so they stay behind.
echo "setup: giving the Rust port the same tree"
rm -rf "$ROOT/rust/kora/assets"
mkdir -p "$ROOT/rust/kora/assets"
(
    cd "$ROOT/assets"
    for item in *; do
        case "$item" in
            obj|MANIFEST.tsv) continue ;;
        esac
        cp -R "$item" "$ROOT/rust/kora/assets/"
    done
)

echo "setup: done"
echo "  Python tools: python3 -m kora info x/"
echo "  Rust port:    cd rust/kora && cargo run --release"
echo "                (or, from here: cargo run --release --manifest-path rust/kora/Cargo.toml)"
