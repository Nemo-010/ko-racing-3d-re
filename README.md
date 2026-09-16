# K.O. Racing 3D (`KORa_17_612805.jar`) — reverse-engineering notes

> **This repository was written entirely by an LLM.** Every line of it - the
> Python tooling, these notes and the Rust port - was produced by Claude
> (Anthropic) from the decompiled game, under human direction. No part of it
> was hand-written by a person. The game is Jollybox's; see
> [Authorship](#authorship).

## Target

| | |
| --- | --- |
| File | `KORa_17_612805.jar` |
| Size | 1,209,202 bytes |
| SHA-256 | `25b3a89613d140277931cfb9d4c3704f7b9a406a9275042e736be4d05405698a` |
| MD5 | `45e492b8e27f101c8e52d7cc6f3f3746` |
| MIDlet name | K.O. Racing 3D |
| Vendor | Jollybox |
| MIDlet-Version | 1.70 |
| MIDlet class | `KORa` |
| Profiles | MIDP-2.0 / CLDC-1.1, `Nokia-MIDlet-App-Orientation: landscape` |
| APIs | JSR-184 (`javax.microedition.m3g`), JSR-82 Bluetooth, JSR-135 MMAPI, Nokia UI, SMS/RMS |
| Build | `Created-By: 1.7.0-b147 (Oracle)`, `Ant 1.8.2` |

It is a 3D arcade racer: 8 cars, ~20 career tracks plus a deluxe campaign,
four game modes (Career, Online Tour, Bluetooth multiplayer, Training), and
several race rules (circuit, survival, time chase, slideshow).  The 3D
engine is the phone's built-in **JSR-184/M3G** implementation, driven by
custom byte-sized mesh files.  Rendering targets are pre-rendered with
render-to-texture reflections (`_r` models plus `/tex/r.png`).

## What is in this workspace

```
setup.sh                fetches the JAR and unpacks what the tools/port read
x/                      raw JAR contents (resources, class files)
src/                    126 Java classes decompiled with CFR 0.152
assets/                 all 668 packed resources, original paths preserved
assets/obj/             250 meshes converted to Wavefront OBJ
minimaps/               40 track layouts rendered to PNG
campaign.tsv            careers: level order, maps, unlock data (generated)
tools/kora/             dependency-free Python readers + CLI
tools/README.md         exact byte layouts and Rust-porting notes
rust/kora/              playable Rust reimplementation (macroquad + rapier3d)
rust/kora/assets/       the same resource tree, for the port (recreated by setup.sh)
```

The game archive and everything extracted from it are **not** committed: it
is the property of its authors.  Only the reverse-engineering code and notes
are in the repository.  Fetch and unpack the data with:

```sh
./setup.sh    # downloads the JAR; fills x/, assets/, the loose JAR dirs the
              # game reads (lists/, ui/, sounds/) and the port's own tree
```

Tools (Python 3, standard library only):

```sh
python3 -m kora info x/                        # pack index summary
python3 -m kora unpack x/ assets/              # extract every resource
python3 -m kora obj assets/models assets/obj   # meshes -> OBJ
python3 -m kora mapimg assets/levels minimaps/ # track layouts -> PNG
python3 -m kora font assets/fonts/font         # font metrics + char map
python3 -m kora pack x/ rebuilt/ --verify x/   # rebuild the archive, byte for byte
python3 -m kora fontimg assets/fonts/font Hi out.png
python3 -m kora dump assets/ --json            # parse all known formats
```

## Architecture

The codebase is obfuscated with single/double-letter class names (121
top-level classes plus a JSON parser in `vParser/`).  Control flow:

* `KORa` — `MIDlet` entry point.  Sets up the display, the main canvas
  (`cv`, a `GameCanvas`/`Runnable`), wires the shared config hashtable and
  the Vserv ad manager.
* `cv` — main game canvas, frame loop and top-level screen switch.
* `al` — global settings/state (graphics detail, camera, controls, sound,
  vibration, video mode); also parses `.bck` backgrounds.
* `bb` — engine/asset managers (`cf` texture cache, `de` model cache,
  `bf` tile cache, `ae` mid-detail cache, `ap` high-detail cache).
* `bd extends bh`, `r`, `u`, `bt`, `cl` — the race scene, player car,
  physics and rendering.
* `m`, `v`, `w`, `dj`, `u` — online tour: leaderboards, tournaments,
  record upload/download over HTTP.
* `bu extends cu` — Bluetooth multiplayer session (`dh` does JSR-82).
* `co` — the 3D car viewer, `br extends u` — the "deluxe" campaign.
* `aq`, `p`, `g`, `dn`, `t`, `f` — bitmap fonts, text layout and UI text.
* `VservManager` — third-party ad SDK, shipped **unobfuscated**.

### Leftover source

The JAR contains `SensorControls.java.imp`, an uncompiled Java source file
for accelerometer controls (Nokia Sensor API + `DataListener`).  It is dead
code — no `SensorControls.class` exists — but it documents an experimental
motion-control path and confirms the original source tree used descriptive
names such as `Game`, `Settings`, `SensorControls`.

## Assets

The `data` index lists **668 resources totalling 948,917 bytes**:

| Directory | Count | Contents |
| --- | --- | --- |
| `models/` (+ `p/`, `np/`) | 250 | custom meshes: cars, scenery, track pieces, `_r` reflections, `_low` LODs |
| `tiles/` | 169 | 62 `.tl` tiles, 50 `.hd`, 56 `.md`, 1 `.hm` |
| `images/` | 58 | UI, HUD, minimap and map-preview images (`.png`/`.jpg`) |
| `fonts/` | 47 | bitmap fonts + `.tab` glyph tables |
| `levels/` | 45 | 40 `.map` grids, `list.txt`, 4 `8a.mapN.png` previews |
| `objects/` | 35 | `.ob` scenery descriptors |
| `tex/` | 34 | vehicle/prop textures (26 `.png` + 8 `.png_l` low-detail) |
| `cars/` | 21 | 8 usable cars + variants + hidden `hx` |
| `back/` | 5 | `.bck` sky/background descriptors |
| `campaign/` | 4 | career + deluxe definitions |
| `sounds/theme.mid` | 1 | only music file in the JAR |

Note: sound effects (`acc_8k.amr`, `slide_8k.amr`, …) are referenced by
`as.java` but are **not present** in the pack or JAR; only `theme.mid`
ships.

Cars: `COARSE G4 RS/G5` (rally), `BIRDIE 2.5PX` (fashion), `MUSCLE V503`
(vintage), `BURNER E55` (sport), `SPIRIT 320` (bonus), `DOVER S800` (suv),
`LOBSTER CX` (cx), `HELLRIDE 136L` (cool), plus the hidden `hx`.

## File formats

All formats are big endian and use a `u8 length + bytes` string primitive.
Full byte tables are in [`tools/README.md`](tools/README.md). Summary:

* **pack** — `data` is an index (`u16 count`, `i32 page_size`, then
  `str name` + `i32 offset` per entry).  A page is filled while its running
  offset is below `page_size`, so the resource that crosses the boundary
  overflows it and a `data.N` file can exceed 1000 bytes (`data.50` is 4942);
  byte 0 of every page is never written, which is the packer's one piece of
  leftover.  `python3 -m kora pack` writes archives with the same rule, and a
  repack of the shipped one is byte-identical.  The
  game seeks to an offset and reads as far as the format needs.
* **mesh** — 4 floats stored as ASCII text, then `u8`-count vertices of
  `i8 x,y,z + u8 u,v`, `u8` strip lengths and a `u16`-counted `u8` index
  list.  Rendered through M3G scale/bias: `value * scale + (128*scale + bias)`.
* **`.car`** — model, low-model (ignored), texture, display name, 4 stat
  bytes, 39 unread bytes.
* **`.tl`** — tile outline points, edges, heights, two 4-byte side-flag
  blocks, and a triangle-soup collision mesh.  All 62 files parse to the
  exact byte.  The second flag block is the **drivable-side** flag (rotated by
  the cell argument), which is what makes the tracks connected: using it, all
  40 maps form a single connected road graph; using the first block instead,
  28 of them fall apart into fragments.  Most tiles ship no collision mesh,
  which is why the MIDlet's height sampling falls back to a flat plane.  The
  26 tiles that *do* ship a mesh are a height function over the cell's local
  `[0,1]^2` square, rotated by the cell argument and negated times 14 - ramps
  that drop 4.2 units, a platform raised 0.7, and kerbs 0.7-1.4 below the road.
* **`.ob`** — model + texture + flag.
* **`.bck`** — texture + 4 RGB colours + detail byte + two fog scales.
* **`.md` / `.hd`** — mid/high-detail decoration layers.
* **`.map`** — track layout: `u8 width`, `u8 height`, then a variable-length
  flagged grid (each set flag bit contributes a 2-byte payload), then start,
  finish and checkpoint coordinates.  All 40 files parse to the exact byte;
  `minimaps/` shows the result.
* **campaign `.000`** — level list (name, map file, preview coords, mode
  bitmask, unlock thresholds, downloadable extras) and **`.001`** — a pool of
  per-race setup blobs addressed by the offsets in the `.000` table.  Each
  game mode stores its blob with its own layout (five in total), which is
  what the offsets' uneven spacing reflects; all 47 records in the two
  campaigns decode to in-range values, giving each track its lap count,
  theme and opponent grid.
* **fonts** — a metrics file (`/fonts/<name>`), an RGBA atlas (`<name>.png`)
  and a `.tab` character map; glyph rectangles are derived by wrapping the
  atlas by advance width.  All 6 tables and 18 fonts decode.

Every format the game uses to build a track or a car is now decoded; only
`objects/*.hm` and the `_l` low-detail texture variants are unmapped.

## Notable behaviour

* **Ads.** Bundled Vserv SDK (v `j0.1.13`) reports device properties
  (platform, locale, screen, IMEI if available) to
  `http://a.vserv.mobi/delivery/adapi.php`.  Called at start-up and exit.
* **SMS paywall.** The free build gates content behind premium SMS.  The
  carrier is guessed from `wireless.messaging.sms.smsc`:
  +420 → Czech (Axima, `HRAJ KORA` to 9079950), +421 → Slovak (ePay),
  +48 → Polish (ePay).  "Buy" links out to `http://koragame.com/pay`, and
  `aq` sends SMS via `sms://`.  `al.a()` exposes an "ACTIVATION KEY".
* **Online Tour.** HTTP POST to `www.koragame.com/m.php` and `o.php5`
  (`?a=u/l/j/c/n/s`, `?a=tc`, plus `c.php?a=u|d`), with RMS caches
  `KORa_record`, `KORa_tour`, `KORa_tour_list`, `KORa_deluxe`.  Tournaments
  rank best lap, best race and drift score; players lose 10% of points each
  tournament.
* **Bluetooth multiplayer.** JSR-82 `btspp://localhost:<port>;name=kora;
  authenticate=false;authorize=false;encrypt=false`, with a custom service
  record.
* **Persistence is five RMS record stores**, and they are separate concerns:
  `KORa_1.1.1` holds the *settings* (record 1 being a Java `DataOutputStream`
  blob - seventeen ints, fifteen booleans, four bytes and five strings, in the
  fixed order `al.b()` writes them, covering graphics detail, view distance,
  camera, HUD, transparency, the control scheme and its five key bindings,
  auto-throttle, sound and volume, the player name and the activation key);
  `KORa_record` holds career progress (`u.x()`/`u.y()`); `KORa_tour` and
  `KORa_tour_list` hold the online tour and its downloads; and
  `X_VSERV_PARAMETERS` is the ad SDK's.  The name carries the format's version,
  so a stale record store is simply not found rather than misread.
* **Deluxe campaign** (`br`, `KORa_deluxe`, `/images/map2.jpg`) is a second
  career track set shipped in the same JAR.

Nothing in the class set executes code from the network: online data is
parsed as plain records, and the only remote-code-ish surface is the ad
view (it can fetch a URL) — normal for a late-2000s MIDlet.

## Authorship

Everything in this repository was written by an **LLM** - Claude, by Anthropic -
in conversation, from the game's own files and the CFR decompilation. There is
no human author of the code or of the prose: a person chose what to work on and
when to stop, and checked some of the claims against the data, but did not write
any of it. There is no line in the Python tools or the Rust port that a person
typed.

What that means in practice, in both directions:

* **What it does buy.** Nothing is asserted without being written down, and the
  reasoning can be checked claim by claim. Every format in
  [`tools/README.md`](tools/README.md) is backed by a parser that consumes its
  file to the exact final byte; the Rust port's claims are backed by tests; a
  finding that turned out to be wrong is corrected in place with the evidence
  rather than quietly patched. The whole trail from "what is this file" to "the
  numbers are in a table" is in the commits.
* **What it does not buy.** No human has read this code with an author's eye.
  The parts that need eyes are exactly the parts that could not be checked: the
  menu layout, the camera framing, how the rendered theme sounds, how the
  showroom sits in frame. Those were reasoned about and never seen, because the
  environment this was written in has no display and no speakers.

Nothing here was copied from anywhere but the game. The archive, the decompiled
classes and everything extracted from them are gitignored; the code, the prose
and the Rust port are original, if machine-written.

## Reproducing

```sh
./setup.sh                                     # download + unpack the JAR
./jdk-21.0.12.1+1/bin/java -jar cfr.jar KORa_17_612805.jar --outputdir src
python3 -m kora unpack x/ assets/
python3 -m kora obj assets/models assets/obj
python3 -m kora mapimg assets/levels minimaps/
```

The decompiled classes are not committed.  To reproduce them, fetch a CFR
release (`cfr-0.152.jar`) and a JDK, then run the line above.  `javap` is
needed alongside CFR because several obfuscated `be.a` overloads differ only
by return type, which the decompiler cannot disambiguate on its own.

## Rust port

`rust/kora/` is a playable reimplementation in Rust using **macroquad** for
rendering and **rapier3d** for physics.  It reads the original
`data`/`data.<n>` archive directly and parses the game's own formats; the
driving loop, track geometry and collision all come from the shipped assets.

```sh
cd rust/kora
cargo run --release     # drive levels/1.map in cars/rally.car
cargo test --release    # headless checks: archive, all 40 tracks, physics
```

Implemented: resource-archive reader; all format parsers (mesh, `.tl`,
`.map`, `.car`, `.ob`, `.hd`, `.md`, `.tab`/font); track baking into
per-texture meshes; a road graph built from the tiles' drivable-side flags;
a collision world taken from each tile's collision mesh, so ramps and
platforms are real; a rapier raycast vehicle for the player plus AI opponents
that follow the road; lap, checkpoint and timing rules; and menus - main menu,
career event list, quick race, car selection with stat bars, pause and results
- with the deluxe campaign's 13 extra levels opened by points rather than by the
original's SMS purchase, career points and per-race best times, a display-only
showroom, the `.bck` sky, a speedometer and minimap, an OPTIONS screen whose
settings persist in the XDG directories (or beside the executable, or wherever
`KORA_SAVE` points), menus built from macroquad's own UI toolkit and skinned to
the original's palette (so they take a pointer as well as a keyboard), and no
garage or medals,
because the original has none.  No paywall and no network code of
any kind.  On-screen text is set in
Contrail One (SIL Open Font License, bundled in `rust/kora/fonts/`) and
rasterised by macroquad, rather than the pack's bitmap fonts, which the tooling
still decodes.  `cargo test --release`
runs 19 headless checks, including building all 40 tracks, driving AI round
four of them, a whole race run to the flag and scored, and the point and
best-time rules.

Not implemented: sound effects (every `.amr` clip is missing from the pack),
the Bluetooth/Vserv/SMS code paths, drift scoring and the original's menu
artwork.  See
[`rust/kora/README.md`](rust/kora/README.md).

## Suggested next steps

1. The original's mid-race HUD extras (rival-position arrows, speedometer,
   minimap) and the `.bck` sky/background resource.
4. Repack support: the pack format is simple enough to write, enabling
   asset swaps or a JAR rebuild.
