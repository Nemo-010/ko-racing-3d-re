//! Road graph derived from a `.map` plus its tiles.
//!
//! Adjacency comes from each tile's *open-side* flags (the `.tl` block read as
//! `open_sides`), rotated by the cell's 0-3 argument so that the flags line up
//! with world directions.  Two neighbouring cells are connected only when both
//! declare the shared side open, which is what makes the graph match the road
//! the MIDlet's `bs` flood walks: every one of the 40 shipped maps comes out
//! connected, with a mostly degree-2 ring topology.

use std::collections::{HashMap, VecDeque};

use macroquad::prelude::*;

use crate::format::{Map, Tile};
use crate::scene::TILE;

/// Direction indices, matching the MIDlet's `b.a(i)`/`b.b(i)`.
/// 0 = +X, 1 = -Y, 2 = -X, 3 = +Y, in game space (X/Y ground, Z up).
pub const DIRS: [(i32, i32); 4] = [(1, 0), (0, -1), (-1, 0), (0, 1)];

/// The same directions in macroquad space, where `(x, y, z) -> (x, z, -y)`.
pub fn dir_mq(dir: usize) -> Vec3 {
    match dir {
        0 => vec3(1.0, 0.0, 0.0),
        1 => vec3(0.0, 0.0, 1.0),
        2 => vec3(-1.0, 0.0, 0.0),
        _ => vec3(0.0, 0.0, -1.0),
    }
}

pub struct Grid {
    pub width: i32,
    pub height: i32,
    open: Vec<u8>,
    occupied: Vec<bool>,
    pub start: (i32, i32),
    pub finish: (i32, i32),
    pub checkpoints: Vec<(i32, i32)>,
}

