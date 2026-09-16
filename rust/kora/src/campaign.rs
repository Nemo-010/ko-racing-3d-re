//! Career tables: which campaign a track belongs to and how its race is set up.
//!
//! `<campaign>.000` lists the levels and the races, and each race record names
//! a game mode plus the byte offset of its setup inside `<campaign>.001`.
//! Level coordinates are read as 16-bit values even though they look like
//! bytes - `u.c()` calls an overloaded `be.a()` that returns a `short`, which
//! is invisible in the decompiled source because the overloads differ only by
//! return type.

use crate::format::{Campaign, RaceConfig};
use crate::pack::Resources;

/// The two career tables shipped in the pack.
const TABLES: [&str; 2] = ["campaign/campaign", "campaign/deluxe"];

pub struct RaceSetup {
    /// Resource base name, e.g. `campaign/campaign`.
    pub table: String,
    pub level_index: usize,
    pub level_name: String,
    pub config: RaceConfig,
}

impl RaceSetup {
    /// `(laps, opponents)` as the campaign intends them.  A solo time trial
    /// reports no opponents, which the caller may choose to top up.
    pub fn laps_and_opponents(&self) -> (u32, u32) {
        (self.config.laps, self.config.opponents)
    }
}

/// Parse every career table in the pack.
pub fn load(resources: &Resources) -> Vec<(String, Campaign)> {
    let mut tables = Vec::new();
    for base in TABLES {
        let index = format!("{base}.000");
        if let Some(bytes) = resources.get(&index) {
            if let Some(campaign) = Campaign::parse(bytes) {
                tables.push((base.to_string(), campaign));
            }
        }
    }
    tables
}

/// Find the race that drives `map_name`, preferring a mode that races
/// opponents over a solo time trial.
pub fn race_for(resources: &Resources, map_name: &str) -> Option<RaceSetup> {
    for (table, campaign) in load(resources) {
        let blob = resources.get(&format!("{table}.001"))?;
        let Some(level_index) = campaign
            .levels
            .iter()
            .position(|level| level.map == map_name)
        else {
            continue;
        };

        let mut records: Vec<_> = campaign
            .records
            .iter()
            .filter(|record| record.level as usize == level_index)
            .collect();
        // Race modes first, then the lowest mode number, so the choice is
        // stable rather than dependent on the table's row order.
        records.sort_by_key(|record| {
            let is_race = RaceConfig::RACE_MODES.contains(&record.mode);
            (!is_race, record.mode)
        });

        for record in records {
            let offset = record.values[2].max(0) as usize;
            if let Some(config) = RaceConfig::parse(blob, offset, record.mode) {
                return Some(RaceSetup {
                    table: table.clone(),
                    level_index,
                    level_name: campaign.levels[level_index].name.clone(),
                    config,
                });
            }
        }
    }
    None
}
