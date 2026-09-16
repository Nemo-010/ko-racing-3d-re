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

/// One selectable race: a campaign record plus its decoded setup.
#[derive(Clone)]
pub struct RaceEvent {
    pub table: String,
    pub level_index: usize,
    pub name: String,
    pub map: String,
    pub mode: u8,
    pub laps: u32,
    pub opponents: u32,
    pub theme: u8,
    pub threshold: i32,
    /// Points the race is worth.  The MIDlet adds `record[3]` to the player's
    /// total on a win, but a *negative* value is not an award at all: `u.n()`
    /// negates it into a group index and sets that group's unlocked flag
    /// instead, which is how the bonus races hand over tracks and cars.  So
    /// this is the value floored at zero, and [`Self::unlocks`] carries the
    /// group when there is one.
    pub award: i32,
    pub unlocks: Option<u8>,
    /// Stable key for saving a medal against this race.
    pub key: String,
}

impl RaceEvent {
    fn build(table: &str, level_index: usize, name: &str, map: &str, record: &crate::format::RaceRecord, config: RaceConfig) -> RaceEvent {
        RaceEvent {
            table: table.to_string(),
            level_index,
            name: name.to_string(),
            map: map.to_string(),
            mode: record.mode,
            laps: config.laps,
            opponents: if config.is_race() { config.opponents } else { 3 },
            theme: config.theme,
            threshold: record.values[0],
            award: record.values[1].max(0),
            unlocks: (record.values[1] < 0).then(|| (-record.values[1]) as u8),
            key: format!("{table}:{level_index}:{}", record.mode),
        }
    }
}

/// Every race in both career tables, in table order, races before time trials.
pub fn events(resources: &Resources) -> Vec<RaceEvent> {
    let mut events = Vec::new();
    for (table, campaign) in load(resources) {
        let Some(blob) = resources.get(&format!("{table}.001")) else {
            continue;
        };
        for (level_index, level) in campaign.levels.iter().enumerate() {
            let mut records: Vec<_> = campaign
                .records
                .iter()
                .filter(|record| record.level as usize == level_index)
                .collect();
            records.sort_by_key(|record| {
                let is_race = RaceConfig::RACE_MODES.contains(&record.mode);
                (!is_race, record.mode)
            });
            for record in records {
                let offset = record.values[2].max(0) as usize;
                let Some(config) = RaceConfig::parse(blob, offset, record.mode) else {
                    continue;
                };
                events.push(RaceEvent::build(
                    &table,
                    level_index,
                    &level.name,
                    &level.map,
                    record,
                    config,
                ));
            }
        }
    }
    events
}

/// Every track in the pack, for a quick race: the campaign setup where the
/// track has one, otherwise three laps against three opponents.
pub fn quick_events(resources: &Resources) -> Vec<RaceEvent> {
    let mut maps: Vec<String> = resources
        .keys()
        .filter(|name| name.starts_with("levels/") && name.ends_with(".map"))
        .map(|name| name.trim_start_matches("levels/").to_string())
        .collect();
    maps.sort();

    maps.into_iter()
        .map(|map| {
            let setup = race_for(resources, &map);
            let (laps, opponents, theme, mode) = match &setup {
                Some(setup) => (
                    setup.config.laps,
                    if setup.config.is_race() {
                        setup.config.opponents
                    } else {
                        3
                    },
                    setup.config.theme,
                    setup.config.mode,
                ),
                None => (3, 3, 0, 0),
            };
            RaceEvent {
                table: setup
                    .as_ref()
                    .map(|setup| setup.table.clone())
                    .unwrap_or_default(),
                level_index: setup.as_ref().map(|setup| setup.level_index).unwrap_or(0),
                name: map.trim_end_matches(".map").to_uppercase(),
                key: format!("quick:{map}:{mode}"),
                map,
                mode,
                laps,
                opponents,
                theme,
                threshold: 0,
                award: 1,
                unlocks: None,
            }
        })
        .collect()
}
