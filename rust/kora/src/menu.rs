//! The screens outside the race.
//!
//! The front end is the MIDlet's own: the space backdrop with the Earth in it,
//! and the entries along the bottom in a bar with `<` and `>` either side to
//! step through them, which is what the original does rather than a list.  The
//! screens behind it - the map, the race lists, the showroom, the options - are
//! built with macroquad's UI toolkit (`root_ui().button`, `Window`,
//! `progress_bar`), so they take a pointer as well as a keyboard, and they wear
//! the skin in [`crate::theme`] - the MIDlet's own palette of grey panels,
//! gradient headers and a dark red for the choice under the cursor.
//!
//! The exceptions are the front end itself and the racing HUD in [`crate::hud`],
//! which are drawn directly.

use macroquad::prelude::*;
use macroquad::texture::Texture2D;
use macroquad::ui::{hash, root_ui, widgets::Window};

use crate::campaign::RaceEvent;
use crate::labels::mode_name;
use crate::labels;
use crate::pack::Resources;
use crate::progress::{CarInfo, Progress};
use crate::settings::Settings;
use crate::text;
use crate::theme::{self, Theme};

/// The main menu, shared with `main` so the cursor and the labels agree.
/// These are label keys rather than text.
pub const MAIN_ITEMS: [&str; 6] = [
    "menu_career",
    "menu_deluxe",
    "menu_quick",
    "menu_cars",
    "options",
    "menu_quit",
];

/// What a screen wants the program to do next.
pub enum Action {
    None,
    /// The item at this index was chosen, by Enter or by a click.
    Activate(usize),
    Back,
}

fn centred_window(size: Vec2) -> Vec2 {
    vec2(
        (screen_width() - size.x) / 2.0,
        (screen_height() - size.y) / 2.0,
    )
}

/// Move the cursor from the keyboard, and let the pointer move it too: a
/// hovered button becomes the cursor, which is what makes the menus work with
/// a finger on a phone.
fn cursor_keys(cursor: &mut usize, length: usize) {
    if length == 0 {
        return;
    }
    if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::Left) {
        *cursor = (*cursor + length - 1) % length;
    }
    if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::Right) {
        *cursor = (*cursor + 1) % length;
    }
}

pub fn format_time(seconds: f32) -> String {
    let minutes = (seconds / 60.0) as u32;
    let rest = seconds - minutes as f32 * 60.0;
    format!("{minutes}:{rest:05.2}")
}

/// The bottom bar the MIDlet's front end puts its entries in: the current one
/// centred, with `<` and `>` either side, slid in from the side that was
/// pressed.  `s` is the widget that does the sliding, and `y.a(int, int)` gives
/// the bottom 70 pixels of the left and right edges to the two arrows, so the
/// bar works with a finger as well as with the arrow keys.
pub struct Bar {
    /// How far the showing label still has to slide in, in pixels.
    offset: f32,
    showing: usize,
    /// Which side it is sliding in from.
    from: f32,
}

/// `s`: the distance a label comes in from, and how fast.
const SLIDE: f32 = 180.0;
const SLIDE_SPEED: f32 = 900.0;
/// `bg`: the height of the bar along the bottom of the screen.
const BAR_HEIGHT: f32 = 28.0;
/// `y.a(int, int)`: the corner a finger has to be in to be an arrow.
const ARROW_ZONE: f32 = 70.0;

impl Bar {
    pub fn new(cursor: usize) -> Bar {
        Bar {
            offset: 0.0,
            showing: cursor,
            from: 1.0,
        }
    }

    /// `s.a(float)` / `s.b()`: a new entry arrives from the side that was
    /// pressed, and slides to the middle.
    fn update(&mut self, dt: f32, cursor: usize) {
        if cursor != self.showing {
            self.from = if cursor > self.showing { 1.0 } else { -1.0 };
            self.showing = cursor;
            self.offset = SLIDE;
            return;
        }
        self.offset = (self.offset - SLIDE_SPEED * dt).max(0.0);
    }
}

