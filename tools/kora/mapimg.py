"""Render a ``.map`` track layout to a PNG minimap (standard library only).

The layout is the flagged grid from :class:`kora.formats.Map`: an occupied
cell is a track tile, and its two-byte payload is the tile type plus an
argument.  Start, finish and checkpoints are overlaid.  Only ``zlib`` and
``struct`` are used, so the module stays dependency-free and ports easily.
"""

from __future__ import annotations

import struct
import zlib
from typing import List, Tuple

from .formats import Map

EMPTY = (18, 18, 22)
GRID = (38, 38, 46)
# a small palette so different tile types read differently
TILE_PALETTE = [
    (90, 96, 110), (120, 126, 140), (78, 84, 98), (150, 156, 170),
    (64, 70, 84), (110, 100, 84), (96, 88, 76), (72, 82, 74),
]


def _png(width: int, height: int, rgb: bytearray) -> bytes:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    raw = bytearray()
    stride = width * 3
    for y in range(height):
        raw.append(0)
        raw += rgb[y * stride:(y + 1) * stride]
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
            + chunk(b"IEND", b""))


def render(track: Map, scale: int = 12) -> Tuple[int, int, bytearray]:
    """Return ``(width, height, rgb_bytes)`` for *track*."""
    width = track.width * scale
    height = track.height * scale
    rgb = bytearray(width * height * 3)
    for y in range(height):
        row = bytearray()
        for _ in range(track.width):
            row += bytes(EMPTY)
        rgb[y * width * 3:(y + 1) * width * 3] = row

    def rect(cx: int, cy: int, colour: Tuple[int, int, int], inset: int = 0) -> None:
        if not (0 <= cx < track.width and 0 <= cy < track.height):
            return
        line = bytes(colour) * (scale - 2 * inset)
        for py in range(cy * scale + inset, (cy + 1) * scale - inset):
            start = (py * width + cx * scale + inset) * 3
            rgb[start:start + len(line)] = line

    for cy, row in enumerate(track.cells):
        for cx, cell in enumerate(row):
            if cell.tile is not None:
                rect(cx, cy, TILE_PALETTE[cell.tile[0] % len(TILE_PALETTE)], 1)

    rect(*track.start, (80, 220, 120))
    rect(*track.finish, (230, 90, 90))
    for checkpoint in track.checkpoints:
        rect(*checkpoint, (90, 150, 240))
    return width, height, rgb


def render_file(map_path: str, out_path: str, scale: int = 12) -> None:
    track = Map.parse(open(map_path, "rb").read())
    width, height, rgb = render(track, scale)
    with open(out_path, "wb") as fh:
        fh.write(_png(width, height, rgb))
