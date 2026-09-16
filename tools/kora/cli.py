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


def cmd_view(args) -> int:
    """Render a dump `dump_track` wrote, which is the port's own geometry."""
    from . import view

    dump = view.load(args.dump, args.pack)
    parsed = lambda text: tuple(float(part) for part in text.split(","))
    drawn = view.render(
        dump,
        args.out,
        width=args.width,
        height=args.height,
        eye=parsed(args.eye) if args.eye else None,
        target=parsed(args.look) if args.look else None,
        fovy=args.fov,
        textured=not args.flat,
        cull=args.cull,
    )
    print("%d triangles over %d textures -> %s" % (dump.count(), len(drawn), args.out))
    for name, count in sorted(drawn.items(), key=lambda item: -item[1]):
        print("   %-24s %5d" % (name, count))
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
    import os

    with open(args.path, "rb") as handle:
        campaign = formats.Campaign.parse(handle.read())
    # The matching .001 holds the per-race setups, addressed by the third i32
    # of each record.  Each game mode stores its own layout (see RaceConfig).
    races = os.path.splitext(args.path)[0] + ".001"
    blob = None
    if os.path.exists(races):
        with open(races, "rb") as handle:
            blob = handle.read()

    def config_of(entry):
        if blob is None:
            return None
        try:
            return formats.RaceConfig.parse(blob, entry.values[2], entry.a)
        except (ValueError, IndexError):
            return None

    if args.tsv:
        print("campaign\tmode\tlevel\tname\tmap\toffset\tlaps\ttheme\topponents")
        for entry in campaign.extras:
            level = campaign.levels[entry.b] if entry.b < len(campaign.levels) else None
            config = config_of(entry)
            print("%s\t%d\t%d\t%s\t%s\t%d\t%s\t%s\t%s"
                  % (os.path.basename(args.path), entry.a, entry.b,
                     level.name if level else "?", level.map if level else "?",
                     entry.values[2],
                     config.laps if config else "?",
                     config.theme if config else "?",
                     (config.opponents if entry.a in formats.RaceConfig.RACE_MODES
                      else "-") if config else "?"))
        return 0

    print("levels:    %d" % len(campaign.levels))
    print("unlock:    %s" % campaign.unlock)
    print("downloads: %s" % campaign.downloads)
    print("races:     %d" % len(campaign.extras))
    for i, level in enumerate(campaign.levels):
        print("  %2d %-16s %-10s (%d,%d)" % (i, level.name, level.map,
                                              level.x, level.y))
        for entry in campaign.extras:
            if entry.b != i:
                continue
            config = config_of(entry)
            if config is None:
                print("       mode %d  offset %-4d  ?" % (entry.a, entry.values[2]))
                continue
            opponents = (str(config.opponents)
                         if entry.a in formats.RaceConfig.RACE_MODES else "-")
            print("       mode %d  offset %-4d  laps=%d theme=%d opponents=%s"
                  % (entry.a, entry.values[2], config.laps, config.theme, opponents))
    return 0


def cmd_heights(args) -> int:
    """Per-cell road height, sampled from each tile's collision mesh."""
    import os

    from . import formats as fmt

    root = args.resources
    lists = args.lists or os.path.join(os.path.dirname(os.path.abspath(root.rstrip("/"))), "x", "lists")
    with open(os.path.join(lists, "tile_list"), "rb") as handle:
        tile_names = [n for n in handle.read().decode("latin1").replace("\r", "").split("\n") if n]

    cache = {}

    def tile(kind):
        if kind not in cache:
            with open(os.path.join(root, "tiles", tile_names[kind - 1]), "rb") as handle:
                cache[kind] = fmt.Tile.parse(handle.read())
        return cache[kind]

    if args.map:
        maps = [args.map]
    else:
        maps = sorted(n for n in os.listdir(os.path.join(root, "levels")) if n.endswith(".map"))

    for name in maps:
        with open(os.path.join(root, "levels", name), "rb") as handle:
            road = fmt.Map.parse(handle.read())
        print("%s  %dx%d" % (name, road.width, road.height))
        for cy in range(road.height):
            row = []
            for cx in range(road.width):
                cell = road.cells[cy][cx]
                if not cell.tile:
                    row.append("   .  ")
                    continue
                kind, arg = cell.tile
                row.append("%6.2f" % fmt.tile_surface(tile(kind), (0.5, 0.5), arg))
            print("  " + " ".join(row))
        print()
    return 0


