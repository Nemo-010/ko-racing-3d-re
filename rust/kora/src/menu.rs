//! Screens: the main menu, the career and quick-race lists, car selection, the
//! pause menu and the results panel.
//!
//! These are the port's own layout - the MIDlet draws its menus from packed
//! images and text blobs - but every number on them comes from the game's
//! tables: the campaign's levels, entry thresholds and awards, and each car's
//! four stat values from its `.car` file.

use macroquad::prelude::*;

use crate::campaign::RaceEvent;
use crate::progress::{medal_name, CarInfo, Progress, MAX_STAT, STAT_NAMES};
use crate::text;

/// The main menu, shared with `main` so the cursor and the labels agree.
pub const MAIN_ITEMS: [&str; 6] = [
    "CAREER",
    "DELUXE",
    "QUICK RACE",
    "SELECT CAR",
    "GARAGE",
    "QUIT",
];

pub const BACKDROP: Color = Color::new(0.05, 0.07, 0.12, 1.0);
const PANEL: Color = Color::new(1.0, 1.0, 1.0, 0.06);
const HIGHLIGHT: Color = Color::new(0.20, 0.45, 0.85, 0.85);
const LOCKED: Color = Color::new(0.45, 0.45, 0.50, 1.0);
const ACCENT: Color = Color::new(1.0, 0.85, 0.2, 1.0);
const GOLD: Color = Color::new(1.0, 0.82, 0.25, 1.0);
const SILVER: Color = Color::new(0.80, 0.82, 0.86, 1.0);
const BRONZE: Color = Color::new(0.80, 0.55, 0.30, 1.0);

fn medal_color(medal: u8) -> Color {
    match medal {
        3 => GOLD,
        2 => SILVER,
        1 => BRONZE,
        _ => LOCKED,
    }
}

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
    centred("K.O. RACING 3D", screen_height() * 0.16, 54.0, WHITE);
    centred("RUST PORT", screen_height() * 0.16 + 62.0, 22.0, ACCENT);
    centred(
        &format!("CAREER POINTS {}", progress.points),
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
        text::draw_shadow(item, left + 16.0, y, 24.0, WHITE);
    }
    centred(
        "ARROWS SELECT   ENTER CONFIRM   ESC BACK",
        screen_height() - 22.0,
        17.0,
        LOCKED,
    );
}

/// A scrolling list of races, showing the medal already won and greying out
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
        text::draw_shadow(&event.map, 270.0, y, 20.0, color);
        text::draw_shadow(&event.laps.to_string(), 440.0, y, 20.0, color);
        text::draw_shadow(&event.opponents.to_string(), 490.0, y, 20.0, color);
        if open {
            let medal = progress.best(&event.key);
            text::draw_shadow(medal_name(medal), 540.0, y, 20.0, medal_color(medal));
        } else {
            text::draw_shadow(&format!("NEED {}", event.threshold), 540.0, y, 20.0, LOCKED);
        }
    }

    text::draw_shadow("LAPS", 440.0, top - 22.0, 17.0, LOCKED);
    text::draw_shadow("CPU", 490.0, top - 22.0, 17.0, LOCKED);
    text::draw_shadow("MEDAL", 540.0, top - 22.0, 17.0, LOCKED);
    text::draw_shadow(
        &format!(
            "{} of {} events   {} points",
            cursor.min(events.len().saturating_sub(1)) + 1,
            events.len(),
            progress.points
        ),
        32.0,
        screen_height() - 22.0,
        17.0,
        LOCKED,
    );
}

