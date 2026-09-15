//! K.O. Racing 3D - Rust reimplementation of the Jollybox J2ME racer.
//!
//! The original MIDlet reads every asset out of its own `data`/`data.<n>`
//! archive; this port does exactly the same, parsing the model, tile, map
//! and car formats directly instead of converting them.  Rendering is
//! macroquad, physics is rapier3d's raycast vehicle controller.
//!
//! Controls: arrows or WASD to drive, space to handbrake, R to respawn,
//! Esc to quit.  `KORA_MAP=3.map` picks another track, `KORA_CAR=cars/sport.car`
//! another car, and `KORA_ASSETS` another resource directory.

use std::path::PathBuf;

use macroquad::models::{draw_mesh, Mesh};
use macroquad::prelude::*;

use kora::physics::{Input, Vehicle};
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

#[macroquad::main("K.O. Racing 3D - Rust port")]
async fn main() {
    let dir = assets_dir();
    println!("resource pack: {}", dir.display());
    let resources = pack::load(&dir);
    println!("  {} resources indexed", resources.len());

    let map_name = std::env::var("KORA_MAP").unwrap_or_else(|_| "1.map".to_string());
    let mut track = scene::build(&dir, &resources, &map_name);
    println!(
        "track {}: {} mesh batches, {} collision triangles",
        map_name,
        track.meshes.len(),
        track.collision_indices.len()
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

    let mut vehicle = Vehicle::new(
        track.collision_vertices,
        track.collision_indices,
        track.spawn,
        track.spawn_yaw,
        car.half_extents,
    );
    let font = GameFont::load(&resources, "font");

    let mut camera_position = track.spawn + vec3(0.0, 5.0, 9.0);
    let mut fastest = 0.0f32;

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::R) {
            vehicle.reset();
        }

        let mut input = Input::default();
        input.throttle = if is_key_down(KeyCode::Up) || is_key_down(KeyCode::W) {
            1.0
        } else if is_key_down(KeyCode::Down) || is_key_down(KeyCode::S) {
            -0.6
        } else {
            0.0
        };
        // Positive steering turns the wheels left (about +Y).
        input.steer = if is_key_down(KeyCode::Left) || is_key_down(KeyCode::A) {
            1.0
        } else if is_key_down(KeyCode::Right) || is_key_down(KeyCode::D) {
            -1.0
        } else {
            0.0
        };
        input.brake = is_key_down(KeyCode::Space);

        let dt = get_frame_time().min(0.05);
        let substeps = ((dt / (1.0 / 60.0)).ceil() as i32).clamp(1, 4);
        for _ in 0..substeps {
            vehicle.step(dt / substeps as f32, &input);
        }

        let (position, rotation) = vehicle.pose();
        if position.y < -40.0 {
            vehicle.reset();
        }

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

        let mut vertices = car.vertices.clone();
        for vertex in &mut vertices {
            vertex.position = rotation * vertex.position + position;
        }
        draw_mesh(&Mesh {
            vertices,
            indices: car.indices.clone(),
            texture: car_texture.clone(),
        });

        set_default_camera();

        let speed = vehicle.speed();
        fastest = fastest.max(speed);
        if let Some(font) = &font {
            let scale = 2.0;
            font.draw_shadow("K.O. RACING 3D", 16.0, 14.0, scale, WHITE);
            font.draw_shadow(
                &format!("SPEED {:>4}", (speed * 10.0) as i32),
                16.0,
                48.0,
                scale,
                WHITE,
            );
            font.draw_shadow(
                &format!("BEST  {:>4}", (fastest * 10.0) as i32),
                16.0,
                82.0,
                scale,
                Color::new(1.0, 0.85, 0.2, 1.0),
            );
            font.draw_shadow(
                "ARROWS DRIVE   SPACE BRAKE   R RESPAWN",
                16.0,
                screen_height() - 40.0,
                1.5,
                Color::new(0.9, 0.9, 0.9, 1.0),
            );
        } else {
            draw_text(&format!("speed {:.1}", speed), 16.0, 30.0, 30.0, WHITE);
        }
    }
}
