//! K.O. Racing 3D - Rust reimplementation of the Jollybox J2ME racer.
//!
//! The original MIDlet reads every asset out of its own `data`/`data.<n>`
//! archive; this port does exactly the same, parsing the model, tile, map,
//! car, campaign and font formats directly instead of converting them.
//! Rendering is macroquad, physics is rapier3d's raycast vehicle controller.
//!
//! Menus: `KORA_SKIP_MENU=1` boots straight into a race, and `KORA_MAP`,
//! `KORA_CAR`, `KORA_LAPS`, `KORA_OPPONENTS` and `KORA_ASSETS` still override
//! what the menus would pick.  Progress is saved to `KORA_SAVE` (default
//! `kora-save.txt`).

use std::path::PathBuf;

use macroquad::audio::{load_sound_from_bytes, play_sound, set_sound_volume, PlaySoundParams, Sound};
use macroquad::models::{draw_mesh, Mesh};
use macroquad::prelude::*;

use kora::ai::AiDriver;
use kora::campaign::{self, RaceEvent};
use kora::labels;
use kora::menu::Outcome;
use kora::physics::{CarControl, Tuning, World};
use kora::progress::{self, Progress};
use kora::race::Race;
use kora::text;
use kora::settings::Settings;
use kora::{format, hud, menu, music, pack, paths, scene, sky, theme};

/// The asset directory: `KORA_ASSETS`, or `assets` in the working directory.
///
/// That directory is an extracted tree - every resource at its original path,
/// plus the `lists`, `ui` and `sounds` the game reads straight from the JAR -
/// as `setup.sh` leaves it.  Pointing `KORA_ASSETS` at `x` reads the packed
/// archive instead, which is the form the game shipped.
fn assets_dir() -> PathBuf {
    match std::env::var("KORA_ASSETS") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => PathBuf::from("assets"),
    }
}

/// Environment override, if set to a valid number.
fn env_number(name: &str, max: u32) -> Option<u32> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .map(|value| value.min(max))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Main,
    Options,
    Career,
    Deluxe,
    Quick,
    Cars,
    Race,
    Paused,
    Results,
}

/// A race in progress: the track, the cars and the running order.
struct Running {
    event: RaceEvent,
    back: Screen,
    track: scene::Track,
    sky: Option<sky::Sky>,
    geometry: scene::CarGeometry,
    texture: Option<Texture2D>,
    world: World,
    races: Vec<Race>,
    drivers: Vec<AiDriver>,
    player: usize,
    camera: Vec3,
    finish_order: Vec<usize>,
    outcome: Option<Outcome>,
}

impl Running {
    /// Live standings: cars that have finished in their finishing order, then
    /// everyone else by race progress.
    fn standings(&self) -> Vec<usize> {
        let mut order = self.finish_order.clone();
        let mut rest: Vec<usize> = (0..self.world.cars.len())
            .filter(|car| !order.contains(car))
            .collect();
        rest.sort_by(|&a, &b| {
            let (pa, pb) = (
                self.races[a].progress(&self.track.grid, self.world.position(a)),
                self.races[b].progress(&self.track.grid, self.world.position(b)),
            );
            pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
        });
        order.extend(rest);
        order
    }

    fn place_of(&self, car: usize) -> usize {
        self.standings().iter().position(|&c| c == car).unwrap_or(0)
    }
}