/// The front end: the space backdrop, the entries in a bar along the bottom, and
/// the arrows either side to step through them.
pub fn bar_menu(
    resources: &Resources,
    space: &mut crate::space::Space,
    bar: &mut Bar,
    progress: &Progress,
    cursor: &mut usize,
) -> Action {
    let dt = get_frame_time();
    let (width, height) = (screen_width(), screen_height());
    let count = MAIN_ITEMS.len();
    let top = height - BAR_HEIGHT;

    let mut chosen = None;
    if is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::Up) {
        *cursor = (*cursor + count - 1) % count;
    }
    if is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::Down) {
        *cursor = (*cursor + 1) % count;
    }
    if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
        chosen = Some(*cursor);
    }
    // macroquad raises mouse events from touches too, so this is the same code
    // for a click and for a tap on a phone.
    if is_mouse_button_pressed(MouseButton::Left) {
        let point = Vec2::from(mouse_position());
        if point.y > height - ARROW_ZONE && point.x < ARROW_ZONE {
            *cursor = (*cursor + count - 1) % count;
        } else if point.y > height - ARROW_ZONE && point.x > width - ARROW_ZONE {
            *cursor = (*cursor + 1) % count;
        } else if point.y > top {
            chosen = Some(*cursor);
        }
    }

    bar.update(dt, *cursor);
    space.update(dt, 0.0, crate::space::SCALE);
    space.draw();
    draw_frame(resources, bar, progress, *cursor, top);

    if let Some(index) = chosen {
        return Action::Activate(index);
    }
    if is_key_pressed(KeyCode::Escape) {
        return Action::Back;
    }
    Action::None
}

/// The logo, the points, and the bar itself.
fn draw_frame(
    resources: &Resources,
    bar: &Bar,
    progress: &Progress,
    cursor: usize,
    top: f32,
) {
    let (width, height) = (screen_width(), screen_height());
    let scale = (height / 240.0).max(1.0);

    // `bd`: the logo sits in the top right corner, drifting up and down.
    let bob = (get_time() as f32 * 0.4).sin() * 2.0 * scale;
    if let Some(logo) = logo(resources) {
        let size = vec2(logo.width(), logo.height()) * scale;
        draw_texture_ex(
            logo,
            screen_width() - size.x - 4.0 * scale,
            4.0 * scale + bob,
            WHITE,
            DrawTextureParams {
                dest_size: Some(size),
                ..Default::default()
            },
        );
    }
    text::draw_shadow(
        &labels::format("career_points", &[&progress.points.to_string()]),
        6.0 * scale,
        6.0 * scale,
        14.0 * scale,
        WHITE,
    );

    let label = labels::get(MAIN_ITEMS[cursor]);
    let size = 22.0 * scale;
    let slide = bar.from * bar.offset;
    text::draw_shadow(
        &label,
        width / 2.0 + slide - text::width(&label, size) / 2.0,
        top + (BAR_HEIGHT - size) / 2.0,
        size,
        WHITE,
    );

    // `aq.a(Graphics, 0, height - 28, width, 28, 0x999999, 0x666666, true)`.
    bar_gradient(0.0, top, width, BAR_HEIGHT);
    draw_line(0.0, top, width, top, 1.0, theme::BAR_TOP);

    let arrow = 20.0 * scale;
    text::draw_shadow("<", 10.0 * scale, top + (BAR_HEIGHT - arrow) / 2.0, arrow, WHITE);
    text::draw_shadow(
        ">",
        width - 10.0 * scale - text::width(">", arrow),
        top + (BAR_HEIGHT - arrow) / 2.0,
        arrow,
        WHITE,
    );

    let hint = labels::get("keys_menu");
    text::draw_shadow(
        &hint,
        6.0 * scale,
        top - 18.0 * scale,
        13.0 * scale,
        WHITE,
    );
}

/// The bar's gradient, line by line, the way `aq.a` draws it.
fn bar_gradient(x: f32, y: f32, width: f32, height: f32) {
    for step in 0..height as usize {
        let t = step as f32 / height;
        draw_rectangle(
            x,
            y + step as f32,
            width,
            1.0,
            Color::new(
                theme::BAR_TOP.r + (theme::BAR_BOTTOM.r - theme::BAR_TOP.r) * t,
                theme::BAR_TOP.g + (theme::BAR_BOTTOM.g - theme::BAR_TOP.g) * t,
                theme::BAR_TOP.b + (theme::BAR_BOTTOM.b - theme::BAR_TOP.b) * t,
                1.0,
            ),
        );
    }
}

/// `/images/lg.png`, the front end's logo, decoded once.
fn logo(resources: &Resources) -> Option<&'static Texture2D> {
    static LOGO: std::sync::OnceLock<Option<Texture2D>> = std::sync::OnceLock::new();
    LOGO.get_or_init(|| {
        let texture =
            Texture2D::from_file_with_format(resources.get("images/lg.png")?, None);
        texture.set_filter(FilterMode::Linear);
        Some(texture)
    })
    .as_ref()
}

