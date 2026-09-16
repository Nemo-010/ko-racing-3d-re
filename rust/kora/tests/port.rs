//! Headless checks for the ported asset pipeline.
//!
//! These run without a GPU: the resource archive, every format parser, the
//! track geometry builder and the rapier vehicle all work on plain data.

use std::path::PathBuf;

use macroquad::prelude::{vec3, Vec3};
use kora::physics::{CarControl, Tuning, World};
use kora::{format, pack, scene};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

#[test]
fn pack_index_is_complete() {
    let resources = pack::load(&assets());
    assert!(
        resources.len() >= 668,
        "expected the whole archive, got {}",
        resources.len()
    );
    for required in [
        "levels/1.map",
        "cars/rally.car",
        "tiles/s.tl",
        "tex/texpack.png",
        "fonts/font",
        "fonts/font.tab",
        "fonts/font.png",
    ] {
        assert!(resources.contains_key(required), "missing {required}");
    }
}

#[test]
fn every_map_parses() {
    let resources = pack::load(&assets());
    let mut maps: Vec<_> = resources
        .keys()
        .filter(|name| name.starts_with("levels/") && name.ends_with(".map"))
        .collect();
    maps.sort();
    assert_eq!(maps.len(), 40, "expected 40 track layouts");
    for name in maps {
        let map = format::Map::parse(&resources[name])
            .unwrap_or_else(|| panic!("{name} did not parse"));
        assert!(map.width > 0 && map.height > 0);
        assert_eq!(map.cells.len(), map.height as usize);
    }
}

#[test]
fn every_car_and_tile_parses() {
    let resources = pack::load(&assets());
    let cars = resources
        .keys()
        .filter(|name| name.starts_with("cars/") && name.ends_with(".car"))
        .count();
    assert_eq!(cars, 21);
    for (name, bytes) in &resources {
        if name.ends_with(".car") {
            let car = format::Car::parse(bytes).unwrap_or_else(|| panic!("{name}"));
            assert!(!car.model.is_empty());
        } else if name.ends_with(".tl") {
            format::Tile::parse(bytes).unwrap_or_else(|| panic!("{name}"));
        } else if name.ends_with(".ob") {
            format::ObjectDef::parse(bytes).unwrap_or_else(|| panic!("{name}"));
        } else if name.ends_with(".md") {
            format::MidDetail::parse(bytes).unwrap_or_else(|| panic!("{name}"));
        } else if name.ends_with(".hd") {
            format::HighDetail::parse(bytes).unwrap_or_else(|| panic!("{name}"));
        }
    }
}

#[test]
fn track_geometry_and_collision() {
    let dir = assets();
    let resources = pack::load(&dir);
    let track = scene::build(&dir, &resources, "1.map");

    assert!(!track.meshes.is_empty(), "no mesh batches");
    assert_eq!(track.meshes.len(), track.texture_paths.len());
    assert!(
        track.collision_indices.len() >= track.grid.path().len() * 2,
        "{} collision triangles for {} road cells",
        track.collision_indices.len(),
        track.grid.path().len()
    );
    assert!(!track.walls.is_empty(), "the track has no barriers");
    for mesh in &track.meshes {
        assert!(!mesh.vertices.is_empty());
        assert_eq!(mesh.indices.len() % 3, 0);
    }

    let car = format::Car::parse(&resources["cars/rally.car"]).unwrap();
    let geometry = scene::build_car(&resources, &car).expect("rally model");
    assert!(!geometry.vertices.is_empty());
    assert!(geometry.half_extents.x > 0.1 && geometry.half_extents.z > 0.5);
}

#[test]
fn every_track_builds_geometry() {
    let dir = assets();
    let resources = pack::load(&dir);
    let mut maps: Vec<String> = resources
        .keys()
        .filter(|name| name.starts_with("levels/") && name.ends_with(".map"))
        .map(|name| name.trim_start_matches("levels/").to_string())
        .collect();
    maps.sort();
    for name in maps {
        let track = scene::build(&dir, &resources, &name);
        assert!(!track.meshes.is_empty(), "{name}: no geometry");
        assert!(
            track.collision_indices.len() > 20,
            "{name}: only {} collision triangles",
            track.collision_indices.len()
        );
        for mesh in &track.meshes {
            assert!(mesh.vertices.len() <= 65_535, "{name}: too many vertices");
        }
    }
}

#[test]
fn car_settles_and_drives_on_the_track() {
    let dir = assets();
    let resources = pack::load(&dir);
    let track = scene::build(&dir, &resources, "1.map");
    let car = format::Car::parse(&resources["cars/rally.car"]).unwrap();
    let geometry = scene::build_car(&resources, &car).unwrap();

    let scene::Track {
        collision_vertices,
        collision_indices,
        walls,
        spawn,
        spawn_yaw,
        ..
    } = track;
    let mut world = World::new(collision_vertices, collision_indices, &walls);
    let player = world.add_car(spawn, spawn_yaw, geometry.half_extents, Tuning::default());

    let idle = [CarControl::default()];
    for _ in 0..240 {
        world.step(1.0 / 60.0, &idle);
    }
    let resting = world.position(player);
    assert!(
        resting.y > -10.0 && resting.y < 40.0,
        "car did not rest on the track: {resting:?}"
    );

    let drive = [CarControl {
        throttle: 1.0,
        ..Default::default()
    }];
    for _ in 0..120 {
        world.step(1.0 / 60.0, &drive);
    }
    let moved = (world.position(player) - resting).length();
    assert!(moved > 0.5, "car did not move under throttle ({moved:.2})");
}

/// Every shipped map must produce a connected road graph whose cells line up
/// with the tiles the scene builder places.
#[test]
fn grid_is_connected_for_every_track() {
    use kora::grid::Grid;
    let dir = assets();
    let resources = pack::load(&dir);
    let maps: Vec<String> = {
        let mut names: Vec<String> = resources
            .keys()
            .filter(|name| name.starts_with("levels/") && name.ends_with(".map"))
            .map(|name| name.trim_start_matches("levels/").to_string())
            .collect();
        names.sort();
        names
    };
    for name in maps {
        let track = scene::build(&dir, &resources, &name);
        let grid: Grid = track.grid;
        let cells = grid.path();
        assert!(!cells.is_empty(), "{name}: no road cells");
        assert!(grid.occupied(grid.start.0, grid.start.1), "{name}: start off-road");
        assert!(grid.occupied(grid.finish.0, grid.finish.1), "{name}: finish off-road");

        // Flood fill from the start; every road cell has to be reachable.
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![grid.start];
        seen.insert(grid.start);
        while let Some((x, y)) = stack.pop() {
            for (_, nx, ny) in grid.neighbours(x, y) {
                if seen.insert((nx, ny)) {
                    stack.push((nx, ny));
                }
            }
        }
        assert_eq!(
            seen.len(),
            cells.len(),
            "{name}: road graph is not connected ({} of {} cells)",
            seen.len(),
            cells.len()
        );

        // Every road cell must have at least one drivable side, and the start
        // must lead somewhere the race direction can follow.
        for &(x, y) in &cells {
            assert!(
                !grid.neighbours(x, y).is_empty(),
                "{name}: cell ({x},{y}) is a dead end"
            );
        }
        assert!(grid.race_dir().is_some(), "{name}: no race direction");

        // The starting grid has to sit on road cells, facing along the track.
        let slots = grid.grid_slots(6);
        assert!(!slots.is_empty(), "{name}: empty starting grid");
        for &(position, _) in &slots {
            let (sx, sy) = grid.cell_of(position);
            assert!(
                grid.occupied(sx, sy),
                "{name}: grid slot at ({sx},{sy}) is off-road"
            );
        }
    }
}

/// The AI target must always be a point on another drivable cell.
#[test]
fn ai_targets_stay_on_the_road() {
    let dir = assets();
    let resources = pack::load(&dir);
    let track = scene::build(&dir, &resources, "1.map");
    let grid = &track.grid;
    let mut checked = 0;
    for (x, y) in grid.path() {
        for (dir_index, _, _) in grid.neighbours(x, y) {
            let heading = kora::grid::dir_mq(dir_index);
            let position = grid.center(x, y);
            let target = grid
                .target(position, heading)
                .expect("target for a connected side");
            let (tx, ty) = grid.cell_of(target);
            assert!(
                grid.occupied(tx, ty),
                "target for cell ({x},{y}) side {dir_index} left the road"
            );
            checked += 1;
        }
    }
    assert!(checked > 30, "only checked {checked} sides");
}

