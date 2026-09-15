"""Render text with the game's bitmap fonts (standard library only).

Combines a font base file (``/fonts/<name>``), its optional ``<name>.tab``
character map and its ``<name>.png`` atlas; see
:class:`kora.formats.Font` for the byte layout.
"""

from __future__ import annotations

import os
from typing import Optional, Tuple

from . import png
from .formats import Font, FontTable


def load_font(base_path: str) -> Tuple[Font, int, int, bytearray]:
    """Load ``base_path`` and return ``(font, atlas_w, atlas_h, atlas_rgba)``."""
    with open(base_path, "rb") as fh:
        base = fh.read()
    table: Optional[FontTable] = None
    tab_path = base_path + ".tab"
    if os.path.exists(tab_path):
        with open(tab_path, "rb") as fh:
            table = FontTable.parse(fh.read())
    font = Font.parse(base, table)
    width, height, rgba = png.read_file(base_path + ".png")
    return font, width, height, rgba


def render(
    font: Font,
    atlas_width: int,
    atlas_height: int,
    atlas: bytes,
    text: str,
    colour: Tuple[int, int, int, int] = (255, 255, 255, 255),
) -> Tuple[int, int, bytearray]:
    """Compose *text* into a new RGBA buffer of height ``cell_height``."""
    rects = font.layout(atlas_width)
    width = font.text_width(text)
    height = font.cell_height
    out = bytearray(width * height * 4)
    pen = 0
    for ch in text:
        glyph = font.glyph_for(ch)
        advance = font.width(ch)
        if not (0 <= glyph < len(rects)):
            pen += advance
            continue
        gx, gy, gw, gh = rects[glyph]
        for row in range(min(gh, height)):
            for col in range(min(gw, width - pen)):
                si = ((gy + row) * atlas_width + (gx + col)) * 4
                di = (row * width + pen + col) * 4
                alpha = atlas[si + 3]
                if alpha == 0:
                    continue
                out[di] = (atlas[si] * colour[0]) // 255
                out[di + 1] = (atlas[si + 1] * colour[1]) // 255
                out[di + 2] = (atlas[si + 2] * colour[2]) // 255
                out[di + 3] = (alpha * colour[3]) // 255
        pen += advance
    return width, height, out


def render_file(base_path: str, text: str, out_path: str,
                colour: Tuple[int, int, int, int] = (255, 255, 255, 255)) -> None:
    font, aw, ah, atlas = load_font(base_path)
    width, height, rgba = render(font, aw, ah, atlas, text, colour)
    png.write_file(out_path, width, height, rgba)
