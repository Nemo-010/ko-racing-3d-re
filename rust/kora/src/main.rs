//! K.O. Racing 3D - Rust reimplementation of the Jollybox J2ME racer.
//!
//! The original MIDlet reads every asset out of its own `data`/`data.<n>`
//! archive; this port does exactly the same, parsing the model, tile, map,
//! car and font formats directly instead of converting them.  Rendering is
//! macroquad, physics is rapier3d's raycast vehicle controller.
//!
//! Controls: arrows or WASD to drive, space to handbrake, R to restart,
//! Esc to quit.  `KORA_MAP=3.map` picks another track, `KORA_CAR=cars/sport.car`
//! another car, `KORA_LAPS=5` and `KORA_OPPONENTS=5` set the race, and
//! `KORA_ASSETS` points at a different resource directory.

use std::path::PathBuf;

use macroquad::models::{draw_mesh, Mesh};
use macroquad::prelude::*;

use kora::ai::AiDriver;
use kora::physics::{CarControl, World};
use kora::race::Race;
use kora::text::GameFont;
use kora::{format, pack, scene};

fn assets_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("KORA_ASSETS") {
        return PathBuf::from(dir);
    }
    for candidate in ["assets", "../assets", "rust/kora/assets"] {
        let path = PathBuf::from(candidate);
        if path.join("data").exists() {
            return path;
        }
    }
    PathBuf::from("assets")
}

fn env_number(name: &str, default: u32, max: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
        .min(max)
}

fn format_time(seconds: f32) -> String {
    let minutes = (seconds / 60.0) as u32;
    let rest = seconds - minutes as f32 * 60.0;
    format!("{minutes}:{rest:05.2}")
}

