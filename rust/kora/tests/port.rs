//! Headless checks for the ported asset pipeline.
//!
//! These run without a GPU: the resource archive, every format parser, the
//! track geometry builder and the rapier vehicle all work on plain data.

use std::path::PathBuf;

use kora::physics::{Input, Vehicle};
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
        track.collision_indices.len() > 100,
        "only {} collision triangles",
        track.collision_indices.len()
    );
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
fn car_settles_on_the_track() {
    let dir = assets();
    let resources = pack::load(&dir);
    let track = scene::build(&dir, &resources, "1.map");
    let car = format::Car::parse(&resources["cars/rally.car"]).unwrap();
    let geometry = scene::build_car(&resources, &car).unwrap();

    let mut vehicle = Vehicle::new(
        track.collision_vertices,
        track.collision_indices,
        track.spawn,
        track.spawn_yaw,
        geometry.half_extents,
    );
    let input = Input::default();
    for _ in 0..240 {
        vehicle.step(1.0 / 60.0, &input);
    }
    let (position, _) = vehicle.pose();
    assert!(
        position.y > -10.0 && position.y < 40.0,
        "car did not rest on the track: {position:?}"
    );

    // And it should actually drive forward under throttle.
    let drive = Input {
        throttle: 1.0,
        ..Default::default()
    };
    for _ in 0..120 {
        vehicle.step(1.0 / 60.0, &drive);
    }
    let (moved, _) = vehicle.pose();
    assert!(
        (moved - position).length() > 0.5,
        "car did not move under throttle"
    );
}
