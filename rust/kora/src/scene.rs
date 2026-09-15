//! Builds the static track from `.map` + `.tl` + `.md` + `.hd` + `.ob`.
//!
//! All geometry is baked into a handful of macroquad meshes (one per texture)
//! with the tile models transformed into world space on the CPU.  The same
//! tile triangles are also emitted as a collision trimesh for rapier.
//!
//! Coordinate convention: the game is Z-up (`X`, `Y` are the ground plane and
//! `Z` is height).  macroquad/rapier are Y-up, so every vertex is rotated
//! -90 degrees about X: `(x, y, z) -> (x, z, -y)`.  That mapping is
//! orientation-preserving, so triangle winding survives.
//!
//! Geometry building never touches the GPU, so it can be exercised headlessly;
//! [`Track::attach_textures`] uploads the atlases later, at runtime.

use std::collections::HashMap;
use std::path::Path;

use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;
use macroquad::texture::Texture2D;
use rapier3d::prelude::Vector as RVector;

use crate::format;
use crate::pack::{self, Resources};

/// World size of one tile (`ar.c` in the MIDlet).
pub const TILE: f32 = 14.0;
/// Static node scale the game applies to tile/detail/object models (`ar.a`).
pub const WORLD_SCALE: f32 = 7.01;

pub struct Track {
    pub meshes: Vec<Mesh>,
    /// Texture resource for each mesh, parallel to `meshes`.
    pub texture_paths: Vec<String>,
    pub collision_vertices: Vec<RVector>,
    pub collision_indices: Vec<[u32; 3]>,
    pub spawn: Vec3,
    pub spawn_yaw: f32,
}

impl Track {
    /// Upload every batch's texture.  Needs a live macroquad graphics context.
    pub fn attach_textures(&mut self, res: &Resources) {
        let mut cache: HashMap<String, Texture2D> = HashMap::new();
        for (mesh, path) in self.meshes.iter_mut().zip(self.texture_paths.iter()) {
            if !cache.contains_key(path) {
                if let Some(bytes) = res.get(path.trim_start_matches('/')) {
                    let texture = Texture2D::from_file_with_format(bytes, Some(ImageFormat::Png));
                    texture.set_filter(FilterMode::Linear);
                    cache.insert(path.clone(), texture);
                }
            }
            mesh.texture = cache.get(path).cloned();
        }
    }
}

struct Part {
    verts: Vec<Vertex>,
    idx: Vec<u32>,
}

struct Batch {
    parts: Vec<Part>,
}

impl Batch {
    fn new() -> Batch {
        Batch {
            parts: vec![Part {
                verts: Vec::new(),
                idx: Vec::new(),
            }],
        }
    }

    /// macroquad meshes index with `u16`, so start a new part before overflow.
    fn triangle(&mut self, a: (Vec3, Vec2), b: (Vec3, Vec2), c: (Vec3, Vec2)) {
        if self.parts.last().unwrap().verts.len() + 3 > 60_000 {
            self.parts.push(Part {
                verts: Vec::new(),
                idx: Vec::new(),
            });
        }
        let part = self.parts.last_mut().unwrap();
        let base = part.verts.len() as u32;
        part.verts.push(Vertex::new2(a.0, a.1, WHITE));
        part.verts.push(Vertex::new2(b.0, b.1, WHITE));
        part.verts.push(Vertex::new2(c.0, c.1, WHITE));
        part.idx.extend_from_slice(&[base, base + 1, base + 2]);
    }
}

struct Builder<'a> {
    res: &'a Resources,
    batches: HashMap<String, Batch>,
    collision_vertices: Vec<RVector>,
    collision_indices: Vec<[u32; 3]>,
}