/// The four stat bars `ba.a(car, stat)` feeds the garage display.
///
/// One function covers both car selection and the garage: pass `focus` as the
/// stat index being upgraded to show the highlight and the price, or `None`
/// for plain selection.  Bars show what the car actually has now, with
/// anything bought in the garage picked out in gold.
pub fn draw_cars(
    cars: &[CarInfo],
    progress: &Progress,
    cursor: usize,
    focus: Option<usize>,
    message: &str,
) {
    let garage = focus.is_some();
    centred(if garage { "GARAGE" } else { "SELECT CAR" }, 12.0, 32.0, WHITE);
    centred(
        &format!("CAREER POINTS {}", progress.points),
        46.0,
        17.0,
        ACCENT,
    );

    let row = 62.0;
    let top = 82.0;
    for (index, car) in cars.iter().enumerate() {
        let y = top + index as f32 * row;
        if index == cursor {
            draw_rectangle(24.0, y - 8.0, screen_width() - 48.0, row - 10.0, HIGHLIGHT);
        }
        let active = index == progress.car;
        text::draw_shadow(
            &car.name,
            32.0,
            y,
            23.0,
            if active { ACCENT } else { WHITE },
        );
        if active {
            text::draw_shadow("IN USE", 250.0, y + 4.0, 14.0, ACCENT);
        }

        let stats = progress.stats(&car.file, car.stats);
        for (stat, value) in stats.iter().enumerate() {
            let by = y + 22.0 + stat as f32 * 9.0;
            let selected = garage && index == cursor && focus == Some(stat);
            text::draw_shadow(
                STAT_NAMES[stat],
                330.0,
                by,
                if selected { 14.0 } else { 12.0 },
                if selected { WHITE } else { LOCKED },
            );
            for segment in 0..MAX_STAT {
                let x = 420.0 + segment as f32 * 13.0;
                let bought = segment >= car.stats[stat];
                let colour = if segment >= *value {
                    Color::new(1.0, 1.0, 1.0, 0.12)
                } else if bought {
                    ACCENT
                } else {
                    Color::new(0.75, 0.80, 0.88, 1.0)
                };
                draw_rectangle(x, by + 1.0, 9.0, 7.0, colour);
            }
        }

        if garage && index == cursor {
            match progress.next_upgrade_cost(&car.file, car.stats, focus.unwrap()) {
                Some(cost) => text::draw_shadow(
                    &format!("ENTER: {} UPGRADE FOR {}", STAT_NAMES[focus.unwrap()], cost),
                    32.0,
                    y + row - 16.0,
                    15.0,
                    if cost <= progress.points { ACCENT } else { LOCKED },
                ),
                None => text::draw_shadow(
                    &format!("{} IS AT MAXIMUM", STAT_NAMES[focus.unwrap()]),
                    32.0,
                    y + row - 16.0,
                    15.0,
                    LOCKED,
                ),
            }
        }
    }

    if !message.is_empty() {
        text::draw_shadow(message, 32.0, screen_height() - 46.0, 17.0, WHITE);
    }
    text::draw_shadow(
        if garage {
            "UP/DOWN CAR   LEFT/RIGHT STAT   ENTER BUY   ESC BACK"
        } else {
            "ARROWS SELECT   ENTER CHOOSE   ESC BACK"
        },
        32.0,
        screen_height() - 22.0,
        15.0,
        LOCKED,
    );
}

pub fn draw_pause(cursor: usize) {
    let items = ["RESUME", "RESTART", "QUIT TO MENU"];
    let width = 260.0;
    let left = (screen_width() - width) / 2.0;
    let top = screen_height() * 0.4;
    panel(Rect::new(left - 12.0, top - 30.0, width + 24.0, items.len() as f32 * 34.0 + 56.0));
    centred("PAUSED", top - 44.0, 32.0, WHITE);
    for (index, item) in items.iter().enumerate() {
        let y = top + index as f32 * 34.0;
        if index == cursor {
            draw_rectangle(left - 6.0, y - 4.0, width + 12.0, 30.0, HIGHLIGHT);
        }
        text::draw_shadow(item, left + 16.0, y, 24.0, WHITE);
    }
}

#[derive(Clone)]
pub struct Outcome {
    pub place: usize,
    pub medal: u8,
    pub gained: u32,
    pub total_time: f32,
    pub best_lap: Option<f32>,
    pub laps: u32,
    pub cars: usize,
}

pub fn draw_results(event: &RaceEvent, progress: &Progress, outcome: &Outcome) {
    let width = 460.0;
    let left = (screen_width() - width) / 2.0;
    let top = screen_height() * 0.22;
    panel(Rect::new(left - 12.0, top - 46.0, width + 24.0, 300.0));
    centred(&event.name, top - 58.0, 34.0, WHITE);

    let line = |index: usize, label: &str, value: &str, color: Color| {
        let y = top + index as f32 * 32.0;
        text::draw_shadow(label, left + 20.0, y, 21.0, LOCKED);
        text::draw_shadow(value, left + width - 20.0 - text::width(value, 21.0), y, 21.0, color);
    };
    line(0, "POSITION", &format!("{} of {}", outcome.place + 1, outcome.cars),
         if outcome.place == 0 { GOLD } else { WHITE });
    line(1, "LAPS", &format!("{}", outcome.laps), WHITE);
    line(2, "TOTAL", &format_time(outcome.total_time), WHITE);
    line(3, "BEST LAP", &outcome.best_lap.map(format_time).unwrap_or_else(|| "--:--".into()), ACCENT);
    line(4, "MEDAL", medal_name(outcome.medal), medal_color(outcome.medal));
    line(5, "POINTS", &format!("+{}  (total {})", outcome.gained, progress.points), ACCENT);

    centred("ENTER CONTINUE", top + 224.0, 20.0, LOCKED);
}

pub fn format_time(seconds: f32) -> String {
    let minutes = (seconds / 60.0) as u32;
    let rest = seconds - minutes as f32 * 60.0;
    format!("{minutes}:{rest:05.2}")
}