/// A lap needs every checkpoint in order; sitting on the line is not a lap.
#[test]
fn laps_require_checkpoints_in_order() {
    use kora::race::Race;
    let dir = assets();
    let resources = pack::load(&dir);

    // A circuit with no checkpoints: leaving and returning to the finish line
    // is one lap, and idling on the line is not.
    let track = scene::build(&dir, &resources, "1.map");
    let grid = &track.grid;
    assert_eq!(grid.gates().len(), 1, "1.map should be a simple circuit");
    let line = grid.center(grid.finish.0, grid.finish.1);
    let away = line + vec3(400.0, 0.0, 0.0);
    let mut race = Race::new(grid, 2, line, 0.0);
    race.update(1.0, grid, line);
    assert_eq!(race.lap, 0, "an idle car on the line scored a lap");
    race.update(2.0, grid, away);
    race.update(3.0, grid, line);
    assert_eq!(race.lap, 1);
    race.update(4.0, grid, away);
    race.update(5.0, grid, line);
    assert_eq!(race.lap, 2);
    assert!(race.finished, "race with 2 laps did not finish");
    assert_eq!(race.lap_times.len(), 2);

    // A track with checkpoints must refuse to count a lap until they are done.
    let track = scene::build(&dir, &resources, "mc5.map");
    let grid = &track.grid;
    assert_eq!(grid.gates().len(), 3, "mc5.map should carry two checkpoints");
    let line = grid.center(grid.finish.0, grid.finish.1);
    let away = line + vec3(0.0, 0.0, 400.0);
    let mut race = Race::new(grid, 3, line, 0.0);
    race.update(1.0, grid, away);
    race.update(2.0, grid, line);
    assert_eq!(race.lap, 0, "a lap was counted without the checkpoints");
    for &gate in grid.gates()[1..].iter() {
        let centre = grid.center(gate.0, gate.1);
        race.update(3.0, grid, centre + vec3(200.0, 0.0, 0.0));
        race.update(4.0, grid, centre);
    }
    race.update(5.0, grid, away);
    race.update(6.0, grid, line);
    assert_eq!(race.lap, 1, "checkpoints in order did not complete a lap");
}

/// A grid of opponents, driven only by the AI, must get round the track.
#[test]
fn opponents_drive_the_track() {
    use kora::grid::Grid;
    use kora::physics::{CarControl, Tuning, World};
    use kora::race::Race;
    let dir = assets();
    let resources = pack::load(&dir);
    let track = scene::build(&dir, &resources, "1.map");
    let car = format::Car::parse(&resources["cars/rally.car"]).unwrap();
    let geometry = scene::build_car(&resources, &car).unwrap();

    let scene::Track {
        grid,
        surface,
        collision_vertices,
        collision_indices,
        walls,
        spawn,
        spawn_yaw,
        ..
    } = track;
    let grid: Grid = grid;

    let mut world = World::new(collision_vertices, collision_indices, &walls);
    for &(spot, yaw) in grid.grid_slots(4).iter() {
        world.add_car(spot, yaw, geometry.half_extents, Tuning::default());
    }
    let cars = world.cars.len();
    let _ = (spawn, spawn_yaw);

    let mut races: Vec<Race> = (0..cars).map(|i| Race::new(&grid, 3, world.position(i), 0.0)).collect();
    let mut travelled = vec![0.0f32; cars];
    let mut previous: Vec<Vec3> = (0..cars).map(|i| world.position(i)).collect();
    let mut drivers: Vec<kora::ai::AiDriver> =
        (0..cars).map(|_| kora::ai::AiDriver::new(1.0)).collect();

    // 1.map is roughly 200 units round and the AI averages ~7 units/s, so a
    // lap takes ~30 s; 90 s gives everyone room for two.
    let steps = 90 * 60;
    for step in 0..steps {
        let mut controls = vec![CarControl::default(); cars];
        for index in 0..cars {
            let (position, rotation) = world.pose(index);
            let heading = rotation * vec3(0.0, 0.0, -1.0);
            controls[index] = drivers[index].control(
                &grid,
                position,
                heading,
                world.speed(index),
                1.0 / 60.0,
            );
        }
        world.step(1.0 / 60.0, &controls);
        for index in 0..cars {
            let place = world.position(index);
            if let Some(height) = surface.height_at(place) {
                world.lift_to(index, height + geometry.half_extents.y + 0.02);
            }
            let place = world.position(index);
            assert!(place.y > -30.0, "car {index} fell off at step {step}");
            travelled[index] += (place - previous[index]).length();
            previous[index] = place;
            races[index].update(step as f64 / 60.0, &grid, place);
        }
    }

    for index in 0..cars {
        // A lap is ~200 units; a car that never moves must not pass this.
        assert!(
            travelled[index] > 250.0,
            "car {index} only covered {:.1} units in 90 s",
            travelled[index]
        );
        assert!(
            races[index].lap >= 1,
            "car {index} completed no lap of 1.map in 90 s"
        );
        assert!(
            world.position(index).y > -5.0,
            "car {index} ended up off the track"
        );
    }
    let quickest = races
        .iter()
        .filter_map(|race| race.best)
        .fold(f32::MAX, f32::min);
    assert!(
        quickest > 10.0 && quickest < 70.0,
        "implausible best lap of {quickest:.1} s"
    );
}

/// The AI has to stay on the road on every kind of track, not just the first.
#[test]
fn opponents_survive_other_tracks() {
    use kora::ai::AiDriver;
    use kora::physics::{CarControl, Tuning, World};
    let dir = assets();
    let resources = pack::load(&dir);
    let car = format::Car::parse(&resources["cars/rally.car"]).unwrap();
    let geometry = scene::build_car(&resources, &car).unwrap();

    // A long checkpoint circuit, a big open one and a twisty one.
    for map in ["mc5.map", "sp3.map", "19.map"] {
        let track = scene::build(&dir, &resources, map);
        let scene::Track {
            grid,
            surface,
            collision_vertices,
            collision_indices,
            walls,
            ..
        } = track;
        let mut world = World::new(collision_vertices, collision_indices, &walls);
        for &(spot, yaw) in grid.grid_slots(3).iter() {
            world.add_car(spot, yaw, geometry.half_extents, Tuning::default());
        }
        let cars = world.cars.len();
        let mut drivers: Vec<AiDriver> =
            (0..cars).map(|_| AiDriver::new(0.95)).collect();

        for step in 0..(40 * 60) {
            let mut controls = vec![CarControl::default(); cars];
            for index in 0..cars {
                let (position, rotation) = world.pose(index);
                let heading = rotation * vec3(0.0, 0.0, -1.0);
                controls[index] = drivers[index].control(
                    &grid,
                    position,
                    heading,
                    world.speed(index),
                    1.0 / 60.0,
                );
            }
            world.step(1.0 / 60.0, &controls);
            for index in 0..cars {
                let place = world.position(index);
                if let Some(height) = surface.height_at(place) {
                    world.lift_to(index, height + geometry.half_extents.y + 0.02);
                }
                let place = world.position(index);
                assert!(
                    place.y > -20.0,
                    "{map}: car {index} fell off at step {step}"
                );
                let (cx, cy) = grid.cell_of(vec3(place.x, 0.0, place.z));
                assert!(
                    grid.occupied(cx, cy),
                    "{map}: car {index} left the road at step {step} ({place:?})"
                );
            }
        }
    }
}

