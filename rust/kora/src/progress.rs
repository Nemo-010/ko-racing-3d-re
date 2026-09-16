//! Career progress: points and best times.
//!
//! The MIDlet keeps two things per race.  A running total of points gates which
//! races are offered (`u.c()` compares a record's first value against it and
//! `u.a(r)` adds the second), and each race keeps a **best time**: the level
//! table initialises its stored value to `Integer.MAX_VALUE` for "no record
//! yet", and `r.c(int)` returns whichever of the new time and the old one is
//! better.
//!
//! There are no medals.  The game has no medal, gold, silver or bronze anywhere
//! in its text; what it shows is the best time and the best lap.  An earlier
//! version of this port invented medals to grade a finish, and that is gone.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// The eight cars `ba.a` offers, in the order the game lists them.
pub const CARS: [&str; 8] = [
    "rally.car",
    "fashion.car",
    "vintage.car",
    "sport.car",
    "bonus.car",
    "suv.car",
    "cx.car",
    "cool.car",
];

/// What a finished race did to the save.
pub struct Finish {
    /// Points paid, which is zero unless this was the first pass.
    pub gained: u32,
    /// The record before the race, if there was one.
    pub previous_best: Option<f32>,
    pub best_time: f32,
    /// Whether this run set a new record.
    pub improved: bool,
}

pub struct Progress {
    pub points: u32,
    /// Races whose award has already been paid, so it is paid once.
    passed: HashSet<String>,
    /// Best finish time per race, keyed by `table:level:mode`.
    times: HashMap<String, f32>,
    /// Index into [`CARS`].
    pub car: usize,
}

impl Default for Progress {
    fn default() -> Self {
        Progress {
            points: 0,
            passed: HashSet::new(),
            times: HashMap::new(),
            car: 0,
        }
    }
}

impl Progress {
    pub fn load(path: &Path) -> Progress {
        let Ok(text) = fs::read_to_string(path) else {
            return Progress::default();
        };
        let mut progress = Progress::default();
        for line in text.lines() {
            if let Some(value) = line.strip_prefix("points ") {
                progress.points = value.trim().parse().unwrap_or(0);
            } else if let Some(value) = line.strip_prefix("car ") {
                progress.car = value.trim().parse::<usize>().unwrap_or(0).min(CARS.len() - 1);
            } else if let Some(key) = line.strip_prefix("passed ") {
                progress.passed.insert(key.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("time ") {
                if let Some((key, value)) = rest.rsplit_once(' ') {
                    if let Ok(seconds) = value.trim().parse::<f32>() {
                        progress.times.insert(key.to_string(), seconds);
                    }
                }
            }
        }
        progress
    }

    pub fn save(&self, path: &Path) {
        let mut text = format!("points {}\ncar {}\n", self.points, self.car);
        for key in &self.passed {
            text.push_str(&format!("passed {key}\n"));
        }
        for (key, seconds) in &self.times {
            text.push_str(&format!("time {key} {seconds:.3}\n"));
        }
        if let Err(error) = fs::write(path, text) {
            eprintln!("could not save progress to {}: {error}", path.display());
        }
    }

    /// The stored best time for a race, if it has been finished.
    pub fn best_time(&self, key: &str) -> Option<f32> {
        self.times.get(key).copied()
    }

    /// Whether a race with this entry threshold is open yet.  The lowest
    /// threshold in the tables is 1, and the first group of levels is
    /// unlocked from the start, so a threshold of 1 is open at zero points.
    pub fn open(&self, threshold: i32) -> bool {
        self.points as i32 + 1 >= threshold
    }

    /// Record a finish.
    ///
    /// The record's own award is paid once, the first time a race is passed,
    /// which is what the MIDlet does - so re-driving a race cannot be farmed.
    /// The time is kept whenever it beats the stored one, win or lose.  A
    /// record whose value is not an award (the bonus races, which unlock a
    /// group) pays nothing.
    pub fn record(&mut self, key: &str, time: f32, award: i32) -> Finish {
        let previous_best = self.times.get(key).copied();
        let improved = match previous_best {
            Some(best) => time > 0.0 && time < best,
            None => time > 0.0,
        };
        if improved {
            self.times.insert(key.to_string(), time);
        }

        let mut gained = 0;
        if !self.passed.contains(key) && award > 0 && time > 0.0 {
            self.passed.insert(key.to_string());
            self.points += award as u32;
            gained = award as u32;
        }

        Finish {
            gained,
            previous_best,
            best_time: self.times.get(key).copied().unwrap_or(time),
            improved,
        }
    }
}

/// A selectable car: its file, display name and the four stat values the
/// setup screen draws bars for.
pub struct CarInfo {
    pub file: String,
    pub name: String,
    pub stats: [u8; 4],
}

/// Build the car list the way `ba.a` orders it, reading each `.car` file.
pub fn car_infos(resources: &crate::pack::Resources) -> Vec<CarInfo> {
    CARS.iter()
        .filter_map(|file| {
            let car = crate::format::Car::parse(resources.get(&format!("cars/{file}"))?)?;
            Some(CarInfo {
                file: (*file).to_string(),
                name: car.name,
                stats: car.stats,
            })
        })
        .collect()
}
