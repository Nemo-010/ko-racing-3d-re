//! The racing HUD: a speedometer, a minimap, and where everyone else is.
//!
//! The MIDlet's own HUD is a strip along the bottom of the screen.  `r.java`
//! feeds it `(int)(car.j() * 100)` and `(int)(car.j() / car.k() * 100)` - a
//! speed and its fraction of the car's maximum - and the `cg` element draws
//! that fraction as a bar (`fillRect(0, y, width * value / max, 1)`), so the
//! speedometer here is a number and a bar.
//!
//! The minimap follows `bs.a()`: it builds a `width * 3` by `height * 3` image
//! of the track, a three-pixel block per cell with the walls drawn in, and
//! `bs.a(int, int, z)` maps a position onto it.  This draws the same grid and
//! puts every car on it, with a tick along each one's heading.

use macroquad::prelude::*;

use crate::grid::Grid;
use crate::scene::TILE;
use crate::text;

const PANEL: Color = Color::new(0.0, 0.0, 0.0, 0.45);
const ROAD: Color = Color::new(1.0, 1.0, 1.0, 0.65);
const PLAYER: Color = Color::new(1.0, 0.85, 0.2, 1.0);
const RIVAL: Color = Color::new(0.85, 0.90, 0.98, 0.95);

/// The speedometer reads out `speed * SPEED_SCALE`, which puts the port's cars
/// in the range a road car would show.  The bar is the fraction of
/// `TOP_SPEED`, a little above what any of them actually reach.
pub const SPEED_SCALE: f32 = 10.0;
pub const TOP_SPEED: f32 = 14.0;

/// One car to place on the minimap: where it is and which way it faces.
pub struct Marker {
    pub position: Vec3,
    pub heading: Vec3,
    pub player: bool,
}

pub fn draw_speedometer(speed: f32) {
    let width = 200.0;
    let right = screen_width() - 24.0;
    let left = right - width;
    let top = screen_height() - 92.0;

    draw_rectangle(left - 10.0, top - 10.0, width + 20.0, 46.0, PANEL);
    let shown = format!("{}", (speed * SPEED_SCALE).max(0.0) as i32);
    text::draw_shadow(&shown, right - text::width(&shown, 32.0), top - 4.0, 32.0, WHITE);

    let fraction = (speed / TOP_SPEED).clamp(0.0, 1.0);
    draw_rectangle(left, top + 32.0, width, 7.0, Color::new(1.0, 1.0, 1.0, 0.16));
    draw_rectangle(left, top + 32.0, width * fraction, 7.0, PLAYER);
}

/// Where the minimap sits: `(pixels a cell, left, top, cell count)`.  Split out
/// from the drawing so the mapping can be checked without a window.
///
/// `bs.a()`'s three pixels per cell, scaled to fit a corner of the screen.
pub fn minimap_layout(grid: &Grid, screen_height: f32) -> (f32, f32, f32) {
    let longest = grid.width.max(grid.height) as f32;
    let scale = (220.0 / (3.0 * longest)).clamp(0.8, 2.5);
    (3.0 * scale, 24.0, screen_height)
}

/// Screen position of a world point on the minimap.
pub fn minimap_point(grid: &Grid, position: Vec3, cell: f32, left: f32, bottom: f32) -> Vec2 {
    let height = grid.height as f32 * cell;
    let top = bottom - height - 24.0;
    // A cell is TILE across and the map's Y runs the other way from world Z.
    vec2(
        left + position.x / TILE * cell,
        top - position.z / TILE * cell,
    )
}

pub fn draw_minimap(grid: &Grid, markers: &[Marker]) {
    let (cell, left, bottom) = minimap_layout(grid, screen_height());
    let width = grid.width as f32 * cell;
    let height = grid.height as f32 * cell;
    let top = bottom - height - 24.0;

    draw_rectangle(left - 6.0, top - 6.0, width + 12.0, height + 12.0, PANEL);
    for (x, y) in grid.path() {
        draw_rectangle(
            left + x as f32 * cell,
            top + y as f32 * cell,
            cell,
            cell,
            ROAD,
        );
    }

    for marker in markers {
        let point = minimap_point(grid, marker.position, cell, left, bottom);
        let (x, y) = (point.x, point.y);
        let colour = if marker.player { PLAYER } else { RIVAL };
        let size = if marker.player { 5.0 } else { 3.5 };
        draw_rectangle(x - size * 0.5, y - size * 0.5, size, size, colour);
        // The tick along the heading is what tells a rival from the pack.
        let heading = vec2(marker.heading.x, -marker.heading.z);
        draw_line(
            x,
            y,
            x + heading.x * cell * 2.0,
            y + heading.y * cell * 2.0,
            if marker.player { 2.0 } else { 1.0 },
            colour,
        );
    }
}