/// Both career tables must parse, and every race record must decode to values
/// the game could actually use.
#[test]
fn campaign_tables_decode() {
    use kora::campaign;
    use kora::format::RaceConfig;
    let resources = pack::load(&assets());
    let tables = campaign::load(&resources);
    assert_eq!(tables.len(), 2, "expected campaign.000 and deluxe.000");

    let expected = [("campaign/campaign", 18usize, 34usize), ("campaign/deluxe", 13, 13)];
    for ((base, table), (want_base, levels, records)) in tables.iter().zip(expected) {
        assert_eq!(base, want_base);
        assert_eq!(table.levels.len(), levels, "{base}: levels");
        assert_eq!(table.records.len(), records, "{base}: race records");
        let blob = &resources[&format!("{base}.001")];

        for record in &table.records {
            let offset = record.values[2].max(0) as usize;
            let config = RaceConfig::parse(blob, offset, record.mode)
                .unwrap_or_else(|| panic!("{base}: mode {} at {offset} did not decode", record.mode));
            assert!(record.level < table.levels.len() as u8, "{base}: bad level");
            assert!((1..=9).contains(&config.laps), "{base}: laps {}", config.laps);
            assert!(config.theme <= 4, "{base}: theme {}", config.theme);
            if config.is_race() {
                assert!(
                    (1..=7).contains(&config.opponents),
                    "{base}: opponents {} in mode {}",
                    config.opponents,
                    config.mode
                );
            } else {
                let limit = config.time_limit.expect("time trial needs a clock");
                assert!(limit > 0 && limit < 2_000_000, "{base}: clock {limit}");
            }
            // The player car is either a real car index or one of the deluxe
            // time-attack markers (>= 50).
            assert!(
                config.car < 8 || config.car >= 50,
                "{base}: car index {}",
                config.car
            );
        }
    }

    // The career table's level names and maps should line up with the pack.
    let (_, career) = &tables[0];
    for level in &career.levels {
        assert!(
            resources.contains_key(&format!("levels/{}", level.map)),
            "level {} names a missing map {}",
            level.name,
            level.map
        );
    }
}

/// The lookup has to agree with the tables, and fall back for quick-race maps.
#[test]
fn campaign_lookup_picks_the_right_race() {
    use kora::campaign;
    let resources = pack::load(&assets());

    let cases = [
        ("ma1.map", Some((0u8, 2u32, 3u32, 4u8))),   // career race, 3 opponents
        ("mc2.map", Some((0, 4, 3, 1))),
        ("sp1.map", Some((3, 3, 3, 4))),
        ("mc5.map", Some((1, 1, 3, 1))),
        ("sp3.map", Some((2, 1, 0, 0))),             // solo time trial
        ("1.map", None),                             // not in a campaign
        ("19.map", None),
    ];
    for (map, want) in cases {
        match (campaign::race_for(&resources, map), want) {
            (None, None) => {}
            (Some(setup), Some((mode, laps, opponents, theme))) => {
                assert_eq!(setup.config.mode, mode, "{map}: mode");
                assert_eq!(setup.config.laps, laps, "{map}: laps");
                assert_eq!(setup.config.opponents, opponents, "{map}: opponents");
                assert_eq!(setup.config.theme, theme, "{map}: theme");
            }
            (got, want) => panic!(
                "{map}: expected {want:?}, got {:?}",
                got.map(|s| (s.config.mode, s.config.laps, s.config.opponents, s.config.theme))
            ),
        }
    }
}

/// The campaign theme selects a tile variant; building either variant of a
/// track must still produce a complete road.
#[test]
fn campaign_themes_still_build() {
    let dir = assets();
    let resources = pack::load(&dir);
    for map in ["ma1.map", "sp1.map", "mc5.map"] {
        let plain = scene::build_themed(&dir, &resources, map, 0);
        let themed = scene::build_themed(&dir, &resources, map, 3);
        assert!(!themed.meshes.is_empty(), "{map}: theme 3 produced no geometry");
        assert_eq!(
            themed.collision_indices.len(),
            plain.collision_indices.len(),
            "{map}: the collider must not depend on the theme"
        );
        // Theme 3 drops detail, never adds it.
        let count = |track: &scene::Track| -> usize {
            track.meshes.iter().map(|mesh| mesh.vertices.len()).sum()
        };
        assert!(
            count(&themed) <= count(&plain),
            "{map}: theme 3 added geometry"
        );
    }
}

/// The collider has to come from each tile's collision mesh, so tracks with
/// bridges and ramps stop being flat.
#[test]
fn collision_meshes_give_tracks_elevation() {
    let dir = assets();
    let resources = pack::load(&dir);

    // 1.map uses h1.tl, a ramp whose collision mesh drops 4.2 units, and
    // ma1.map uses vl.tl, a platform raised 0.7 above the road plane.
    let hills = scene::build(&dir, &resources, "1.map");
    let span = |track: &scene::Track| {
        track
            .collision_vertices
            .iter()
            .map(|v| v.y)
            .fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)))
    };
    let (low, high) = span(&hills);
    assert!(low <= -4.1, "1.map should drop to about -4.2, got {low:.2}");
    assert!(high.abs() < 0.1, "1.map should not rise, got {high:.2}");

    // Values straight from the collision meshes, cross-checked against the
    // Python decoder: a ramp mid-point, a kerb, and a plain road tile.
    let height = |x: i32, y: i32| hills.surface.height_at(hills.grid.center(x, y));
    assert!((height(4, 7).unwrap() + 2.1).abs() < 0.01, "ramp midpoint");
    assert!((height(2, 2).unwrap() + 0.70).abs() < 0.01, "kerb");
    assert!(height(2, 6).unwrap().abs() < 0.01, "plain road tile is flat");
    assert_eq!(height(0, 0), None, "off-track cells have no surface");

    let raised = scene::build(&dir, &resources, "ma1.map");
    let (_, top) = span(&raised);
    assert!(top >= 0.6, "ma1.map should be raised, got {top:.2}");
    assert!(raised.collision_vertices.iter().any(|v| v.y > 0.5));

    // The collider must be the height function the runtime queries: every cell
    // whose tile ships a mesh has a grid vertex at its centre, and that vertex
    // has to sit at exactly the height `height_at` reports.
    for map in ["1.map", "ma1.map", "sp3.map", "mc5.map"] {
        let track = scene::build(&dir, &resources, map);
        let mut checked = 0;
        for (x, y) in track.grid.path() {
            let centre = track.grid.center(x, y);
            let Some(height) = track.surface.height_at(centre) else {
                continue;
            };
            if height.abs() < 1e-6 {
                continue;
            }
            let found = track.collision_vertices.iter().any(|v| {
                (v.x - centre.x).abs() < 1e-3
                    && (v.z - centre.z).abs() < 1e-3
                    && (v.y - height).abs() < 1e-3
            });
            assert!(
                found,
                "{map}: cell ({x},{y}) is at {height:.2} but the collider has no vertex there"
            );
            checked += 1;
        }
        assert!(checked > 0, "{map}: no elevated cell was cross-checked");
    }
}

/// Cars have to be able to drive the tracks now that they have elevation: the
/// MIDlet lifts its car onto the surface, and the port has to do the same or a
/// step between two tiles traps it.
#[test]
fn cars_climb_the_track_elevation() {
    use kora::ai::AiDriver;
    use kora::physics::{CarControl, Tuning, World};
    let dir = assets();
    let resources = pack::load(&dir);
    let car = format::Car::parse(&resources["cars/rally.car"]).unwrap();
    let geometry = scene::build_car(&resources, &car).unwrap();

    // 1.map has a 4.2-unit ramp; without the lift the cars end up stuck in it.
    let track = scene::build(&dir, &resources, "1.map");
    let scene::Track {
        grid,
        surface,
        collision_vertices,
        collision_indices,
        walls,
        ..
    } = track;
    let mut world = World::new(collision_vertices, collision_indices, &walls);
    for &(spot, yaw) in grid.grid_slots(2).iter() {
        world.add_car(spot, yaw, geometry.half_extents, Tuning::default());
    }
    let cars = world.cars.len();
    let ride = geometry.half_extents.y + 0.02;
    let mut drivers: Vec<AiDriver> = (0..cars).map(|_| AiDriver::new(1.0)).collect();
    let mut lowest = f32::MAX;

    for _ in 0..(60 * 60) {
        let mut controls = vec![CarControl::default(); cars];
        for index in 0..cars {
            let (position, rotation) = world.pose(index);
            let heading = rotation * vec3(0.0, 0.0, -1.0);
            controls[index] =
                drivers[index].control(&grid, position, heading, world.speed(index), 1.0 / 60.0);
        }
        world.step(1.0 / 60.0, &controls);
        for index in 0..cars {
            let position = world.position(index);
            if let Some(height) = surface.height_at(position) {
                world.lift_to(index, height + ride);
            }
            lowest = lowest.min(world.position(index).y);
        }
    }
    assert!(
        lowest < -1.0,
        "the cars never went down the ramp (lowest {lowest:.2})"
    );
}