def cmd_pack(args) -> int:
    """Rebuild a resource archive from a pack directory or an extracted tree."""
    from . import pack as pack_module

    if os.path.exists(os.path.join(args.source, pack_module.INDEX_NAME)):
        archive = pack_module.ResourcePack.load(args.source)
        resources = [(r.name, r.data) for r in archive.iter_resources()]
        scratch = pack_module.scratch_bytes(args.source, max(
            e.split(archive.page_size)[0] for e in archive.entries) + 1)
        page_size = archive.page_size
        print("repacking %s: %d resources, %d pages"
              % (args.source, len(resources), max(scratch) + 1))
    else:
        resources = pack_module.read_tree(args.source)
        scratch = {}
        page_size = args.page_size
        print("packing %s: %d resources" % (args.source, len(resources)))

    if args.zero_scratch:
        scratch = {}

    pages = pack_module.write_pack(resources, args.out, page_size=page_size, scratch=scratch)
    total = sum(len(data) for _name, data in resources)
    print("wrote %s: %d resources, %d pages, %d bytes of resource data"
          % (args.out, len(resources), pages, total))

    if args.verify:
        differences = pack_module.diff_packs(args.out, args.verify)
        if not differences:
            print("verify: byte-identical to %s" % args.verify)
            return 0
        print("verify: %d file(s) differ from %s" % (len(differences), args.verify))
        for line in differences[:10]:
            print("   " + line)
        return 1
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

    p = sub.add_parser("view", help="render a track the port dumped, to a PNG")
    p.add_argument("dump", help="the file `cargo run --bin dump_track` wrote")
    p.add_argument("out", help="PNG to write")
    p.add_argument("--pack", default="../x", help="pack directory, for the textures")
    p.add_argument("--width", type=int, default=960)
    p.add_argument("--height", type=int, default=540)
    p.add_argument("--eye", help="camera position, x,y,z")
    p.add_argument("--look", help="camera target, x,y,z")
    p.add_argument("--fov", type=float, help="vertical field of view, degrees")
    p.add_argument("--flat", action="store_true", help="flat colours, no textures")
    p.add_argument("--cull", action="store_true", help="drop back faces, as the game does")
    p.set_defaults(func=cmd_view)

    p = sub.add_parser("campaign", help="show a campaign .000 definition")
    p.add_argument("path")
    p.add_argument("--tsv", action="store_true", help="emit tab-separated rows")
    p.set_defaults(func=cmd_campaign)

    p = sub.add_parser("pack", help="rebuild an archive from a pack dir or a resource tree")
    p.add_argument("source", help="a pack directory (holding data/data.*) or an extracted tree")
    p.add_argument("out", help="directory to write data and data.* into")
    p.add_argument("--page-size", type=int, default=1000,
                   help="page size for a tree rebuild (default 1000)")
    p.add_argument("--zero-scratch", action="store_true",
                   help="leave each page's unwritten first byte as zero")
    p.add_argument("--verify", help="compare the output against this archive")
    p.set_defaults(func=cmd_pack)

    p = sub.add_parser("heights", help="road height per cell, from tile collision meshes")
    p.add_argument("resources", help="unpacked resource directory (the one holding tiles/ and levels/)")
    p.add_argument("map", nargs="?", help="one levels/*.map name; omit for all")
    p.add_argument("--lists", help="directory holding lists/tile_list (default: ../x/lists)")
    p.set_defaults(func=cmd_heights)

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