/// Build a race from an event: track, textures, cars and race state.
fn start_race(
    resources: &pack::Resources,
    dir: &PathBuf,
    event: &RaceEvent,
    car_file: &str,
    settings: &Settings,
    back: Screen,
) -> Option<Running> {
    let detail = match settings.quality {
        kora::settings::Quality::Low => scene::Detail::Base,
        kora::settings::Quality::Medium => scene::Detail::Mid,
        kora::settings::Quality::High => scene::Detail::Full,
    };
    let mut track = scene::build_detailed(dir, resources, &event.map, event.theme, detail);
    track.attach_textures(resources);
    // The theme byte picks one of the five backgrounds (`al.q(j)`).
    let sky = sky::Sky::load(resources, event.theme);

    let car_def = resources
        .get(&format!("cars/{car_file}"))
        .and_then(|bytes| kora::format::Car::parse(bytes))?;
    let mut geometry = scene::build_car(resources, &car_def)?;
    let texture = scene::load_car_texture(resources, &mut geometry);
    let tuning = Tuning::from_stats(car_def.stats);

    let laps = env_number("KORA_LAPS", 99).unwrap_or(event.laps).max(1);
    let opponents = env_number("KORA_OPPONENTS", 7).unwrap_or(event.opponents);

    let mut world = World::new(
        track.collision_vertices.clone(),
        track.collision_indices.clone(),
        &track.walls,
    );
    let mut player = 0;
    for (index, &(spot, yaw)) in track.grid.grid_slots(1 + opponents as usize).iter().enumerate() {
        let car = world.add_car(spot, yaw, geometry.half_extents, tuning);
        if index == 0 {
            player = car;
        }
    }

    let races: Vec<Race> = (0..world.cars.len())
        .map(|index| Race::new(&track.grid, laps, world.position(index), 0.0))
        .collect();
    let drivers: Vec<AiDriver> = (0..world.cars.len())
        .map(|index| AiDriver::new(if index == player { 1.0 } else { 0.84 + 0.06 * index as f32 }))
        .collect();

    let camera = track.spawn + vec3(0.0, 5.0, 9.0);
    println!(
        "race: {} ({}) mode {} laps {} opponents {} theme {}",
        event.name, event.map, event.mode, laps, opponents, event.theme
    );

    Some(Running {
        event: event.clone(),
        back,
        track,
        sky,
        geometry,
        texture,
        world,
        races,
        drivers,
        player,
        camera,
        finish_order: Vec::new(),
        outcome: None,
    })
}

impl Running {
    fn restart(&mut self, laps: u32) {
        for index in 0..self.world.cars.len() {
            self.world.reset(index);
            let position = self.world.position(index);
            self.races[index] = Race::new(&self.track.grid, laps, position, 0.0);
        }
        self.finish_order.clear();
        self.outcome = None;
        self.camera = self.track.spawn + vec3(0.0, 5.0, 9.0);
    }

    /// Advance the race by one frame.  Returns the outcome once the player has
    /// finished.
    fn update(&mut self, dt: f32, laps: u32, settings: &Settings) -> Option<Outcome> {
        let mut controls = vec![CarControl::default(); self.world.cars.len()];
        // The control scheme picks the keys; auto-throttle drives for you.
        let (accelerate, brake) = settings.scheme.throttle_keys();
        let (left, right) = settings.scheme.steer_keys();
        controls[self.player].throttle = if settings.auto_throttle || is_key_down(accelerate) {
            1.0
        } else if is_key_down(brake) {
            -0.6
        } else {
            0.0
        };
        // Positive steering turns the wheels left (about +Y).
        controls[self.player].steer = if is_key_down(left) {
            1.0
        } else if is_key_down(right) {
            -1.0
        } else {
            0.0
        };
        controls[self.player].brake = is_key_down(KeyCode::Space);

        for index in 0..self.world.cars.len() {
            if index == self.player {
                continue;
            }
            let (position, rotation) = self.world.pose(index);
            let heading = rotation * vec3(0.0, 0.0, -1.0);
            controls[index] = self.drivers[index].control(
                &self.track.grid,
                position,
                heading,
                self.world.speed(index),
                dt,
            );
        }

        let substeps = ((dt / (1.0 / 60.0)).ceil() as i32).clamp(1, 4);
        for _ in 0..substeps {
            self.world.step(dt / substeps as f32, &controls);
        }

        let ride = self.geometry.half_extents.y + 0.02;
        for index in 0..self.world.cars.len() {
            let place = self.world.position(index);
            if place.y < -40.0 {
                self.world.reset(index);
                continue;
            }
            // The MIDlet sets its car's height from the track's collision mesh
            // every frame, which is how it crosses the steps between tiles.
            if let Some(height) = self.track.surface.height_at(place) {
                self.world.lift_to(index, height + ride);
            }
        }

        let now = get_time();
        for index in 0..self.world.cars.len() {
            let before = self.races[index].finished;
            self.races[index].update(now, &self.track.grid, self.world.position(index));
            if self.races[index].finished && !before {
                self.finish_order.push(index);
            }
        }

        if self.outcome.is_none() && self.races[self.player].finished {
            let place = self
                .finish_order
                .iter()
                .position(|&car| car == self.player)
                .unwrap_or(0);
            let race = &self.races[self.player];
            self.outcome = Some(Outcome {
                place,
                gained: 0,
                total_time: race.finish_time.unwrap_or(0.0),
                best_lap: race.best,
                laps,
                cars: self.world.cars.len(),
                improved: false,
                previous_best: None,
            });
            return self.outcome.clone();
        }
        None
    }

