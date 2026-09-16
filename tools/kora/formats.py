"""Parsers for the K.O. Racing 3D per-resource formats.

Every parser mirrors the corresponding MIDlet class; the class name is noted
so the Java and Python stay easy to compare.  All reads are big endian and
unsigned unless stated otherwise.

Confirmed formats (``parse`` consumes the whole blob):

``.car``   car descriptor          (class ``ba``)
``.ob``    scenery/object entry    (class ``ai``)
``.bck``   sky/background          (class ``al``)
``.md``    mid-detail tile item    (class ``bc``)
``.hd``    high-detail tile item   (class ``bp``)
``.tl``    track tile geometry     (class ``ar`` + ``a``)

A ``str`` below is a u8 length followed by that many Latin-1 bytes, the
encoding used by ``be.a(InputStream)``.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass, field
from typing import List, Optional, Tuple


# --------------------------------------------------------------------------
# low-level reader
# --------------------------------------------------------------------------
class Reader:
    """Cursor over a byte blob, big endian, string = u8 length + bytes."""

    def __init__(self, blob: bytes):
        self.blob = blob
        self.pos = 0

    @property
    def remaining(self) -> int:
        return len(self.blob) - self.pos

    def u8(self) -> int:
        value = self.blob[self.pos]
        self.pos += 1
        return value

    def i8(self) -> int:
        value = self.blob[self.pos]
        self.pos += 1
        return value - 256 if value > 127 else value

    def u16(self) -> int:
        value = struct.unpack_from(">H", self.blob, self.pos)[0]
        self.pos += 2
        return value

    def i16(self) -> int:
        value = struct.unpack_from(">h", self.blob, self.pos)[0]
        self.pos += 2
        return value

    def i32(self) -> int:
        value = struct.unpack_from(">i", self.blob, self.pos)[0]
        self.pos += 4
        return value

    def u32(self) -> int:
        value = struct.unpack_from(">I", self.blob, self.pos)[0]
        self.pos += 4
        return value

    def raw(self, count: int) -> bytes:
        value = self.blob[self.pos:self.pos + count]
        self.pos += count
        return value

    def string(self) -> str:
        length = self.u8()
        return self.raw(length).decode("latin1")

    def float_milli(self) -> float:
        """A value the game stores as an integer and divides by 1000."""
        return self.u32() / 1000.0

    def done(self) -> None:
        if self.pos != len(self.blob):
            raise ValueError("trailing bytes: %d" % self.remaining)


# --------------------------------------------------------------------------
# .car  -  class ba
# --------------------------------------------------------------------------
@dataclass
class Car:
    model: str
    low_model: str
    texture: str
    name: str
    stats: List[int]           # four u8 tuning values (power/handling/...)
    extra: bytes               # 39 bytes the reader skips

    @classmethod
    def parse(cls, blob: bytes) -> "Car":
        r = Reader(blob)
        model = r.string()
        low_model = r.string()
        texture = r.string()
        name = r.string()
        stats = [r.u8() for _ in range(4)]
        return cls(model, low_model, texture, name, stats, r.raw(r.remaining))

    @classmethod
    def path_of(cls, blob: bytes, name: str) -> "Car":
        return cls.parse(blob)


# --------------------------------------------------------------------------
# .ob  -  class ai
# --------------------------------------------------------------------------
@dataclass
class ObjectDef:
    model: str
    texture: str
    flag: bool                 # passed to the model cache as "textured"/wrap

    @classmethod
    def parse(cls, blob: bytes) -> "ObjectDef":
        r = Reader(blob)
        model = r.string()
        texture = r.string()
        flag = r.u8() != 0
        r.done()
        return cls(model, texture, flag)


# --------------------------------------------------------------------------
# .bck  -  class al (method q(int))
# --------------------------------------------------------------------------
@dataclass
class Background:
    """Sky box: base texture plus four packed 0x00RRGGBB colours.

    The four colours are read, two of them kept as public fields, two
    discarded; they drive the background gradient/fog.  ``detail`` and the
    two milli-floats control the cloud/horizon layers.  The loader first
    looks for ``<name>.jpg`` and falls back to ``<name>.png``.
    """

    texture: str
    colours: List[int]         # 4 packed RGB values
    detail: int
    scale_a: float
    scale_b: float

    @classmethod
    def parse(cls, blob: bytes) -> "Background":
        r = Reader(blob)
        texture = r.string()
        colours = [r.u32() for _ in range(4)]
        detail = r.u8()
        scale_a = r.float_milli()
        scale_b = r.float_milli()
        r.done()
        return cls(texture, colours, detail, scale_a, scale_b)


# --------------------------------------------------------------------------
# .md  -  class bc
# --------------------------------------------------------------------------
@dataclass
class MidDetail:
    """A mid-detail tile item: model + texture, plus one unused byte.

    ``bc.a(cf)`` reads only the two strings and closes the stream; the
    trailing byte shipped in every file is ignored by this build (likely a
    rotation/height left over from the exporter).
    """

    model: str
    texture: str
    trailing: Optional[int]

    @classmethod
    def parse(cls, blob: bytes) -> "MidDetail":
        r = Reader(blob)
        model = r.string()
        texture = r.string()
        trailing = r.u8() if r.remaining else None
        r.done()
        return cls(model, texture, trailing)


# --------------------------------------------------------------------------
# .hd  -  class bp
# --------------------------------------------------------------------------
@dataclass
class HighDetailEntry:
    kind: int
    position: Tuple[float, float, float]   # offsets in tile units
    scale: Tuple[float, float, float]      # stored /100
    scale2: Tuple[float, float, float]     # stored /100


@dataclass
class HighDetail:
    entries: List[HighDetailEntry]
    trailing: Optional[int]

    @classmethod
    def parse(cls, blob: bytes) -> "HighDetail":
        r = Reader(blob)
        count = r.u8()
        entries = []
        for _ in range(count):
            kind = r.u8()
            position = tuple(r.u8() / 100.0 - 1.0 for _ in range(3))
            scale = tuple(r.u8() / 100.0 for _ in range(3))
            scale2 = tuple(r.u8() / 100.0 for _ in range(3))
            entries.append(HighDetailEntry(kind, position, scale, scale2))
        trailing = r.u8() if r.remaining else None
        r.done()
        return cls(entries, trailing)


# --------------------------------------------------------------------------
# .tl  -  class ar and its collision helper a
# --------------------------------------------------------------------------
@dataclass
class Tile:
    name: str
    texture: str
    variant: int                            # first byte, discarded by reader
    points: List[Tuple[float, float]]       # polygon outline, percent of tile
    edges: List[Tuple[int, int]]            # index pairs into ``points``
    heights: List[Tuple[float, float]]      # (value, threshold-or-world-height)
    solid: List[bool]                       # four side flags, edge rendering only
    open_sides: List[bool]                  # four drivable-side flags (see note)
    collision: Optional["Collision"]

    @classmethod
    def parse(cls, blob: bytes) -> "Tile":
        r = Reader(blob)
        name = r.string()
        texture = r.string()
        variant = r.u8()

        points = [(float(r.u8()), float(r.u8())) for _ in range(r.u8())]

        flat = [r.u8() for _ in range(r.u8())]
        if len(flat) % 2:
            raise ValueError("edge list has odd length")
        edges = [(flat[i], flat[i + 1]) for i in range(0, len(flat), 2)]

        # tile edge length in world units is ar.c == 14; heights >= 100
        # encode (100 + n) which the game turns into c * n / 100.
        heights = []
        for _ in range(r.u8()):
            value = r.u8()
            if value < 100:
                heights.append((float(value), 1000.0))
            else:
                heights.append((float(r.u8()), 14.0 * (value - 100) / 100.0))

        solid = [r.u8() != 0 for _ in range(4)]
        # A side is drivable when the matching flag is set.  The world side is
        # (i + arg) % 4, where arg is the cell's rotation from the .map.  This
        # is what class `ar` keeps in `b[]` and what `bm.b(i)` returns, i.e.
        # the flag the `bs` track flood branches on; despite reading like
        # "walls" it is not the closed-side flag.  Using `solid` here instead
        # leaves most maps in disconnected fragments.
        if r.u8() != 0:
            open_sides = [r.u8() != 0 for _ in range(4)]
        else:
            open_sides = list(solid)
        r.u8()                              # discarded

        collision = Collision.parse(r) if r.remaining else None
        r.done()
        return cls(name, texture, variant,
                   [(x, y) for x, y in points], edges, heights,
                   solid, open_sides, collision)


@dataclass
class Collision:
    """Triangle soup used for height sampling (class ``a``).

    The local frame is the cell's ``[0,1]^2`` square.  ``bm.a(float, float)``
    rotates a sample point by the cell's ``.map`` argument before the mesh is
    queried, and the third component is *negated* when the triangle is built,
    so a point's world height is ``-z * 14`` (the same 14-unit scale the ``.tl``
    heights use).

    Vertex components are bytes divided by 100, and anything above 2.0 wraps to
    ``value - 2.56``, which only ever fires on the top of the byte range.  A
    vertex count of zero means the stream ends there, with no triangles.
    """

    vertices: List[Tuple[float, float, float]]
    triangles: List[Tuple[int, int, int]]

    def height(self, local: Tuple[float, float], arg: int = 0) -> Optional[float]:
        """World height at a cell-local point, or ``None`` outside the mesh."""
        point = rotate_sample(local, arg)
        for i0, i1, i2 in self.triangles:
            (x0, y0, z0), (x1, y1, z1), (x2, y2, z2) = (
                self.vertices[i0], self.vertices[i1], self.vertices[i2])
            det = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2)
            if abs(det) < 1e-9:
                continue
            l0 = ((y1 - y2) * (point[0] - x2) + (x2 - x1) * (point[1] - y2)) / det
            l1 = ((y2 - y0) * (point[0] - x2) + (x0 - x2) * (point[1] - y2)) / det
            if l0 >= -1e-6 and l1 >= -1e-6 and l0 + l1 <= 1.0 + 1e-6:
                return -(l0 * z0 + l1 * z1 + (1.0 - l0 - l1) * z2) * 14.0
        return None

    @classmethod
    def parse(cls, reader: Reader) -> "Collision":
        count = reader.u8()
        if count <= 0:
            return cls([], [])
        vertices = []
        for _ in range(count):
            comps = []
            for _ in range(3):
                value = reader.u8() / 100.0
                if value > 2.0:
                    value -= 2.56
                comps.append(value)
            vertices.append(tuple(comps))
        triangles = []
        for _ in range(reader.u8()):
            triangles.append((reader.u8(), reader.u8(), reader.u8()))
        return cls(vertices, triangles)


def rotate_sample(point: Tuple[float, float], arg: int) -> Tuple[float, float]:
    """``bm.a(float, float)``: rotate a cell-local point into the mesh frame."""
    x, y = point
    arg %= 4
    if arg == 1:
        return (y, 1.0 - x)
    if arg == 2:
        return (1.0 - x, 1.0 - y)
    if arg == 3:
        return (1.0 - y, x)
    return (x, y)


def tile_surface(tile: "Tile", local: Tuple[float, float], arg: int = 0) -> float:
    """The MIDlet's height sampling for one cell: mesh height, else flat zero."""
    if tile.collision is None or not tile.collision.vertices:
        return 0.0
    height = tile.collision.height(local, arg)
    return 0.0 if height is None else height


