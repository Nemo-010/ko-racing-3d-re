"""Parser for the game's custom 3D mesh format and an OBJ exporter.

The format is a stripped-down, byte-sized mesh container.  All integers are
big endian; the four header floats are stored as ASCII strings with a u8
length prefix (that is how the MIDlet reads them: ``Float.valueOf(str)``).

Layout::

    u8[3]        reserved, always 0x00 0x00 0x00
    str          position scale      (float as text)
    u8           reserved
    str          position bias       (float as text)
    u8           reserved
    str          texcoord scale      (float as text)
    u8           reserved
    str          texcoord bias       (float as text)
    u8           vertex_count
    repeat vertex_count:
        i8 x, i8 y, i8 z            # position, signed byte
        i8 u, i8 v                  # texture coordinate, signed byte: the
                                    # file stores it unsigned but the MIDlet
                                    # keeps it in a Java byte[], and M3G decodes
                                    # that as signed (component type 1 = BYTE)
    u8           strip_count
    repeat strip_count:
        u8       strip_length
    u16          index_count
    repeat index_count:
        u8       vertex_index

The values are fed to JSR-184 ``VertexBuffer.setPositions`` /
``setTexCoords`` with the M3G scale/bias convention::

    position = component * position_scale + (128 * position_scale + position_bias)
    texcoord = component * texcoord_scale + (128 * texcoord_scale + texcoord_bias)

The first header float is global model scale, the second shifts the model,
the last two do the same job for the texture coordinates.  Every strip in
the shipped assets is 3 vertices long, i.e. a plain triangle; strips longer
than 3 are still handled correctly on export.

The third component points **down**.  ``a``, the collision mesh reader, negates
it, a road tile keeps its drivable strip at ``z = 0`` with its kerbs at negative
z, and every car model stands its wheels on ``z = 0`` with the body above.  So a
viewer that wants Y-up should map ``(x, y, z) -> (x, -z, -y)``; copying z instead
hangs the whole world under the road, cars included.  ``python3 -m kora view``
renders a track the port built and is the quickest way to see it.

The texture coordinates land inside 0..1 for the cars and for the tile-atlas
sub-rectangles, because the signed bytes and the ``128 * scale + bias`` offset
line up that way - which is what that offset is for.  Reading the bytes
unsigned instead adds ``256 * scale`` to every component above 127, a full
wrap, and puts each car's rear on its nose.  A few scenery models genuinely
wrap (``zdzn`` spans u 0..8), since M3G - like the OpenGL under it - repeats a
lookup rather than clamping, so a viewer that clamps shows those wrong.

A ``_r`` suffix in a model name (e.g. ``rally_r``) marks the mirrored copy
used for the render-to-texture reflection, and is parsed identically.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass
from typing import List, Tuple

_RESERVED = 3
_VERTEX = struct.Struct(">bbbbb")       # x, y, z, u, v - all signed bytes
_TEXCOORD_SCALE = 128.0


@dataclass
class Model:
    """A parsed mesh."""

    position_scale: float
    position_bias: float
    texcoord_scale: float
    texcoord_bias: float
    vertices: List[Tuple[float, float, float]]
    texcoords: List[Tuple[float, float]]
    strip_lengths: List[int]
    indices: List[int]

    # -- derived geometry ------------------------------------------------
    def triangles(self) -> List[Tuple[int, int, int]]:
        """Triangulate the strips (alternating winding per the strip spec)."""
        faces: List[Tuple[int, int, int]] = []
        cursor = 0
        for length in self.strip_lengths:
            strip = self.indices[cursor:cursor + length]
            cursor += length
            for i in range(len(strip) - 2):
                a, b, c = strip[i], strip[i + 1], strip[i + 2]
                faces.append((a, b, c) if i % 2 == 0 else (b, a, c))
        return faces

    # -- parsing ---------------------------------------------------------
    @classmethod
    def parse(cls, blob: bytes) -> "Model":
        pos = 0

        def read_u8() -> int:
            nonlocal pos
            value = blob[pos]
            pos += 1
            return value

        def read_str() -> str:
            nonlocal pos
            length = blob[pos]
            pos += 1
            text = blob[pos:pos + length].decode("latin1")
            pos += length
            return text

        def read_float() -> float:
            return float(read_str())

        if len(blob) < _RESERVED or blob[:3] != b"\x00\x00\x00":
            raise ValueError("not a K.O. Racing model (bad magic)")

        pos = _RESERVED
        position_scale = read_float()
        read_u8()
        position_bias = read_float()
        read_u8()
        texcoord_scale = read_float()
        read_u8()
        texcoord_bias = read_float()

        vertex_count = read_u8()
        p_off = 128.0 * position_scale + position_bias
        t_off = 128.0 * texcoord_scale + texcoord_bias
        vertices: List[Tuple[float, float, float]] = []
        texcoords: List[Tuple[float, float]] = []
        for _ in range(vertex_count):
            x, y, z, u, v = _VERTEX.unpack_from(blob, pos)
            pos += _VERTEX.size
            vertices.append((position_scale * x + p_off,
                             position_scale * y + p_off,
                             position_scale * z + p_off))
            texcoords.append((texcoord_scale * u + t_off,
                              texcoord_scale * v + t_off))

        strip_count = read_u8()
        strip_lengths = list(blob[pos:pos + strip_count])
        pos += strip_count

        index_count = struct.unpack_from(">H", blob, pos)[0]
        pos += 2
        indices = list(blob[pos:pos + index_count])
        pos += index_count

        if pos != len(blob):
            raise ValueError("trailing bytes: %d" % (len(blob) - pos))
        if sum(strip_lengths) != index_count:
            raise ValueError("strip lengths do not sum to index count")
        return cls(position_scale, position_bias, texcoord_scale, texcoord_bias,
                   vertices, texcoords, strip_lengths, indices)

    # -- export ----------------------------------------------------------
    def to_obj(self, name: str) -> str:
        lines = ["# converted from K.O. Racing 3D model format",
                 "o %s" % name]
        lines += ["v %.6f %.6f %.6f" % v for v in self.vertices]
        lines += ["vt %.6f %.6f" % t for t in self.texcoords]
        for a, b, c in self.triangles():
            lines.append("f %d/%d %d/%d %d/%d"
                         % (a + 1, a + 1, b + 1, b + 1, c + 1, c + 1))
        return "\n".join(lines) + "\n"


def model_to_obj(blob: bytes, name: str) -> str:
    """Convenience wrapper: parse *blob* and render it as Wavefront OBJ."""
    return Model.parse(blob).to_obj(name)
