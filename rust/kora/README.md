# K.O. Racing 3D - Rust port

A reimplementation of the Jollybox J2ME racer *K.O. Racing 3D* (MIDlet 1.70)
in Rust, using [macroquad](https://macroquad.rs) for rendering and
[rapier3d](https://rapier.rs) for physics.

It reads the **original `data`/`data.<n>` resource archive directly** and
parses the game's own model, tile, map, car and font formats - nothing is
converted beforehand.  The format layouts are documented in
[`tools/README.md`](../../tools/README.md).

## Running

`assets/` (the resource archive) is not committed - it is the property of the
game's authors.  Fetch it first with the repository's setup script:

```sh
../../setup.sh                           # downloads the JAR, fills assets/
cd rust/kora
cargo run --release                      # track levels/1.map, car cars/rally.car
cargo test --release                     # headless asset + physics checks
```

Environment overrides:

| Variable | Default | Meaning |
| --- | --- | --- |
| `KORA_ASSETS` | `assets` | directory holding `data`, `data.*` and `lists/` |
| `KORA_MAP` | `1.map` | track to load (`levels/*.map`) |
| `KORA_CAR` | `cars/rally.car` | car descriptor to drive |

Controls: **arrows** or **WASD** to drive, **space** handbrake, **R** respawn,
**Esc** quit.

The `assets/` directory in this crate is the unpacked `data`/`data.*` archive
plus the loose `lists/` files that live at the JAR root (`tile_list`,
`md_list`, `hd_list`, `ol`).  Only those loose lists are copied out of the JAR;
everything else is read through the pack reader.

## What is implemented

* **Resource archive reader** (`pack`) - index parse, per-page slicing,
  length recovery from the next entry.
* **Format parsers** (`format`) - mesh, `.tl` tile, `.map` layout, `.car`,
  `.ob`, `.hd`, `.md`, `.tab` font table and font metrics.  These mirror the
  MIDlet classes and are the same layouts the Python tooling uses.
* **Track builder** (`scene`) - walks the `.map` grid, resolves tile types
  through `lists/tile_list`, mid detail through `md_list` and high detail
  through `hd_list` + `ol`, applies each item's 0-3 rotation and bakes
  everything into one macroquad mesh per texture.  The tile triangles are also
  emitted as a rapier trimesh, so the car drives on the actual tile geometry
  rather than a flat plane.
* **Vehicle** (`physics`) - a dynamic chassis with rapier's
  `DynamicRayCastVehicleController`, a trimesh ground collider, four wheels,
  engine/brake/steering and a respawn.
* **HUD** (`text`) - text drawn with the game's own bitmap font
  (`/fonts/font` + `font.tab` + `font.png`), glyphs laid out from the atlas.

## Layout

```
src/pack.rs      resource archive reader
src/format.rs    per-format parsers
src/scene.rs     map -> baked meshes + collision trimesh + car geometry
src/physics.rs   rapier vehicle and world
src/text.rs      bitmap-font HUD
src/main.rs      macroquad front-end
tests/port.rs    headless checks (archive, formats, all 40 tracks, physics)
```

Geometry building never touches the GPU, so `scene::build` and
`scene::build_car` run in tests.  Textures are uploaded separately by
`Track::attach_textures` / `scene::load_car_texture`, which need a live
macroquad context.

## Coordinate conventions

The game is **Z-up**: `X`/`Y` span the ground plane and `Z` is height, and
tile models are authored to match.  macroquad and rapier are Y-up, so every
vertex is rotated -90 degrees about X on the way in:

```
(x, y, z)  ->  (x, z, -y)
```

That mapping is orientation-preserving, so triangle winding and the car's
forward direction survive.  A tile is `ar.c` = **14** world units across, the
static node scale the game applies to tile/detail/object models is
`ar.a` = **7.01**, and one map cell's world position is `cell * 14`.  The car,
unlike the tiles, is *not* scaled by `ar.a` - it keeps its natural model size.

## glam versions

macroquad 0.4 uses **glam 0.27**; rapier3d 0.35 uses **glam 0.33** through the
`glamx` crate.  Both are glam, but they are different major versions, so
`Vec3`/`Quat` are converted component-wise at the boundary
(`vec3(v.x, v.y, v.z)`, `Quat::from_xyzw(...)`) instead of relying on a shared
type.  Inside the physics module the rapier types are used as-is.

## Not implemented

This is the driving engine and one race, not the whole game:

* menus, car selection, the garage and the `.bck` backgrounds;
* the campaign (`.000`/`.001`), unlock progression and medals;
* opponent AI and lap/checkpoint timing (the `.map` trailer carries the start,
  finish and checkpoints, but they are not used yet);
* audio (the referenced `.amr` clips are not shipped in the pack at all) and
  the Bluetooth/Vserv/SMS code paths;
* mid/high-detail placement uses the `.hd` half-tile offsets and rotation, but
  the per-object orientation matrices of class `ai` are simplified to a yaw.
