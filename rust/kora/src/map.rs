//! The career map: the MIDlet's `u` (and `br` for the deluxe tour), which is
//! what `/images/map.jpg` opens.
//!
//! `u.c()` reads the campaign `.000` table - one record per **level**, holding
//! the level's name, the track it loads, whether it is open yet and where its
//! marker sits on the map - and then the race table, whose records each name the
//! level they belong to.  A level is therefore a marker and the races at it are
//! its entries: this screen draws the map, a marker per level that has races,
//! and a panel listing the races at the marker under the cursor.
//!
//! Navigation is the MIDlet's.  Left and right step to the nearest marker either
//! side *by x* (`u.a(int)` compares the markers' x coordinates rather than their
//! distance), up and down step through the races at the marker, and fire starts
//! one.  The markers are `/images/st.png` (a star, for a level still closed) and
//! `/images/qm.png` (a trophy once it opens), eight frames each: four sizes of
//! grey, then the same four in gold.  A chosen marker pulses through its four
//! sizes, which is the animation `u.d` runs.

use macroquad::prelude::*;
use macroquad::texture::Texture2D;

use crate::campaign::RaceEvent;
use crate::format::CampaignLevel;
use crate::labels;
use crate::pack::Resources;
use crate::progress::Progress;
use crate::text;
use crate::theme;

/// The MIDlet lays these screens out in pixels on a 240-high canvas; everything
/// here is scaled off the real height so it stays legible on a desktop window.
const DESIGN_HEIGHT: f32 = 240.0;
/// `u.b(Graphics)`: the width of the panel that lists a marker's races, the
/// height of its title, and of a row.
const PANEL_WIDTH: f32 = 115.0;
const ROW_HEIGHT: f32 = 24.0;
/// How far the panel sits from its marker.
const PANEL_GAP: f32 = 14.0;
/// Two markers closer together than this on screen are the same hit.
const HIT_RADIUS: f32 = 30.0;

/// A marker's sprite sheet: the image and the width of one of its eight frames.
struct Sheet {
    texture: Texture2D,
    frame: f32,
}

impl Sheet {
    fn load(resources: &Resources, path: &str, frame: f32) -> Option<Sheet> {
        Some(Sheet {
            texture: load(resources, path)?,
            frame,
        })
    }

    /// Frame `index` of the eight, wrapped, drawn centred on `centre`.
    fn draw(&self, index: usize, centre: Vec2, scale: f32) {
        let side = self.frame * scale;
        let x = (index % 8) as f32 * self.frame;
        draw_texture_ex(
            &self.texture,
            centre.x - side / 2.0,
            centre.y - side / 2.0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(side, side)),
                source: Some(Rect::new(x, 0.0, self.frame, self.texture.height())),
                ..Default::default()
            },
        );
    }
}

/// One marker: a level, where it sits on the map, and the races that start
/// there.
pub struct Level {
    pub name: String,
    /// Position in map pixels, straight out of the `.000` record.
    pub position: Vec2,
    pub unlocked: bool,
    /// Whether any of its races is a normal one rather than a bonus unlock,
    /// which is what decides between a trophy and a star (`u.a(boolean)`).
    pub has_races: bool,
    pub races: Vec<RaceEvent>,
}

/// What the screen wants done next.
pub enum MapAction {
    None,
    /// Start the race under the cursor.
    Start,
    /// Leave the map.
    Back,
}

pub struct MapScreen {
    image: Texture2D,
    stars: Option<Sheet>,
    trophies: Option<Sheet>,
    levels: Vec<Level>,
    pub cursor: usize,
    pub race: usize,
    /// Where the map is scrolled to, and where it is heading.
    pan: Vec2,
    /// A counter the chosen marker's size is taken from.
    pulse: f32,
    /// The label in the top strip.
    pub title: String,
}

impl Level {
    /// Whether the level is open to the player.  The `.000` record carries a
    /// flag for this, but it is the *initial* state: the port gates a race on
    /// career points the same way its list screens and `Progress::open` do, so
    /// a level opens when its first race does.
    pub fn open(&self, progress: &Progress) -> bool {
        self.races
            .iter()
            .any(|race| progress.open(race.threshold))
    }

    /// The race under `index`, if the level has that many.
    pub fn race(&self, index: usize) -> Option<&RaceEvent> {
        self.races.get(index)
    }
}