#[macroquad::main("K.O. Racing 3D - Rust port")]
async fn main() {
    let dir = assets_dir();
    println!("resource pack: {}", dir.display());
    let resources = pack::load(&dir);
    println!("  {} resources indexed", resources.len());

    let map_name = std::env::var("KORA_MAP").unwrap_or_else(|_| "1.map".to_string());
    let laps = env_number("KORA_LAPS", 3, 99);
    let opponents = env_number("KORA_OPPONENTS", 3, 7);
    let mut track = scene::build(&dir, &resources, &map_name);
    println!(
        "track {}: {} mesh batches, {} collision triangles, {} road cells, {} gates",
        map_name,
        track.meshes.len(),
        track.collision_indices.len(),
        track.grid.path().len(),
        track.grid.gates().len()
    );
    track.attach_textures(&resources);

    let car_name = std::env::var("KORA_CAR").unwrap_or_else(|_| "cars/rally.car".to_string());
    let car_def = resources
        .get(&car_name)
        .and_then(|bytes| format::Car::parse(bytes))
        .expect("car descriptor");
    println!("car: {} ({})", car_def.name, car_def.model);
    let car = scene::build_car(&resources, &car_def).expect("car model");
    let car_texture = scene::load_car_texture(&resources, &car);

    let mut world = World::new(
        track.collision_vertices,
        track.collision_indices,
        &track.walls,
    );

    // Starting grid: the player on the line, opponents behind, all of them
    // placed on road cells and facing the way the circuit is raced.
    let slots = track.grid.grid_slots(1 + opponents as usize);
    let mut player = 0;
    for (index, &(spot, yaw)) in slots.iter().enumerate() {
        let car = world.add_car(spot, yaw, car.half_extents);
        if index == 0 {
            player = car;
        }
    }

    let now = get_time();
    let mut races: Vec<Race> = (0..world.cars.len())
        .map(|index| Race::new(&track.grid, laps, world.position(index), now))
        .collect();
    let mut drivers: Vec<AiDriver> = (0..world.cars.len())
        .map(|index| AiDriver::new(if index == 0 { 1.0 } else { 0.84 + 0.06 * index as f32 }))
        .collect();

    let font = GameFont::load(&resources, "font");
    let mut camera_position = track.spawn + vec3(0.0, 5.0, 9.0);

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::R) {
            let restart = get_time();
            for index in 0..world.cars.len() {
                world.reset(index);
                let position = world.position(index);
                races[index] = Race::new(&track.grid, laps, position, restart);
            }
            camera_position = track.spawn + vec3(0.0, 5.0, 9.0);
        }

        // --- controls -----------------------------------------------------
        let mut controls = vec![CarControl::default(); world.cars.len()];
        let (position, rotation) = world.pose(player);
        controls[player].throttle = if is_key_down(KeyCode::Up) || is_key_down(KeyCode::W) {
            1.0
        } else if is_key_down(KeyCode::Down) || is_key_down(KeyCode::S) {
            -0.6
        } else {
            0.0
        };
        controls[player].steer = if is_key_down(KeyCode::Left) || is_key_down(KeyCode::A) {
            1.0
        } else if is_key_down(KeyCode::Right) || is_key_down(KeyCode::D) {
            -1.0
        } else {
            0.0
        };
        controls[player].brake = is_key_down(KeyCode::Space);

        for index in 0..world.cars.len() {
            if index == player {
                continue;
            }
            let (opponent_position, opponent_rotation) = world.pose(index);
            let heading = opponent_rotation * vec3(0.0, 0.0, -1.0);
            controls[index] = drivers[index].control(
                &track.grid,
                opponent_position,
                heading,
                world.speed(index),
                get_frame_time().min(0.05),
            );
        }

        // --- simulate -----------------------------------------------------
        let dt = get_frame_time().min(0.05);
        let substeps = ((dt / (1.0 / 60.0)).ceil() as i32).clamp(1, 4);
        for _ in 0..substeps {
            world.step(dt / substeps as f32, &controls);
        }

        let now = get_time();
        for index in 0..world.cars.len() {
            let place = world.position(index);
            if place.y < -40.0 {
                if index == player {
                    world.reset(index);
                } else {
                    let (cell_x, cell_y) = track.grid.cell_of(vec3(place.x, 0.0, place.z));
                    let spin = world.pose(index).1;
                    let yaw = 2.0 * spin.y.atan2(spin.w);
                    world.replace(index, track.grid.center(cell_x, cell_y), yaw);
                }
            }
            races[index].update(now, &track.grid, world.position(index));
        }

        // --- camera -------------------------------------------------------
        let forward = rotation * vec3(0.0, 0.0, -1.0);
        let up = vec3(0.0, 1.0, 0.0);
        let desired = position - forward * 5.0 + up * 2.2;
        camera_position = camera_position.lerp(desired, (dt * 5.0).min(1.0));

        let mut camera = Camera3D::default();
        camera.position = camera_position;
        camera.target = position + forward * 3.0 + up * 0.8;
        camera.up = up;
        camera.fovy = 62f32.to_radians();
        camera.z_far = 4000.0;
        set_camera(&camera);

        clear_background(Color::new(0.53, 0.81, 0.92, 1.0));
        for mesh in &track.meshes {
            draw_mesh(mesh);
        }

        for index in 0..world.cars.len() {
            let (place, spin) = world.pose(index);
            let mut vertices = car.vertices.clone();
            for vertex in &mut vertices {
                vertex.position = spin * vertex.position + place;
            }
            draw_mesh(&Mesh {
                vertices,
                indices: car.indices.clone(),
                texture: car_texture.clone(),
            });
        }

        set_default_camera();

        // --- HUD ----------------------------------------------------------
        let standings: Vec<usize> = {
            let mut order: Vec<usize> = (0..world.cars.len()).collect();
            order.sort_by(|&a, &b| {
                races[b]
                    .progress(&track.grid, world.position(b))
                    .partial_cmp(&races[a].progress(&track.grid, world.position(a)))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            order
        };
        let player_place = standings.iter().position(|&car| car == player).unwrap_or(0) + 1;
        let race = &races[player];

        if let Some(font) = &font {
            let scale = 2.0;
            font.draw_shadow(&race_name(&map_name), 16.0, 14.0, scale, WHITE);
            font.draw_shadow(
                &format!("LAP {}/{}", (race.lap + 1).min(race.laps), race.laps),
                16.0,
                48.0,
                scale,
                WHITE,
            );
            font.draw_shadow(
                &format!("POS {}/{}", player_place, world.cars.len()),
                16.0,
                82.0,
                scale,
                WHITE,
            );
            font.draw_shadow(
                &format!("TIME {}", format_time(race.total_time(now))),
                16.0,
                116.0,
                scale,
                WHITE,
            );
            let best = race
                .best
                .map(format_time)
                .unwrap_or_else(|| "--:--.--".to_string());
            font.draw_shadow(
                &format!("LAP {}  BEST {}", format_time(race.lap_time(now)), best),
                16.0,
                150.0,
                scale,
                Color::new(1.0, 0.85, 0.2, 1.0),
            );
            if race.finished {
                let text = format!("FINISHED  {}", format_time(race.finish_time.unwrap_or(0.0)));
                let width = font.width(&text, 3.0);
                font.draw_shadow(
                    &text,
                    (screen_width() - width) / 2.0,
                    screen_height() * 0.35,
                    3.0,
                    Color::new(1.0, 0.9, 0.3, 1.0),
                );
            }
            font.draw_shadow(
                "ARROWS DRIVE   SPACE BRAKE   R RESTART",
                16.0,
                screen_height() - 40.0,
                1.5,
                Color::new(0.9, 0.9, 0.9, 1.0),
            );
        } else {
            draw_text(
                &format!("lap {}  {:.1}", race.lap, world.speed(player)),
                16.0,
                30.0,
                30.0,
                WHITE,
            );
        }
    }
}

fn race_name(map_name: &str) -> String {
    map_name
        .trim_end_matches(".map")
        .to_ascii_uppercase()
        .replace("MAP", "TRACK ")
}
