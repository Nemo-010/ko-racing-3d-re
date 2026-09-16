//! User interface text, kept in `labels.tsv` rather than in the code.
//!
//! The table is embedded with `include_str!`, so the binary stays
//! self-contained, and parsed once on first use.  The third column of the file
//! records where each string came from: a number is the id the game itself
//! uses in `/ui/ui.txt` (which `aq.a(id)` hands to the font), and `-` marks a
//! string the original has no id for, added for the screens and features the
//! port has and the original does not.
//!
//! `{}` in a text is a placeholder, filled in order by [`format`].

use std::collections::HashMap;
use std::sync::LazyLock;

const SOURCE: &str = include_str!("../labels.tsv");

static LABELS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut table = HashMap::new();
    for line in SOURCE.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut columns = line.splitn(3, '\t');
        if let (Some(key), Some(text)) = (columns.next(), columns.next()) {
            table.insert(key, text);
        }
    }
    table
});

/// The text for `key`.
///
/// Unknown keys return the key itself, so a typo shows up on the screen and in
/// the tests rather than panicking from inside a frame.  Keys are `&'static`
/// so that fallback can be returned directly.
pub fn get(key: &'static str) -> &'static str {
    LABELS.get(key).copied().unwrap_or(key)
}

/// Look `key` up and replace each `{}`, in order, with the next argument.
pub fn format(key: &'static str, args: &[&str]) -> String {
    let mut text = get(key).to_string();
    for arg in args {
        match text.find("{}") {
            Some(at) => text.replace_range(at..at + 2, arg),
            None => break,
        }
    }
    text
}

/// The four values a `.car` file carries, in the order `ba.a(car, stat)` uses
/// them and the order `aq.a(127 + stat)` labels them.
pub const STAT_KEYS: [&str; 4] = [
    "stat_speed",
    "stat_acceleration",
    "stat_braking",
    "stat_handling",
];

pub fn stat_name(stat: usize) -> &'static str {
    get(STAT_KEYS.get(stat).copied().unwrap_or(STAT_KEYS[0]))
}

/// The seven game modes, in the order of the mode byte in a `.000` record.
pub const MODE_KEYS: [&str; 7] = [
    "mode_circuit",
    "mode_race",
    "mode_time_chase",
    "mode_survival",
    "mode_head_to_head",
    "mode_slideshow",
    "mode_special",
];

pub fn mode_name(mode: u8) -> &'static str {
    get(MODE_KEYS.get(mode as usize).copied().unwrap_or(MODE_KEYS[6]))
}
