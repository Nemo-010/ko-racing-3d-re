//! Lap, checkpoint and timing state.
//!
//! Gates come from the track: the finish cell followed by the `.map`'s
//! checkpoint list.  A lap is complete when the car has touched every
//! checkpoint and then crossed the finish line again, and the opening crossing
//! - a car starts on or just behind the line - never scores.

use macroquad::prelude::*;

use crate::grid::Grid;

/// Radius of a gate, wide enough to cover both lanes of a two-cell road.
const GATE_RADIUS: f32 = 9.5;

pub struct Race {
    pub laps: u32,
    pub gates: Vec<(i32, i32)>,
    /// Gate the car must reach next; gate 0 is always the finish line.
    pub next: usize,
    pub lap: u32,
    pub lap_times: Vec<f32>,
    pub best: Option<f32>,
    pub finished: bool,
    pub finish_time: Option<f32>,
    started: f64,
    lap_started: f64,
    was_inside: bool,
    suppress_next: bool,
}

impl Race {
    pub fn new(grid: &Grid, laps: u32, position: Vec3, now: f64) -> Race {
        let gates = grid.gates();
        let next = if gates.len() > 1 { 1 } else { 0 };
        let mut race = Race {
            laps: laps.max(1),
            gates,
            next,
            lap: 0,
            lap_times: Vec::new(),
            best: None,
            finished: false,
            finish_time: None,
            started: now,
            lap_started: now,
            was_inside: false,
            suppress_next: false,
        };
        race.was_inside = race.inside(grid, position, next);
        // On a simple circuit (finish == start, no checkpoints) the finish gate
        // is armed from the green light, so a car that starts *behind* the line
        // would score its opening crossing.  Suppress exactly that one.  With
        // checkpoints the finish gate is already gated by the checkpoint order,
        // so nothing needs suppressing.
        race.suppress_next = race.gates.len() == 1 && !race.inside(grid, position, 0);
        race
    }

    fn inside(&self, grid: &Grid, position: Vec3, gate: usize) -> bool {
        let Some(&(x, y)) = self.gates.get(gate) else {
            return false;
        };
        let centre = grid.center(x, y);
        vec2(position.x - centre.x, position.z - centre.z).length() < GATE_RADIUS
    }

    pub fn update(&mut self, now: f64, grid: &Grid, position: Vec3) {
        if self.finished || self.gates.is_empty() {
            return;
        }
        let inside = self.inside(grid, position, self.next);
        if inside && !self.was_inside {
            if self.next == 0 {
                if self.suppress_next {
                    self.suppress_next = false;
                } else {
                    self.complete_lap(now);
                }
                // After the last checkpoint the next target is the finish line.
                self.next = if self.gates.len() > 1 { 1 } else { 0 };
            } else {
                self.next = (self.next + 1) % self.gates.len();
            }
        }
        self.was_inside = self.inside(grid, position, self.next);
    }

    fn complete_lap(&mut self, now: f64) {
        let time = (now - self.lap_started) as f32;
        self.lap_times.push(time);
        if self.best.is_none_or(|best| time < best) {
            self.best = Some(time);
        }
        self.lap += 1;
        self.lap_started = now;
        if self.lap >= self.laps {
            self.finished = true;
            self.finish_time = Some((now - self.started) as f32);
        }
    }

    pub fn lap_time(&self, now: f64) -> f32 {
        (now - self.lap_started) as f32
    }

    pub fn total_time(&self, now: f64) -> f32 {
        (now - self.started) as f32
    }

    /// Race order: laps finished, then gates passed, then closeness to the
    /// next gate.  Higher is further ahead.
    pub fn progress(&self, grid: &Grid, position: Vec3) -> f32 {
        if self.finished {
            return 1_000_000.0 - self.finish_time.unwrap_or(0.0);
        }
        let Some(&(x, y)) = self.gates.get(self.next) else {
            return self.lap as f32;
        };
        let centre = grid.center(x, y);
        let distance = vec2(position.x - centre.x, position.z - centre.z).length();
        self.lap as f32 * self.gates.len() as f32 + self.next as f32 - distance / 1000.0
    }
}