/// The two things the game keeps per race: the award, paid once, and the best
/// time, kept whenever it improves.  There are no medals.
#[test]
fn points_and_best_times_persist() {
    use kora::progress::Progress;
    let mut progress = Progress::default();
    assert!(progress.open(1), "the first race is open at zero points");
    assert!(!progress.open(9), "a 9-point race is not");
    assert_eq!(progress.best_time("career:0:0"), None, "no record yet");

    // First finish: the record is set and the award is paid.
    let first = progress.record("career:0:0", 71.5, 1);
    assert!(first.improved && first.previous_best.is_none());
    assert_eq!(first.gained, 1, "the record's own award, once");
    assert_eq!(progress.points, 1);
    assert_eq!(progress.best_time("career:0:0"), Some(71.5));

    // A slower run changes nothing.
    let slower = progress.record("career:0:0", 80.0, 1);
    assert!(!slower.improved, "a slower run is not a record");
    assert_eq!(slower.gained, 0, "and a race passed twice pays once");
    assert_eq!(progress.best_time("career:0:0"), Some(71.5));
    assert_eq!(progress.points, 1);

    // A quicker run sets the record, and still pays nothing.
    let quicker = progress.record("career:0:0", 66.25, 1);
    assert!(quicker.improved);
    assert_eq!(quicker.previous_best, Some(71.5));
    assert_eq!(progress.best_time("career:0:0"), Some(66.25));
    assert_eq!(progress.points, 1, "the award is paid once, not per record");

    // Another race has its own record and its own award.
    let other = progress.record("career:1:0", 40.0, 2);
    assert_eq!(other.gained, 2);
    assert_eq!(progress.points, 3);
    assert_eq!(progress.best_time("career:1:0"), Some(40.0));

    let path = std::env::temp_dir().join("kora-progress-test.txt");
    progress.car = 2;
    progress.save(&path);
    let mut loaded = Progress::load(&path);
    assert_eq!(loaded.points, 3);
    assert_eq!(loaded.car, 2);
    assert_eq!(loaded.best_time("career:0:0"), Some(66.25));
    assert_eq!(loaded.best_time("career:1:0"), Some(40.0));
    assert_eq!(
        loaded.record("career:0:0", 90.0, 1).gained,
        0,
        "a race already passed stays passed across a save"
    );
    assert!(loaded.open(4) && !loaded.open(5));
    let _ = std::fs::remove_file(&path);
}

/// The career list comes from the two `.000` tables, and the quick-race list
/// covers every track in the pack.
#[test]
fn career_and_quick_lists_are_built() {
    use kora::campaign;
    use kora::progress;
    let resources = pack::load(&assets());

    let events = campaign::events(&resources);
    assert_eq!(events.len(), 47, "34 career records plus 13 deluxe");
    assert_eq!(
        events.iter().filter(|e| e.table == "campaign/campaign").count(),
        34
    );
    assert_eq!(events[0].name, "TIMBERTON");
    assert_eq!(events[0].map, "ma1.map");
    assert_eq!(events[0].mode, 0);
    assert!(
        events
            .iter()
            .all(|e| e.laps >= 1 && e.award >= 0 && e.key.contains(':')),
        "every event needs laps, a non-negative award and a save key"
    );
    assert!(
        events
            .iter()
            .all(|e| e.unlocks.is_some() || e.award > 0),
        "an event either pays points or unlocks something"
    );
    // Within a level, races come before time trials.
    let mut time_trials = std::collections::HashSet::new();
    for event in &events {
        let level = (event.table.clone(), event.level_index);
        if kora::format::RaceConfig::RACE_MODES.contains(&event.mode) {
            assert!(
                !time_trials.contains(&level),
                "{} lists a race after a time trial",
                event.name
            );
        } else {
            time_trials.insert(level);
        }
    }

    let quick = campaign::quick_events(&resources);
    assert_eq!(quick.len(), 40, "every shipped track");
    assert!(quick.iter().all(|e| e.threshold == 0 && e.laps >= 1));
    assert!(quick.iter().any(|e| e.map == "1.map"));

    let cars = progress::car_infos(&resources);
    assert_eq!(cars.len(), 8, "ba.a lists eight cars");
    assert!(cars.iter().all(|car| !car.name.is_empty()));
    assert!(
        cars.iter().all(|car| car.stats.iter().all(|value| *value <= 6)),
        "stat bars draw up to six segments"
    );
    assert!(cars.iter().any(|car| car.file == "rally.car"));
}

/// Run a whole race to the flag with every car on AI, then score it the way
/// the results screen does: position, time, points and a saved record.
#[test]
fn a_race_runs_to_the_flag_and_scores() {
    use kora::ai::AiDriver;
    use kora::physics::{CarControl, Tuning, World};
    use kora::progress::Progress;
    use kora::race::Race;

    let dir = assets();
    let resources = pack::load(&dir);
    let car = format::Car::parse(&resources["cars/rally.car"]).unwrap();
    let geometry = scene::build_car(&resources, &car).unwrap();

    let track = scene::build(&dir, &resources, "1.map");
    let scene::Track {
        grid,
        surface,
        collision_vertices,
        collision_indices,
        walls,
        ..
    } = track;
    let mut world = World::new(collision_vertices, collision_indices, &walls);
    for &(spot, yaw) in grid.grid_slots(4).iter() {
        world.add_car(spot, yaw, geometry.half_extents, Tuning::default());
    }
    let cars = world.cars.len();
    let laps = 2;
    let mut races: Vec<Race> = (0..cars).map(|i| Race::new(&grid, laps, world.position(i), 0.0)).collect();
    let mut drivers: Vec<AiDriver> = (0..cars).map(|_| AiDriver::new(1.0)).collect();
    let ride = geometry.half_extents.y + 0.02;
    let mut finish_order: Vec<usize> = Vec::new();

    for step in 0..(150 * 60) {
        let mut controls = vec![CarControl::default(); cars];
        for index in 0..cars {
            let (position, rotation) = world.pose(index);
            let heading = rotation * vec3(0.0, 0.0, -1.0);
            controls[index] =
                drivers[index].control(&grid, position, heading, world.speed(index), 1.0 / 60.0);
        }
        world.step(1.0 / 60.0, &controls);
        for index in 0..cars {
            let place = world.position(index);
            if let Some(height) = surface.height_at(place) {
                world.lift_to(index, height + ride);
            }
        }
        let now = step as f64 / 60.0;
        for index in 0..cars {
            let before = races[index].finished;
            races[index].update(now, &grid, world.position(index));
            if races[index].finished && !before {
                finish_order.push(index);
            }
        }
        if finish_order.len() == cars {
            break;
        }
    }

    assert_eq!(
        finish_order.len(),
        cars,
        "only {} of {cars} cars finished a {laps}-lap race of 1.map",
        finish_order.len()
    );

    // Score the winner exactly as the results screen does.
    let winner = finish_order[0];
    let place = finish_order.iter().position(|&car| car == winner).unwrap();
    let time = races[winner].finish_time.unwrap();
    let mut progress = Progress::default();
    let result = progress.record("quick:1.map:0", time, 1);
    assert_eq!(place, 0);
    assert_eq!(result.gained, 1, "the record's own award, once");
    assert_eq!(progress.points, 1);
    assert!(result.improved, "the first finish is a record");
    assert_eq!(progress.best_time("quick:1.map:0"), Some(time));
    let race = &races[winner];
    assert!(race.best.is_some() && race.best.unwrap() > 5.0, "laps are timed");
    assert!(race.finish_time.is_some());
    // And the order is a permutation of the grid.
    assert_eq!(finish_order.len(), cars);
    let mut sorted = finish_order.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), cars);
}

/// The bundled face has to parse and cover everything the interface draws.
/// This is the only part of text handling that can be checked without a
/// window: macroquad rasterises through a live graphics context.
#[test]
fn the_bundled_font_covers_the_interface() {
    let bytes = include_bytes!("../fonts/ContrailOne-Regular.ttf");
    let font = fontdue::Font::from_bytes(&bytes[..], fontdue::FontSettings::default())
        .expect("the bundled Contrail One should parse");

    // Every character any string in the UI can contain: the leaderboard and
    // results rows, the stat labels, and the formatted numbers and times.
    // Map files and car files are lower case ("ma1.map", "rally.car").
    let used = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 /:.-+()";
    for ch in used.chars() {
        assert_ne!(
            font.lookup_glyph_index(ch),
            0,
            "the bundled font has no glyph for {ch:?}"
        );
    }

    // And the strings the screens actually build are all covered.
    let samples = [
        "K.O. RACING 3D",
        "CAREER POINTS 12",
        "TIMBERTON",
        "ma1.map",
        "0:12.34",
        "+3  (total 9)",
        "POS 1/4",
        "NEED 27",
        "SPEED",
        "GOLD",
        "ARROWS SELECT   ENTER CONFIRM   ESC BACK",
    ];
    for sample in samples {
        for ch in sample.chars() {
            assert!(
                used.contains(ch),
                "{sample:?} uses {ch:?}, which the font test does not cover"
            );
        }
    }

    // A rendered glyph is a real bitmap, not an empty box.
    let (metrics, bitmap) = font.rasterize('A', 32.0);
    assert!(metrics.width > 4 && metrics.height > 4, "A at 32px is {metrics:?}");
    assert!(bitmap.iter().any(|byte| *byte > 0), "A rasterised blank");
}