# --------------------------------------------------------------------------
# .map  -  class bs
# --------------------------------------------------------------------------
@dataclass
class MapCell:
    """One track cell.  Bit 0 places the tile, bits 1-3 add mid-detail
    scenery, bits 4-6 add high-detail scenery; each set bit contributes a
    two-byte payload (rg ``by2 & r.b(i)`` in class ``bs``)."""

    flags: int
    tile: Optional[Tuple[int, int]]                       # (type, arg)
    mid: List[Tuple[int, int]] = field(default_factory=list)
    high: List[Tuple[int, int]] = field(default_factory=list)


@dataclass
class Map:
    width: int
    height: int
    cells: List[List[MapCell]]       # cells[y][x]
    start: Tuple[int, int]
    finish: Tuple[int, int]
    checkpoints: List[Tuple[int, int]]

    @classmethod
    def parse(cls, blob: bytes) -> "Map":
        r = Reader(blob)
        width = r.u8()
        height = r.u8()
        cells: List[List[MapCell]] = []
        for _ in range(height):
            row = []
            for _ in range(width):
                flags = r.u8()
                payloads = [tuple(r.raw(2)) for bit in range(7) if flags & (1 << bit)]
                tile = payloads[0] if flags & 1 else None
                row.append(MapCell(flags, tile, payloads[1:4], payloads[4:7]))
            cells.append(row)
        start = (r.u8(), r.u8())
        finish = (r.u8(), r.u8())
        checkpoints = [(r.u8(), r.u8()) for _ in range(r.u8())]
        r.done()
        return cls(width, height, cells, start, finish, checkpoints)


