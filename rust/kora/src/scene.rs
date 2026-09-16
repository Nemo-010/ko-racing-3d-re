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
use crate::grid::Grid;
use crate::pack::{self, Resources};

/// World size of one tile (`ar.c` in the MIDlet).
pub const TILE: f32 = 14.0;
/// Static node scale the game applies to tile/detail/object models (`ar.a`).
pub const WORLD_SCALE: f32 = 7.01;

/// Per-cell collision meshes, so the runtime can ask how high the road is at
/// any point and lift a car back onto it.
pub struct SurfaceGrid {
    width: i32,
    height: i32,
    cells: Vec<Option<(u8, Option<format::Collision>)>>,
}

impl SurfaceGrid {
    pub fn height_at(&self, position: Vec3) -> Option<f32> {
        let x = (position.x / TILE).round() as i32;
        let y = (-position.z / TILE).round() as i32;
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return None;
        }
        let (arg, collision) = self.cells[(y * self.width + x) as usize].as_ref()?;
        let centre = vec3(x as f32 * TILE, 0.0, -(y as f32) * TILE);
        let local = vec2(
            (position.x - centre.x) / TILE + 0.5,
            (centre.z - position.z) / TILE + 0.5,
        );
        Some(surface_height(collision.as_ref(), *arg, local))
    }
}