/// A scrolling list of races: the game's own mode name, the lap count, the grid
/// and the stored best time, with anything the career has not opened yet dimmed
/// and marked with the points it needs.
pub fn event_list(
    theme: &Theme,
    progress: &Progress,
    events: &[RaceEvent],
    cursor: &mut usize,
    title: &str,
) -> Action {
    if events.is_empty() {
        return Action::Back;
    }
    *cursor = (*cursor).min(events.len() - 1);

    const VISIBLE: usize = 12;
    let first = (*cursor)
        .saturating_sub(VISIBLE / 2)
        .min(events.len().saturating_sub(VISIBLE));
    let size = vec2(620.0, 190.0 + VISIBLE.min(events.len()) as f32 * 40.0);
    let mut chosen = None;

    {
        let mut ui = root_ui();
        Window::new(hash!("kora-events"), centred_window(size), size)
            .label(title)
            .movable(false)
            .close_button(false)
            .ui(&mut *ui, |ui| {
                ui.label(
                    None,
                    &labels::format(
                        "events_count",
                        &[
                            &(*cursor + 1).to_string(),
                            &events.len().to_string(),
                            &progress.points.to_string(),
                        ],
                    ),
                );
                for (offset, event) in events.iter().skip(first).take(VISIBLE).enumerate() {
                    let index = first + offset;
                    let open = progress.open(event.threshold);
                    let best = progress
                        .best_time(&event.key)
                        .map(format_time)
                        .unwrap_or_else(|| labels::get("no_time").to_string());
                    let right = if open {
                        best
                    } else {
                        labels::format("need_points", &[&event.threshold.to_string()])
                    };
                    let row = format!(
                        "{:<16} {:<10} {:<12} {:<2} {:<2} {}",
                        event.name,
                        event.map,
                        mode_name(event.mode),
                        event.laps,
                        event.opponents,
                        right
                    );

                    let focused = index == *cursor;
                    if focused {
                        ui.push_skin(&theme.selected);
                    } else if !open {
                        ui.push_skin(&theme.locked);
                    }
                    if ui.button(None, row.as_str()) {
                        chosen = Some(index);
                    }
                    let hovered = ui.last_item_hovered();
                    if focused || !open {
                        ui.pop_skin();
                    }
                    if hovered {
                        *cursor = index;
                    }
                }
            });
    }
    cursor_keys(cursor, events.len());

    if let Some(index) = chosen {
        *cursor = index;
        return Action::Activate(index);
    }
    if is_key_pressed(KeyCode::Enter) {
        return Action::Activate(*cursor);
    }
    if is_key_pressed(KeyCode::Escape) {
        return Action::Back;
    }
    Action::None
}

/// Car selection, with the four values `ba.a(car, stat)` feeds as bars.  The
/// showroom behind it is drawn by `main`; there is nothing to buy.
pub fn car_list(theme: &Theme, cars: &[CarInfo], progress: &Progress, cursor: &mut usize) -> Action {
    if cars.is_empty() {
        return Action::Back;
    }
    *cursor = (*cursor).min(cars.len() - 1);
    let mut chosen = None;

    let size = vec2(300.0, 90.0 + cars.len() as f32 * 46.0);
    {
        let mut ui = root_ui();
        Window::new(hash!("kora-cars"), vec2(24.0, 60.0), size)
            .label(labels::get("menu_cars"))
            .movable(false)
            .close_button(false)
            .ui(&mut *ui, |ui| {
                for (index, car) in cars.iter().enumerate() {
                    let focused = index == *cursor;
                    let active = index == progress.car;
                    let text = if active {
                        format!("{}  ({})", car.name, labels::get("in_use"))
                    } else {
                        car.name.clone()
                    };
                    if focused {
                        ui.push_skin(&theme.selected);
                    }
                    if ui.button(None, text.as_str()) {
                        chosen = Some(index);
                    }
                    let hovered = ui.last_item_hovered();
                    if focused {
                        ui.pop_skin();
                    }
                    if hovered {
                        *cursor = index;
                    }
                }
            });
    }

    // The bars for the car under the cursor, as macroquad progress bars.
    let stats_size = vec2(300.0, 190.0);
    let stats_position = vec2(24.0, 110.0 + cars.len() as f32 * 46.0 + 20.0);
    if stats_position.y + stats_size.y < screen_height() - 20.0 {
        let mut ui = root_ui();
        let car = &cars[*cursor];
        Window::new(hash!("kora-stats"), stats_position, stats_size)
            .label(&car.name)
            .movable(false)
            .close_button(false)
            .ui(&mut *ui, |ui| {
                for (stat, value) in car.stats.iter().enumerate() {
                    ui.progress_bar(labels::stat_name(stat), *value as f32 / 6.0);
                }
            });
    }

    cursor_keys(cursor, cars.len());

    if let Some(index) = chosen {
        *cursor = index;
        return Action::Activate(index);
    }
    if is_key_pressed(KeyCode::Enter) {
        return Action::Activate(*cursor);
    }
    if is_key_pressed(KeyCode::Escape) {
        return Action::Back;
    }
    Action::None
}