/// Group the race table's entries onto the map's markers, which is what
/// `u.a(boolean)` does: a level becomes a marker only if something starts from
/// it, and the races come out in table order.
pub fn markers(events: &[RaceEvent], levels: &[CampaignLevel]) -> Vec<Level> {
    let mut markers: Vec<Level> = Vec::new();
    for (index, level) in levels.iter().enumerate() {
        let races: Vec<RaceEvent> = events
            .iter()
            .filter(|event| event.level_index == index)
            .cloned()
            .collect();
        if races.is_empty() {
            continue;
        }
        markers.push(Level {
            name: level.name.clone(),
            position: vec2(level.x as f32, level.y as f32),
            unlocked: level.unlocked != 0,
            has_races: races.iter().any(|race| race.unlocks.is_none()),
            races,
        });
    }
    markers
}

/// `u.a(int)`: the nearest marker along in that direction, comparing x only -
/// the markers run left to right across the map as the career progresses.
pub fn next_marker(levels: &[Level], cursor: usize, right: bool) -> Option<usize> {
    let Some(here) = levels.get(cursor).map(|level| level.position.x) else {
        return None;
    };
    let mut best: Option<usize> = None;
    for (index, level) in levels.iter().enumerate() {
        if index == cursor {
            continue;
        }
        let x = level.position.x;
        if if right { x <= here } else { x >= here } {
            continue;
        }
        let better = match best {
            None => true,
            Some(other) => {
                let other = levels[other].position.x;
                if right {
                    x < other
                } else {
                    x > other
                }
            }
        };
        if better {
            best = Some(index);
        }
    }
    best
}

impl MapScreen {
    pub fn new(
        resources: &Resources,
        table: &str,
        title: String,
        events: &[RaceEvent],
        levels: &[CampaignLevel],
        progress: &Progress,
    ) -> Option<MapScreen> {
        // `u` opens `/images/map.jpg`, `br` opens `/images/map2.jpg`.
        let name = if table.ends_with("deluxe") {
            "images/map2.jpg"
        } else {
            "images/map.jpg"
        };
        let markers = markers(events, levels);
        if markers.is_empty() {
            return None;
        }
        // `u.r()`: open on the first marker with an open race of its own.
        let cursor = markers
            .iter()
            .position(|level| level.open(progress) && level.has_races)
            .unwrap_or(0);
        Some(MapScreen {
            image: load(resources, name)?,
            stars: Sheet::load(resources, "images/st.png", 29.0),
            trophies: Sheet::load(resources, "images/qm.png", 26.0),
            levels: markers,
            cursor,
            race: 0,
            pan: Vec2::ZERO,
            pulse: 0.0,
            title,
        })
    }

    /// The level under the cursor and the race chosen at it.
    pub fn level(&self) -> &Level {
        &self.levels[self.cursor]
    }

    pub fn event(&self) -> Option<&RaceEvent> {
        self.level().races.get(self.race)
    }

    pub fn index(&self) -> usize {
        self.cursor
    }

    /// How much of this map is done, for the strip's corner: the share of races
    /// that have a stored time, which is the `u.c + "%"` the MIDlet shows.
    fn completed(&self, progress: &Progress) -> u32 {
        let total: usize = self.levels.iter().map(|level| level.races.len()).sum();
        if total == 0 {
            return 0;
        }
        let done = self
            .levels
            .iter()
            .flat_map(|level| level.races.iter())
            .filter(|race| progress.best_time(&race.key).is_some())
            .count();
        (100 * done / total) as u32
    }

    /// The whole picture, at the largest size that fits, so the map keeps its
    /// own aspect and the whole of it stays visible.  `u` gets this for free on
    /// a phone - its canvas is smaller than the 350-pixel map, so it draws it at
    /// one to one and pans - but a desktop window is larger than the map, and
    /// stretching it to cover the window would both crop and distort the layout
    /// the markers are placed on.
    fn zoom(&self) -> f32 {
        (screen_width() / self.image.width()).min(screen_height() / self.image.height())
    }

    /// `u.c(int, int)`: put the map where the chosen marker can be seen.  When
    /// the whole map fits, that is the middle of the screen; when it does not,
    /// it pans, clamped so no gap opens at an edge.
    fn centre_on(&self, cursor: usize) -> Vec2 {
        let zoom = self.zoom();
        let marker = self.levels[cursor].position * zoom;
        let wanted = vec2(screen_width(), screen_height()) / 2.0 - marker;
        let map = vec2(self.image.width(), self.image.height()) * zoom;
        let axis = |want: f32, map: f32, screen: f32| {
            if map <= screen {
                (screen - map) / 2.0
            } else {
                want.clamp(screen - map, 0.0)
            }
        };
        vec2(
            axis(wanted.x, map.x, screen_width()),
            axis(wanted.y, map.y, screen_height()),
        )
    }