pub struct Track {
    /// Road graph: which cells exist and which sides are drivable.
    pub grid: Grid,
    /// The height function the collider was built from.
    pub surface: SurfaceGrid,
    pub meshes: Vec<Mesh>,
    /// Texture resource for each mesh, parallel to `meshes`.
    pub texture_paths: Vec<String>,
    pub collision_vertices: Vec<RVector>,
    pub collision_indices: Vec<[u32; 3]>,
    /// Edge barriers as `(centre, half extents)` in macroquad space.
    pub walls: Vec<(Vec3, Vec3)>,
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

/// Samples per cell edge when tessellating a cell whose surface is not flat.
/// The collision mesh is a height function over the cell's local `[0,1]^2`
/// square and most tiles only cover part of it, so the collider samples the
/// function instead of emitting the raw triangles: a kerb strip, a sunken
/// floor and a raised platform then all come out as one surface.
const SURFACE_STEPS: usize = 8;

/// `bm.a(float, float)`: the sample point is rotated into the mesh's frame.
fn rotate_sample(point: Vec2, arg: u8) -> Vec2 {
    match arg % 4 {
        1 => vec2(point.y, 1.0 - point.x),
        2 => vec2(1.0 - point.x, 1.0 - point.y),
        3 => vec2(1.0 - point.y, point.x),
        _ => point,
    }
}

/// The game's height function: the interpolated collision mesh height in world
/// units, or zero when the tile has no mesh or the point falls outside it.
fn surface_height(collision: Option<&format::Collision>, arg: u8, local: Vec2) -> f32 {
    let Some(collision) = collision else {
        return 0.0;
    };
    if collision.vertices.is_empty() {
        return 0.0;
    }
    let point = rotate_sample(local, arg);
    for triangle in &collision.triangles {
        let [a, b, c] = [
            collision.vertices[triangle[0]],
            collision.vertices[triangle[1]],
            collision.vertices[triangle[2]],
        ];
        let determinant = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
        if determinant.abs() < 1e-9 {
            continue;
        }
        let u = ((b[1] - c[1]) * (point.x - c[0]) + (c[0] - b[0]) * (point.y - c[1])) / determinant;
        let v = ((c[1] - a[1]) * (point.x - c[0]) + (a[0] - c[0]) * (point.y - c[1])) / determinant;
        if u >= -1e-6 && v >= -1e-6 && u + v <= 1.0 + 1e-6 {
            // The third component is negated when the MIDlet builds the
            // triangle, and the mesh shares the tile's 14-unit scale.
            return -(u * a[2] + v * b[2] + (1.0 - u - v) * c[2]) * TILE;
        }
    }
    0.0
}

fn tile_has_mesh(tile: Option<&format::Tile>) -> bool {
    tile.and_then(|tile| tile.collision.as_ref())
        .is_some_and(|collision| !collision.vertices.is_empty())
}

fn world_of(centre: Vec3, local: Vec2, height: f32) -> Vec3 {
    vec3(
        centre.x + (local.x - 0.5) * TILE,
        height,
        centre.z - (local.y - 0.5) * TILE,
    )
}

fn push_triangle(
    vertices: &mut Vec<RVector>,
    indices: &mut Vec<[u32; 3]>,
    a: Vec3,
    b: Vec3,
    c: Vec3,
) {
    let base = vertices.len() as u32;
    for point in [a, b, c] {
        vertices.push(RVector::new(point.x, point.y, point.z));
    }
    indices.push([base, base + 1, base + 2]);
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
}

impl<'a> Builder<'a> {
    fn new(res: &'a Resources) -> Builder<'a> {
        Builder {
            res,
            batches: HashMap::new(),
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

    }
}

fn parse_model(res: &Resources, path: &str) -> Option<format::Model> {
    res.get(path.trim_start_matches('/'))
        .and_then(|bytes| format::Model::parse(bytes))
}

/// Detail remap from `bm.a(ae/ap, kind, arg)`: on theme 3 the MIDlet drops a
/// range of mid/high-detail indices, and `bp` swaps two object kinds.
fn themed_detail(theme: u8, kind: u8) -> u8 {
    if theme == 3
        && (kind < 19
            || kind == 22
            || kind == 28
            || kind == 29
            || kind == 30
            || (32..=36).contains(&kind))
    {
        0
    } else {
        kind
    }
}

fn themed_object(theme: u8, kind: u8) -> u8 {
    if theme == 3 {
        match kind {
            0 | 1 => 5,
            2 | 3 => 4,
            other => other,
        }
    } else {
        kind
    }
}

pub fn build(dir: &Path, res: &Resources, map_name: &str) -> Track {
    build_themed(dir, res, map_name, 0)
}

/// Build a track with a specific tile/texture variant (the campaign's `theme`).
pub fn build_themed(dir: &Path, res: &Resources, map_name: &str, theme: u8) -> Track {
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
                        );
                    }
                }
            }

            for &(kind, arg) in &cell.mid {
                let kind = themed_detail(theme, kind);
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
                    );
                }
            }

            for &(kind, arg) in &cell.high {
                let kind = themed_detail(theme, kind);
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
                    let object_kind = themed_object(theme, entry.kind);
                    if object_kind == 0 {
                        continue;
                    }
                    if !objects.contains_key(&object_kind) {
                        if let Some(file) = list_entry(&ob_list, object_kind) {
                            if let Some(ob) = res
                                .get(&format!("objects/{file}"))
                                .and_then(|bytes| format::ObjectDef::parse(bytes))
                            {
                                objects.insert(object_kind, ob);
                            }
                        }
                    }
                    let Some(object) = objects.get(&object_kind) else {
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
                    );
                }
            }
        }
    }

    let origin = [map.start.0 as f32 * TILE, map.start.1 as f32 * TILE, 0.0];
    let grid = Grid::build(&map, &tiles);
    // Start facing the way the circuit is raced.
    let (spawn, spawn_yaw) = grid
        .grid_slots(1)
        .first()
        .copied()
        .map(|(position, yaw)| (position, yaw))
        .unwrap_or((
            vec3(origin[0], 1.2, -origin[1]),
            0.0,
        ));

    let Builder { batches, .. } = builder;

    // Physics ground.  The MIDlet samples each tile's collision mesh for
    // height and falls back to a flat plane at zero when the tile has none -
    // which is most of them (`s.tl` and friends ship no collision data).
    //
    // The collider is that height function: a sub-divided patch over every
    // cell whose tile carries a mesh, a flat quad otherwise.  A mesh can cover
    // only part of its cell, so sampling it on a grid gives one surface rather
    // than overlapping sheets.  Steps between neighbouring cells are left as
    // they are and the car is lifted onto the surface (see `World::lift_to`),
    // which is how the MIDlet drives its own car over them.
    let mut collision_vertices: Vec<RVector> = Vec::new();
    let mut collision_indices: Vec<[u32; 3]> = Vec::new();
    let mut walls: Vec<(Vec3, Vec3)> = Vec::new();
    let half_tile = TILE * 0.5;

    let tile_at = |x: i32, y: i32| -> Option<(u8, u8)> {
        if !grid.occupied(x, y) {
            return None;
        }
        map.cells[y as usize][x as usize].tile
    };
    let mesh_of = |kind: u8| -> Option<&format::Collision> {
        tiles
            .get(&kind)
            .and_then(|tile| tile.collision.as_ref())
            .filter(|collision| !collision.vertices.is_empty())
    };

    for (x, y) in grid.path() {
        let centre = grid.center(x, y);
        let (kind, arg) = tile_at(x, y).unwrap();
        let mesh = mesh_of(kind);

        if mesh.is_none() {
            let base = collision_vertices.len() as u32;
            for (dx, dz) in [
                (-half_tile, -half_tile),
                (-half_tile, half_tile),
                (half_tile, half_tile),
                (half_tile, -half_tile),
            ] {
                collision_vertices.push(RVector::new(centre.x + dx, 0.0, centre.z + dz));
            }
            collision_indices.push([base, base + 1, base + 2]);
            collision_indices.push([base, base + 2, base + 3]);
        } else {
            let steps = SURFACE_STEPS;
            for i in 0..steps {
                for j in 0..steps {
                    let lo = vec2(i as f32, j as f32) / steps as f32;
                    let hi = vec2((i + 1) as f32, (j + 1) as f32) / steps as f32;
                    let corner = |local: Vec2| {
                        world_of(centre, local, surface_height(mesh, arg, local))
                    };
                    let (a, b, c, d) = (
                        corner(vec2(lo.x, lo.y)),
                        corner(vec2(lo.x, hi.y)),
                        corner(vec2(hi.x, hi.y)),
                        corner(vec2(hi.x, lo.y)),
                    );
                    push_triangle(&mut collision_vertices, &mut collision_indices, a, b, c);
                    push_triangle(&mut collision_vertices, &mut collision_indices, a, c, d);
                }
            }
        }

        // A barrier on every side that is not drivable, standing on the
        // surface rather than at zero.
        for dir in 0..4 {
            if grid.open_sides(x, y) & (1 << dir) != 0 {
                continue;
            }
            let ground = surface_height(
                mesh,
                arg,
                match dir {
                    0 => vec2(1.0, 0.5),
                    1 => vec2(0.5, 0.0),
                    2 => vec2(0.0, 0.5),
                    _ => vec2(0.5, 1.0),
                },
            );
            let direction = crate::grid::dir_mq(dir);
            let (hx, hz) = if dir % 2 == 0 {
                (0.6, half_tile)
            } else {
                (half_tile, 0.6)
            };
            walls.push((
                centre + direction * half_tile + vec3(0.0, ground, 0.0),
                vec3(hx, 1.1, hz),
            ));
        }
    }

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

    let surface = SurfaceGrid {
        width: grid.width,
        height: grid.height,
        cells: (0..grid.height)
            .flat_map(|y| (0..grid.width).map(move |x| (x, y)))
            .map(|(x, y)| {
                map.cells[y as usize][x as usize].tile.map(|(kind, arg)| {
                    (
                        arg,
                        tiles
                            .get(&kind)
                            .and_then(|tile| tile.collision.clone())
                            .filter(|collision| !collision.vertices.is_empty()),
                    )
                })
            })
            .collect(),
    };

    Track {
        grid,
        surface,
        meshes,
        texture_paths,
        collision_vertices,
        collision_indices,
        walls,
        spawn,
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