pub fn pause_menu(theme: &Theme, cursor: &mut usize, music: Option<bool>) -> Action {
    const ITEMS: [&str; 3] = ["pause_resume", "pause_restart", "pause_quit"];
    let mut chosen = None;
    let size = vec2(320.0, 190.0);
    {
        let mut ui = root_ui();
        Window::new(hash!("kora-pause"), centred_window(size), size)
            .label(labels::get("pause_title"))
            .movable(false)
            .close_button(false)
            .ui(&mut *ui, |ui| {
                for (index, key) in ITEMS.iter().enumerate() {
                    let focused = index == *cursor;
                    if focused {
                        ui.push_skin(&theme.selected);
                    }
                    if ui.button(None, labels::get(key)) {
                        chosen = Some(index);
                    }
                    let hovered = ui.last_item_hovered();
                    if focused {
                        ui.pop_skin();
                    }
                    if hovered {
                        *cursor = index;
                    }
                }
                if let Some(playing) = music {
                    ui.label(
                        None,
                        labels::get(if playing { "music_on" } else { "music_off" }),
                    );
                }
            });
    }
    cursor_keys(cursor, ITEMS.len());

    if let Some(index) = chosen {
        *cursor = index;
        return Action::Activate(index);
    }
    if is_key_pressed(KeyCode::Enter) {
        return Action::Activate(*cursor);
    }
    if is_key_pressed(KeyCode::Escape) {
        return Action::Activate(0);
    }
    Action::None
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

pub fn results(
    theme: &Theme,
    event: &RaceEvent,
    progress: &Progress,
    outcome: &Outcome,
) -> Action {
    let size = vec2(480.0, 340.0);
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
    let lines = [
        (labels::get("col_mode"), mode_name(event.mode).to_string()),
        (
            labels::get("results_position"),
            format!("{} of {}", outcome.place + 1, outcome.cars),
        ),
        (labels::get("results_laps"), outcome.laps.to_string()),
        (labels::get("results_total"), format_time(outcome.total_time)),
        (
            labels::get("results_best"),
            outcome
                .best_lap
                .map(format_time)
                .unwrap_or_else(|| labels::get("no_time").to_string()),
        ),
        (labels::get("results_record"), record),
        (
            labels::get("points"),
            labels::format(
                "results_points",
                &[&outcome.gained.to_string(), &progress.points.to_string()],
            ),
        ),
    ];

    let mut chosen = false;
    {
        let mut ui = root_ui();
        Window::new(hash!("kora-results"), centred_window(size), size)
            .label(&event.name)
            .movable(false)
            .close_button(false)
            .ui(&mut *ui, |ui| {
                for (label, value) in lines.iter() {
                    ui.label(None, &format!("{label}  {value}"));
                }
                ui.push_skin(&theme.selected);
                chosen = ui.button(None, labels::get("results_continue"));
                ui.pop_skin();
            });
    }

    if chosen || is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Escape) {
        return Action::Activate(0);
    }
    Action::None
}


// --------------------------------------------------------------------------
// options
// --------------------------------------------------------------------------
/// What the options screen wants done about it.
pub enum OptionsAction {
    None,
    /// Wipe the career, once the confirmation is answered.
    ResetCareer,
    Back,
}

/// The rows, in the order they are drawn.  Every one of them cycles a value
/// with left and right or a click, which is how the MIDlet's own settings
/// screens work - its rows are value pickers, not sliders.
const OPTION_ROWS: [&str; 10] = [
    "quality",
    "camera",
    "visibility",
    "background",
    "hud",
    "scheme",
    "auto_throttle",
    "music",
    "volume",
    "reset_career",
];

/// The current value of a row, as a label key or a number.
fn option_value(settings: &Settings, row: usize) -> String {
    let on_off = |on: bool| labels::get(if on { "on" } else { "off" }).to_string();
    match row {
        0 => labels::get(settings.quality.key()).to_string(),
        1 => labels::get(settings.camera.key()).to_string(),
        2 => labels::get(settings.visibility.key()).to_string(),
        3 => on_off(settings.background),
        4 => on_off(settings.hud),
        5 => labels::get(settings.scheme.key()).to_string(),
        6 => on_off(settings.auto_throttle),
        7 => on_off(settings.music),
        8 => format!("{} / {}", settings.volume_steps(), crate::settings::MAX_VOLUME_STEPS),
        _ => String::new(),
    }
}

