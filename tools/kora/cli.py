"""Command line interface for the K.O. Racing 3D tools.

Examples::

    python -m kora info x/                       # inspect the pack index
    python -m kora unpack x/ assets/             # extract every resource
    python -m kora obj assets/models assets/obj  # models -> OBJ
    python -m kora dump assets/ --json           # parse known formats
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import sys
from collections import Counter

from . import formats
from . import fontimg
from . import mapimg
from .model import Model
from .pack import ResourcePack


def _ext_histogram(pack: ResourcePack) -> Counter:
    histogram: Counter = Counter()
    for resource in pack.iter_resources():
        histogram[os.path.splitext(resource.name)[1] or "(none)"] += 1
    return histogram


def cmd_info(args) -> int:
    pack = ResourcePack.load(args.jar_dir)
    print("index:      %s" % os.path.join(args.jar_dir, "data"))
    print("resources:  %d" % len(pack.entries))
    print("page size:  %d bytes" % pack.page_size)
    pages = pack.pages()
    print("pages:      %d (data.0 .. data.%d)" % (len(pages), max(pages)))
    print("resources:  %d bytes" % sum(r.size for r in pack.iter_resources()))
    print("by type:")
    for ext, count in sorted(_ext_histogram(pack).items()):
        print("  %-8s %d" % (ext, count))
    if args.names:
        for resource in pack.iter_resources():
            print("%s\t%d" % (resource.name, resource.size))
    return 0


def cmd_unpack(args) -> int:
    pack = ResourcePack.load(args.jar_dir)
    written = pack.extract_all(args.out_dir)
    manifest = os.path.join(args.out_dir, "MANIFEST.tsv")
    pack.write_manifest(manifest)
    print("unpacked %d resources into %s (manifest: %s)"
          % (written, args.out_dir, manifest))
    return 0


def cmd_obj(args) -> int:
    os.makedirs(args.out_dir, exist_ok=True)
    written = 0
    for path in glob.glob(os.path.join(args.models_dir, "**", "*"), recursive=True):
        if not os.path.isfile(path):
            continue
        rel = os.path.relpath(path, args.models_dir)
        try:
            model = Model.parse(open(path, "rb").read())
        except Exception as exc:  # noqa: BLE001
            print("skip %s: %s" % (path, exc))
            continue
        dest = os.path.join(args.out_dir, rel + ".obj")
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        with open(dest, "w") as fh:
            fh.write(model.to_obj(rel))
        written += 1
    print("wrote %d OBJ files to %s" % (written, args.out_dir))
    return 0


def _dump(obj):
    if hasattr(obj, "__dataclass_fields__"):
        return {key: _dump(getattr(obj, key)) for key in obj.__dataclass_fields__}
    if isinstance(obj, (list, tuple)):
        return [_dump(item) for item in obj]
    if isinstance(obj, bytes):
        return obj.hex()
    return obj


def cmd_dump(args) -> int:
    result = {}
    for path in sorted(glob.glob(os.path.join(args.assets_dir, "**", "*"), recursive=True)):
        if not os.path.isfile(path):
            continue
        name = os.path.basename(path)
        try:
            parsed = formats.parse_resource(name, open(path, "rb").read())
        except (ValueError, KeyError):
            continue
        result[os.path.relpath(path, args.assets_dir)] = _dump(parsed)

    if args.json:
        json.dump(result, sys.stdout, indent=2, sort_keys=True)
        sys.stdout.write("\n")
    else:
        counts: Counter = Counter()
        for key in result:
            counts[os.path.splitext(key)[1]] += 1
        print("parsed %d resources" % len(result))
        for ext, count in sorted(counts.items()):
            print("  %-6s %d" % (ext, count))
    return 0


def cmd_mapimg(args) -> int:
    import glob as _glob

    if os.path.isdir(args.src):
        os.makedirs(args.out, exist_ok=True)
        written = 0
        for path in sorted(_glob.glob(os.path.join(args.src, "*.map"))):
            dest = os.path.join(args.out, os.path.splitext(os.path.basename(path))[0] + ".png")
            mapimg.render_file(path, dest, args.scale)
            written += 1
        print("rendered %d minimaps into %s" % (written, args.out))
    else:
        mapimg.render_file(args.src, args.out, args.scale)
        print("wrote %s" % args.out)
    return 0


def cmd_campaign(args) -> int:
    campaign = formats.Campaign.parse(open(args.path, "rb").read())
    if args.tsv:
        print("#\tname\tmap\tx\ty\ta\tb\tflag\texoffset")
        offsets = [e.values[2] for e in campaign.extras]
        for i, level in enumerate(campaign.levels):
            skip = offsets[i] if i < len(offsets) else -1
            print("%d\t%s\t%s\t%d\t%d\t%d\t%d\t%d\t%d"
                  % (i, level.name, level.map, level.x, level.y,
                     level.a, level.b, level.flag, skip))
    else:
        print("levels:    %d" % len(campaign.levels))
        print("unlock:    %s" % campaign.unlock)
        print("downloads: %s" % campaign.downloads)
        print("extras:    %d" % len(campaign.extras))
        for i, level in enumerate(campaign.levels):
            print("  %2d %-16s %-10s (%d,%d)" % (i, level.name, level.map,
                                                  level.x, level.y))
    return 0


def cmd_font(args) -> int:
    font, aw, ah, _atlas = fontimg.load_font(args.path)
    print("glyphs:      %d" % font.glyph_count)
    print("cell height: %d" % font.cell_height)
    print("atlas:       %dx%d" % (aw, ah))
    print("widths:      %s" % font.widths)
    if font.table is not None:
        printable = []
        for code, glyph in zip(font.table.char_codes, font.table.glyphs):
            ch = chr(code) if 32 <= code < 127 else "\\x%02x" % code
            printable.append("%s=%d" % (ch, glyph))
        print("table:       %s" % " ".join(printable))
    else:
        print("table:       (none; glyph = char code)")
    return 0


def cmd_fontimg(args) -> int:
    fontimg.render_file(args.path, args.text, args.out)
    font, aw, _ah, _atlas = fontimg.load_font(args.path)
    print("wrote %s (%d glyphs, %d px wide)"
          % (args.out, font.glyph_count, font.text_width(args.text)))
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="kora", description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("info", help="inspect the resource-pack index")
    p.add_argument("jar_dir", help="directory with data, data.0, ...")
    p.add_argument("--names", action="store_true", help="list every resource")
    p.set_defaults(func=cmd_info)

    p = sub.add_parser("unpack", help="extract every resource")
    p.add_argument("jar_dir")
    p.add_argument("out_dir")
    p.set_defaults(func=cmd_unpack)

    p = sub.add_parser("obj", help="convert custom models to Wavefront OBJ")
    p.add_argument("models_dir")
    p.add_argument("out_dir")
    p.set_defaults(func=cmd_obj)

    p = sub.add_parser("dump", help="parse known formats and summarise/emit JSON")
    p.add_argument("assets_dir")
    p.add_argument("--json", action="store_true", help="emit JSON to stdout")
    p.set_defaults(func=cmd_dump)

    p = sub.add_parser("mapimg", help="render .map track layouts to PNG minimaps")
    p.add_argument("src", help="a .map file or a directory of them")
    p.add_argument("out", help="output .png or output directory")
    p.add_argument("--scale", type=int, default=12, help="pixels per tile (default 12)")
    p.set_defaults(func=cmd_mapimg)

    p = sub.add_parser("campaign", help="show a campaign .000 definition")
    p.add_argument("path")
    p.add_argument("--tsv", action="store_true", help="emit tab-separated rows")
    p.set_defaults(func=cmd_campaign)

    p = sub.add_parser("font", help="inspect a bitmap font (base + .tab + .png)")
    p.add_argument("path", help="font base file, e.g. assets/fonts/font")
    p.set_defaults(func=cmd_font)

    p = sub.add_parser("fontimg", help="render text with a bitmap font")
    p.add_argument("path", help="font base file")
    p.add_argument("text")
    p.add_argument("out", help="output .png")
    p.set_defaults(func=cmd_fontimg)

    return parser


def main(argv=None) -> int:
    args = build_parser().parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