    pub fn update(&mut self, dt: f32) {
        self.pulse = (self.pulse + dt * 4.0) % 4.0;
        self.pan = self.pan.lerp(self.centre_on(self.cursor), (dt * 8.0).min(1.0));
    }

    pub fn input(&mut self) -> (MapAction, Option<usize>) {
        let mut action = MapAction::None;
        let mut chosen = None;
        if is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::Right) {
            let right = is_key_pressed(KeyCode::Right);
            if let Some(index) = self.next_marker(right) {
                self.cursor = index;
                self.race = 0;
                chosen = Some(index);
            }
        }
        let count = self.level().races.len();
        if count > 0 {
            if is_key_pressed(KeyCode::Up) {
                self.race = (self.race + count - 1) % count;
            }
            if is_key_pressed(KeyCode::Down) {
                self.race = (self.race + 1) % count;
            }
        }
        // A click picks a marker, and a click on the chosen one starts its
        // race.  macroquad raises mouse events from touches as well, so this is
        // a tap on a phone too.
        if is_mouse_button_pressed(MouseButton::Left) {
            let point = Vec2::from(mouse_position());
            if let Some(index) = self.marker_at(point) {
                if index == self.cursor {
                    action = MapAction::Start;
                } else {
                    self.cursor = index;
                    self.race = 0;
                    chosen = Some(index);
                }
            }
        }
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space) {
            action = MapAction::Start;
        }
        if is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::Backspace) {
            action = MapAction::Back;
        }
        (action, chosen)
    }

    fn next_marker(&self, right: bool) -> Option<usize> {
        next_marker(&self.levels, self.cursor, right)
    }

    fn marker_at(&self, point: Vec2) -> Option<usize> {
        let zoom = self.zoom();
        let reach = HIT_RADIUS * (screen_height() / DESIGN_HEIGHT).max(1.0);
        self.levels.iter().position(|level| {
            let centre = self.pan + level.position * zoom;
            (point - centre).length() < reach
        })
    }

    pub fn draw(&self, progress: &Progress) {
        clear_background(BLACK);
        let zoom = self.zoom();
        let scale = (screen_height() / DESIGN_HEIGHT).max(1.0);
        draw_texture_ex(
            &self.image,
            self.pan.x,
            self.pan.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(self.image.width(), self.image.height()) * zoom),
                ..Default::default()
            },
        );
        for (index, level) in self.levels.iter().enumerate() {
            let centre = self.pan + level.position * zoom;
            let open = level.open(progress);
            self.draw_marker(level, centre, open, index == self.cursor, scale);
        }
        if let Some(level) = self.levels.get(self.cursor) {
            let centre = self.pan + level.position * zoom;
            self.draw_panel(level, centre, progress, scale);
        }
        self.draw_strip(progress, scale);
    }

    fn draw_marker(&self, level: &Level, centre: Vec2, open: bool, chosen: bool, scale: f32) {
        let (sheet, base) = if !open {
            (self.stars.as_ref(), 0)
        } else if level.has_races {
            (self.trophies.as_ref(), 4)
        } else {
            (self.trophies.as_ref(), 0)
        };
        let Some(sheet) = sheet else {
            return;
        };
        let step = if chosen { self.pulse as usize } else { 0 };
        sheet.draw(base + step, centre, scale);
    }

    /// `u.b(Graphics)`: a panel beside the marker, a title with the level's
    /// name, then a row per race.  The rows report the mode, which is the same
    /// thing the list screens show.
    fn draw_panel(&self, level: &Level, centre: Vec2, progress: &Progress, scale: f32) {
        let width = PANEL_WIDTH * scale;
        let title_height = ROW_HEIGHT * scale;
        let open = level.open(progress);
        let rows = level.races.len().min(7);
        let height = if open {
            // One race: the title and a row for its best time.
            title_height + 2.0 * ROW_HEIGHT * scale
        } else {
            // Locked: the title and a row per race, so the player can see what
            // is behind the marker.
            title_height + ROW_HEIGHT * scale * rows as f32
        };
        // Keep the panel on the screen: it flips to the other side of the
        // marker if it would run off the right or the bottom.
        let mut x = centre.x + PANEL_GAP * scale;
        let mut y = centre.y - title_height / 2.0;
        if x + width > screen_width() {
            x = centre.x - width - PANEL_GAP * scale;
        }
        if y + height > screen_height() {
            y = screen_height() - height;
        }
        x = x.max(2.0);
        y = y.max(30.0 * scale);

        draw_rectangle(
            x - 2.0 * scale,
            y - 2.0 * scale,
            width,
            height + 3.0 * scale,
            BLACK,
        );
        // `u`: the title is dark red once the level is open and grey while it
        // is not.
        gradient(
            x,
            y,
            width,
            title_height,
            if open { theme::SPECIAL } else { theme::ROW },
            theme::ROW,
        );
        text::draw(
            &level.name,
            x + width - text::width(&level.name, 15.0 * scale) - 5.0 * scale,
            y + (title_height - 15.0 * scale) / 2.0,
            15.0 * scale,
            WHITE,
        );

        let body = y + title_height;
        draw_rectangle(x, body, width, height - title_height, theme::PANEL);
        if open {
            // One race: report its best time instead of a list of one.
            let record = self
                .event()
                .and_then(|race| progress.best_time(&race.key))
                .map(format_time)
                .unwrap_or_else(|| labels::get("no_time").to_string());
            let row = body + 8.0 * scale;
            text::draw(
                labels::get("col_best"),
                x + 8.0 * scale,
                row,
                14.0 * scale,
                theme::INK,
            );
            text::draw(
                &record,
                x + width - text::width(&record, 14.0 * scale) - 7.0 * scale,
                row,
                14.0 * scale,
                theme::INK,
            );
            return;
        }
        for (index, race) in level.races.iter().enumerate() {
            if index >= rows {
                break;
            }
            let top = body + ROW_HEIGHT * scale * index as f32;
            if index == self.race {
                draw_rectangle(
                    x + scale,
                    top + scale,
                    width - 2.0 * scale,
                    ROW_HEIGHT * scale - 2.0 * scale,
                    theme::ROW_SELECTED,
                );
            }
            let name = labels::mode_name(race.mode);
            text::draw(
                &name,
                x + width - text::width(&name, 14.0 * scale) - 3.0 * scale,
                top + (ROW_HEIGHT * scale - 14.0 * scale) / 2.0,
                14.0 * scale,
                theme::INK,
            );
        }
    }

    /// The strip along the top, which is where "SELECT RACE" lives.
    fn draw_strip(&self, progress: &Progress, scale: f32) {
        let height = 20.0 * scale;
        gradient(
            0.0,
            0.0,
            screen_width(),
            height,
            theme::BAR_TOP,
            theme::BAR_BOTTOM,
        );
        text::draw(
            &self.title,
            5.0 * scale,
            (height - 14.0 * scale) / 2.0,
            14.0 * scale,
            WHITE,
        );
        let label = labels::get("select_race");
        text::draw(
            &label,
            screen_width() / 2.0 - text::width(&label, 14.0 * scale) / 2.0,
            (height - 14.0 * scale) / 2.0,
            14.0 * scale,
            WHITE,
        );
        let percent = format!("{}%", self.completed(progress));
        let corner = 40.0 * scale;
        gradient(
            screen_width() - corner,
            0.0,
            corner,
            height,
            WHITE,
            theme::ROW_SELECTED,
        );
        text::draw(
            &percent,
            screen_width() - corner / 2.0 - text::width(&percent, 14.0 * scale) / 2.0,
            (height - 14.0 * scale) / 2.0,
            14.0 * scale,
            theme::INK,
        );
        let hint = labels::get("map_keys");
        text::draw(
            &hint,
            5.0 * scale,
            screen_height() - 18.0 * scale,
            13.0 * scale,
            WHITE,
        );
    }
}

fn format_time(seconds: f32) -> String {
    crate::menu::format_time(seconds)
}

/// `aq.a(Graphics, int, int, int, int, int, int, boolean)`: a gradient bar,
/// drawn line by line from the first colour to the second.
fn gradient(x: f32, y: f32, width: f32, height: f32, top: Color, bottom: Color) {
    let steps = height.max(1.0) as usize;
    for step in 0..steps {
        let t = step as f32 / steps as f32;
        draw_rectangle(
            x,
            y + step as f32,
            width,
            1.0,
            Color::new(
                top.r + (bottom.r - top.r) * t,
                top.g + (bottom.g - top.g) * t,
                top.b + (bottom.b - top.b) * t,
                1.0,
            ),
        );
    }
}

fn load(resources: &Resources, path: &str) -> Option<Texture2D> {
    let texture = Texture2D::from_file_with_format(resources.get(path)?, None);
    texture.set_filter(FilterMode::Linear);
    Some(texture)
}
