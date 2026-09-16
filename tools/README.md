# K.O. Racing 3D — format tools

Dependency-free Python (3.8+, standard library only) readers for the custom
formats used by the MIDlet **K.O. Racing 3D** (Jollybox, `MIDlet-Version
1.70`, MIDP-2.0 / CLDC-1.1, JSR-184 `javax.microedition.m3g`).

The code is written so the byte layouts below carry over 1:1 to a Rust
rewrite:

* every integer is **big endian**;
* `str` means `u8 length` followed by that many bytes, decoded as Latin-1
  (this is what the MIDlet's `be.a(InputStream)` helper does);
* unsigned bytes are used almost everywhere — the only signed values are
  the model vertex/texcoord components;
* no third-party packages, no mmap, plain `bytes`/slices and struct
  unpacking.

## Usage

```sh
# inspect the packed archive left in the JAR root
python3 -m kora info x/

# extract all 668 resources, preserving their original paths
python3 -m kora unpack x/ assets/

# convert every custom mesh to Wavefront OBJ
python3 -m kora obj assets/models assets/obj

# render the .map track layouts to PNG minimaps
python3 -m kora mapimg assets/levels minimaps/

# inspect and render the bitmap fonts
python3 -m kora font assets/fonts/font
python3 -m kora fontimg assets/fonts/font "K.O. RACING" out.png

# parse every recognised format; --json emits the full structure
python3 -m kora dump assets/ --json > dump.json
```

`unpack_resources.py` and `model2obj.py` are thin compatibility wrappers
around the same package. To use the package from another script, put
`tools/` on `sys.path` and `import kora`.

## Class map

| Obfuscated class | Role |
| --- | --- |
| `bl` | resource-pack reader (`data` + `data.N`), also handles real `.lvl` entries |
| `be` | primitive binary reader (`u8`, `u16`, `u32`, `str`) |
| `ba` | `.car` parser |
| `ai` | `.ob` parser |
| `al` | settings + `.bck` parser |
| `bc` | `.md` parser |
| `bp` | `.hd` parser |
| `ar` (+ `a`) | `.tl` parser and collision mesh |
| `bs` | `.map` parser and track model |
| `u` | campaign `.000` reader |
| `r`, `cu`, `dk` | level list, minimap image and per-level race config |
| `p`, `dn` | bitmap fonts (proportional and fixed numeric) |
| `g` | `.tab` character-to-glyph map |

## Resource pack (`data`, `data.0` …)

```
u16   resource_count
i32   page_size                 # always 1000
repeat resource_count:
    u8    name_len
    u8[]  name                  # e.g. "cars/1.car"
    i32   offset                # absolute offset in the virtual stream
```

`page = offset / page_size`, `skip = offset % page_size`. The blob runs
from `skip` in `data.<page>` to the next entry in that page (or EOF). Byte 0
of every page is unused scratch, so the first resource usually starts at
offset 1. The game itself never needs the length; it seeks and lets the
format parser stop when it is done.

## `.car` — car descriptor (`ba.a(int)`)

```
str   model                 # base name for /models/<model>
str   low_model             # read then discarded by this build
str   texture               # /tex/<texture>
str   name                  # display name, e.g. "COARSE G4 RS"
u8    stat0                 # four tuning bytes (power/handling/grip/…)
u8    stat1
u8    stat2
u8    stat3
i8[]  extra[39]             # unread by this build
```

## `.ob` — scenery object (`ai.a(cf)`)

```
str   model                 # /models/<model>
str   texture               # /tex/<texture>
u8    flag                  # != 0 selects the wrapped/cached model variant
```

## `.bck` — sky / background (`al.q(int)`)

```
str   texture               # tried as /images/<texture>.jpg, then .png
u32   colour0               # packed 0x00RRGGBB
u32   colour1
u32   colour2
u32   colour3
u8    detail
u32   scale_a               # /1000
u32   scale_b               # /1000
```

## `.md` — mid-detail tile item (`bc.a(cf)`)

```
str   model
str   texture
u8    trailing              # present in every file, ignored by the reader
```

## `.hd` — high-detail tile item (`bp.a(b)`)

```
u8    count
repeat count:
    u8   kind
    u8   x, y, z            # position: value/100 - 1
    u8   s0, s1, s2         # scale: value/100
    u8   t0, t1, t2         # second scale: value/100
u8    trailing              # present in every file, ignored by the reader
```

## `.tl` — track tile (`ar.a(cf)` + `a(InputStream)`)

```
u8    variant               # read then discarded
u8    point_count
repeat point_count:
    u8 x, u8 y              # outline point, percent of the tile
u8    edge_count
repeat edge_count:
    u8 vertex_index         # flat list, consumed as pairs -> polygon edges
u8    height_count
repeat height_count:
    u8 value
    u8 extra  (only when value >= 100)
u8    solid[4]              # four side flags, edge rendering / minimap only
                          #   1 = the side is closed
                          #   world side = solid[(i + arg) % 4]
u8    split                 # 0 -> open = solid
u8    open[4]               (only when split != 0)
u8    unused
collision:
    u8 vertex_count
    if vertex_count > 0:
        repeat vertex_count: u8 x, u8 y, u8 z     # /100, wrap >2.0 by -2.56
        u8 triangle_count
        repeat triangle_count: u8 a, u8 b, u8 c
```

When `value < 100` the height entry is `(value, 1000.0)`; otherwise the
world height is `14 * (value - 100) / 100` (`14` is the tile edge length,
`ar.c`). All 62 shipped `.tl` files parse with zero trailing bytes.

### Collision mesh

26 of the 57 tiles ship a collision mesh; the other 31 (including every plain
road tile: `s.tl`, `c.tl`, `f.tl`) ship none, and the game then treats the whole
cell as flat at height zero.  The mesh is a height function, not a surface to
walk on:

* Vertices are in the cell's `[0,1]^2` square.  `bm.a(float, float)` rotates a
  sample point by the cell's `.map` argument before querying the mesh, so the
  mesh is stored in a frame rotated by `-arg` relative to the cell.
* The third component is **negated** when `a.java` builds each triangle, and
  the mesh shares the tile's 14-unit scale, so a point's world height is
  `-z * 14`.  The same 14 appears in the `.tl` `heights` list as
  `14 * (value - 100) / 100`.
* Where the mesh does not cover the point, the height is zero - which is why a
  kerb tile like `bra1.tl` (a strip along `x` in `[0, 0.2]`) is mostly flat
  road with a 0.7-unit gutter at one edge.

Confirmed against the visual models: sampling the collision height and the
model's top surface at the same cell-local point gives `model_z / collision_z`
of about 14 for 21 of the 26 tiles (the rest have kerbs or walls above the
surface, so their topmost triangle is not the road).  Comparing neighbouring
cells' heights at their shared edge is what pins the rotation and rules out a
scale of 7 or 1.

The ranges are real: `h1.tl` and `h4.tl` ramp from 0 down to -4.2, `br1.tl` and
`br2.tl` sit at -4.2, `vl.tl` is a platform raised to +0.7, and the `bra*` kerbs
sit 0.7-1.4 below the road.  `python3 -m kora heights assets/ 1.map` prints the
per-cell heights for a track.

**The second flag block is the drivable-side flag, not walls.** Class `ar`
keeps it in `b[]` and `bm.b(i)` returns `ar.b((i + arg) % 4)`; the `bs`
flood that walks the track branches on exactly that call. Reading it as
"open" is what makes the road graph work: with `open` (rotated by `arg`)
all 40 shipped maps become fully connected, with a mostly degree-2 ring
topology. Reading `solid` instead leaves most maps in disconnected
fragments. The `solid` block is only used to draw tile edges and the
minimap, and most tiles ship no collision mesh at all - which is why the
MIDlet's height sampling falls back to a flat plane at height zero.

## `.map` — track layout (`bs.a(InputStream, boolean)`)

```
u8    width
u8    height
repeat height times (y):
  repeat width times (x):
    u8   flags                 # bit 0 = tile, bits 1-3 = mid detail,
                               # bits 4-6 = high detail
    for each set bit, in order 0..6:
        u8 first, u8 second     # two-byte payload
                               # bit 0: (tile_type, arg) -> bm constructor
                               # bits 1-3: (type, arg) -> bm.a/b/c(ae, ..)
                               # bits 4-6: (type, arg) -> bm.a/b/c(ap, ..)
u8    start_x
u8    start_y
u8    finish_x
u8    finish_y
u8    checkpoint_count
repeat checkpoint_count: u8 x, u8 y
```

The grid is **not** a flat `width*height` array: each cell advertises its
payloads through the flag bits, so the stream is variable length. All 40
shipped `.map` files parse with zero trailing bytes.

## Campaign `.000` — career definition (`u.c()`)

```
u8    level_count
repeat level_count:
    i16 x, i16 y                # preview / editor position
    str name                    # e.g. "TIMBERTON"
    str map                     # e.g. "ma1.map"
    u8  modes                   # bitmask of the game modes offered here
    u8  flags
    u8  unlocked                # 0 locked, 1 unlocked

u8    unlock_read               # the reader allocates unlock_read + 1 slots
repeat unlock_read: i32 value   # medal / star thresholds
u8    download_count
repeat download_count:
    i32 value
    str filename                # e.g. "full.txt"
u8    extra_count
repeat extra_count:
    u8  a, u8 b
    i32 v0, i32 v1, i32 v2      # v2 is the skip offset into the `.001` file
```

Both shipped files parse with zero trailing bytes.

## Campaign `.001` — per-level race config (`r.b()` / `cu.b()` / `dk.b()`)

The file is a sequence of six-byte blocks padded to the offsets stored in
the `.000` extras table; the game seeks to `v2`, reads one block and stops.

```
u8    k
u8    j
u8    flag
u8    h
u8    m
u8    opponents            # car indices are generated, not stored
```

Use `Campaign.extras[i].values[2]` as the offset and
`RaceConfig.parse_at(blob, offset)`.  It is not in the extension dispatch
because a whole file cannot be parsed without those offsets.

## Fonts (`p`, `dn`, `g`)

A bitmap font is three resources sharing a base name (e.g. ``font``):

``/fonts/font``      glyph metrics (no extension)
``/fonts/font.tab``  character -> glyph map (class ``g``, optional)
``/fonts/font.png``  RGBA atlas

Metrics file (class ``p``):

```
u8    glyph_count
u8    advance_width[glyph_count]
u8    cell_height
```

Character map (class ``g``):

```
u8    has_glyph_index
u8    count
repeat count:
    u16le char_code
    u8    glyph_index     # only when has_glyph_index != 0
```

Without the index the glyph slot is the entry position; an unknown
character maps to glyph 0.  Glyphs are packed left to right in the atlas
and wrap to a new row when ``x + width > atlas_width``, advancing ``y`` by
``cell_height``.  ``dn`` is a fixed-grid numeric font (digits, ``:``, ``.``,
``+``, ``-``) used for the speed/lap HUD.  ``python3 -m kora font`` prints a
font's metrics and table; ``python3 -m kora fontimg`` renders sample text.

## Not yet fully decoded

| Path | Notes |
| --- | --- |
| `objects/*.hm` | single leftover resource. |
| `*.png_l` textures | low-detail variants; ordinary PNG data with a `_l` suffix. |
| `sounds/*` | `.amr` clips referenced by `as.java` are **not in the pack**; only `sounds/theme.mid` ships in the JAR. |

## UI text (`ui/*.txt`)

The MIDlet's own labels are not images.  `aq.a(int)` returns the encoded text
for a label id, and it delegates to a `f` instance that reads `/ui/ui.txt` as
UTF-8 lines of the form ``id:text`` (the one id it treats specially is 231,
which lands in `al.e`).  Five files ship: `ui.txt` (the interface), plus
`help.txt`, `online.txt`, `full.txt` and `bob.txt`.

Two ranges matter for the port:

```
127 SPEED         128 ACCELERATION   129 BRAKING      130 HANDLING
```

drawn as ``aq.a(127 + i)`` beside each car's four values, so the order in a
`.car` is speed, acceleration, braking, handling;

```
202 CIRCUIT  203 RACE  204 TIME CHASE  205 SURVIVAL
206 HEAD TO HEAD  207 SLIDESHOW  208 SPECIAL
```

which is one per game mode, in the same order as the mode byte of a `.000`
race record - and the three that carry a clock instead of a starting grid
(modes 2, 5 and 6) are exactly TIME CHASE, SLIDESHOW and SPECIAL.

There is no label for a garage or an upgrade anywhere in the file, which is
consistent with there being no economy in the build at all.

## Rust port

`rust/kora/` reimplements the engine in Rust (macroquad + rapier3d) and reads
the same formats straight from `data`/`data.<n>`; the module split is
`pack.rs` (archive), `format.rs` (the parsers above), `scene.rs` (map to
meshes and collision), `physics.rs` (rapier vehicle) and `text.rs` (bitmap
font).  `cargo test --release` builds every one of the 40 tracks headlessly,
so the layouts above are exercised on all shipped data rather than a sample.