impl<'a> Builder<'a> {
    fn new(res: &'a Resources) -> Builder<'a> {
        Builder {
            res,
            batches: HashMap::new(),
            collision_vertices: Vec::new(),
            collision_indices: Vec::new(),
        }
    }

    /// Transform one model instance and append it to its texture batch.
    fn place(
        &mut self,
        model_path: &str,
        texture_path: &str,
        scale: [f32; 3],
        yaw: f32,
        origin: [f32; 3],
        collide: bool,
    ) {
        let Some(model) = parse_model(self.res, model_path) else {
            return;
        };

        let (cos, sin) = (yaw.cos(), yaw.sin());
        let mut geometry: Vec<[Vec3; 3]> = Vec::new();
        let mut uvs: Vec<[Vec2; 3]> = Vec::new();
        for face in model.triangles() {
            let mut points = [Vec3::ZERO; 3];
            let mut uv = [Vec2::ZERO; 3];
            for k in 0..3 {
                let i = face[k];
                let v = model.positions[i];
                let gx = v[0] * scale[0];
                let gy = v[1] * scale[1];
                let gz = v[2] * scale[2];
                let rx = gx * cos - gy * sin;
                let ry = gx * sin + gy * cos;
                points[k] = vec3(rx + origin[0], gz + origin[2], -(ry + origin[1]));
                uv[k] = vec2(model.texcoords[i][0], model.texcoords[i][1]);
            }
            geometry.push(points);
            uvs.push(uv);
        }

        let batch = self
            .batches
            .entry(texture_path.to_string())
            .or_insert_with(Batch::new);
        for (points, uv) in geometry.iter().zip(uvs.iter()) {
            batch.triangle((points[0], uv[0]), (points[1], uv[1]), (points[2], uv[2]));
        }

        if collide {
            for points in &geometry {
                let base = self.collision_vertices.len() as u32;
                for p in points {
                    self.collision_vertices.push(RVector::new(p.x, p.y, p.z));
                }
                self.collision_indices.push([base, base + 1, base + 2]);
            }
        }
    }
}

fn parse_model(res: &Resources, path: &str) -> Option<format::Model> {
    res.get(path.trim_start_matches('/'))
        .and_then(|bytes| format::Model::parse(bytes))
}

pub fn build(dir: &Path, res: &Resources, map_name: &str) -> Track {
    let tile_list = pack::lines(&pack::read_jar_file(dir, "lists/tile_list"));
    let md_list = pack::lines(&pack::read_jar_file(dir, "lists/md_list"));
    let hd_list = pack::lines(&pack::read_jar_file(dir, "lists/hd_list"));
    let ob_list = pack::lines(&pack::read_jar_file(dir, "lists/ol"));

    let map_path = format!("levels/{map_name}");
    let map = format::Map::parse(
        res.get(&map_path)
            .unwrap_or_else(|| panic!("resource {map_path} not in pack")),
    )
    .expect("malformed .map");

    let mut builder = Builder::new(res);
    // Definition caches, keyed by the 1-based index stored in the map.
    let mut tiles: HashMap<u8, format::Tile> = HashMap::new();
    let mut mids: HashMap<u8, format::MidDetail> = HashMap::new();
    let mut highs: HashMap<u8, format::HighDetail> = HashMap::new();
    let mut objects: HashMap<u8, format::ObjectDef> = HashMap::new();

    let half = std::f32::consts::FRAC_PI_2;
    let list_entry = |list: &[String], index: u8| -> Option<String> {
        list.get((index as usize).checked_sub(1)?).cloned()
    };

    for (y, row) in map.cells.iter().enumerate() {
        for (x, cell) in row.iter().enumerate() {
            let ox = x as f32 * TILE;
            let oy = y as f32 * TILE;

            if let Some((kind, arg)) = cell.tile {
                if kind > 0 {
                    if !tiles.contains_key(&kind) {
                        if let Some(file) = list_entry(&tile_list, kind) {
                            if let Some(tile) = res
                                .get(&format!("tiles/{file}"))
                                .and_then(|bytes| format::Tile::parse(bytes))
                            {
                                tiles.insert(kind, tile);
                            }
                        }
                    }
                    if let Some(tile) = tiles.get(&kind) {
                        builder.place(
                            &format!("models/p/{}", tile.name),
                            &format!("tex/{}", tile.texture),
                            [WORLD_SCALE; 3],
                            arg as f32 * half,
                            [ox, oy, 0.0],
                            true,
                        );
                    }
                }
            }

            for &(kind, arg) in &cell.mid {
                if kind == 0 {
                    continue;
                }
                if !mids.contains_key(&kind) {
                    if let Some(file) = list_entry(&md_list, kind) {
                        if let Some(md) = res
                            .get(&format!("tiles/{file}"))
                            .and_then(|bytes| format::MidDetail::parse(bytes))
                        {
                            mids.insert(kind, md);
                        }
                    }
                }
                if let Some(md) = mids.get(&kind) {
                    builder.place(
                        &format!("models/{}", md.model),
                        &format!("tex/{}", md.texture),
                        [WORLD_SCALE; 3],
                        arg as f32 * half,
                        [ox, oy, 0.0],
                        false,
                    );
                }
            }

            for &(kind, arg) in &cell.high {
                if kind == 0 {
                    continue;
                }
                if !highs.contains_key(&kind) {
                    if let Some(file) = list_entry(&hd_list, kind) {
                        if let Some(hd) = res
                            .get(&format!("tiles/{file}"))
                            .and_then(|bytes| format::HighDetail::parse(bytes))
                        {
                            highs.insert(kind, hd);
                        }
                    }
                }
                let Some(hd) = highs.get(&kind) else { continue };
                // `bp` places each entry at half-tile offsets, mirrored per side.
                for entry in &hd.entries {
                    if entry.kind == 0 {
                        continue;
                    }
                    if !objects.contains_key(&entry.kind) {
                        if let Some(file) = list_entry(&ob_list, entry.kind) {
                            if let Some(ob) = res
                                .get(&format!("objects/{file}"))
                                .and_then(|bytes| format::ObjectDef::parse(bytes))
                            {
                                objects.insert(entry.kind, ob);
                            }
                        }
                    }
                    let Some(object) = objects.get(&entry.kind) else {
                        continue;
                    };
                    let px = 7.0 * entry.position[0];
                    let py = 7.0 * entry.position[1];
                    let pz = -7.0 * entry.position[2];
                    let (lx, ly) = match arg {
                        0 => (ox - py, oy - px),
                        1 => (ox - px, oy - py),
                        2 => (ox + py, oy + px),
                        _ => (ox + px, oy + py),
                    };
                    builder.place(
                        &format!("models/{}", object.model),
                        &format!("tex/{}", object.texture),
                        [WORLD_SCALE; 3],
                        arg as f32 * half,
                        [lx, ly, pz],
                        false,
                    );
                }
            }
        }
    }

    let (sx, sy) = map.start;
    let origin = [sx as f32 * TILE, sy as f32 * TILE, 0.0];
    // Face the first neighbouring cell that is part of the track.
    let mut spawn_yaw = 0.0f32;
    for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
        if map
            .cell(sx as i32 + dx, sy as i32 + dy)
            .is_some_and(|c| c.tile.is_some())
        {
            // The car's nose is +Y in game space; yaw rotates that onto (dx, dy).
            spawn_yaw = (-(dx as f32)).atan2(dy as f32);
            break;
        }
    }

