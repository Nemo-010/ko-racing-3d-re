#!/usr/bin/env python3
"""Compatibility wrapper: unpack the K.O. Racing resource pack.

See ``kora/pack.py`` for the format description; ``kora`` is the real
implementation.  Usage::

    unpack_resources.py <jar-dir> <output-dir>
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from kora.pack import ResourcePack  # noqa: E402


def main() -> int:
    jar_dir = sys.argv[1] if len(sys.argv) > 1 else "x"
    out_dir = sys.argv[2] if len(sys.argv) > 2 else "assets"
    pack = ResourcePack.load(jar_dir)
    written = pack.extract_all(out_dir)
    pack.write_manifest(os.path.join(out_dir, "MANIFEST.tsv"))
    print("unpacked %d resources into %s" % (written, out_dir))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
