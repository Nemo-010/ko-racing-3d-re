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
cargo run --release                      # main menu
cargo test --release                     # headless checks
```

| Variable | Default | Meaning |
| --- | --- | --- |
| `KORA_ASSETS` | `assets` | directory holding `data`, `data.*` and `lists/` |
| `KORA_SAVE` | `kora-save.txt` | where career points and medals are kept |
| `KORA_SKIP_MENU` | unset | set to boot straight into a race |
| `KORA_MAP` | `1.map` | track for `KORA_SKIP_MENU` (any of the 40) |
| `KORA_CAR` | `cars/rally.car` | car to select at startup |
| `KORA_LAPS` | campaign, else `3` | overrides the race length |
| `KORA_OPPONENTS` | campaign, else `3` | overrides the size of the grid |

Controls: **arrows** or **WASD** to drive, **space** handbrake, **Esc** pause,
and in the menus arrows move, **Enter** confirms and **Esc** goes back.

## What is implemented

* **Resource archive reader** (`pack`) - index parse, per-page slicing,
  length recovery from the next entry.
* **Format parsers** (`format`) - mesh, `.tl` tile, `.map` layout, `.car`,
  `.ob`, `.hd`, `.md`, `.tab` font table and font metrics, mirroring the
  MIDlet classes.
* **Track builder** (`scene`) - walks the `.map` grid, resolves tile types
  through `lists/tile_list`, mid detail through `md_list` and high detail
  through `hd_list` + `ol`, applies each item's 0-3 rotation and bakes
  everything into one macroquad mesh per texture.  It also builds the physics
  road from each tile's **collision mesh**: a sub-divided patch over every cell
  whose tile carries one (26 of the 57 do, including the ramps that drop 4.2
  units and the platform raised 0.7), a flat quad for the rest, and a barrier
  standing on the surface wherever a side is not drivable.  `SurfaceGrid` keeps
  the same height function for runtime queries.
* **Road graph** (`grid`) - adjacency from the tile's drivable-side flags,
  rotated by the cell argument.  All 40 shipped maps come out connected, which
  is how the flag semantics were confirmed rather than guessed.  The graph
  provides the race direction, a starting grid, and the "next point on the
  road" lookahead the AI steers at.
* **Career tables** (`campaign`) - `campaign.000` and `deluxe.000` list the
  levels and races, and each race record points at its setup in the matching
  `.001`.  Every game mode stores that setup with its own layout, all five of
  which are decoded, so a track that belongs to a campaign takes its lap count
  and opponent grid from the game's own data.
* **Race rules** (`race`) - laps, checkpoints in order, timing, best lap and
  race position.  The opening crossing of the line never scores, and a car
  that starts behind it has that first crossing suppressed.
* **Opponents** (`ai`) - cars follow the road locally instead of a
  precomputed racing line, slowing for the corner they are about to take, with
  a stuck detector that reverses out of a barrier.
* **Vehicle** (`physics`) - one shared `PhysicsWorld` for the player and the
  opponents, each a dynamic chassis driven by rapier's
  `DynamicRayCastVehicleController`, so they collide with each other.  Cars are
  lifted back onto the road surface when they sink below it (`World::lift_to`),
  which is what the MIDlet does every frame and what lets a car cross the steps
  between two tiles instead of being trapped by them.
* **Menus** (`menu`) - main menu, the career event list, a quick-race list of
  all 40 tracks, car selection with each car's four stat bars, a pause menu and
  a results panel.  Locked events grey out and show the points they need.
* **Career progress** (`progress`) - medals and points.  A race's entry
  threshold and award come from its `.000` record, the medal from the finishing
  position (gold, silver, bronze), and a better medal on a race already won
  only pays the difference so a race cannot be farmed.  Points and the chosen
  car are saved to `KORA_SAVE`.
* **HUD** (`text`) - lap, position, total time, current and best lap.

## Layout

```
src/pack.rs      resource archive reader
src/progress.rs  medals, points, car list and the save file
src/menu.rs      every screen outside the race
src/format.rs    per-format parsers
src/grid.rs      road graph, race direction, starting grid
src/campaign.rs  career tables -> laps/opponents/theme for a track
src/scene.rs     map -> baked meshes + collision + barriers + car geometry
src/physics.rs   rapier world and cars
src/ai.rs        opponent driving
src/race.rs      laps, checkpoints and timing
src/text.rs      text drawing, over macroquad's rasteriser
src/main.rs      macroquad front-end
tests/port.rs    headless checks
```

Geometry building never touches the GPU, so `scene::build` and
`scene::build_car` run in tests.  Textures are uploaded separately by
`Track::attach_textures` / `scene::load_car_texture`, which need a live
macroquad context.

## Tests

`cargo test --release` runs headlessly:

| Test | Covers |
| --- | --- |
| `pack_index_is_complete` | the archive index and a sample of resources |
| `every_map_parses`, `every_car_and_tile_parses` | every `.map`, `.car`, `.tl`, `.ob`, `.md`, `.hd` |
| `every_track_builds_geometry` | all 40 tracks bake meshes and collision |
| `grid_is_connected_for_every_track` | road graph connectivity, race direction and starting grid on all 40 |
| `ai_targets_stay_on_the_road` | the AI's lookahead never leaves a drivable cell |
| `laps_require_checkpoints_in_order` | the lap state machine, including the opening crossing |
| `car_settles_and_drives_on_the_track` | the vehicle rests on the road and moves under throttle |
| `opponents_drive_the_track` | four AI cars complete laps of 1.map and post sane lap times |
| `opponents_survive_other_tracks` | AI on a long checkpoint circuit, a big open track and a twisty one |
| `campaign_tables_decode` | both career tables, every race record, all five mode layouts |
| `campaign_lookup_picks_the_right_race` | the map -> race lookup, including quick-race maps with no entry |
| `campaign_themes_still_build` | building a track in theme 3 produces the same collider |
| `collision_meshes_give_tracks_elevation` | ramps and platforms reach the collider, and the collider matches the runtime height function |
| `cars_climb_the_track_elevation` | AI cars drive down 1.map's 4.2-unit ramp and back out |
| `medals_and_points_persist` | medal rules, points only for an improvement, save/load |
| `career_and_quick_lists_are_built` | the 47 career events, the 40 quick-race tracks and the 8 cars |
| `a_race_runs_to_the_flag_and_scores` | a whole 2-lap race with four AI cars, scored as the results screen does |

## Text

Everything on screen goes through macroquad's own text renderer, which
rasterises glyphs with `fontdue` at whatever pixel size is asked for, so the
menus and HUD stay sharp at every scale.  `src/text.rs` is a thin wrapper that
keeps the callers thinking in "top of a line" coordinates and adds a drop
shadow; the default `ProggyClean` face is monospaced, which suits the columnar
event and car lists.

The pack's own fonts are still fully decoded - `python3 -m kora font` prints a
font's metrics and character map and `python3 -m kora fontimg` renders sample
text, and `tools/README.md` has the layouts.  They are not used at runtime
because macroquad's `Font` is fontdue-based and takes TrueType outlines, so a
bitmap atlas cannot be loaded into it.

Text is the one thing not covered by the tests: macroquad rasterises it through
a live graphics context, so measuring or drawing a string needs a window.

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
Direction indices match the MIDlet: 0 = +X, 1 = -Y, 2 = -X, 3 = +Y.

## glam versions

macroquad 0.4 uses **glam 0.27**; rapier3d 0.35 uses **glam 0.33** through the
`glamx` crate.  Both are glam, but different major versions, so `Vec3`/`Quat`
are converted component-wise at the boundary (`vec3(v.x, v.y, v.z)`,
`Quat::from_xyzw(...)`).  Inside the physics module the rapier types are used
as-is.

## Physics notes

* The road collider is **flat quads at `y = 0`**, plus a barrier on every
  closed side.  The MIDlet samples each tile's collision mesh for height and
  falls back to a flat plane when there is none - and most tiles ship no
  collision data (`s.tl` and friends).  Baking the *visual* triangles instead
  leaves gaps where cars drop through, which is exactly what happened before
  the switch.
* Wheel connection points must start **above** the road: a connection at the
  chassis floor puts the suspension ray under the trimesh and no wheel ever
  reports contact.
* rapier's default wheel tuning assumes ~0.5 m of suspension travel.
  Equilibrium compression is `g / (4 * stiffness)` regardless of mass, so a
  car half a unit tall needs stiffness ~24 or the chassis drags on the road.

## Not implemented

* the mid-race HUD of the original (its rival-position arrows, speedometer and
  minimap) and the garage's car upgrades; the `.bck` backgrounds;
* the original's own menu art and text, which live in packed images and text
  blobs - the port draws its screens from the tables instead;
* the exact original medal rule: the tables give each race an entry threshold
  and an award, but what *performance* earns the medal is not in them, so the
  port uses the finishing position;
* audio (the referenced `.amr` clips are not shipped in the pack at all) and
  the Bluetooth/Vserv/SMS code paths;

* class `ai`'s per-object orientation matrices are simplified to a yaw for
  high-detail scenery.