    let Builder {
        batches,
        collision_vertices,
        collision_indices,
        ..
    } = builder;

    let mut meshes = Vec::new();
    let mut texture_paths = Vec::new();
    for (texture_path, batch) in batches {
        for part in batch.parts {
            if part.verts.is_empty() {
                continue;
            }
            meshes.push(Mesh {
                vertices: part.verts,
                indices: part.idx.iter().map(|&i| i as u16).collect(),
                texture: None,
            });
            texture_paths.push(texture_path.clone());
        }
    }

    Track {
        meshes,
        texture_paths,
        collision_vertices,
        collision_indices,
        spawn: vec3(origin[0], 1.2, -origin[1]),
        spawn_yaw,
    }
}

/// A car body in macroquad space, centred on its local origin.
pub struct CarGeometry {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
    pub texture_path: String,
    pub half_extents: Vec3,
}

/// Build the player car from its `.car` descriptor (class `ba`).
///
/// Unlike tiles the car is *not* scaled by `ar.a`; the MIDlet only nudges it
/// with `at.b(0.96, 0.96, 0.9)`, so the natural model size is used.
pub fn build_car(res: &Resources, car: &format::Car) -> Option<CarGeometry> {
    let model = parse_model(res, &format!("models/{}", car.model))?;

    // Per-axis squash the MIDlet applies to the rally body.
    let scale = [0.96f32, 0.96, 0.9];
    let (min, max) = model.bounds();
    let centre_y = 0.5 * (min[2] + max[2]) * scale[2];

    let mut vertices = Vec::new();
    let mut indices: Vec<u16> = Vec::new();
    for face in model.triangles() {
        let base = vertices.len() as u16;
        for &i in &face {
            let v = model.positions[i];
            let p = vec3(v[0] * scale[0], v[2] * scale[2] - centre_y, -v[1] * scale[1]);
            vertices.push(Vertex::new2(
                p,
                vec2(model.texcoords[i][0], model.texcoords[i][1]),
                WHITE,
            ));
        }
        indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    Some(CarGeometry {
        vertices,
        indices,
        texture_path: format!("tex/{}", car.texture),
        half_extents: vec3(
            (max[0] - min[0]) * 0.5 * scale[0],
            (max[2] - min[2]) * 0.5 * scale[2],
            (max[1] - min[1]) * 0.5 * scale[1],
        ),
    })
}

/// Upload a car texture.  Needs a live macroquad graphics context.
pub fn load_car_texture(res: &Resources, geometry: &CarGeometry) -> Option<Texture2D> {
    let bytes = res.get(geometry.texture_path.trim_start_matches('/'))?;
    let texture = Texture2D::from_file_with_format(bytes, Some(ImageFormat::Png));
    texture.set_filter(FilterMode::Linear);
    Some(texture)
}