# --------------------------------------------------------------------------
# campaign .000  -  class u (method c())
# --------------------------------------------------------------------------
@dataclass
class CampaignLevel:
    x: int                  # i16, map preview / editor coordinates
    y: int
    name: str
    map: str
    a: int                  # i8
    b: int
    flag: int               # i8, usually 0 for locked, 1 for unlocked


@dataclass
class CampaignTailEntry:
    a: int
    b: int
    values: Tuple[int, int, int]    # three i32; the last is often the
                                    # skip offset into the matching .001


@dataclass
class Campaign:
    levels: List[CampaignLevel]
    unlock: List[int]               # u32 thresholds (``p - 1`` entries)
    downloads: List[Tuple[int, str]]
    extras: List[CampaignTailEntry]

    @classmethod
    def parse(cls, blob: bytes) -> "Campaign":
        r = Reader(blob)
        levels = []
        for _ in range(r.u8()):
            x = r.i16()
            y = r.i16()
            name = r.string()
            map_name = r.string()
            a, b, flag = r.i8(), r.i8(), r.i8()
            levels.append(CampaignLevel(x, y, name, map_name, a, b, flag))
        count = r.u8()                 # the game stores this + 1
        unlock = [r.u32() for _ in range(count)]
        downloads = [(r.u32(), r.string()) for _ in range(r.u8())]
        extras = [CampaignTailEntry(r.u8(), r.u8(), (r.u32(), r.u32(), r.u32()))
                  for _ in range(r.u8())]
        r.done()
        return cls(levels, unlock, downloads, extras)


