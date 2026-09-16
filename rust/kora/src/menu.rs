//! Screens: the main menu, the career and quick-race lists, car selection, the
//! pause menu and the results panel.
//!
//! These are the port's own layout - the MIDlet draws its menus from packed
//! images and text blobs - but every number on them comes from the game's
//! tables: the campaign's levels, entry thresholds and awards, and each car's
//! four stat values from its `.car` file.

use macroquad::prelude::*;

use crate::campaign::RaceEvent;
use crate::labels;
use crate::progress::{CarInfo, Progress};
use crate::text;

/// The main menu, shared with `main` so the cursor and the labels agree.
/// These are label keys rather than text.
pub const MAIN_ITEMS: [&str; 5] = [
    "menu_career",
    "menu_deluxe",
    "menu_quick",
    "menu_cars",
    "menu_quit",
];

pub const BACKDROP: Color = Color::new(0.05, 0.07, 0.12, 1.0);
const PANEL: Color = Color::new(1.0, 1.0, 1.0, 0.06);
const HIGHLIGHT: Color = Color::new(0.20, 0.45, 0.85, 0.85);
const LOCKED: Color = Color::new(0.45, 0.45, 0.50, 1.0);
const ACCENT: Color = Color::new(1.0, 0.85, 0.2, 1.0);

/// A panel behind a menu so the text stays readable over the sky.
fn panel(rect: Rect) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, PANEL);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, Color::new(1.0, 1.0, 1.0, 0.15));
}

fn centred(label: &str, y: f32, size: f32, color: Color) {
    let width = text::width(label, size);
    text::draw_shadow(label, (screen_width() - width) / 2.0, y, size, color);
}

pub fn draw_main(progress: &Progress, cursor: usize) {
    centred(labels::get("kora"), screen_height() * 0.16, 54.0, WHITE);
    centred(labels::get("port"), screen_height() * 0.16 + 62.0, 22.0, ACCENT);
    centred(
        &labels::format("career_points", &[&progress.points.to_string()]),
        screen_height() * 0.16 + 92.0,
        19.0,
        LOCKED,
    );

    let items = MAIN_ITEMS;
    let row = 34.0;
    let top = screen_height() * 0.42;
    let width = 240.0;
    let left = (screen_width() - width) / 2.0;
    panel(Rect::new(left - 12.0, top - 14.0, width + 24.0, items.len() as f32 * row + 20.0));
    for (index, item) in items.iter().enumerate() {
        let y = top + index as f32 * row;
        if index == cursor {
            draw_rectangle(left - 6.0, y - 4.0, width + 12.0, row - 4.0, HIGHLIGHT);
        }
        text::draw_shadow(labels::get(item), left + 16.0, y, 24.0, WHITE);
    }
    centred(labels::get("keys_menu"), screen_height() - 22.0, 17.0, LOCKED);
}

/// A scrolling list of races, showing the stored best time and greying out
/// the ones the player's points have not reached.
pub fn draw_events(progress: &Progress, events: &[RaceEvent], cursor: usize, title: &str) {
    centred(title, 12.0, 32.0, WHITE);
    let row = 26.0;
    let top = 56.0;
    let visible = ((screen_height() - top - 40.0) / row) as usize;
    let first = cursor.saturating_sub(visible.saturating_sub(1) / 2).min(events.len().saturating_sub(visible));

    for (offset, event) in events.iter().skip(first).take(visible).enumerate() {
        let index = first + offset;
        let y = top + offset as f32 * row;
        let open = progress.open(event.threshold);
        if index == cursor {
            draw_rectangle(24.0, y - 3.0, screen_width() - 48.0, row - 3.0, HIGHLIGHT);
        }
        let color = if open { WHITE } else { LOCKED };
        text::draw_shadow(&event.name, 32.0, y, 20.0, color);
        text::draw_shadow(&event.map, 260.0, y, 20.0, color);
        text::draw_shadow(labels::mode_name(event.mode), 370.0, y, 20.0, LOCKED);
        text::draw_shadow(&event.laps.to_string(), 500.0, y, 20.0, color);
        text::draw_shadow(&event.opponents.to_string(), 545.0, y, 20.0, color);
        if open {
            let best = progress
                .best_time(&event.key)
                .map(format_time)
                .unwrap_or_else(|| labels::get("no_time").to_string());
            text::draw_shadow(&best, 590.0, y, 20.0, ACCENT);
        } else {
            text::draw_shadow(
                &labels::format("need_points", &[&event.threshold.to_string()]),
                590.0,
                y,
                20.0,
                LOCKED,
            );
        }
    }

    text::draw_shadow(labels::get("col_mode"), 370.0, top - 22.0, 17.0, LOCKED);
    text::draw_shadow(labels::get("col_laps"), 500.0, top - 22.0, 17.0, LOCKED);
    text::draw_shadow(labels::get("col_cpu"), 545.0, top - 22.0, 17.0, LOCKED);
    text::draw_shadow(labels::get("col_best"), 590.0, top - 22.0, 17.0, LOCKED);
    text::draw_shadow(
        &labels::format(
            "events_count",
            &[
                &(cursor.min(events.len().saturating_sub(1)) + 1).to_string(),
                &events.len().to_string(),
                &progress.points.to_string(),
            ],
        ),
        32.0,
        screen_height() - 42.0,
        17.0,
        LOCKED,
    );
    text::draw_shadow(labels::get("keys_events"), 32.0, screen_height() - 20.0, 15.0, LOCKED);
}

