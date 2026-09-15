"""Resource-pack reader for K.O. Racing 3D (Jollybox, MIDlet v1.70).

The MIDlet ships its resources in a small custom archive instead of keeping
the original directory tree inside the JAR::

    data          index of every resource
    data.0        page 0
    data.1        page 1
    ...

Index layout (big endian, no packing)::

    u16   resource_count
    i32   page_size                       # bytes of one data.<n> page (1000)
    repeat resource_count:
        u8    name_len
        bytes name[name_len]             # ASCII, e.g. "cars/1.car"
        i32   offset                     # absolute offset in the virtual
                                         # concatenation of all pages

A resource lives in page ``offset // page_size`` starting at byte
``offset % page_size`` and ends where the next resource of the same page
begins, or at EOF.  The first byte of every page is not referenced by any
entry (the packer leaves it as scratch space), so a page's first resource
normally starts at offset 1.

The reader in the game (class ``bl``) never learns resource lengths; it
opens the page stream, ``skip``s to the offset and lets each format parser
read as far as it needs.  This module recovers the length from the next
entry, which is what makes whole-tree extraction possible.
"""

from __future__ import annotations

import os
import struct
from collections import defaultdict
from dataclasses import dataclass
from typing import Dict, Iterator, List, Optional

INDEX_NAME = "data"
PAGE_PREFIX = "data."

_HEADER = struct.Struct(">Hi")          # resource_count, page_size
_OFFSET = struct.Struct(">i")           # absolute resource offset


@dataclass(frozen=True)
class Entry:
    """One row of the index."""

    name: str
    offset: int

    def split(self, page_size: int) -> tuple:
        """Return ``(page, offset_within_page)``."""
        return divmod(self.offset, page_size)


@dataclass(frozen=True)
class Resource:
    """A single resource recovered from a page."""

    name: str
    page: int
    offset: int          # offset inside the page
    data: bytes

    @property
    def size(self) -> int:
        return len(self.data)

    def __len__(self) -> int:
        return len(self.data)


class ResourcePack:
    """Parsed ``data`` index plus its ``data.<n>`` pages."""

    def __init__(self, entries: List[Entry], page_size: int, directory: str):
        self.entries = entries
        self.page_size = page_size
        self.directory = directory
        self._cache: Optional[Dict[int, List[Resource]]] = None

    # -- loading ---------------------------------------------------------
    @classmethod
    def load(cls, jar_dir: str) -> "ResourcePack":
        """Parse ``<jar_dir>/data``."""
        with open(os.path.join(jar_dir, INDEX_NAME), "rb") as fh:
            blob = fh.read()

        count, page_size = _HEADER.unpack_from(blob, 0)
        if page_size <= 0:
            raise ValueError("invalid page size %d" % page_size)

        pos = _HEADER.size
        entries: List[Entry] = []
        for _ in range(count):
            name_len = blob[pos]
            pos += 1
            name = blob[pos:pos + name_len].decode("latin1")
            pos += name_len
            offset = _OFFSET.unpack_from(blob, pos)[0]
            pos += _OFFSET.size
            entries.append(Entry(name, offset))

        if pos != len(blob):
            raise ValueError(
                "index has %d trailing bytes (expected %d entries)"
                % (len(blob) - pos, count)
            )
        return cls(entries, page_size, jar_dir)

    # -- access ----------------------------------------------------------
    def pages(self) -> Dict[int, List[Resource]]:
        """Every resource grouped by page, sorted by offset."""
        if self._cache is None:
            grouped: Dict[int, List[tuple]] = defaultdict(list)
            for entry in self.entries:
                page, skip = entry.split(self.page_size)
                grouped[page].append((skip, entry.name))

            result: Dict[int, List[Resource]] = {}
            for page, items in grouped.items():
                blob = self._read_page(page)
                items.sort()
                resources = []
                for i, (skip, name) in enumerate(items):
                    end = items[i + 1][0] if i + 1 < len(items) else len(blob)
                    resources.append(Resource(name, page, skip, blob[skip:end]))
                result[page] = resources
            self._cache = result
        return self._cache

    def iter_resources(self) -> Iterator[Resource]:
        for page in sorted(self.pages()):
            yield from self.pages()[page]

    def read(self, name: str) -> bytes:
        """Return the raw bytes of one resource by its original path."""
        for resource in self.iter_resources():
            if resource.name == name:
                return resource.data
        raise KeyError(name)

    # -- output ----------------------------------------------------------
    def extract_all(self, out_dir: str) -> int:
        """Write every resource to ``out_dir`` preserving its path."""
        written = 0
        for resource in self.iter_resources():
            dest = os.path.join(out_dir, resource.name)
            parent = os.path.dirname(dest)
            if parent:
                os.makedirs(parent, exist_ok=True)
            with open(dest, "wb") as fh:
                fh.write(resource.data)
            written += 1
        return written

    def write_manifest(self, path: str) -> None:
        with open(path, "w") as fh:
            for resource in self.iter_resources():
                fh.write("%s\t%d\t%d\t%d\n"
                         % (resource.name, resource.page, resource.offset, resource.size))

    # -- internals -------------------------------------------------------
    def _read_page(self, page: int) -> bytes:
        with open(os.path.join(self.directory, "%s%d" % (PAGE_PREFIX, page)), "rb") as fh:
            return fh.read()