# --------------------------------------------------------------------------
# campaign .001  -  class r / cu / dk / cd / bv
# --------------------------------------------------------------------------
@dataclass
class RaceConfig:
    """One per-race setup, read from ``<campaign>.001`` at a record's offset.

    A race record in the ``.000`` table names a game mode and carries the byte
    offset of its setup inside the matching ``.001``.  Every mode stores that
    setup with its own layout, so the field order below is taken from the
    reader the MIDlet uses for that mode::

        mode 0 / 4  (cu.b)  u8 laps, u8 theme, u8 flag, u8 car, u8 param, u8 opponents
        mode 1      (r.b)   u8 theme, u8 flag, u8 car, u8 param, u8 opponents   (one lap)
        mode 2 / 6  (dk.b)  u8 theme, u8 flag, u8 laps, u8 car, i32 time_limit
        mode 3      (cd.b)  u8 theme, u8 flag, u8 car, u8 param, u8 opponents   (laps := opponents)
        mode 5      (bv.b)  u8 theme, u8 flag, u8 laps, u8 car, i32 time_limit

    ``theme`` selects the tile/texture variant (``bm``/``bp`` compare it with
    3, and ``dk`` forces it to 3 for ``8a.map``); ``car`` is the player's car
    index, except that values >= 50 mark the deluxe time-attack entries, where
    the trailing i32 is the time limit instead.
    """

    mode: int
    laps: int
    theme: int
    flag: bool
    car: int
    opponents: int
    param: Optional[int] = None
    time_limit: Optional[int] = None

    #: Modes that are a race against opponents rather than a solo time trial.
    RACE_MODES = (0, 1, 3, 4)

    @classmethod
    def parse(cls, blob: bytes, offset: int, mode: int) -> "RaceConfig":
        r = Reader(blob)
        r.pos = offset
        if mode in (0, 4):
            laps, theme, flag = r.u8(), r.u8(), r.u8() != 0
            car, param, opponents = r.u8(), r.u8(), r.u8()
            return cls(mode, laps, theme, flag, car, opponents, param)
        if mode == 1:
            theme, flag = r.u8(), r.u8() != 0
            car, param, opponents = r.u8(), r.u8(), r.u8()
            return cls(mode, 1, theme, flag, car, opponents, param)
        if mode in (2, 5, 6):
            theme, flag, laps, car = r.u8(), r.u8() != 0, r.u8(), r.u8()
            return cls(mode, laps, theme, flag, car, 0, None, r.i32())
        if mode == 3:
            theme, flag = r.u8(), r.u8() != 0
            car, param, opponents = r.u8(), r.u8(), r.u8()
            return cls(mode, opponents, theme, flag, car, opponents, param)
        raise ValueError("unknown race mode %d" % mode)


def race_config_for(resources, map_name: str):
    """Find the race record that drives *map_name*.

    Returns ``(campaign_name, level_index, RaceConfig)`` for the first
    campaign level whose map matches, preferring a race mode with opponents.
    """
    for base in ("campaign/campaign", "campaign/deluxe"):
        if base + ".000" not in resources or base + ".001" not in resources:
            continue
        campaign = Campaign.parse(resources[base + ".000"])
        blob = resources[base + ".001"]
        for index, level in enumerate(campaign.levels):
            if level.map != map_name:
                continue
            records = [e for e in campaign.extras if e.b == index]
            if not records:
                continue
            records.sort(key=lambda e: (e.a not in RaceConfig.RACE_MODES, e.a))
            entry = records[0]
            return base, index, RaceConfig.parse(blob, entry.values[2], entry.a)
    return None