impl Grid {
    pub fn build(map: &Map, tiles: &HashMap<u8, Tile>) -> Grid {
        let width = map.width as i32;
        let height = map.height as i32;
        let mut open = vec![0u8; (width * height) as usize];
        let mut occupied = vec![false; (width * height) as usize];

        for y in 0..height {
            for x in 0..width {
                let Some((kind, arg)) = map.cells[y as usize][x as usize].tile else {
                    continue;
                };
                let Some(tile) = tiles.get(&kind) else {
                    continue;
                };
                let index = (y * width + x) as usize;
                occupied[index] = true;
                let mut mask = 0u8;
                for dir in 0..4 {
                    if tile.open_sides[((dir as u8 + arg) % 4) as usize] {
                        mask |= 1 << dir;
                    }
                }
                open[index] = mask;
            }
        }

        Grid {
            width,
            height,
            open,
            occupied,
            start: (map.start.0 as i32, map.start.1 as i32),
            finish: (map.finish.0 as i32, map.finish.1 as i32),
            checkpoints: map
                .checkpoints
                .iter()
                .map(|&(x, y)| (x as i32, y as i32))
                .collect(),
        }
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return None;
        }
        Some((y * self.width + x) as usize)
    }

    pub fn occupied(&self, x: i32, y: i32) -> bool {
        self.index(x, y).is_some_and(|i| self.occupied[i])
    }

    pub fn open_sides(&self, x: i32, y: i32) -> u8 {
        self.index(x, y).map_or(0, |i| self.open[i])
    }

    /// Is the shared side between `(x, y)` and its neighbour in `dir` drivable?
    pub fn connected(&self, x: i32, y: i32, dir: usize) -> bool {
        if self.open_sides(x, y) & (1 << dir) == 0 {
            return false;
        }
        let (dx, dy) = DIRS[dir];
        let (nx, ny) = (x + dx, y + dy);
        self.open_sides(nx, ny) & (1 << ((dir + 2) % 4)) != 0
    }

    pub fn neighbours(&self, x: i32, y: i32) -> Vec<(usize, i32, i32)> {
        (0..4)
            .filter(|&dir| self.connected(x, y, dir))
            .map(|dir| {
                let (dx, dy) = DIRS[dir];
                (dir, x + dx, y + dy)
            })
            .collect()
    }

    /// Cell containing a macroquad-space point (cell centres are `TILE` apart).
    pub fn cell_of(&self, position: Vec3) -> (i32, i32) {
        (
            (position.x / TILE).round() as i32,
            (-position.z / TILE).round() as i32,
        )
    }

    pub fn center(&self, x: i32, y: i32) -> Vec3 {
        vec3(x as f32 * TILE, 0.0, -(y as f32) * TILE)
    }

    /// Base surface height of a cell (the tile models sit at `y = 0`).
    pub fn path(&self) -> Vec<(i32, i32)> {
        (0..self.height)
            .flat_map(|y| (0..self.width).map(move |x| (x, y)))
            .filter(|&(x, y)| self.occupied(x, y))
            .collect()
    }

    /// Direction out of `(x, y)` that best matches `heading` (a horizontal
    /// vector); falls back to the least-bad open side rather than reversing.
    pub fn pick_dir(&self, x: i32, y: i32, heading: Vec2) -> Option<usize> {
        let mut best: Option<(f32, usize)> = None;
        for (dir, _, _) in self.neighbours(x, y) {
            let d = dir_mq(dir);
            let dot = d.x * heading.x + d.z * heading.y;
            if best.is_none_or(|(best_dot, _)| dot > best_dot) {
                best = Some((dot, dir));
            }
        }
        best.map(|(_, dir)| dir)
    }

    /// A point on the road ahead of the car, used both by the AI and to sanity
    /// check the player's progress.
    pub fn target(&self, position: Vec3, heading: Vec3) -> Option<Vec3> {
        let (x, y) = self.cell_of(position);
        let heading2 = vec2(heading.x, heading.z).normalize_or_zero();
        let dir = self.pick_dir(x, y, heading2)?;
        let (dx, dy) = DIRS[dir];
        let (nx, ny) = (x + dx, y + dy);
        let near = self.center(nx, ny);
        // Half a cell further along, so the car aims through the corner.
        let ahead = match self.pick_dir(nx, ny, vec2(dir_mq(dir).x, dir_mq(dir).z)) {
            Some(next) => {
                let (ex, ey) = DIRS[next];
                self.center(nx + ex, ny + ey)
            }
            None => near,
        };
        Some(near.lerp(ahead, 0.5))
    }

    /// Walk backwards along the road from `from`, for laying out a starting
    /// grid.  Returns the cells behind the start, nearest first.
    pub fn cells_behind(&self, from: (i32, i32), heading: Vec3, count: usize) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        let mut cur = from;
        let mut back = -vec2(heading.x, heading.z).normalize_or_zero();
        for _ in 0..count {
            let mut best: Option<(f32, i32, i32)> = None;
            for (_, nx, ny) in self.neighbours(cur.0, cur.1) {
                let delta = self.center(nx, ny) - self.center(cur.0, cur.1);
                let dot = delta.x * back.x + delta.z * back.y;
                if best.is_none_or(|(best_dot, _, _)| dot > best_dot) {
                    best = Some((dot, nx, ny));
                }
            }
            let Some((_, nx, ny)) = best else { break };
            let delta = self.center(nx, ny) - self.center(cur.0, cur.1);
            back = vec2(delta.x, delta.z).normalize_or_zero();
            cur = (nx, ny);
            out.push(cur);
        }
        out
    }

    /// Which way round the circuit is raced.
    ///
    /// The start cell has two drivable sides; the race direction is the one
    /// leading to the first checkpoint, found by labelling every cell with the
    /// start side it descends from.  With no checkpoints either way round is a
    /// valid lap, so the first open side wins.
    pub fn race_dir(&self) -> Option<usize> {
        let neighbours = self.neighbours(self.start.0, self.start.1);
        if neighbours.is_empty() {
            return None;
        }
        if neighbours.len() < 2 {
            return Some(neighbours[0].0);
        }
        let mut arm: HashMap<(i32, i32), usize> = HashMap::new();
        let mut queue = VecDeque::new();
        arm.insert(self.start, usize::MAX);
        for &(dir, nx, ny) in &neighbours {
            arm.insert((nx, ny), dir);
            queue.push_back((nx, ny));
        }
        while let Some((x, y)) = queue.pop_front() {
            let side = arm[&(x, y)];
            for (_, nx, ny) in self.neighbours(x, y) {
                if !arm.contains_key(&(nx, ny)) {
                    arm.insert((nx, ny), side);
                    queue.push_back((nx, ny));
                }
            }
        }
        for gate in self.gates() {
            match arm.get(&gate) {
                Some(&side) if side != usize::MAX => return Some(side),
                _ => {}
            }
        }
        Some(neighbours[0].0)
    }

    /// Starting grid: the first slot is the start cell itself and the rest sit
    /// on the road behind it, each facing the direction of travel there.
    pub fn grid_slots(&self, count: usize) -> Vec<(Vec3, f32)> {
        let Some(dir) = self.race_dir() else {
            return Vec::new();
        };
        let mut cells = vec![self.start];
        let mut cur = self.start;
        let mut previous = None;
        // Heading we are walking in, i.e. backwards along the track.
        let mut backward = -dir_mq(dir);
        for _ in 1..count {
            let mut best: Option<(f32, i32, i32)> = None;
            for (_, nx, ny) in self.neighbours(cur.0, cur.1) {
                if previous == Some((nx, ny)) {
                    continue;
                }
                let delta = self.center(nx, ny) - self.center(cur.0, cur.1);
                let dot = delta.x * backward.x + delta.z * backward.z;
                if best.is_none_or(|(best_dot, _, _)| dot > best_dot) {
                    best = Some((dot, nx, ny));
                }
            }
            let Some((_, nx, ny)) = best else { break };
            let delta = self.center(nx, ny) - self.center(cur.0, cur.1);
            backward = vec3(delta.x, 0.0, delta.z).normalize_or_zero();
            previous = Some(cur);
            cur = (nx, ny);
            cells.push(cur);
        }

        // Short tracks may not have room for a full grid; stack the surplus
        // on the last cell rather than handing back fewer slots than asked.
        while cells.len() < count {
            let last = *cells.last().unwrap();
            cells.push(last);
        }

        let mut slots = Vec::with_capacity(count);
        let mut stack = 0.0f32;
        for (index, &cell) in cells.iter().enumerate() {
            if index > 0 && cell == cells[index - 1] {
                stack += 1.0;
            } else {
                stack = 0.0;
            }
            let here = self.center(cell.0, cell.1);
            let ahead = if index == 0 {
                here + dir_mq(dir) * TILE
            } else {
                let previous = cells[index - 1];
                self.center(previous.0, previous.1)
            };
            let facing = (ahead - here).normalize_or_zero();
            let yaw = (-facing.x).atan2(-facing.z);
            let side = if index % 2 == 0 { -1.0 } else { 1.0 };
            let lateral = if index == 0 { 0.0 } else { side * 2.5 };
            let offset = vec3(-facing.z, 0.0, facing.x) * lateral - facing * (stack * 3.5);
            slots.push((here + offset + vec3(0.0, 1.2 + stack * 0.3, 0.0), yaw));
        }
        slots
    }

    /// Ordered gate cells for a lap: checkpoint list, if the map has one.
    pub fn gates(&self) -> Vec<(i32, i32)> {
        let mut gates = vec![self.finish];
        gates.extend(self.checkpoints.iter().copied());
        if gates.len() > 1 && gates.last() == gates.first() {
            gates.pop();
        }
        gates
    }
}