/// The deluxe levels are a second campaign, opened by career points alone.
/// The port has no entitlement flag, purchase or server behind them, unlike
/// the original, which gates them behind an SMS unlock.
#[test]
fn the_deluxe_campaign_opens_on_points_alone() {
    use kora::campaign;
    use kora::progress::Progress;
    let resources = pack::load(&assets());
    let all = campaign::events(&resources);

    let deluxe: Vec<_> = all.iter().filter(|e| e.table.ends_with("deluxe")).collect();
    let career: Vec<_> = all.iter().filter(|e| e.table.ends_with("campaign")).collect();
    assert_eq!(deluxe.len(), 13, "the deluxe table's levels");
    assert_eq!(career.len(), 34);

    let career_maps: std::collections::HashSet<_> =
        career.iter().map(|e| e.map.clone()).collect();
    let deluxe_maps: std::collections::HashSet<_> =
        deluxe.iter().map(|e| e.map.clone()).collect();
    assert!(deluxe_maps.contains("3.map") && deluxe_maps.contains("13.map"));
    assert!(
        !deluxe_maps.is_subset(&career_maps),
        "the deluxe table has to add tracks, not repeat the career"
    );

    // A brand new save can already enter the first deluxe event; the rest open
    // as points come in.  Nothing else is consulted.
    let mut progress = Progress::default();
    let first = deluxe.iter().map(|e| e.threshold).min().unwrap();
    let last = deluxe.iter().map(|e| e.threshold).max().unwrap();
    assert!(first <= 1, "the first deluxe event should be open at once");
    assert!(progress.open(first));
    assert!(!progress.open(last), "the last one needs points");
    progress.points = last.max(1) as u32;
    assert!(progress.open(last), "and points alone open it");
}

/// The four `.car` values reach the handling, each moving its own part.
///
/// Their names are the game's own - `aq.a(127 + i)` draws them and `ui/ui.txt`
/// gives those ids - so they are pinned here: a silent change would quietly
/// relabel the setup screen.
#[test]
fn car_stats_change_the_handling() {
    use kora::labels;
    use kora::physics::Tuning;

    assert_eq!(
        labels::STAT_KEYS.map(labels::get),
        ["SPEED", "ACCELERATION", "BRAKING", "HANDLING"]
    );

    let base = [3, 5, 5, 1];
    let stock = Tuning::from_stats(base);
    let raise = |stat: usize, value: u8| {
        let mut stats = base;
        stats[stat] = value;
        Tuning::from_stats(stats)
    };

    // SPEED buys top end by lowering drag, and touches nothing else.
    let quick = raise(0, 6);
    assert!(quick.linear_damping < stock.linear_damping);
    assert_eq!(quick.engine_force, stock.engine_force);
    assert_eq!(quick.brake, stock.brake);
    assert_eq!(quick.steer, stock.steer);

    // ACCELERATION buys engine force, and nothing else.
    let brisk = raise(1, 6);
    assert!(brisk.engine_force > stock.engine_force);
    assert_eq!(brisk.linear_damping, stock.linear_damping);
    assert_eq!(brisk.brake, stock.brake);

    // BRAKING buys brake force, and nothing else.
    let stops = raise(2, 6);
    assert!(stops.brake > stock.brake);
    assert!(stops.brake > raise(2, 1).brake);
    assert_eq!(stops.engine_force, stock.engine_force);
    assert_eq!(stops.friction, stock.friction);

    // HANDLING buys steering angle and the grip to use it.
    let nimble = raise(3, 6);
    assert!(nimble.steer > stock.steer);
    assert!(nimble.friction > stock.friction);
    assert_eq!(nimble.brake, stock.brake);
    assert_eq!(nimble.engine_force, stock.engine_force);

    // The default is the first car `ba.a` lists, sitting on the constants the
    // port was calibrated with before the values were wired up at all.
    let default = Tuning::default();
    assert_eq!(default.engine_force, stock.engine_force);
    assert_eq!(default.linear_damping, stock.linear_damping);
    assert_eq!(default.steer, stock.steer);
    assert_eq!(default.friction, stock.friction);
    assert_eq!(default.brake, stock.brake);
}

/// The seven race modes carry the game's own names, and the three that hold a
/// clock rather than a starting grid are the non-circuit ones.
#[test]
fn race_modes_are_named_by_the_game() {
    use kora::format::RaceConfig;
    use kora::labels;

    assert_eq!(
        labels::MODE_KEYS.map(labels::get),
        [
            "CIRCUIT",
            "RACE",
            "TIME CHASE",
            "SURVIVAL",
            "HEAD TO HEAD",
            "SLIDESHOW",
            "SPECIAL"
        ]
    );
    for (mode, key) in labels::MODE_KEYS.iter().enumerate() {
        assert_eq!(labels::mode_name(mode as u8), labels::get(key));
    }
    // Out of range falls back rather than panicking.
    assert_eq!(labels::mode_name(200), "SPECIAL");

    // A time trial is exactly a mode that stores a clock, and those are TIME
    // CHASE, SLIDESHOW and SPECIAL.
    for mode in 0..7u8 {
        let is_trial = [2u8, 5, 6].contains(&mode);
        assert_eq!(
            !RaceConfig::RACE_MODES.contains(&mode),
            is_trial,
            "mode {mode} ({}) disagrees about being a time trial",
            labels::mode_name(mode)
        );
        if is_trial {
            assert!(matches!(
                labels::mode_name(mode),
                "TIME CHASE" | "SLIDESHOW" | "SPECIAL"
            ));
        }
    }
}

/// A car with better values really does go quicker, and still drives a track.
#[test]
fn a_better_car_is_quicker() {
    use kora::physics::{CarControl, Tuning, World};
    let dir = assets();
    let resources = pack::load(&dir);
    let car = format::Car::parse(&resources["cars/rally.car"]).unwrap();
    let geometry = scene::build_car(&resources, &car).unwrap();

    let run = |tuning: Tuning| -> f32 {
        let track = scene::build(&dir, &resources, "1.map");
        let scene::Track {
            collision_vertices,
            collision_indices,
            walls,
            spawn,
            spawn_yaw,
            ..
        } = track;
        let mut world = World::new(collision_vertices, collision_indices, &walls);
        world.add_car(spawn, spawn_yaw, geometry.half_extents, tuning);
        let flat_out = [CarControl {
            throttle: 1.0,
            ..Default::default()
        }];
        // Long enough to build speed, short enough not to reach the first
        // corner and spoil the comparison with a barrier.
        for _ in 0..90 {
            world.step(1.0 / 60.0, &flat_out);
        }
        world.speed(0)
    };

    let rally = run(Tuning::from_stats([3, 5, 5, 1]));
    let best = run(Tuning::from_stats([6, 6, 6, 6]));
    assert!(
        best > rally,
        "the best car in the list should be quicker: {best:.2} against {rally:.2}"
    );
}