# --------------------------------------------------------------------------
# .tab  -  class g (character -> glyph index)
# --------------------------------------------------------------------------
@dataclass
class FontTable:
    """Maps character codes to glyph slots in the atlas.

    Layout (class ``g``)::

        u8    has_glyph_index
        u8    count
        repeat count:
            u16le char_code
            u8    glyph_index     # only when has_glyph_index != 0

    Without the optional index the glyph slot is the entry position, and a
    character that is absent falls back to glyph 0 (``g.a(char)``).
    """

    char_codes: List[int]
    glyphs: List[int]

    @classmethod
    def parse(cls, blob: bytes) -> "FontTable":
        r = Reader(blob)
        has_index = r.u8() != 0
        count = r.u8()
        codes: List[int] = []
        glyphs: List[int] = []
        for i in range(count):
            code = r.u8() | (r.u8() << 8)
            codes.append(code)
            glyphs.append(r.u8() if has_index else i)
        r.done()
        return cls(codes, glyphs)

    def index(self, ch: str) -> int:
        """Glyph slot for *ch* (0 when the character is not present)."""
        code = ord(ch)
        lo, hi = 0, len(self.char_codes) - 1
        while lo <= hi:
            mid = (lo + hi) // 2
            if code < self.char_codes[mid]:
                hi = mid - 1
            elif code > self.char_codes[mid]:
                lo = mid + 1
            else:
                return self.glyphs[mid]
        return 0

    def char(self, glyph: int) -> str:
        if 0 <= glyph < len(self.char_codes):
            return chr(self._code_for(glyph))
        return "\x00"

    def _code_for(self, glyph: int) -> int:
        for code, slot in zip(self.char_codes, self.glyphs):
            if slot == glyph:
                return code
        return 0


# --------------------------------------------------------------------------
# font base file  -  class p
# --------------------------------------------------------------------------
@dataclass
class Font:
    """A bitmap font: per-glyph advance widths plus a shared atlas.

    Base file (``/fonts/<name>``, no extension)::

        u8    glyph_count
        u8    advance_width[glyph_count]
        u8    cell_height

    The atlas is ``<name>.png`` and the optional ``<name>.tab`` maps
    characters to glyph slots.  Glyphs are packed left to right and wrap to
    a new row when ``x + width > atlas_width``, advancing ``y`` by
    ``cell_height``.
    """

    glyph_count: int
    widths: List[int]
    cell_height: int
    table: Optional[FontTable] = None

    @classmethod
    def parse(cls, blob: bytes, table: Optional[FontTable] = None) -> "Font":
        r = Reader(blob)
        glyph_count = r.u8()
        widths = [r.u8() for _ in range(glyph_count)]
        cell_height = r.u8()
        r.done()
        return cls(glyph_count, widths, cell_height, table)

    def glyph_for(self, ch: str) -> int:
        if self.table is not None:
            return self.table.index(ch)
        return ord(ch)

    def width(self, ch: str) -> int:
        glyph = self.glyph_for(ch)
        return self.widths[glyph] if 0 <= glyph < self.glyph_count else 0

    def text_width(self, text: str) -> int:
        return sum(self.width(ch) for ch in text)

    def layout(self, atlas_width: int) -> List[Tuple[int, int, int, int]]:
        """Return ``(x, y, width, height)`` for every glyph in the atlas."""
        rects = []
        x = y = 0
        for width in self.widths:
            if x + width > atlas_width:
                x = 0
                y += self.cell_height
            rects.append((x, y, width, self.cell_height))
            x += width
        return rects


# --------------------------------------------------------------------------
# dispatch
# --------------------------------------------------------------------------
PARSERS = {
    ".car": Car.parse,
    ".ob": ObjectDef.parse,
    ".bck": Background.parse,
    ".md": MidDetail.parse,
    ".hd": HighDetail.parse,
    ".tl": Tile.parse,
    ".map": Map.parse,
    ".000": Campaign.parse,
    ".tab": FontTable.parse,
}


def parse_resource(name: str, blob: bytes):
    """Parse *blob* using the parser for the extension of *name*."""
    import os

    parser = PARSERS.get(os.path.splitext(name)[1])
    if parser is None:
        raise ValueError("no parser for %r" % name)
    return parser(blob)
