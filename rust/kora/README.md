# K.O. Racing 3D - Rust port

> **Written entirely by an LLM.** Every module, test, comment and line of this
> port was produced by Claude (Anthropic) from the game's own resources and the
> decompiled MIDlet, under human direction. None of it was hand-written and no
> human has reviewed it. See the [repository
> README](../../README.md#authorship) for what that does and does not buy.

A reimplementation of the Jollybox J2ME racer *K.O. Racing 3D* (MIDlet 1.70)
in Rust, using [macroquad](https://macroquad.rs) for rendering and
[rapier3d](https://rapier.rs) for physics.

It reads the **original `data`/`data.<n>` resource archive directly** and
parses the game's own model, tile, map, car and font formats - nothing is
converted beforehand.  The format layouts are documented in
[`tools/README.md`](../../tools/README.md).

## Running

No game data is committed - it is the property of its authors.  Fetch it with
the repository's setup script, which leaves an **extracted resource tree** in
`assets/`, the port's own copy of it here, and the loose JAR directories the
game reads (`lists/`, `ui/`, `sounds/`):

```sh
../../setup.sh                           # downloads the JAR, fills the trees
cd rust/kora && cargo run --release      # main menu
cargo test --release                     # headless checks
```

The port reads `assets` **in the working directory**, so both of these work:

```sh
cd rust/kora && cargo run --release
cargo run --release --manifest-path rust/kora/Cargo.toml   # from the root
```

Because that directory is a tree rather than the packed archive, swapping an
asset is a matter of editing the file and running again.  Point `KORA_ASSETS` at
`x` to read the archive the game actually shipped instead - the reader takes
either form, and a test checks the two agree byte for byte.

| Variable | Default | Meaning |
| --- | --- | --- |
| `KORA_ASSETS` | `assets` | resource tree (or the `data`/`data.*` archive) to read |
| `KORA_SAVE` | per-user data dir | file for points, best times and the chosen car |
| `KORA_SETTINGS` | per-user config dir | file for the options screen's settings |
| `KORA_DATA_DIR` | per-user data dir | overrides where the data directory is |
| `KORA_CONFIG_DIR` | per-user config dir | overrides where the config directory is |
| `KORA_MUSIC` | unset | set to `0` to start with the music off |
| `KORA_SKIP_MENU` | unset | set to boot straight into a race |
| `KORA_MAP` | `1.map` | track for `KORA_SKIP_MENU` (any of the 40) |
| `KORA_CAR` | `cars/rally.car` | car to select at startup |
| `KORA_LAPS` | campaign, else `3` | overrides the race length |
| `KORA_OPPONENTS` | campaign, else `3` | overrides the size of the grid |

Controls: the **control scheme** picked in OPTIONS decides which keys drive -
CLASSIC is the arrow keys, LEFT-HANDED is WASD and RIGHT-HANDED is the numeric
keypad - **space** is the handbrake, **Esc** pauses, **M** toggles the music,
and in the menus the arrows move, **Enter** confirms and **Esc** goes back.

## What is implemented

* **Resource reader** (`pack`) - either form: the archive the game shipped
  (`data` plus `data.<n>`, index parsed, pages sliced, lengths recovered from
  the next entry) or an extracted tree, every file keyed by its path.  The port
  reads the tree by default, because swapping an asset should not need a repack
  step, and `KORA_ASSETS=x` reads the archive instead.
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
* **The front end** (`space`, `menu`, `map`) - the original's own: the starfield
  and the planet on their black backdrop, the entries along the bottom in a bar
  with `<` and `>` either side to step through them, the jump into the map that
  choosing CAREER starts, and the map itself - `/images/map.jpg` with a marker
  per level, the races that start from each in a panel, and the arrows to walk
  the markers.  See [The front end](#the-front-end).
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
* **Showroom** - the car screen draws the highlighted car on a turntable,
  spinning at the ten degrees a second the MIDlet's own preview uses, with the
  four stat bars beside it.  Display only: nothing is for sale.  All eight cars
  are built once at startup.
* **Sound** (`music`) - the game ships one sound, `sounds/theme.mid`, a
  16-track General MIDI file of 1273 notes over about 84 seconds.  macroquad
  plays WAV, OGG and MP3 but not MIDI, so the port reads the file and renders
  it itself: parse the tracks, mix a simple voice per part, encode a WAV in
  memory and hand that to macroquad, looped.  It is a rendition rather than the
  original handset's audio - a General MIDI file names the instrument, not the
  sound - but the notes, tempo and arrangement are the file's own.  `M` toggles
  it and the pause menu shows which way it is.
* **Sky** (`sky`) - the `.bck` backgrounds.  A track's theme byte picks one of
  the five (`al.a` lists them clear, rain, snow, desert and sunset, and
  `al.q(j)` is called with the theme while the race is set up), and the file
  names a strip under `/images/` that is tried as `.jpg` and then `.png`, which
  is the reader's own rule.  M3G scales a 2D background image to the viewport,
  so the strip is stretched to the screen and the scene drawn over it.
* **HUD extras** (`hud`) - a speedometer and a minimap.  `r.java` feeds the HUD
  a speed and its fraction of the car's maximum and the `cg` element draws that
  fraction as a bar, so the port shows a number and a bar; `bs.a()` builds a
  three-pixel-per-cell image of the track and maps positions onto it, so the
  port draws the same grid with every car on it and a tick along each heading,
  which is where the rivals are.
* **Menus** (`menu`) - main menu, the career event list, the deluxe list, a
  quick-race list of all 40 tracks, the car screen and its showroom, a pause
  menu and a results panel.  Locked events grey out and show the
  points they need, each event carries its stored best time, and each shows the
  game's own name for its mode (CIRCUIT, RACE, TIME CHASE, SURVIVAL, HEAD TO
  HEAD, SLIDESHOW, SPECIAL).
* **Career progress** (`progress`) - points and best times, which is the pair
  the game keeps.  A race's entry threshold and award come from its `.000`
  record: the MIDlet holds a running total, refuses to offer a race whose gate
  sits above it, and pays the record's own value once when a race is passed and
  never again.  A *negative* value is not an award at all - `u.n()` negates it
  into a group index and unlocks that group - and seven bonus races in
  `campaign.000` carry one, so the port marks those as unlocks and pays
  nothing.
* **Best times** - every finish is timed and the quickest is kept, win or lose,
  the way `r.c(int)` keeps whichever of the new time and the old one is better
  and `u.c()` initialises the stored value to `Integer.MAX_VALUE` for "no record
  yet".  There are no medals: the game has no medal, gold, silver or bronze
  anywhere in its text, and an earlier version of this port invented them.  The
  results panel says NEW RECORD with the old time beside it when one falls.
* **Garage** (`progress` + `menu`) - spend career points on any car's four
  values, up to 6, which is the ceiling the game's own best car carries.  Each
  purchase changes what the car is like to drive, and the upgrades are saved
  with the rest of the progress.
* **HUD** (`text`) - lap, position, total time, current and best lap.

## Layout

```
assets/          the extracted resource tree (from ./setup.sh, not committed)
fonts/           Contrail One + its OFL licence (bundled via include_bytes)
labels.tsv       every string the interface draws, with its provenance
src/music.rs     MIDI to WAV, for the theme
src/pack.rs      resource archive reader
src/progress.rs  points, best times, car list and the save file
src/labels.rs    UI text, loaded from labels.tsv
src/menu.rs      every screen outside the race
src/format.rs    per-format parsers
src/grid.rs      road graph, race direction, starting grid
src/campaign.rs  career tables -> laps/opponents/theme for a track
src/scene.rs     map -> baked meshes + collision + barriers + car geometry
src/physics.rs   rapier world and cars
src/ai.rs        opponent driving
src/race.rs      laps, checkpoints and timing
src/settings.rs  the options screen's settings, and their file
src/sky.rs       the .bck sky and horizon
src/space.rs     the front end's backdrop: stars, and the planet
src/map.rs       the career map: markers, races, and the panel
src/hud.rs       speedometer and minimap
src/theme.rs     the ui skin: the MIDlet's palette over macroquad's widgets
src/text.rs      text drawing, over macroquad's rasteriser
src/main.rs      macroquad front-end
src/bin/dump_track.rs   writes a built track to a file, for `kora view`
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
| `points_and_best_times_persist` | the award paid once, the time kept when it improves, save/load |
| `career_and_quick_lists_are_built` | the 47 career events, the 40 quick-race tracks and the 8 cars |
| `a_race_runs_to_the_flag_and_scores` | a whole 2-lap race with four AI cars, scored as the results screen does |
| `bonus_races_unlock_rather_than_award` | the seven negative records are unlocks, pay nothing, and the groups are 1..7 |
| `the_bundled_font_covers_the_interface` | the bundled face parses, covers every character the UI builds, and rasterises |
| `the_deluxe_campaign_opens_on_points_alone` | the 13 deluxe events are extra tracks, opened by points with nothing else consulted |
| `car_stats_change_the_handling` | each of the four values moves its own part of the tuning |
| `upgrades_make_a_car_quicker` | a maxed car reaches a higher speed than a stock one |
| `race_modes_are_named_by_the_game` | the seven mode names, and that the clock-carrying modes are the non-circuit ones |
| `the_label_table_is_well_formed` | the TSV parses, keys are unique, every key the code uses resolves, placeholders fill |
| `car_texture_coordinates_land_on_the_car` | the texture coordinates are not flipped: they land on the bodywork, glass and livery, not the empty field beside them |
| `every_track_texture_coordinate_fits_its_tiled_atlas` | every mesh of every track keeps its coordinates inside the tiled copy of its atlas |
| `tiling_a_texture_is_the_same_as_wrapping_it` | a lookup in the tiled copy is a wrapping lookup in the original, and needs no copy when the coordinates already fit |
| `every_career_race_has_a_marker_on_the_map` | every race in both tables lands on exactly one marker, and every marker is on the picture |
| `the_career_map_walks_west_to_east_and_back` | the arrows reach every marker and never skip one, in both directions |
| `the_planet_grows_from_a_disc_to_the_whole_view` | the planet is a modest disc at rest and covers the screen when the jump lands |
| `the_pictures_the_front_end_needs_decode` | the maps, both planet plates and the five sky strips all decode |
| `the_game_z_axis_points_down` | the models' Z runs downwards: road tiles keep their kerbs above the road and every car stands on its wheels |
| `every_career_race_has_a_marker_on_the_map`, `the_career_map_walks_west_to_east_and_back` | the career map's markers, and the arrows that walk them |

## No paywall, no server

The original gates its deluxe campaign behind an SMS purchase: `cv` checks an
entitlement flag (`al.h()`) and, when it is clear, shows the purchase screens,
and `br` sets that flag after the payment flow.  It also ships an Online Tour
whose leaderboards talk to `koragame.com`, a host that has been gone for years,
plus a Vserv ad SDK and a Bluetooth mode.

**None of that is reproduced.**  The deluxe levels are simply a second list in
the port, opened by career points exactly like the career ones, and the game
makes no network request of any kind - the dependency list is macroquad and
rapier3d, and a grep for anything socket-shaped in `src/` finds only this
paragraph's own subject.  Everything is earned by racing and kept in a local
text file.

The garage is the port's own feature rather than a ported one: the original has
no economy at all (there is no price, cost or purchase anywhere in its 126
classes), its "garage" `co` is a 3D car viewer, and the four values in each
`.car` only ever reach the bars on the setup screen - the 39 bytes that follow
them are never read by this build.  So the mapping from those four values to
engine, damping, steering, friction and brakes is the port's, chosen so the
first car in `ba.a`'s list lands on the constants the port was calibrated with;
`physics::Tuning` documents it.

## The front end

The MIDlet's front end is not a list.  `bd` draws a black `Background`, a
billboard of `/tex/ea.jpg` - the Earth from orbit - tilted -45 degrees about X
so the planet is seen from slightly above and spun about Z, and in front of it
`ce`, a starfield whose stars are pushed outward from the middle of the screen
by a "warp" factor.  The entries are not stacked either: each one is an `s`,
which is a label in the bar along the bottom with `<` and `>` either side, slid
in from the side that was pressed, and `y.a(int, int)` turns the bottom 70
pixels of the left and right edges into arrow buttons, so a finger can drive it.
The port does the same, drawn directly rather than through macroquad's widgets:
`space` for the backdrop and `menu::bar_menu` for the bar.

Choosing CAREER or DELUXE starts the jump.  `bd.F()` sets a target and
`bd.b(float)` runs the billboard's scale at it exponentially from 1.2, handing
the *same* number to `ce`, so the planet grows and the stars scatter together;
`/tex/ms.jpg`, the same picture radially blurred, takes over partway, and past
15.2 the map is set down in front of you.  `main::Jump` is that ramp.

The map is `u` for the career and `br` for the deluxe tour.  Each opens its own
picture - `/images/map.jpg` and `/images/map2.jpg` - and puts the campaign's
levels on it as markers: `/images/st.png`, a star, while a level is closed, and
`/images/qm.png`, a trophy, once it opens, eight frames each of four sizes in
grey and four in gold, with the chosen one pulsing through its sizes, which is
the animation `u.d` runs.  A marker is a level with at least one race starting
from it, and choosing one lists those races in a panel beside it.

Where the markers come from is worth spelling out, because it is spread across
two tables: the campaign `.000` lists the *levels* (name, track, the flag the
file carries, and the marker's x and y **in map pixels**), and each race record
names the level it belongs to.  `map::markers` does that grouping, which is what
`u.a(boolean)` does, and it is the one part of this that can be tested without a
window, so it is.

The arrows move the way `u.a(int)` moves: to the nearest marker along **by x**,
not by distance.  The markers are scattered over the map rather than laid out in
career order, so the walk goes west to east and back; the property that matters
is that it reaches every marker and never skips one, which
`the_career_map_walks_west_to_east_and_back` checks.  The port gates a level on
career points (`Progress::open`) rather than on the `.000` flag, the same rule
its lists use - the flag is the file's initial state, and the port has no server
or purchase to move it.

The camera seeing the billboard is the game's own: `bq.a` sets the M3G
perspective to a 90 degree vertical field of view, so a 2x2 quad 2.5 units away
covers `2 / (2 * 2.5)` of the screen height and the tilt flattens that to 0.71 of
it.  `space` applies that projection itself rather than standing up a 3D camera,
because the stars have to stay behind the planet and macroquad's 3D pass clears
the frame it draws into.  By the time the ramp is over the quad's near edge has
passed behind the camera - which is what flying *into* a planet is, and which a
projection cannot express, since those points would mirror through the middle of
the view - so the depth is held at the near plane and the last frames fill the
frame instead of turning it inside out.

## Audio and macroquad's features

macroquad 0.4 has `default = []`, so the sound system is behind a feature:
without `features = ["audio"]` the audio module is a stub whose
`load_sound_from_bytes` succeeds and plays nothing.  This crate enables it, and
on Linux that pulls in `quad-alsa-sys`, so building needs the ALSA headers.  It
is worth knowing before blaming your own code for the silence.

The same trap catches the pictures.  macroquad builds the `image` crate behind
`Texture2D::from_file_with_format` with only **PNG and TGA** turned on, so a
`.jpg` of the game's - the career map, both planet plates - fails to decode, and
the failure is easy to miss because every one of those loads is optional and
falls back to drawing nothing.  Cargo unifies features, so this crate asks for
the decoder itself:

```toml
image = { version = "0.24", default-features = false, features = ["jpeg"] }
```

which turns it on for macroquad's copy of the same crate too, without adding a
second copy of it.  `the_pictures_the_front_end_needs_decode` fails if that
line ever goes away.

## Where the save and the settings live

`paths.rs` resolves a per-user directory per platform, keeping configuration and
data apart the way the XDG spec asks:

| platform | settings | save |
| --- | --- | --- |
| Linux/BSD | `$XDG_CONFIG_HOME/kora/settings.txt` (`~/.config/kora`) | `$XDG_DATA_HOME/kora/save.txt` (`~/.local/share/kora`) |
| macOS | `~/Library/Application Support/kora` | same |
| Windows | `%APPDATA%\kora` | same |
| Android | `/data/data/<package>/files/kora` | same |
| web | none - there is no filesystem | none |

An empty XDG variable means unset, as the spec says.  `KORA_SAVE` and
`KORA_SETTINGS` name the files outright and `KORA_DATA_DIR`/`KORA_CONFIG_DIR`
the directories, which is how a launcher hands a sandboxed build somewhere it
may write.  A file already sitting in the working directory wins over all of it,
so a save that predates the move is not orphaned.

**macroquad does not do any of this**, which is worth knowing before looking for
the switch.  Its `load_file` is read-only and its "storage" module is an
in-memory map; there is no writable-path API and no `chdir`.  On Android that
bites: `load_file` goes through `AAssetManager` for the APK's assets, and a
relative path from `std::fs` points at the process's working directory, which is
`/` there and not writable.  An app may only write to its own directory, so
`paths.rs` works the package name out of `/proc/self/cmdline` and puts the files
under `/data/data/<package>/files/kora` - dependency-free, but **not verified on
a device**, only type-checked.  On the web there is no filesystem at all;
persistence needs `localStorage` or IndexedDB, which is what `quad-storage`
wraps there (its native backend writes a file relative to the working directory,
so it has the Android problem too).

Both `cargo check --target aarch64-linux-android` and
`cargo check --target wasm32-unknown-unknown` pass, so the port at least
compiles for the three platforms; only the desktop one has been run.

## Settings

The OPTIONS screen is the original's settings list, doing the things that still
mean something on a desktop: **graphics quality** (which gates the mid and high
detail layers when a track is built), **camera** (behind, far or inside),
**visibility** (the far plane), **background** (the `.bck` sky or a flat
colour), **HUD**, **control scheme**, **auto-throttle**, **music** and
**volume**, and **reset career** behind a confirmation.  Every value is drawn
with the game's own label for it (`16 LOW`, `19 MEDIUM`, `15 HIGH`, `27 CLASSIC`
and so on), and every value changes something - a test asserts that each pair of
states behaves differently, including that the three control schemes really do
drive with different keys.

The MIDlet keeps settings in a record store called **`KORa_1.1.1`** - the
version is part of the name, so a stale store is not found rather than misread -
with record 1 a Java `DataOutputStream` blob of seventeen ints, fifteen
booleans, four bytes and five strings.  Career progress is a *different* store,
`KORa_record`, and the online tour uses two more.  A desktop port has no record
stores, so settings go to `KORA_SETTINGS` and progress to `KORA_SAVE`, both
plain text.

One option is deliberately absent: the player name (`101 YOUR NAME:`).  The
MIDlet uploads it with a leaderboard entry; the port has no leaderboard, so a
stored name would be the one setting that changed nothing.

## Labels

Every string the interface draws lives in `labels.tsv`, not in the code:

```
key	text	source
hud_lap	LAP {}/{}	162
stat_speed	SPEED	127
mode_time_chase	TIME CHASE	204
menu_cars	SELECT YOUR CAR	125
```

`key` is what the code asks for, `text` is what is drawn, and `source` records
where the string came from: a number is the id the game itself uses in its own
`/ui/ui.txt`, and `-` marks a string the original has no id for, added for the
screens and features the port has and the original does not.  25 of the 60 are
the game's own.  `{}` is a placeholder, filled in order by `labels::format`.

The table is embedded with `include_str!`, so the binary stays self-contained,
and `src/labels.rs` is a thin lookup over it: `get`, `format`, plus the stat and
mode names the rest of the code asks for by index.  An unknown key returns the
key itself rather than panicking from inside a frame, and a test walks the file,
checks the keys are unique, the source column parsed, and that every key the
code asks for is present.

Keeping the text here rather than in Rust means a label can be corrected or
translated, and diffed against the game's own file, without touching code.

## Text

Everything on screen is set in **Contrail One**, bundled in `fonts/` and
rasterised by macroquad's own text renderer (`fontdue`) at whatever pixel size
is asked for, so the menus and HUD stay sharp at every scale.  `src/text.rs`
loads the face once and wraps `measure_text` / `draw_text_ex`, keeping the
callers thinking in "top of a line" coordinates and adding a drop shadow.
Because the face is proportional, the event and car lists draw each column at
its own x rather than padding with spaces.

Contrail One is Copyright (c) 2011 Sorkin Type Co and released under the **SIL
Open Font License 1.1**; the licence ships with it at `fonts/OFL.txt`.  The
reserved font names are "Contrail" and "Contrail One", so the file is
redistributed unmodified and under its own name.

The pack's own fonts are still fully decoded - `python3 -m kora font` prints a
font's metrics and character map and `python3 -m kora fontimg` renders sample
text, and `tools/README.md` has the layouts.  They are not used at runtime
because macroquad's `Font` is fontdue-based and takes TrueType outlines, so a
bitmap atlas cannot be loaded into it.

Text is the one thing not covered by the tests: macroquad rasterises it through
a live graphics context, so measuring or drawing a string needs a window.

## What the world is drawn at

The world is rendered offscreen at the MIDlet's own resolution - 240 lines, the
window's shape - and scaled up to the window, because the artwork was made for
it: a track tile carries about a hundred pixels of texture, and a 14-unit tile
on a 720-line window magnifies that four to eight times, which reads as a mosaic
of texels rather than as a road.  The HUD and the menus stay at the window's own
resolution, where text belongs.  The camera also uses the game's own field of
view, 90 degrees: `bq.a` sets the M3G perspective to that for every view.

## Coordinate conventions

The game is Z-up on paper, but **its models put Z downwards**, and the port has
to follow the models rather than the paper.  Three things say so, and all three
are checked by `the_game_z_axis_points_down`:

* `a`, the collision mesh reader, negates the third component of every triangle
  it builds, so a *positive* world height comes out of a *negative* z;
* a road tile keeps its drivable strip at `z = 0` and raises its kerbs to
  negative z, so those kerbs are only above the road if z runs downwards;
* all twenty car models stand their wheels on `z = 0` with the whole body above
  it, which is where a car's origin has to be for the game to sit it on the
  road.

macroquad and rapier are Y-up, so the axes are swapped *and* the third negated:

```
(x, y, z)  ->  (x, -z, -y)
```

Copying `z` instead lays the whole world out mirrored - the track hanging under
the road, the cars driving along its underside, upside down.  The mapping also
reverses orientation, so the port's triangles come out wound the opposite way
to the MIDlet's; the game draws with `PolygonMode.setCulling(160)` and macroquad
culls nothing, so the winding is cosmetic here and the orientation is not.  A tile is `ar.c` = **14** world units across, the
static node scale the game applies to tile/detail/object models is
`ar.a` = **7.01**, and one map cell's world position is `cell * 14`.  The car,
unlike the tiles, is *not* scaled by `ar.a` - it keeps its natural model size.
Direction indices match the MIDlet: 0 = +X, 1 = -Y, 2 = -X, 3 = +Y.

### Textures

M3G measures texture coordinates from the **top left** of the image with `v`
growing downwards, which is where macroquad measures them from too, so the
coordinates are used exactly as the mesh stores them - **there is no `1 - v`
anywhere**.  (A vertical flip still samples a picture, just the wrong one, so
`car_texture_coordinates_land_on_the_car` checks that the triangles land on the
bodywork and the livery rather than the empty field beside it.)

The models were authored against OpenGL's default `GL_REPEAT` and rely on it:
the coordinates are centred on zero and leave 0..1 in both axes - a track tile
spans u 0.39..1.50, a car's unwrap u 0.54..1.41 with negative v - because the
authors let the lookup wrap round the image.  macroquad has no wrap mode at all
(miniquad's texture parameters default to `Clamp` and nothing exposes them), and
clamping smears an edge pixel across every polygon that leaves 0..1, which is
what turned the tracks into a mess.

`scene::Tiling`, built from the coordinates each texture is actually used over,
emulates it instead: the image is tiled over the integer window the coordinates
occupy and the coordinates are rescaled into that window, so a clamped lookup in
the copy is exactly a repeating lookup in the original.  The floor of an atlas
that needs it is 512x512, about 2.5 MB across the whole game, and no shader.
`every_track_texture_coordinate_fits_its_tiled_atlas` checks that every mesh of
every track ends up inside its own copy.

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

## Known wrong: the collision height's sign

`scene::surface_height` reads `-z * 14` where it should read `+z * 14`, and the
note on that function explains why it is left alone for now.  The MIDlet's `a`
negates the third component of every collision triangle because its own world
has Z pointing down, and this port maps that world to Y-up once, in
`game_to_world` - so the negation must not be repeated.  All 26 tiles that ship
a collision mesh agree: a tile whose model raises a 4.2-unit wall reports
`+4.2`, and a sunken one (`vl`) reports `-0.7`, matching its model to the
hundredth.

Flipping it looks like a one-character change and is not.  The same negation
also *buries* the edge barriers this port builds for itself, which are placed at
`height_at` too, and it turns the map's own wall tiles into pits the cars drop
into harmlessly: between them the two mistakes cancelled, which is why the port
has never actually collided with a wall, and why every ramp in it is inside out
in a way no driving test can see - the collider and the height function share
the source.  Correcting the sign has to come with the barrier placement and the
AI checked against real walls, in one change.

## Not implemented


* the original's own menu *layout* for the screens behind the front end: the
  front end, the jump and the map are its (`bd`, `s`, `ce`, `u`, `br`), but the
  race list, the showroom and the options are the port's, laid out with
  macroquad's widgets rather than positioned by hand the way the MIDlet does -
  its `cm`/`bi`/`db`/`af` widget tree and the per-widget coordinates are not
  reproduced;
* what counts as "passing" a race, which gates the award: the table gives each
  race an entry threshold and an award but not the finish condition, and the
  MIDlet's own test is a method this port has not read apart.  A podium finish
  is the port's stand-in;
* the sound effects: every `.amr` clip the MIDlet references is missing from
  the pack, so only the theme exists to play;
* the Bluetooth/Vserv/SMS code paths;

* class `ai`'s per-object orientation matrices are simplified to a yaw for
  high-detail scenery;
* the original's paid and online features, deliberately: see above.
