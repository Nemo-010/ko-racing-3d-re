"""Resource-pack reader for K.O. Racing 3D (Jollybox, MIDlet v1.70).

The MIDlet ships its resources in a small custom archive instead of keeping
the original directory tree inside the JAR::

    data          index of every resource
    data.0        page 0
    data.1        page 1
    ...

Index layout (big endian, no packing)::

    u16   resource_count
    i32   page_size                       # the page stride (1000), not the
                                         # length of a page file
    repeat resource_count:
        u8    name_len
        bytes name[name_len]             # ASCII, e.g. "cars/1.car"
        i32   offset                     # page * page_size + skip

A resource lives in page ``offset // page_size`` starting at byte
``offset % page_size`` and ends where the next resource of the same page
begins, or at EOF.  The first byte of every page is not referenced by any
entry (the packer leaves it as scratch space), so a page's first resource
starts at offset 1.

A page file is **not** ``page_size`` bytes long.  A page is filled while its
running offset is below ``page_size``, so the resource that crosses the
boundary overflows the page rather than starting the next one: ``data.50`` of
the shipped archive is 4942 bytes.  What ``page_size`` partitions is the
*offsets*, which is why ``skip`` is always under it while a file may be much
longer.  Replaying that rule over the 668 resource sizes reproduces every
stored offset exactly, which is what :func:`write_pack` relies on.

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


# --------------------------------------------------------------------------
# writing
# --------------------------------------------------------------------------
def write_pack(
    resources,
    out_dir: str,
    page_size: int = 1000,
    scratch: Optional[Dict[int, int]] = None,
) -> int:
    """Write a resource archive: ``data`` plus ``data.<n>`` pages.

    *resources* is an ordered sequence of ``(name, bytes)``.  Returns the number
    of pages written.

    The packing rule was recovered from the shipped archive and then checked
    against all 668 of its entries, which it reproduces exactly:

    * a page is filled while the running offset is **below** ``page_size``, so
      the resource that crosses the boundary overflows the page rather than
      starting the next one - that is why `data.50` is 4942 bytes rather than
      1000;
    * each page therefore begins at offset 1, and offsets are
      ``page * page_size + skip`` with a skip always under ``page_size``;
    * the byte at offset 0 of a page is never written.  It holds whatever the
      packer's buffer happened to contain - 252, 138, 180 ... in the shipped
      archive, matching nothing derivable - so *scratch* may carry those bytes
      across, which is what makes repacking an existing archive byte-identical
      instead of merely equivalent.
    """
    os.makedirs(out_dir, exist_ok=True)
    scratch = scratch or {}

    entries: List[tuple] = []
    pages: Dict[int, bytes] = {}
    page, skip = 0, 1
    blob = bytearray([scratch.get(0, 0) & 0xFF])
    for name, data in resources:
        if skip >= page_size:
            pages[page] = bytes(blob)
            page += 1
            skip = 1
            blob = bytearray([scratch.get(page, 0) & 0xFF])
        entries.append((name, page * page_size + skip))
        blob += data
        skip += len(data)
    pages[page] = bytes(blob)

    index = bytearray(_HEADER.pack(len(entries), page_size))
    for name, offset in entries:
        raw = name.encode("latin1")
        if len(raw) > 255:
            raise ValueError("resource name too long: %r" % name)
        index.append(len(raw))
        index += raw
        index += _OFFSET.pack(offset)

    with open(os.path.join(out_dir, INDEX_NAME), "wb") as handle:
        handle.write(index)
    for page, data in pages.items():
        with open(os.path.join(out_dir, "%s%d" % (PAGE_PREFIX, page)), "wb") as handle:
            handle.write(data)
    return len(pages)


def read_tree(root: str) -> List[tuple]:
    """Read an extracted resource tree in the order the archive held it.

    A `MANIFEST.tsv` written by :meth:`ResourcePack.write_manifest` records that
    order; without one the paths are sorted, which is deterministic but not the
    order the shipped archive used.
    """
    manifest = os.path.join(root, "MANIFEST.tsv")
    if os.path.exists(manifest):
        names = []
        with open(manifest, "r") as handle:
            for line in handle:
                if line.strip():
                    names.append(line.split("\t")[0])
    else:
        names = []
        for base, _dirs, files in os.walk(root):
            for name in files:
                if name == "MANIFEST.tsv":
                    continue
                full = os.path.join(base, name)
                names.append(os.path.relpath(full, root).replace(os.sep, "/"))
        names.sort()

    resources = []
    for name in names:
        with open(os.path.join(root, name), "rb") as handle:
            resources.append((name, handle.read()))
    return resources


def scratch_bytes(directory: str, page_count: int) -> Dict[int, int]:
    """The unwritten byte at offset 0 of each page, for an exact repack."""
    result = {}
    for page in range(page_count):
        path = os.path.join(directory, "%s%d" % (PAGE_PREFIX, page))
        with open(path, "rb") as handle:
            head = handle.read(1)
        result[page] = head[0] if head else 0
    return result


def _archive_files(directory: str) -> set:
    """The files that make up an archive, ignoring anything beside them."""
    return {
        name for name in os.listdir(directory)
        if name == INDEX_NAME or name.startswith(PAGE_PREFIX)
    }


def diff_packs(written: str, reference: str) -> List[str]:
    """Compare two archives file by file; an empty list means identical."""
    differences = []
    for name in sorted(_archive_files(written) | _archive_files(reference)):
        left = os.path.join(written, name)
        right = os.path.join(reference, name)
        if not os.path.exists(left):
            differences.append("only in the reference: %s" % name)
            continue
        if not os.path.exists(right):
            differences.append("only in the output: %s" % name)
            continue
        with open(left, "rb") as handle:
            a = handle.read()
        with open(right, "rb") as handle:
            b = handle.read()
        if a != b:
            differing = sum(1 for x, y in zip(a, b) if x != y) + abs(len(a) - len(b))
            first = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
            differences.append(
                "%s: %d bytes differ of %d, first at %d (%r against %r)"
                % (name, differing, max(len(a), len(b)), first,
                   a[first:first + 1], b[first:first + 1])
            )
    return differences