/// The label table is data, so it can be checked as data: every line parses,
/// keys are unique, every key the code asks for resolves, and the source
/// column only ever holds one of the game's ids or a dash.
#[test]
fn the_label_table_is_well_formed() {
    use kora::labels;
    use std::collections::HashSet;

    let source = include_str!("../labels.tsv");
    let mut keys = HashSet::new();
    let mut from_the_game = 0;
    for (number, line) in source.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        assert_eq!(columns.len(), 3, "line {}: expected three columns", number + 1);
        let (key, text, origin) = (columns[0], columns[1], columns[2]);
        assert!(!key.is_empty(), "line {}: empty key", number + 1);
        assert!(!text.is_empty(), "line {}: empty text for {key}", number + 1);
        assert!(
            origin == "-" || origin.parse::<u16>().is_ok(),
            "line {}: source {origin:?} is neither a dash nor an id",
            number + 1
        );
        if origin != "-" {
            from_the_game += 1;
        }
        assert!(keys.insert(key), "line {}: {key} appears twice", number + 1);
    }
    assert!(keys.len() > 40, "the table looks short: {}", keys.len());
    assert!(
        from_the_game >= 20,
        "only {from_the_game} labels are the game's own; the rest are guesses"
    );

    // A key that is in the file must resolve to something other than itself,
    // which is how a missing entry falls back.
    for key in &keys {
        assert_ne!(labels::get(key), *key, "{key} did not resolve");
    }

    // And every key the code asks for has to be in the file.  `get` returns
    // the key on a miss, so a typo would otherwise reach the screen.
    let required = [
        labels::STAT_KEYS.as_slice(),
        labels::MODE_KEYS.as_slice(),
        &[
            "kora",
            "port",
            "career_points",
            "events_count",
            "need_points",
            "in_use",
            "car_selected",
            "pause_title",
            "pause_resume",
            "pause_restart",
            "pause_quit",
            "col_mode",
            "col_laps",
            "col_cpu",
            "col_best",
            "results_position",
            "results_laps",
            "results_total",
            "results_best",
            "results_record",
            "results_new_record",
            "results_none",
            "results_points",
            "results_continue",
            "hud_lap",
            "hud_pos",
            "hud_time",
            "hud_lap_best",
            "no_time",
            "race_needs",
            "load_failed",
            "keys_menu",
            "keys_events",
            "keys_cars",
            "keys_race",
            "points",
        ],
    ];
    for (index, group) in required.iter().enumerate() {
        for key in *group {
            assert!(keys.contains(key), "group {index}: {key} is missing from the table");
        }
    }

    // Placeholders: `format` fills them in order and leaves nothing behind.
    assert_eq!(labels::format("hud_lap", &["1", "3"]), "LAP 1/3");
    assert_eq!(labels::format("hud_pos", &["2", "4"]), "POS 2/4");
    assert_eq!(labels::format("results_points", &["3", "12"]), "+3  (total 12)");
}

/// A race record's third value is signed, and a negative one is an unlock
/// group rather than an award: `u.n()` negates it and sets that group's
/// unlocked flag.  The port must not read one as the other.
#[test]
fn bonus_races_unlock_rather_than_award() {
    use kora::campaign;
    use kora::progress::Progress;
    let resources = pack::load(&assets());
    let events = campaign::events(&resources);

    let unlockers: Vec<_> = events.iter().filter(|e| e.unlocks.is_some()).collect();
    let awarders: Vec<_> = events.iter().filter(|e| e.unlocks.is_none()).collect();
    assert_eq!(unlockers.len(), 7, "the bonus races in campaign.000");
    assert_eq!(awarders.len(), 40, "34 campaign plus 13 deluxe, less the seven");

    // Every award is positive and every unlock group is 1..=7.
    assert!(awarders.iter().all(|e| e.award > 0), "an award must be a gain");
    let mut groups: Vec<u8> = unlockers.iter().filter_map(|e| e.unlocks).collect();
    groups.sort_unstable();
    assert_eq!(groups, vec![1, 2, 3, 4, 5, 6, 7]);
    assert!(
        unlockers.iter().all(|e| e.award == 0),
        "a bonus race must not also pay points"
    );

    // Finishing one of them keeps a time but pays nothing.
    let bonus = unlockers[0];
    let mut progress = Progress::default();
    let result = progress.record(&bonus.key, 55.0, bonus.award);
    assert_eq!(result.gained, 0, "a bonus race pays no points");
    assert_eq!(progress.points, 0);
    assert!(result.improved, "but it still sets a time");

    // Whereas an ordinary race pays the record's own value, once.
    let ordinary = awarders[0];
    let first = progress.record(&ordinary.key, 60.0, ordinary.award);
    assert_eq!(first.gained, ordinary.award as u32);
    assert_eq!(progress.points, ordinary.award as u32);
    let again = progress.record(&ordinary.key, 50.0, ordinary.award);
    assert_eq!(again.gained, 0, "passing the same race twice pays once");
    assert!(again.improved, "though the quicker run is kept");
}

/// The showroom loads every car up front, so all eight have to build, and each
/// has to be centred on its own origin or the turntable would spin it off the
/// edge of the view.
#[test]
fn every_car_builds_for_the_showroom() {
    use kora::progress;
    let resources = pack::load(&assets());
    let cars = progress::car_infos(&resources);
    assert_eq!(cars.len(), 8, "ba.a lists eight");

    for car in &cars {
        let definition = format::Car::parse(&resources[&format!("cars/{}", car.file)])
            .unwrap_or_else(|| panic!("{} did not read", car.name));
        let geometry = scene::build_car(&resources, &definition)
            .unwrap_or_else(|| panic!("{} did not build", car.name));
        assert!(!geometry.vertices.is_empty(), "{} is empty", car.name);
        assert_eq!(geometry.indices.len() % 3, 0);
        assert!(
            geometry.half_extents.min_element() > 0.05,
            "{} is flat, so it would be invisible on the turntable",
            car.name
        );

        // Centred on its own origin: the camera looks at (0, 0.15, 0).
        let low = geometry
            .vertices
            .iter()
            .map(|v| v.position.y)
            .fold(f32::MAX, f32::min);
        let high = geometry
            .vertices
            .iter()
            .map(|v| v.position.y)
            .fold(f32::MIN, f32::max);
        assert!(low < -0.05 && high > 0.05, "{} is not centred: {low}..{high}", car.name);
    }
}

/// The `.bck` backgrounds: all five parse, and each names an image that is
/// actually in the pack once the reader's own `.jpg`-then-`.png` rule is
/// applied.  A theme byte of 0..4 indexes the list, so a missing one would
/// leave a track with no sky.
#[test]
fn the_five_backgrounds_resolve() {
    use kora::format::Background;
    use kora::sky;
    let resources = pack::load(&assets());

    assert_eq!(
        sky::THEMES,
        ["clear", "rain", "snow", "desert", "sunset"],
        "al.a lists them in this order"
    );

    for theme in 0..5u8 {
        let name = sky::THEMES[theme as usize];
        let bytes = resources
            .get(&format!("back/{name}.bck"))
            .unwrap_or_else(|| panic!("{name}.bck is missing"));
        let background = Background::parse(bytes).unwrap_or_else(|| panic!("{name}.bck"));
        assert_eq!(background.colours.len(), 4);
        assert!(background.detail <= 4, "{name}: detail {}", background.detail);
        assert!(background.scale_a > 0.0 && background.scale_b > 0.0);

        // The reader tries the name with .jpg first and falls back to .png.
        let stem = background
            .texture
            .rsplit_once('.')
            .map_or(background.texture.as_str(), |(stem, _)| stem);
        let found = format!("images/{stem}.jpg");
        let fallback = format!("images/{stem}.png");
        assert!(
            resources.contains_key(&found) || resources.contains_key(&fallback),
            "{name} names {} and neither {found} nor {fallback} is in the pack",
            background.texture
        );
    }

    // Spot values, straight from clear.bck.
    let clear = Background::parse(&resources["back/clear.bck"]).unwrap();
    assert_eq!(clear.texture, "bc.png");
    assert_eq!(clear.scale_a, 1.0);
    assert_eq!(clear.scale_b, 1.0);
}