    fn draw_world(&mut self, dt: f32, settings: &Settings) {
        let (position, rotation) = self.world.pose(self.player);
        let forward = rotation * vec3(0.0, 0.0, -1.0);
        let up = vec3(0.0, 1.0, 0.0);
        let (back, height) = settings.camera.placement();
        let desired = position - forward * back + up * height;
        self.camera = self.camera.lerp(desired, (dt * 5.0).min(1.0));

        // The sky is a 2D backdrop, the way M3G draws a `Background` into the
        // viewport before the scene, so it belongs under the default (screen)
        // camera.  Drawing it after `set_camera` would instead drop the whole
        // image into the world as a 1280x720 quad standing at the origin.
        match &self.sky {
            Some(sky) if settings.background => sky.draw(),
            _ => clear_background(Color::new(0.53, 0.81, 0.92, 1.0)),
        }

        let mut camera = Camera3D::default();
        camera.position = self.camera;
        camera.target = position + forward * 3.0 + up * 0.8;
        camera.up = up;
        camera.fovy = if settings.camera == kora::settings::Camera::Inside {
            70f32.to_radians()
        } else {
            62f32.to_radians()
        };
        camera.z_far = settings.visibility.far_plane();
        set_camera(&camera);

        for mesh in &self.track.meshes {
            draw_mesh(mesh);
        }
        for index in 0..self.world.cars.len() {
            // From inside you sit in the car; drawing its shell would put the
            // camera behind a wall of polygons.
            if settings.camera == kora::settings::Camera::Inside && index == self.player {
                continue;
            }
            let (place, spin) = self.world.pose(index);
            let mut vertices = self.geometry.vertices.clone();
            for vertex in &mut vertices {
                vertex.position = spin * vertex.position + place;
            }
            draw_mesh(&Mesh {
                vertices,
                indices: self.geometry.indices.clone(),
                texture: self.texture.clone(),
            });
        }
        set_default_camera();
    }

    fn draw_hud(&self, now: f64, laps: u32, settings: &Settings) {
        if !settings.hud {
            return;
        }
        let race = &self.races[self.player];
        let place = self.place_of(self.player) + 1;
        text::draw_shadow(&self.event.name, 16.0, 14.0, 28.0, WHITE);
        text::draw_shadow(
            &labels::format(
                "hud_lap",
                &[&(race.lap + 1).min(laps).to_string(), &laps.to_string()],
            ),
            16.0,
            50.0,
            28.0,
            WHITE,
        );
        text::draw_shadow(
            &labels::format(
                "hud_pos",
                &[&place.to_string(), &self.world.cars.len().to_string()],
            ),
            16.0,
            86.0,
            28.0,
            WHITE,
        );
        text::draw_shadow(
            &labels::format("hud_time", &[&menu::format_time(race.total_time(now))]),
            16.0,
            122.0,
            28.0,
            WHITE,
        );
        let best = race
            .best
            .map(menu::format_time)
            .unwrap_or_else(|| labels::get("no_time").to_string());
        text::draw_shadow(
            &labels::format(
                "hud_lap_best",
                &[&menu::format_time(race.lap_time(now)), &best],
            ),
            16.0,
            158.0,
            28.0,
            Color::new(1.0, 0.85, 0.2, 1.0),
        );
        text::draw_shadow(
            labels::get("keys_race"),
            16.0,
            screen_height() - 40.0,
            20.0,
            Color::new(0.9, 0.9, 0.9, 1.0),
        );

        let markers: Vec<hud::Marker> = (0..self.world.cars.len())
            .map(|car| {
                let (position, rotation) = self.world.pose(car);
                hud::Marker {
                    position,
                    heading: rotation * vec3(0.0, 0.0, -1.0),
                    player: car == self.player,
                }
            })
            .collect();
        hud::draw_minimap(&self.track.grid, &markers);
        hud::draw_speedometer(self.world.speed(self.player));
    }
}

