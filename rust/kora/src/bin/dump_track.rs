//! Dump a built track to a text file, so it can be looked at.
//!
//! Building geometry never touches the GPU (`scene::build`), and this binary
//! never makes a window, so it runs anywhere - including where the game itself
//! cannot start, which is exactly where "the track looks wrong" has to be
//! investigated.  `tools/kora view` renders the dump to a PNG.
//!
//! ```sh
//! cargo run --release --bin dump_track -- assets 1.map /tmp/1.track
//! cd ../.. && python3 -m kora view /tmp/1.track /tmp/1.png
//! ```
//!
//! The format is one record per line, triangles as three vertex lines each:
//!
//! ```text
//! TEX <texture resource>
//! V <x> <y> <z> <u> <v>
//! CAR <texture resource>            # the player's car follows, same V lines
//! CAMERA <eye xyz> <target xyz> <fov degrees>
//! SLOT <x> <y> <z> <yaw>            # one per car on the starting grid
//! ```

use std::io::Write;
use std::path::PathBuf;

use macroquad::prelude::{vec3, Vec3};

use kora::settings::Camera;
use kora::{format, pack, scene};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().unwrap_or_else(|| "assets".to_string()));
    let map = args.next().unwrap_or_else(|| "1.map".to_string());
    let out = PathBuf::from(args.next().unwrap_or_else(|| "/tmp/track.dump".to_string()));
    let detail = args.next().unwrap_or_else(|| "full".to_string());
    let theme: u8 = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);

    let resources = pack::load(&dir);
    let detail = match detail.as_str() {
        "base" => scene::Detail::Base,
        "mid" => scene::Detail::Mid,
        _ => scene::Detail::Full,
    };
    let mut track = scene::build_detailed(&dir, &resources, &map, theme, detail);
    // The texture a batch asks for, so the viewer can tell them apart.
    println!(
        "built {} meshes, {} collision triangles, spawn {:?} yaw {:.2}",
        track.meshes.len(),
        track.collision_indices.len(),
        track.spawn,
        track.spawn_yaw
    );

    let mut text = String::new();
    for (mesh, path) in track.meshes.iter().zip(track.texture_paths.iter()) {
        text.push_str(&format!("TEX {path}\n"));
        for triangle in mesh.indices.chunks(3) {
            for &index in triangle {
                let vertex = &mesh.vertices[index as usize];
                text.push_str(&format!(
                    "V {} {} {} {} {}\n",
                    vertex.position.x, vertex.position.y, vertex.position.z, vertex.uv.x, vertex.uv.y
                ));
            }
        }
    }

    // The cars sit where the grid puts them; the player's body is dumped once,
    // in its own local space, and placed by the viewer.
    let car_file = std::env::var("KORA_CAR").unwrap_or_else(|_| "rally.car".to_string());
    if let Some(definition) = resources
        .get(&format!("cars/{car_file}"))
        .and_then(|bytes| format::Car::parse(bytes))
    {
        if let Some(geometry) = scene::build_car(&resources, &definition) {
            text.push_str(&format!("CAR {}\n", geometry.texture_path));
            for triangle in geometry.indices.chunks(3) {
                for &index in triangle {
                    let vertex = &geometry.vertices[index as usize];
                    text.push_str(&format!(
                        "V {} {} {} {} {}\n",
                        vertex.position.x,
                        vertex.position.y,
                        vertex.position.z,
                        vertex.uv.x,
                        vertex.uv.y
                    ));
                }
            }
            // The front end's own camera, so what is rendered is what the game
            // shows rather than a view chosen to flatter it.
            let (back, height) = Camera::Chase.placement();
            let forward = vec3(track.spawn_yaw.sin(), 0.0, -track.spawn_yaw.cos());
            let up = Vec3::Y;
            let eye = track.spawn - forward * back + up * height;
            let target = track.spawn + forward * 3.0 + up * 0.8;
            text.push_str(&format!(
                "CAMERA {} {} {} {} {} {} {}\n",
                eye.x,
                eye.y,
                eye.z,
                target.x,
                target.y,
                target.z,
                62.0
            ));
            for (spot, yaw) in track.grid.grid_slots(8) {
                text.push_str(&format!(
                    "SLOT {} {} {} {}\n",
                    spot.x, spot.y, spot.z, yaw
                ));
            }
        }
    }

    // Touching the texture list is what keeps `attach_textures` from being
    // folded away here; the geometry is the point, not the upload.
    track.texture_paths.sort();

    let mut file = std::fs::File::create(&out).expect("could not write the dump");
    file.write_all(text.as_bytes()).expect("could not write the dump");
    println!("wrote {} lines to {}", text.lines().count(), out.display());
}