fn cycle_option(settings: &mut Settings, row: usize, forward: bool) {
    match row {
        0 => settings.quality = if forward { settings.quality.next() } else { settings.quality.previous() },
        1 => settings.camera = if forward { settings.camera.next() } else { settings.camera.previous() },
        2 => settings.visibility = if forward { settings.visibility.next() } else { settings.visibility.previous() },
        3 => settings.background = !settings.background,
        4 => settings.hud = !settings.hud,
        5 => settings.scheme = if forward { settings.scheme.next() } else { settings.scheme.previous() },
        6 => settings.auto_throttle = !settings.auto_throttle,
        7 => settings.music = !settings.music,
        8 => settings.cycle_volume(forward),
        _ => {}
    }
}

/// Which section heading a row falls under, if any.
fn option_section(row: usize) -> Option<&'static str> {
    match row {
        0 => Some("graphics"),
        5 => Some("controls"),
        7 => Some("sound"),
        _ => None,
    }
}

pub fn options(
    theme: &Theme,
    settings: &mut Settings,
    cursor: &mut usize,
    confirming: &mut bool,
) -> OptionsAction {
    let mut activate: Option<usize> = None;
    let mut backward = false;
    let mut action = OptionsAction::None;

    if *confirming {
        // `105 ARE YOU SURE?` with `103 YES` and `104 NO`.
        let size = vec2(320.0, 160.0);
        let mut chosen: Option<&str> = None;
        {
            let mut ui = root_ui();
            Window::new(hash!("kora-reset"), centred_window(size), size)
                .label(labels::get("reset_career"))
                .movable(false)
                .close_button(false)
                .ui(&mut *ui, |ui| {
                    ui.label(None, labels::get("sure"));
                    if ui.button(None, labels::get("yes")) {
                        chosen = Some("yes");
                    }
                    if ui.button(None, labels::get("no")) {
                        chosen = Some("no");
                    }
                });
        }
        match chosen {
            Some(choice) => {
                *confirming = false;
                if choice == "yes" {
                    return OptionsAction::ResetCareer;
                }
                return OptionsAction::None;
            }
            None => {}
        }
        if is_key_pressed(KeyCode::Escape) {
            *confirming = false;
        }
        return OptionsAction::None;
    }

    let size = vec2(420.0, 120.0 + (OPTION_ROWS.len() + 3) as f32 * 42.0);
    {
        let mut ui = root_ui();
        Window::new(hash!("kora-options"), centred_window(size), size)
            .label(labels::get("options"))
            .movable(false)
            .close_button(false)
            .ui(&mut *ui, |ui| {
                for row in 0..OPTION_ROWS.len() {
                    if let Some(section) = option_section(row) {
                        ui.label(None, labels::get(section));
                    }
                    let value = option_value(settings, row);
                    let text = if row == 9 {
                        labels::get("reset_career").to_string()
                    } else {
                        format!("{}: {}", labels::get(OPTION_ROWS[row]), value)
                    };
                    let focused = row == *cursor;
                    if focused {
                        ui.push_skin(&theme.selected);
                    }
                    if ui.button(None, text.as_str()) {
                        activate = Some(row);
                    }
                    let hovered = ui.last_item_hovered();
                    if focused {
                        ui.pop_skin();
                    }
                    if hovered {
                        *cursor = row;
                    }
                }
                ui.push_skin(&theme.selected);
                if ui.button(None, labels::get("main_menu")) {
                    action = OptionsAction::Back;
                }
                ui.pop_skin();
            });
    }

    // The original's rows are value pickers: left and right change the value,
    // and confirm does the same as the pointer.
    if is_key_pressed(KeyCode::Up) {
        *cursor = (*cursor + OPTION_ROWS.len() - 1) % OPTION_ROWS.len();
    }
    if is_key_pressed(KeyCode::Down) {
        *cursor = (*cursor + 1) % OPTION_ROWS.len();
    }
    if is_key_pressed(KeyCode::Left) {
        backward = true;
        activate = Some(*cursor);
    }
    if is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::Enter) {
        activate = Some(*cursor);
    }
    if is_key_pressed(KeyCode::Escape) {
        return OptionsAction::Back;
    }

    if let Some(row) = activate {
        if row == 9 {
            *confirming = true;
        } else {
            cycle_option(settings, row, !backward);
        }
    }
    action
}
