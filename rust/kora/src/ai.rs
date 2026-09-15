//! Opponent driving: follow the road graph without a precomputed racing line.
//!
//! The AI asks the grid for a point half a cell ahead along the best-aligned
//! drivable side, steers at it and slows down for the corner it is about to
//! take.  Because the target always comes from the tile connectivity, the cars
//! stay on the road through junctions and lane changes.

use macroquad::prelude::*;

use crate::grid::Grid;
use crate::physics::CarControl;

/// One AI car's state, including a stuck detector so a car pinned against a
/// barrier reverses out instead of grinding there for the rest of the race.
pub struct AiDriver {
    skill: f32,
    stuck: f32,
    reversing: f32,
    steer: f32,
}

impl AiDriver {
    pub fn new(skill: f32) -> AiDriver {
        AiDriver {
            skill,
            stuck: 0.0,
            reversing: 0.0,
            steer: 0.0,
        }
    }

    pub fn control(
        &mut self,
        grid: &Grid,
        position: Vec3,
        heading: Vec3,
        speed: f32,
        dt: f32,
    ) -> CarControl {
        if self.reversing > 0.0 {
            self.reversing -= dt;
            return CarControl {
                throttle: -0.7,
                steer: -self.steer,
                brake: false,
            };
        }
        let control = drive(grid, position, heading, speed, self.skill);
        self.steer = control.steer;
        if speed < 0.6 && control.throttle > 0.0 {
            self.stuck += dt;
        } else {
            self.stuck = 0.0;
        }
        if self.stuck > 1.5 {
            self.stuck = 0.0;
            self.reversing = 0.9;
        }
        control
    }
}

/// Top speed an AI car aims for on a straight, before its skill factor.
const STRAIGHT_SPEED: f32 = 11.0;

pub fn drive(grid: &Grid, position: Vec3, heading: Vec3, speed: f32, skill: f32) -> CarControl {
    let mut control = CarControl::default();
    let Some(target) = grid.target(position, heading) else {
        // Off the road entirely: creep forward so it can recover.
        control.throttle = 0.35;
        return control;
    };

    let to = vec3(target.x - position.x, 0.0, target.z - position.z);
    let distance = to.length();
    if distance < 0.05 {
        control.throttle = 0.6;
        return control;
    }
    let direction = to / distance;
    let forward = heading.normalize_or_zero();
    // Positive cross product means the target is to the left, which is what a
    // positive steering value does (rotation about +Y).
    let cross = forward.cross(direction).y;
    let dot = forward.x * direction.x + forward.z * direction.z;
    let angle = cross.atan2(dot);

    control.steer = (angle * 2.4).clamp(-1.0, 1.0);

    // Brake into the corner, and look one cell further ahead so the car slows
    // before the turn rather than in the middle of it.
    let mut corner = angle.abs();
    if let Some(next) = grid.target(target + direction * 1.5, direction) {
        let next_dir = vec3(next.x - position.x, 0.0, next.z - position.z);
        if next_dir.length_squared() > 1e-6 {
            let next_dir = next_dir.normalize();
            let further = next_dir.cross(forward).y.atan2(next_dir.dot(forward)).abs();
            corner = corner.max(further);
        }
    }

    let target_speed = STRAIGHT_SPEED * skill * (1.0 - 0.55 * (corner / 1.6).clamp(0.0, 1.0));
    if speed > target_speed * 1.12 {
        control.brake = true;
        control.throttle = 0.0;
    } else if speed > target_speed {
        control.throttle = 0.0;
    } else {
        control.throttle = 1.0;
    }
    control
}