/// The minimap has to put every car inside its own panel, on the right cell:
/// world Z maps to the map's Y the other way round, which is the kind of sign
/// error that looks fine until you notice the markers are in a mirror.
#[test]
fn the_minimap_maps_the_track_the_right_way_up() {
    use kora::hud;
    let dir = assets();
    let resources = pack::load(&dir);
    let track = scene::build(&dir, &resources, "1.map");
    let grid = &track.grid;

    let (cell, left, bottom) = hud::minimap_layout(grid, 720.0);
    let width = grid.width as f32 * cell;
    let height = grid.height as f32 * cell;
    let top = bottom - height - 24.0;

    for (x, y) in grid.path() {
        let point = hud::minimap_point(grid, grid.center(x, y), cell, left, bottom);
        assert!(
            point.x >= left - 0.01 && point.x <= left + width + 0.01,
            "cell ({x},{y}) is off the map horizontally: {}",
            point.x
        );
        assert!(
            point.y >= top - 0.01 && point.y <= top + height + 0.01,
            "cell ({x},{y}) is off the map vertically: {}",
            point.y
        );
    }

    // And the mapping is not mirrored.  A cell further along +X is further
    // right, and one further along +Y - which is -Z in world space - is
    // further down, so cell row 0 is the top row of the map.
    let a = hud::minimap_point(grid, grid.center(1, 1), cell, left, bottom);
    let b = hud::minimap_point(grid, grid.center(2, 1), cell, left, bottom);
    let c = hud::minimap_point(grid, grid.center(1, 2), cell, left, bottom);
    assert!(b.x > a.x && (b.y - a.y).abs() < 0.01);
    assert!(c.y > a.y && (c.x - a.x).abs() < 0.01);
}

/// The soundtrack is rendered from the game's own MIDI, so the render can be
/// checked against what the file says it holds: 1273 notes over about 84
/// seconds, turned into a WAV of the right length that is not silence.
#[test]
fn the_theme_renders_from_the_games_midi() {
    use kora::music;
    let path = assets().join("sounds/theme.mid");
    let Ok(midi) = std::fs::read(&path) else {
        eprintln!("skipping: {} is not there (run ./setup.sh)", path.display());
        return;
    };

    // Counted independently from the file with a separate parser.
    assert_eq!(music::note_count(&midi), 1273, "notes in the theme");
    let length = music::duration(&midi).expect("the theme has a length");
    assert!(
        (80.0..90.0).contains(&length),
        "the theme runs for {length:.1} s, expected about 84"
    );

    let wav = music::render(&midi).expect("the theme should render");
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    assert_eq!(&wav[12..16], b"fmt ");
    assert_eq!(&wav[36..40], b"data");

    // A mono 16-bit WAV of the parsed length, plus a little tail.
    let bytes = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]) as usize;
    assert_eq!(bytes, wav.len() - 44, "the data chunk length");
    let rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
    assert_eq!(rate, music::SAMPLE_RATE);
    let seconds = bytes as f64 / (rate as f64 * 2.0);
    assert!(
        (length..length + 1.5).contains(&seconds),
        "{seconds:.1} s of audio for a {length:.1} s piece"
    );

    // And it is not silence, nor a clipped mess.
    let samples: Vec<i16> = wav[44..]
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
    let loud = samples.iter().filter(|s| s.unsigned_abs() > 64).count();
    assert!(peak > 8000, "the render is nearly silent: peak {peak}");
    assert!(peak <= 32767, "the render clips");
    assert!(
        loud * 100 / samples.len() > 20,
        "only {loud} of {} samples carry any signal",
        samples.len()
    );
}

/// The rule the shipped archive was packed with, replayed over the resource
/// sizes.  This is what the repack tool implements: a page is filled while the
/// running offset is below the page size, the resource that crosses the
/// boundary overflows the page, and each page therefore starts at offset 1.
#[test]
fn the_archive_packing_rule_reproduces_every_offset() {
    // This one is about the shipped archive itself, which lives beside the JAR.
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../x");
    if !dir.join("data").exists() {
        eprintln!("skipping: {} is not there (run ./setup.sh)", dir.display());
        return;
    }
    let resources = pack::load_archive(&dir);

    let index = std::fs::read(dir.join("data")).expect("the archive index");
    let count = u16::from_be_bytes([index[0], index[1]]) as usize;
    let page_size = i32::from_be_bytes([index[2], index[3], index[4], index[5]]) as usize;
    let mut at = 6usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let length = index[at] as usize;
        at += 1;
        let name = String::from_utf8_lossy(&index[at..at + length]).into_owned();
        at += length;
        let offset =
            i32::from_be_bytes([index[at], index[at + 1], index[at + 2], index[at + 3]]) as usize;
        at += 4;
        entries.push((name, offset));
    }
    assert_eq!(at, index.len(), "the index should have no trailing bytes");

    let (mut page, mut skip) = (0usize, 1usize);
    let mut wrong = Vec::new();
    for (name, offset) in &entries {
        if skip >= page_size {
            page += 1;
            skip = 1;
        }
        let derived = page * page_size + skip;
        if *offset != derived {
            wrong.push(format!("{name}: stored {offset}, rule says {derived}"));
        }
        skip += resources[name].len();
    }
    assert!(
        wrong.is_empty(),
        "{} of {} entries disagree with the packing rule: {wrong:?}",
        wrong.len(),
        entries.len()
    );

    // And the consequence that makes the format self-consistent: a skip is
    // always under the page size, while a page *file* may be longer.
    let pages = entries
        .iter()
        .map(|(_, offset)| offset / page_size)
        .max()
        .unwrap_or(0)
        + 1;
    let mut over = 0;
    for page in 0..pages {
        let length = std::fs::metadata(dir.join(format!("data.{page}")))
            .map(|meta| meta.len())
            .unwrap_or(0);
        if length > page_size as u64 {
            over += 1;
        }
    }
    assert!(
        over * 2 > pages as u64,
        "only {over} of {pages} page files are longer than the page size, \
         which is not the archive this rule describes"
    );
}

/// The tree the port reads and the archive the game shipped have to agree byte
/// for byte: one is the other, unpacked.  The tree carries the loose JAR
/// directories on top, and nothing else.
#[test]
fn the_tree_and_the_archive_agree() {
    use std::path::PathBuf;
    let archive_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../x");
    if !archive_dir.join("data").exists() {
        eprintln!("skipping: {} is not there (run ./setup.sh)", archive_dir.display());
        return;
    }

    let tree = pack::load(&assets());
    let archive = pack::load_archive(&archive_dir);
    assert_eq!(archive.len(), 668, "the shipped archive's resource count");

    let mut differing = Vec::new();
    for (name, data) in &archive {
        match tree.get(name) {
            Some(other) if other == data => {}
            Some(_) => differing.push(format!("{name}: bytes differ")),
            None => differing.push(format!("{name}: missing from the tree")),
        }
    }
    assert!(
        differing.is_empty(),
        "{} of {} resources differ: {differing:?}",
        differing.len(),
        archive.len()
    );

    // Everything else in the tree is a loose JAR directory rather than a packed
    // resource: the lists, the interface text, the theme.
    let extra: Vec<&String> = tree
        .keys()
        .filter(|name| !archive.contains_key(*name))
        .collect();
    assert!(!extra.is_empty(), "the tree should carry the loose directories too");
    for name in extra {
        assert!(
            name.starts_with("lists/") || name.starts_with("ui/") || name.starts_with("sounds/"),
            "{name} is in the tree but is neither a resource nor a loose JAR file"
        );
    }
}