/// Render one car on a turntable in the right half of the screen.
///
/// The camera orbits a car centred on its own origin, and the viewport keeps it
/// off the list.  `Camera3D` works its aspect out from the whole window, so a
/// half-width viewport has to be told its own.
fn draw_showroom(geometry: &scene::CarGeometry, texture: &Option<Texture2D>, degrees: f32) {
    let (left, top) = (screen_width() * 0.46, 40.0);
    let width = screen_width() - left - 16.0;
    let height = screen_height() - top - 60.0;
    if width < 32.0 || height < 32.0 {
        return;
    }

    let angle = degrees.to_radians();
    let distance = 2.6;
    let mut camera = Camera3D::default();
    camera.position = vec3(distance * angle.sin(), 1.35, distance * angle.cos());
    camera.target = vec3(0.0, 0.15, 0.0);
    camera.up = vec3(0.0, 1.0, 0.0);
    camera.fovy = 45f32.to_radians();
    camera.aspect = Some(width / height);
    camera.viewport = Some((left as i32, top as i32, width as i32, height as i32));
    camera.z_far = 60.0;
    set_camera(&camera);

    draw_mesh(&Mesh {
        vertices: geometry.vertices.clone(),
        indices: geometry.indices.clone(),
        texture: texture.clone(),
    });
    set_default_camera();
}

