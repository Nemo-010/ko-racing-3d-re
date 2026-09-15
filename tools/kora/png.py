"""Minimal PNG reader/writer (standard library only).

Only what the K.O. Racing assets need: 8-bit non-interlaced images with
colour types 0 (grey), 2 (RGB), 3 (palette), 4 (grey+alpha) and 6 (RGBA).
Everything is normalised to RGBA bytes on read.  Keeping this in pure
Python keeps the package dependency-free and easy to port to Rust; in Rust
the `png` crate replaces it wholesale.
"""

from __future__ import annotations

import struct
import zlib
from typing import List, Tuple

_SIGNATURE = b"\x89PNG\r\n\x1a\n"
_CHANNELS = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}


def _paeth(a: int, b: int, c: int) -> int:
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    if pb <= pc:
        return b
    return c


def read(data: bytes) -> Tuple[int, int, bytearray]:
    """Decode *data* and return ``(width, height, rgba_bytes)``."""
    if data[:8] != _SIGNATURE:
        raise ValueError("not a PNG")
    pos = 8
    width = height = depth = colour = 0
    palette = b""
    idat = bytearray()
    while pos < len(data):
        length = struct.unpack_from(">I", data, pos)[0]
        tag = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + length]
        pos += 12 + length
        if tag == b"IHDR":
            width, height, depth, colour, _comp, _filt, interlace = struct.unpack(
                ">IIBBBBB", body)
            if depth != 8 or interlace != 0:
                raise ValueError("unsupported PNG (depth=%d interlace=%d)"
                                 % (depth, interlace))
        elif tag == b"PLTE":
            palette = body
        elif tag == b"tRNS":
            pass  # binary transparency only; palette handled below
        elif tag == b"IDAT":
            idat += body
        elif tag == b"IEND":
            break

    channels = _CHANNELS[colour]
    raw = zlib.decompress(bytes(idat))
    stride = width * channels
    out = bytearray(width * height * 4)
    prev = bytearray(stride)
    p = 0
    for y in range(height):
        filt = raw[p]
        p += 1
        line = bytearray(raw[p:p + stride])
        p += stride
        if filt == 1:
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 0xFF
        elif filt == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif filt == 3:
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 0xFF
        elif filt == 4:
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                up = prev[i]
                upleft = prev[i - channels] if i >= channels else 0
                line[i] = (line[i] + _paeth(left, up, upleft)) & 0xFF
        for x in range(width):
            si = x * channels
            di = (y * width + x) * 4
            if colour == 0:
                out[di:di + 4] = bytes((line[si], line[si], line[si], 255))
            elif colour == 2:
                out[di:di + 4] = bytes((line[si], line[si + 1], line[si + 2], 255))
            elif colour == 3:
                idx = line[si]
                out[di:di + 4] = bytes((palette[idx * 3], palette[idx * 3 + 1],
                                        palette[idx * 3 + 2], 255))
            elif colour == 4:
                out[di:di + 4] = bytes((line[si], line[si], line[si], line[si + 1]))
            else:
                out[di:di + 4] = line[si:si + 4]
        prev = line
    return width, height, out


def read_file(path: str) -> Tuple[int, int, bytearray]:
    with open(path, "rb") as fh:
        return read(fh.read())


def write(width: int, height: int, rgba: bytes) -> bytes:
    """Encode RGBA bytes as a PNG."""

    def chunk(tag: bytes, body: bytes) -> bytes:
        return (struct.pack(">I", len(body)) + tag + body
                + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF))

    raw = bytearray()
    stride = width * 4
    for y in range(height):
        raw.append(0)
        raw += rgba[y * stride:(y + 1) * stride]
    return (_SIGNATURE
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
            + chunk(b"IEND", b""))


def write_file(path: str, width: int, height: int, rgba: bytes) -> None:
    with open(path, "wb") as fh:
        fh.write(write(width, height, rgba))