/// The settings round-trip, and every one of them changes something: a setting
/// whose two states behave the same way is not a setting.
#[test]
fn settings_persist_and_every_value_does_something() {
    use kora::settings::{Camera, Quality, Scheme, Settings, Visibility, MAX_VOLUME_STEPS};

    let mut settings = Settings::default();
    settings.quality = Quality::Medium;
    settings.camera = Camera::Inside;
    settings.visibility = Visibility::Near;
    settings.background = false;
    settings.hud = false;
    settings.scheme = Scheme::RightHanded;
    settings.auto_throttle = true;
    settings.music = false;
    settings.volume = 0.7;

    let path = std::env::temp_dir().join("kora-settings-test.txt");
    settings.save(&path);
    let loaded = Settings::load(&path);
    assert_eq!(loaded.quality, Quality::Medium);
    assert_eq!(loaded.camera, Camera::Inside);
    assert_eq!(loaded.visibility, Visibility::Near);
    assert!(!loaded.background);
    assert!(!loaded.hud);
    assert_eq!(loaded.scheme, Scheme::RightHanded);
    assert!(loaded.auto_throttle);
    assert!(!loaded.music);
    assert!((loaded.volume - 0.7).abs() < 0.01, "volume {}", loaded.volume);
    let _ = std::fs::remove_file(&path);

    // Defaults survive a round trip too, so a missing file is not a special case.
    let defaults = Settings::default();
    defaults.save(&path);
    let loaded = Settings::load(&path);
    assert_eq!(loaded.quality, defaults.quality);
    assert_eq!(loaded.camera, defaults.camera);
    assert_eq!(loaded.scheme, defaults.scheme);
    assert_eq!(loaded.visibility, defaults.visibility);
    assert_eq!(loaded.background, defaults.background);
    assert_eq!(loaded.hud, defaults.hud);
    assert_eq!(loaded.auto_throttle, defaults.auto_throttle);
    assert_eq!(loaded.music, defaults.music);
    let _ = std::fs::remove_file(&path);

    // Every cycle is its own inverse.
    for quality in Quality::ALL {
        assert_eq!(quality.next().previous(), quality);
    }
    for visibility in [Visibility::Near, Visibility::Medium, Visibility::Far] {
        assert_eq!(visibility.next().previous(), visibility);
    }
    for camera in [Camera::Chase, Camera::Far, Camera::Inside] {
        assert_eq!(camera.next().previous(), camera);
    }
    for scheme in [Scheme::Classic, Scheme::LeftHanded, Scheme::RightHanded] {
        assert_eq!(scheme.next().previous(), scheme);
    }

    // Each control scheme drives with different keys, so the setting is real.
    let schemes = [Scheme::Classic, Scheme::LeftHanded, Scheme::RightHanded];
    for (index, a) in schemes.iter().enumerate() {
        for b in &schemes[index + 1..] {
            assert_ne!(a.throttle_keys(), b.throttle_keys(), "{a:?} against {b:?}");
            assert_ne!(a.steer_keys(), b.steer_keys(), "{a:?} against {b:?}");
        }
    }

    // The camera and the view distance are ordered rather than arbitrary.
    assert!(Camera::Inside.placement().0 < Camera::Chase.placement().0);
    assert!(Camera::Chase.placement().0 < Camera::Far.placement().0);
    assert!(Visibility::Near.far_plane() < Visibility::Medium.far_plane());
    assert!(Visibility::Medium.far_plane() < Visibility::Far.far_plane());

    // Volume counts in steps and wraps at both ends.
    let mut volume = Settings::default();
    volume.set_volume_steps(-3);
    assert_eq!(volume.volume_steps(), 0);
    volume.set_volume_steps(MAX_VOLUME_STEPS + 3);
    assert_eq!(volume.volume_steps(), MAX_VOLUME_STEPS);
    volume.set_volume_steps(MAX_VOLUME_STEPS);
    volume.cycle_volume(true);
    assert_eq!(volume.volume_steps(), 0, "past the top wraps to silence");
    volume.set_volume_steps(0);
    volume.cycle_volume(false);
    assert_eq!(volume.volume_steps(), MAX_VOLUME_STEPS, "and under the bottom");
}

/// Graphics quality reaches the geometry: the mid and high detail layers are
/// what it gates, and the road underneath does not depend on it.
#[test]
fn graphics_quality_gates_the_detail_layers() {
    use kora::scene::{self, Detail};
    let dir = assets();
    let resources = pack::load(&dir);

    let vertices = |detail: Detail| -> usize {
        scene::build_detailed(&dir, &resources, "mc5.map", 0, detail)
            .meshes
            .iter()
            .map(|mesh| mesh.vertices.len())
            .sum()
    };
    let base = vertices(Detail::Base);
    let mid = vertices(Detail::Mid);
    let full = vertices(Detail::Full);
    assert!(base < mid, "the mid layer added nothing: {base} against {mid}");
    assert!(mid < full, "the high layer added nothing: {mid} against {full}");

    // The drivable road is the same at every detail: quality is decoration.
    let base_track = scene::build_detailed(&dir, &resources, "mc5.map", 0, Detail::Base);
    let full_track = scene::build_detailed(&dir, &resources, "mc5.map", 0, Detail::Full);
    assert_eq!(
        base_track.collision_indices.len(),
        full_track.collision_indices.len()
    );
    assert_eq!(
        base_track.grid.path().len(),
        full_track.grid.path().len()
    );
}

/// Where files go, on every platform this can be checked from.
///
/// The rules are a value rather than a `cfg`, so the XDG, macOS, Windows and
/// Android answers are all exercised here even though only one of them is the
/// machine running the test.
#[test]
fn file_locations_follow_each_platforms_rules() {
    use kora::paths::{self, Dir, Target};
    use std::collections::HashMap;
    use std::path::PathBuf;

    let env = |pairs: &[(&str, &str)]| {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |name: &str| map.get(name).cloned()
    };

    // XDG on unix, with the spec's own fallbacks.
    let none = env(&[("HOME", "/home/player")]);
    assert_eq!(
        paths::resolve(Dir::Config, Target::Unix, &none),
        Some(PathBuf::from("/home/player/.config/kora"))
    );
    assert_eq!(
        paths::resolve(Dir::Data, Target::Unix, &none),
        Some(PathBuf::from("/home/player/.local/share/kora"))
    );
    let xdg = env(&[
        ("HOME", "/home/player"),
        ("XDG_CONFIG_HOME", "/xdg/config"),
        ("XDG_DATA_HOME", "/xdg/data"),
    ]);
    assert_eq!(
        paths::resolve(Dir::Config, Target::Unix, &xdg),
        Some(PathBuf::from("/xdg/config/kora"))
    );
    assert_eq!(
        paths::resolve(Dir::Data, Target::Unix, &xdg),
        Some(PathBuf::from("/xdg/data/kora"))
    );

    // An empty variable means unset, as the XDG spec says.
    let empty = env(&[("HOME", "/home/player"), ("XDG_DATA_HOME", ""), ("XDG_CONFIG_HOME", "")]);
    assert_eq!(
        paths::resolve(Dir::Data, Target::Unix, &empty),
        Some(PathBuf::from("/home/player/.local/share/kora"))
    );

    // An explicit directory wins over all of it, on every platform.
    for target in [Target::Unix, Target::MacOs, Target::Windows, Target::Android] {
        let pinned = env(&[
            ("HOME", "/home/player"),
            ("APPDATA", "/appdata"),
            ("XDG_DATA_HOME", "/xdg/data"),
            ("KORA_DATA_DIR", "/pinned/data"),
        ]);
        assert_eq!(
            paths::resolve(Dir::Data, target, &pinned),
            Some(PathBuf::from("/pinned/data")),
            "{target:?} ignored KORA_DATA_DIR"
        );
    }

    assert_eq!(
        paths::resolve(Dir::Data, Target::MacOs, &none),
        Some(PathBuf::from("/home/player/Library/Application Support/kora"))
    );
    assert_eq!(
        paths::resolve(
            Dir::Data,
            Target::Windows,
            &env(&[("APPDATA", "C:/Users/player/AppData/Roaming")])
        ),
        Some(PathBuf::from("C:/Users/player/AppData/Roaming/kora"))
    );

    // No HOME and no override: nothing to resolve to, rather than a guess.
    assert_eq!(paths::resolve(Dir::Config, Target::Unix, &env(&[])), None);
    assert_eq!(paths::resolve(Dir::Data, Target::Windows, &env(&[])), None);

    // The web has no filesystem, so persistence there is not a path at all.
    assert_eq!(paths::resolve(Dir::Data, Target::Web, &none), None);

    // Android is the case worth knowing about: it is a private directory per
    // package, and it is not derivable from the environment.
    assert_eq!(
        paths::resolve(Dir::Data, Target::Android, &none),
        None,
        "a host cannot work out an Android package's private directory"
    );
}

/// The way the README says to run it from the repository root, with the
/// manifest path, so `assets` resolves to the tree the tools share.  That tree
/// still holds tool output - the extraction manifest and the Wavefront files -
/// which the loader has to leave alone.
#[test]
fn the_repository_root_tree_loads() {
    use std::path::PathBuf;
    let tree = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    if !tree.is_dir() {
        eprintln!("skipping: {} is not there (run ./setup.sh)", tree.display());
        return;
    }

    let resources = pack::load(&tree);
    assert!(resources.len() >= 668, "{} resources", resources.len());
    for required in [
        "levels/1.map",
        "cars/rally.car",
        "tiles/s.tl",
        "tex/texpack.png",
        "lists/tile_list",
        "ui/ui.txt",
        "sounds/theme.mid",
    ] {
        assert!(
            resources.contains_key(required),
            "{required} is missing from the tree at the repository root"
        );
    }

    // Tool output sits in the same directory and is not a resource.
    assert!(!resources.contains_key("MANIFEST.tsv"));
    assert!(
        !resources.keys().any(|name| name.ends_with(".obj")),
        "the Wavefront files were loaded as resources"
    );
}