#[macroquad::main("K.O. Racing 3D - Rust port")]
async fn main() {
    let dir = assets_dir();
    println!("resource pack: {}", dir.display());
    let resources = pack::load(&dir);
    println!("  {} resources indexed", resources.len());

    let cars = progress::car_infos(&resources);
    // The two career tables are listed separately, the way the original has
    // one screen for the campaign and another for the deluxe levels.  Unlike
    // the original, the deluxe list is gated by career points rather than by
    // an SMS purchase: nothing here needs a server or a payment.
    let all_events = campaign::events(&resources);
    let career: Vec<RaceEvent> = all_events
        .iter()
        .filter(|event| event.table.ends_with("campaign"))
        .cloned()
        .collect();
    let deluxe: Vec<RaceEvent> = all_events
        .iter()
        .filter(|event| event.table.ends_with("deluxe"))
        .cloned()
        .collect();
    let quick = campaign::quick_events(&resources);
    // The showroom draws the picked car in 3D, the way `u.j()` renders
    // `bd.a.a(car, angle)` through a transform while the panel spins at ten
    // degrees a second.  Display only: nothing here can be bought.
    let showroom: Vec<(scene::CarGeometry, Option<Texture2D>)> = cars
        .iter()
        .filter_map(|car| {
            let definition =
                format::Car::parse(resources.get(&format!("cars/{}", car.file))?)?;
            let mut geometry = scene::build_car(&resources, &definition)?;
            let texture = scene::load_car_texture(&resources, &mut geometry);
            Some((geometry, texture))
        })
        .collect();
    println!("  {} cars in the showroom", showroom.len());
    println!(
        "  {} cars, {} career events, {} deluxe events, {} quick-race tracks",
        cars.len(),
        career.len(),
        deluxe.len(),
        quick.len()
    );

    let settings_path = match std::env::var("KORA_SETTINGS") {
        Ok(path) => PathBuf::from(path),
        Err(_) => paths::file(paths::Dir::Config, "settings.txt"),
    };
    let mut settings = Settings::load(&settings_path);
    println!("settings: {}", settings_path.display());

    // The one sound the game ships is a MIDI file at the JAR root, not in the
    // resource pack, so `setup.sh` puts it beside the pack.  macroquad cannot
    // play MIDI, so it is rendered here and handed over as a WAV.
    if std::env::var("KORA_MUSIC").map(|v| v == "0").unwrap_or(false) {
        settings.music = false;
    }
    let theme = std::fs::read(dir.join("sounds/theme.mid")).ok();
    let now = || std::time::Instant::now();
    let started = now();
    let track: Option<Sound> = match theme.as_deref().map(music::render) {
        Some(Some(wav)) => {
            println!(
                "  theme rendered to {:.1} s of audio in {:?}",
                wav.len() as f32 / (music::SAMPLE_RATE as f32 * 2.0),
                started.elapsed()
            );
            load_sound_from_bytes(&wav).await.ok()
        }
        _ => {
            eprintln!("  no sounds/theme.mid beside the pack, running silent");
            None
        }
    };
    let mut music_playing = false;
    if settings.music {
        if let Some(track) = &track {
            play_sound(
                track,
                PlaySoundParams {
                    looped: true,
                    volume: settings.volume,
                },
            );
            music_playing = true;
        }
    }

    let theme = theme::build();

    let save_path = match std::env::var("KORA_SAVE") {
        Ok(path) => PathBuf::from(path),
        Err(_) => paths::file(paths::Dir::Data, "save.txt"),
    };
    let mut progress = Progress::load(&save_path);
    println!("progress: {}", save_path.display());
    if let Ok(name) = std::env::var("KORA_CAR") {
        if let Some(index) = cars.iter().position(|car| format!("cars/{}", car.file) == name) {
            progress.car = index;
        }
    }

    // `KORA_SKIP_MENU=1` boots straight into a race, which is handy from a
    // shell and keeps the environment overrides meaningful.
    let mut screen = Screen::Main;
    let mut cursor = 0usize;
    let mut showroom_spin = 0.0f32;
    let mut confirming_reset = false;
    let mut running: Option<Running> = None;
    let mut message = String::new();

    if std::env::var("KORA_SKIP_MENU").is_ok() {
        let wanted = std::env::var("KORA_MAP").ok();
        let event = wanted
            .as_ref()
            .and_then(|map| quick.iter().find(|event| &event.map == map))
            .or_else(|| quick.first())
            .cloned();
        if let Some(event) = event {
            let file = cars
                .get(progress.car)
                .map(|car| car.file.clone())
                .unwrap_or_else(|| "rally.car".to_string());
            running = start_race(&resources, &dir, &event, &file, &settings, Screen::Quick);
            if running.is_some() {
                screen = Screen::Race;
            }
        }
    }

    loop {
        let dt = get_frame_time().min(0.05);
        clear_background(theme::BACKDROP);

        if let Some(track) = &track {
            if is_key_pressed(KeyCode::M) {
                settings.music = !settings.music;
                settings.save(&settings_path);
                music_playing = settings.music;
                if !music_playing {
                    macroquad::audio::stop_sound(track);
                }
            }
            // The settings screen owns the volume; M and the row both feed it.
            if settings.music && !music_playing {
                play_sound(
                    track,
                    PlaySoundParams {
                        looped: true,
                        volume: settings.volume,
                    },
                );
                music_playing = true;
            }
            set_sound_volume(track, if music_playing { settings.volume } else { 0.0 });
        }

        match screen {
            Screen::Main => {
                if let menu::Action::Activate(choice) = menu::main_menu(&theme, &progress, &mut cursor)
                {
                    message.clear();
                    match choice {
                        0 => {
                            cursor = 0;
                            screen = Screen::Career;
                        }
                        1 => {
                            cursor = 0;
                            screen = Screen::Deluxe;
                        }
                        2 => {
                            cursor = 0;
                            screen = Screen::Quick;
                        }
                        3 => {
                            cursor = progress.car.min(cars.len().saturating_sub(1));
                            screen = Screen::Cars;
                        }
                        4 => {
                            cursor = 0;
                            screen = Screen::Options;
                        }
                        _ => break,
                    }
                }
                if !message.is_empty() {
                    text::draw_shadow(&message, 16.0, 12.0, 19.0, WHITE);
                }
            }

            Screen::Options => {
                match menu::options(&theme, &mut settings, &mut cursor, &mut confirming_reset) {
                    menu::OptionsAction::ResetCareer => {
                        progress = Progress::default();
                        progress.save(&save_path);
                        message = labels::get("reset_complete").to_string();
                    }
                    menu::OptionsAction::Back => {
                        settings.save(&settings_path);
                        cursor = 0;
                        screen = Screen::Main;
                    }
                    menu::OptionsAction::None => {}
                }
            }

            Screen::Career | Screen::Deluxe | Screen::Quick => {
                let (events, title) = match screen {
                    Screen::Career => (&career, labels::get("menu_career")),
                    Screen::Deluxe => (&deluxe, labels::get("menu_deluxe")),
                    _ => (&quick, labels::get("menu_quick")),
                };
                if events.is_empty() {
                    screen = Screen::Main;
                } else {
                    match menu::event_list(&theme, &progress, events, &mut cursor, title) {
                        menu::Action::Activate(index) => {
                            let event = events[index].clone();
                            if !progress.open(event.threshold) {
                                message = labels::format(
                                    "race_needs",
                                    &[&event.name.to_uppercase(), &event.threshold.to_string()],
                                );
                            } else {
                                let file = cars
                                    .get(progress.car)
                                    .map(|car| car.file.clone())
                                    .unwrap_or_else(|| "rally.car".to_string());
                                running = start_race(&resources, &dir, &event, &file, &settings, screen);
                                if running.is_some() {
                                    screen = Screen::Race;
                                } else {
                                    message = labels::format(
                                        "load_failed",
                                        &[&event.map.to_uppercase()],
                                    );
                                }
                            }
                        }
                        menu::Action::Back => {
                            cursor = 0;
                            screen = Screen::Main;
                        }
                        menu::Action::None => {}
                    }
                }
            }

            Screen::Cars => {
                if cars.is_empty() {
                    screen = Screen::Main;
                } else {
                    cursor = cursor.min(cars.len() - 1);
                    // Spin at the game's own rate: `u.b(float)` advances its
                    // angle by ten degrees a second and wraps at 360.
                    showroom_spin = (showroom_spin + 10.0 * dt) % 360.0;
                    if let Some((geometry, texture)) = showroom.get(cursor) {
                        draw_showroom(geometry, texture, showroom_spin);
                    }
                    match menu::car_list(&theme, &cars, &progress, &mut cursor) {
                        menu::Action::Activate(index) => {
                            progress.car = index;
                            progress.save(&save_path);
                            message = labels::format("car_selected", &[&cars[index].name]);
                            cursor = 0;
                            screen = Screen::Main;
                        }
                        menu::Action::Back => {
                            cursor = 0;
                            screen = Screen::Main;
                        }
                        menu::Action::None => {}
                    }
                }
            }

            Screen::Race => {
                let Some(run) = running.as_mut() else {
                    screen = Screen::Main;
                    continue;
                };
                if is_key_pressed(KeyCode::Escape) {
                    cursor = 0;
                    screen = Screen::Paused;
                } else {
                    let laps = env_number("KORA_LAPS", 99).unwrap_or(run.event.laps).max(1);
                    let finished = run.update(dt, laps, &settings);
                    run.draw_world(dt, &settings);
                    run.draw_hud(get_time(), laps, &settings);
                    if let Some(outcome) = finished {
                        // Keep the time if it beats the record, and pay the
                        // record's award the first time a race is passed.
                        let result =
                            progress.record(&run.event.key, outcome.total_time, run.event.award);
                        progress.save(&save_path);
                        if let Some(run) = running.as_mut() {
                            if let Some(outcome) = run.outcome.as_mut() {
                                outcome.gained = result.gained;
                                outcome.improved = result.improved;
                                outcome.previous_best = result.previous_best;
                            }
                        }
                        screen = Screen::Results;
                    }
                }
            }

            Screen::Paused => {
                let music = track.as_ref().map(|_| music_playing);
                match menu::pause_menu(&theme, &mut cursor, music) {
                    menu::Action::Activate(0) => screen = Screen::Race,
                    menu::Action::Activate(1) => {
                        if let Some(run) = running.as_mut() {
                            let laps =
                                env_number("KORA_LAPS", 99).unwrap_or(run.event.laps).max(1);
                            run.restart(laps);
                        }
                        screen = Screen::Race;
                    }
                    menu::Action::Activate(_) => {
                        running = None;
                        cursor = 0;
                        screen = Screen::Main;
                    }
                    _ => {}
                }
            }

            Screen::Results => {
                let action = match running.as_ref() {
                    Some(run) => menu::results(
                        &theme,
                        &run.event,
                        &progress,
                        run.outcome.as_ref().expect("an outcome"),
                    ),
                    None => menu::Action::Back,
                };
                if !matches!(action, menu::Action::None) {
                    let back = running.as_ref().map(|run| run.back).unwrap_or(Screen::Main);
                    running = None;
                    cursor = 0;
                    screen = back;
                }
            }
        }

        next_frame().await;
    }
}
