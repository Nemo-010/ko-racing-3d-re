//! The front end's backdrop: space, with the Earth in it.
//!
//! `bd` puts a black M3G `Background` behind a billboard of `/tex/ea.jpg` -
//! the Earth photographed from orbit - and tilts it -45 degrees about X before
//! spinning it about Z, so the menu shows the planet from above.  `ce` is the
//! starfield in front of it: stars that drift in brightness and are projected
//! outward from the middle of the screen by a "warp" factor.  Choosing an entry
//! ramps the billboard's scale from 1.2 up past 15 and drags the stars along
//! with it, and `/tex/ms.jpg` - the same picture, radially blurred - takes over
//! partway.  That is the jump the map arrives out of.
//!
//! The camera seeing the billboard is the game's own: `bq.a` sets the M3G
//! perspective to a 90 degree vertical field of view, so a 2x2 quad 2.5 units
//! away covers `2 / (2 * 2.5)` of the screen height and the tilt flattens that
//! to 0.71 of it.  The port applies that projection itself rather than standing
//! up a 3D camera, because the stars have to stay behind the planet and
//! macroquad's 3D pass clears the frame it draws into.

use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;
use macroquad::texture::Texture2D;

use crate::pack::Resources;

/// `bq.a`: the MIDlet renders through a 90 degree vertical field of view.
const FOVY: f32 = std::f32::consts::FRAC_PI_2;
/// `bd`: how far the billboard is from the camera.
const DISTANCE: f32 = 2.5;
/// `bd`: the tilt that turns the billboard into a planet seen from above.
const TILT: f32 = std::f32::consts::FRAC_PI_4;
/// `bd.a()`: the billboard's resting scale, and where the jump starts.
pub const SCALE: f32 = 1.2;
/// `bd.b(float)`: the scale the jump runs to before the map takes over.
pub const SCALE_LIMIT: f32 = 15.2;
/// `bd`: the spin, in degrees a second.
const SPIN: f32 = 20.0;
/// Facets per axis the billboard is projected with, so the perspective across a
/// tilted quad comes out right instead of as one flat trapezoid.
const FACETS: usize = 8;
/// How close to the camera a projected point may come before it is held there.
const NEAR_PLANE: f32 = 0.05;

/// One of `ce`'s stars: a fixed position whose brightness and size drift.
struct Star {
    x: f32,
    y: f32,
    grey: f32,
    speed: f32,
    life: f32,
    width: f32,
    height: f32,
}

impl Star {
    fn new(width: f32, height: f32) -> Star {
        Star {
            x: rand::gen_range(0.0, width),
            y: rand::gen_range(0.0, height),
            grey: rand::gen_range(0.0, 255.0),
            speed: 0.0,
            life: 0.0,
            // `ce`: a star is one or two pixels across in each direction.
            width: 1.0 + (rand::gen_range(0, 18) / 17) as f32,
            height: 1.0 + (rand::gen_range(0, 18) / 17) as f32,
        }
    }
}

pub struct Space {
    stars: Vec<Star>,
    earth: Option<Texture2D>,
    blur: Option<Texture2D>,
    /// Spin about the planet's axis, in radians.
    spin: f32,
    /// How hard the stars are dragged outward: zero while a menu is up.
    warp: f32,
    /// Billboard scale, which the jump ramps up.
    scale: f32,
}

impl Space {
    /// The star count is a density over the screen (`ce`), so the size has to
    /// be known up front; it is passed in rather than measured so this works
    /// without a graphics context.
    pub fn new(resources: &Resources, width: f32, height: f32) -> Space {
        let count = ((width * height) * 0.001) as usize;
        Space {
            stars: (0..count).map(|_| Star::new(width, height)).collect(),
            earth: load_texture(resources, "tex/ea.jpg"),
            blur: load_texture(resources, "tex/ms.jpg"),
            spin: 0.0,
            warp: 0.0,
            scale: SCALE,
        }
    }

    /// `ce.a(float, float)` and `bd.a(float)`: the stars twinkle, the planet
    /// turns, and `warp` and `scale` are whatever the screen above wants them
    /// to be - zero and resting for a menu, ramping for a jump.
    pub fn update(&mut self, dt: f32, warp: f32, scale: f32) {
        self.spin = (self.spin + SPIN.to_radians() * dt) % std::f32::consts::TAU;
        self.warp = warp;
        self.scale = scale;
        for star in &mut self.stars {
            if star.life <= 0.0 {
                star.speed = 10.0 - rand::gen_range(0, 22) as f32;
                star.life = (0.2 + rand::gen_range(0.0, 1.0)) / 2.0;
            }
            star.grey = (star.grey + 20.0 * dt * star.speed).clamp(0.0, 255.0);
            star.life -= dt;
        }
    }