/// The car list, with the four stat bars `ba.a(car, stat)` feeds the setup
/// screen.  The right half of the screen is left clear for the showroom, which
/// `main` draws behind this.
///
/// The bars are the only thing the four values are for: there is no garage and
/// nothing to buy, and the original's own `co` class is not a showroom either -
/// it never loads a car.
pub fn draw_cars(cars: &[CarInfo], progress: &Progress, cursor: usize) {
    centred(labels::get("menu_cars"), 12.0, 30.0, WHITE);

    let row = 58.0;
    let top = 74.0;
    for (index, car) in cars.iter().enumerate() {
        let y = top + index as f32 * row;
        let active = index == progress.car;
        if index == cursor {
            draw_rectangle(16.0, y - 6.0, screen_width() * 0.44, row - 8.0, HIGHLIGHT);
        }
        text::draw_shadow(
            &car.name,
            24.0,
            y,
            21.0,
            if active { ACCENT } else { WHITE },
        );
        if active {
            text::draw_shadow(labels::get("in_use"), 214.0, y + 5.0, 13.0, ACCENT);
        }
        for (stat, value) in car.stats.iter().enumerate() {
            let by = y + 20.0 + stat as f32 * 9.0;
            text::draw_shadow(labels::stat_name(stat), 24.0, by, 11.0, LOCKED);
            for segment in 0..6u8 {
                let x = 130.0 + segment as f32 * 11.0;
                draw_rectangle(
                    x,
                    by + 1.0,
                    9.0,
                    6.0,
                    if segment < *value {
                        Color::new(0.75, 0.80, 0.88, 1.0)
                    } else {
                        Color::new(1.0, 1.0, 1.0, 0.12)
                    },
                );
            }
        }
    }

    text::draw_shadow(labels::get("keys_cars"), 24.0, screen_height() - 22.0, 15.0, LOCKED);
}

pub fn draw_pause(cursor: usize) {
    let items = ["pause_resume", "pause_restart", "pause_quit"];
    let width = 260.0;
    let left = (screen_width() - width) / 2.0;
    let top = screen_height() * 0.4;
    panel(Rect::new(left - 12.0, top - 30.0, width + 24.0, items.len() as f32 * 34.0 + 56.0));
    centred(labels::get("pause_title"), top - 44.0, 32.0, WHITE);
    for (index, item) in items.iter().enumerate() {
        let y = top + index as f32 * 34.0;
        if index == cursor {
            draw_rectangle(left - 6.0, y - 4.0, width + 12.0, 30.0, HIGHLIGHT);
        }
        text::draw_shadow(labels::get(item), left + 16.0, y, 24.0, WHITE);
    }
}

#[derive(Clone)]
pub struct Outcome {
    pub place: usize,
    pub gained: u32,
    pub total_time: f32,
    pub best_lap: Option<f32>,
    pub laps: u32,
    pub cars: usize,
    /// Whether this run beat the stored time, and what the record was.
    pub improved: bool,
    pub previous_best: Option<f32>,
}

pub fn draw_results(event: &RaceEvent, progress: &Progress, outcome: &Outcome) {
    let width = 460.0;
    let left = (screen_width() - width) / 2.0;
    let top = screen_height() * 0.22;
    panel(Rect::new(left - 12.0, top - 46.0, width + 24.0, 330.0));
    centred(&event.name, top - 58.0, 34.0, WHITE);

    let line = |index: usize, label: &str, value: &str, color: Color| {
        let y = top + index as f32 * 32.0;
        text::draw_shadow(label, left + 20.0, y, 21.0, LOCKED);
        text::draw_shadow(value, left + width - 20.0 - text::width(value, 21.0), y, 21.0, color);
    };
    line(0, labels::get("col_mode"), labels::mode_name(event.mode), LOCKED);
    line(1, labels::get("results_position"), &format!("{} of {}", outcome.place + 1, outcome.cars),
         if outcome.place == 0 { GOLD } else { WHITE });
    line(2, labels::get("results_laps"), &outcome.laps.to_string(), WHITE);
    line(3, labels::get("results_total"), &format_time(outcome.total_time), WHITE);
    line(
        4,
        labels::get("results_best"),
        &outcome
            .best_lap
            .map(format_time)
            .unwrap_or_else(|| labels::get("no_time").to_string()),
        ACCENT,
    );
    let record = match (outcome.improved, outcome.previous_best) {
        (true, Some(previous)) => format!(
            "{}  (was {})",
            labels::get("results_new_record"),
            format_time(previous)
        ),
        (true, None) => labels::get("results_new_record").to_string(),
        (false, Some(best)) => labels::format("results_record", &[&format_time(best)]),
        (false, None) => labels::get("results_none").to_string(),
    };
    line(5, labels::get("results_record"), &record, ACCENT);
    line(
        6,
        labels::get("points"),
        &labels::format(
            "results_points",
            &[&outcome.gained.to_string(), &progress.points.to_string()],
        ),
        ACCENT,
    );

    centred(labels::get("results_continue"), top + 250.0, 20.0, LOCKED);
}

pub fn format_time(seconds: f32) -> String {
    let minutes = (seconds / 60.0) as u32;
    let rest = seconds - minutes as f32 * 60.0;
    format!("{minutes}:{rest:05.2}")
}
