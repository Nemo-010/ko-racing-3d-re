//! Career progress: points, medals and the selected car.
//!
//! The `.000` tables give each race an entry threshold (`values[0]`, which the
//! MIDlet compares against the player's score) and an award (`values[1]`, added
//! to it on a win).  The port keeps the same two numbers and hangs medals off
//! the finishing position, which is the part the tables do not spell out:
//! first place is gold, second silver, third bronze, and a better medal on a
//! race you have already won only pays the difference so a race cannot be
//! farmed.

use std::collections::HashMap;
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

/// Highest value a stat can be raised to.  Six is what the game's own best
/// car, SPIRIT 320, carries, so it is the ceiling the original implies.
pub const MAX_STAT: u8 = 6;

/// The four values a `.car` file carries are drawn in the garage as
/// speed, grip, accel and brakes.  Higher is better in all four, which is what
/// makes an upgrade unambiguous; the labels are the port's, because the
/// original's own are packed image blobs.
pub const STAT_NAMES: [&str; 4] = ["SPEED", "GRIP", "ACCEL", "BRAKES"];

/// Career points needed to raise a stat from `value` to `value + 1`.
pub const fn upgrade_cost(value: u8) -> u32 {
    2 * value as u32
}

pub const GOLD: u8 = 3;
pub const SILVER: u8 = 2;
pub const BRONZE: u8 = 1;

pub fn medal_name(medal: u8) -> &'static str {
    match medal {
        GOLD => "GOLD",
        SILVER => "SILVER",
        BRONZE => "BRONZE",
        _ => "-",
    }
}

/// Medal for a finishing position (0-based), or 0 for no medal.
pub fn medal_for_place(place: usize) -> u8 {
    match place {
        0 => GOLD,
        1 => SILVER,
        2 => BRONZE,
        _ => 0,
    }
}

pub struct Progress {
    pub points: u32,
    /// Best medal per race, keyed by `table:level:mode`.
    pub medals: HashMap<String, u8>,
    /// Purchased stat increases per car file, one entry per stat.
    pub upgrades: HashMap<String, [u8; 4]>,
    /// Index into [`CARS`].
    pub car: usize,
}

impl Default for Progress {
    fn default() -> Self {
        Progress {
            points: 0,
            medals: HashMap::new(),
            upgrades: HashMap::new(),
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
            } else if let Some(rest) = line.strip_prefix("upgrade ") {
                let mut parts = rest.split_whitespace();
                if let (Some(file), Some(a), Some(b), Some(c), Some(d)) = (
                    parts.next(),
                    parts.next(),
                    parts.next(),
                    parts.next(),
                    parts.next(),
                ) {
                    let values = [a, b, c, d].map(|value| value.parse().unwrap_or(0));
                    progress.upgrades.insert(file.to_string(), values);
                }
            } else if let Some(rest) = line.strip_prefix("medal ") {
                if let Some((key, value)) = rest.split_once(' ') {
                    if let Ok(medal) = value.trim().parse::<u8>() {
                        progress.medals.insert(key.to_string(), medal);
                    }
                }
            }
        }
        progress
    }

    pub fn save(&self, path: &Path) {
        let mut text = format!("points {}\ncar {}\n", self.points, self.car);
        for (key, medal) in &self.medals {
            text.push_str(&format!("medal {key} {medal}\n"));
        }
        for (file, bought) in &self.upgrades {
            text.push_str(&format!(
                "upgrade {file} {} {} {} {}\n",
                bought[0], bought[1], bought[2], bought[3]
            ));
        }
        if let Err(error) = fs::write(path, text) {
            eprintln!("could not save progress to {}: {error}", path.display());
        }
    }

    pub fn best(&self, key: &str) -> u8 {
        self.medals.get(key).copied().unwrap_or(0)
    }

    /// Whether a race with this entry threshold is open yet.  The lowest
    /// threshold in the tables is 1, and the first group of levels is
    /// unlocked from the start, so a threshold of 1 is open at zero points.
    pub fn open(&self, threshold: i32) -> bool {
        self.points as i32 + 1 >= threshold
    }

    /// The four values a car actually has, with anything bought in the garage.
    pub fn stats(&self, file: &str, base: [u8; 4]) -> [u8; 4] {
        let bought = self.upgrades.get(file).copied().unwrap_or([0; 4]);
        let mut stats = base;
        for (stat, added) in stats.iter_mut().zip(bought) {
            *stat = (*stat + added).min(MAX_STAT);
        }
        stats
    }

    /// What the next point in `stat` would cost, or `None` at the ceiling.
    pub fn next_upgrade_cost(&self, file: &str, base: [u8; 4], stat: usize) -> Option<u32> {
        let current = self.stats(file, base)[stat];
        if current >= MAX_STAT {
            None
        } else {
            Some(upgrade_cost(current))
        }
    }

    /// Spend career points on one stat.  Returns the cost, or `None` if the
    /// stat is maxed or the points are not there.
    pub fn buy_upgrade(&mut self, file: &str, base: [u8; 4], stat: usize) -> Option<u32> {
        let cost = self.next_upgrade_cost(file, base, stat)?;
        if self.points < cost {
            return None;
        }
        self.points -= cost;
        self.upgrades.entry(file.to_string()).or_insert([0; 4])[stat] += 1;
        Some(cost)
    }

    /// Record a finish.  Returns the medal won and the points it paid.
    pub fn record(&mut self, key: &str, place: usize, award: i32) -> (u8, u32) {
        let medal = medal_for_place(place);
        let previous = self.best(key);
        if medal <= previous {
            return (medal, 0);
        }
        self.medals.insert(key.to_string(), medal);
        let gained = (award.max(1) as u32) * (medal - previous) as u32;
        self.points += gained;
        (medal, gained)
    }
}

/// A selectable car: its file, display name and the four stat values the
/// garage draws bars for.
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