    pub fn draw(&self) {
        clear_background(BLACK);
        let centre = vec2(screen_width() / 2.0, screen_height() / 2.0);
        for star in &self.stars {
            // `ce.a(Graphics)`: stars are pushed away from the middle of the
            // screen, which is what the jump looks like.
            let x = star.x + self.warp * (star.x - centre.x);
            let y = star.y + self.warp * (star.y - centre.y);
            let grey = star.grey;
            draw_rectangle(
                x,
                y,
                star.width,
                star.height,
                Color::new(
                    grey / 255.0,
                    grey / 255.0,
                    (grey - 80.0).max(0.0) / 255.0,
                    1.0,
                ),
            );
        }

        // Past the halfway point the blurred plate stands in for the planet,
        // the way `bd` swaps billboards.
        let blurred = self.scale > SCALE_LIMIT * 0.4;
        let Some(texture) = (if blurred { &self.blur } else { &self.earth }) else {
            return;
        };
        let screen = vec2(screen_width(), screen_height());
        draw_mesh(&billboard(texture, self.scale, self.spin, centre, screen));
    }
}

/// Where a point of the billboard lands on the screen: scaled, spun about Z,
/// tilted 45 degrees about X, and projected through the game's perspective.
///
/// `bd` runs the scale up to `SCALE_LIMIT`, by which point the quad's near edge
/// has passed *behind* the camera - which is what flying into the planet is
/// meant to look like, but a projection cannot express it: the point would
/// mirror through the middle of the view.  The depth is therefore held just in
/// front of the camera, so the last few frames fill the frame instead of
/// turning it inside out.
fn project(local: Vec2, scale: f32, spin: f32, centre: Vec2, screen: Vec2) -> Vec2 {
    let (sin_tilt, cos_tilt) = TILT.sin_cos();
    let half_height = screen.y / 2.0 / (FOVY / 2.0).tan();
    let (sin, cos) = spin.sin_cos();
    let (x, y) = (local.x * scale, local.y * scale);
    let (spun_x, spun_y) = (x * cos - y * sin, x * sin + y * cos);
    let depth = (DISTANCE + spun_y * sin_tilt).max(NEAR_PLANE);
    let perspective = half_height / depth;
    vec2(
        centre.x + spun_x * perspective,
        centre.y - spun_y * cos_tilt * perspective,
    )
}

/// The half extents in pixels of the planet's projection with no spin, which is
/// how much of the view it covers.
pub fn planet_reach(scale: f32, screen: Vec2) -> Vec2 {
    let centre = screen / 2.0;
    let corner = |x: f32, y: f32| project(vec2(x, y), scale, 0.0, centre, screen);
    let far = corner(1.0, 1.0);
    let near = corner(-1.0, -1.0);
    vec2(
        (far.x - centre.x).abs().max((near.x - centre.x).abs()),
        (far.y - centre.y).abs().max((near.y - centre.y).abs()),
    )
}

/// The planet, drawn the way M3G would: a 2x2 quad scaled, spun about Z,
/// tilted 45 degrees about X and projected through the game's perspective.
fn billboard(texture: &Texture2D, scale: f32, spin: f32, centre: Vec2, screen: Vec2) -> Mesh {
    let mut vertices = Vec::with_capacity((FACETS + 1) * (FACETS + 1));
    for j in 0..=FACETS {
        for i in 0..=FACETS {
            let u = i as f32 / FACETS as f32;
            let v = j as f32 / FACETS as f32;
            // The quad is two units across and centred on its origin; v = 0 is
            // the top of the picture, which is +y here.
            let local = vec2(u - 0.5, 0.5 - v) * 2.0;
            let at = project(local, scale, spin, centre, screen);
            vertices.push(Vertex::new2(vec3(at.x, at.y, 0.0), vec2(u, v), WHITE));
        }
    }

    let mut indices = Vec::with_capacity(FACETS * FACETS * 6);
    let stride = FACETS as u16 + 1;
    for j in 0..FACETS as u16 {
        for i in 0..FACETS as u16 {
            let corner = j * stride + i;
            indices.extend_from_slice(&[
                corner,
                corner + 1,
                corner + stride,
                corner + 1,
                corner + stride + 1,
                corner + stride,
            ]);
        }
    }

    Mesh {
        vertices,
        indices,
        texture: Some(texture.clone()),
    }
}

/// The MIDlet loads these by name through `bl.a`, and both are JPEGs.  The
/// format is left to be sniffed, the way `sky` loads its `.jpg` strips.
fn load_texture(resources: &Resources, path: &str) -> Option<Texture2D> {
    let texture = Texture2D::from_file_with_format(resources.get(path)?, None);
    texture.set_filter(FilterMode::Linear);
    Some(texture)
}
