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

use macroquad::models::{draw_mesh, Mesh};
use macroquad::prelude::*;

use kora::ai::AiDriver;
use kora::campaign::{self, RaceEvent};
use kora::labels;
use kora::menu::{self, Outcome};
use kora::physics::{CarControl, Tuning, World};
use kora::progress::{self, Progress};
use kora::race::Race;
use kora::text;
use kora::{pack, scene};

fn assets_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("KORA_ASSETS") {
        return PathBuf::from(dir);
    }
    for candidate in ["assets", "../assets", "rust/kora/assets"] {
        let path = PathBuf::from(candidate);
        if path.join("data").exists() {
            return path;
        }
    }
    PathBuf::from("assets")
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
    Career,
    Deluxe,
    Quick,
    Cars,
    Garage,
    Race,
    Paused,
    Results,
}

/// A race in progress: the track, the cars and the running order.
struct Running {
    event: RaceEvent,
    back: Screen,
    track: scene::Track,
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
    progress: &Progress,
    back: Screen,
) -> Option<Running> {
    let mut track = scene::build_themed(dir, resources, &event.map, event.theme);
    track.attach_textures(resources);

    let car_def = resources
        .get(&format!("cars/{car_file}"))
        .and_then(|bytes| kora::format::Car::parse(bytes))?;
    let geometry = scene::build_car(resources, &car_def)?;
    let texture = scene::load_car_texture(resources, &geometry);
    // Anything bought in the garage changes the car the race is driven in.
    let tuning = Tuning::from_stats(progress.stats(car_file, car_def.stats));

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
    fn update(&mut self, dt: f32, laps: u32) -> Option<Outcome> {
        let mut controls = vec![CarControl::default(); self.world.cars.len()];
        controls[self.player].throttle = if is_key_down(KeyCode::Up) || is_key_down(KeyCode::W) {
            1.0
        } else if is_key_down(KeyCode::Down) || is_key_down(KeyCode::S) {
            -0.6
        } else {
            0.0
        };
        // Positive steering turns the wheels left (about +Y).
        controls[self.player].steer = if is_key_down(KeyCode::Left) || is_key_down(KeyCode::A) {
            1.0
        } else if is_key_down(KeyCode::Right) || is_key_down(KeyCode::D) {
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
                medal: progress::medal_for_place(place),
                gained: 0,
                total_time: race.finish_time.unwrap_or(0.0),
                best_lap: race.best,
                laps,
                cars: self.world.cars.len(),
            });
            return self.outcome.clone();
        }
        None
    }

    fn draw_world(&mut self, dt: f32) {
        let (position, rotation) = self.world.pose(self.player);
        let forward = rotation * vec3(0.0, 0.0, -1.0);
        let up = vec3(0.0, 1.0, 0.0);
        let desired = position - forward * 5.0 + up * 2.2;
        self.camera = self.camera.lerp(desired, (dt * 5.0).min(1.0));

        let mut camera = Camera3D::default();
        camera.position = self.camera;
        camera.target = position + forward * 3.0 + up * 0.8;
        camera.up = up;
        camera.fovy = 62f32.to_radians();
        camera.z_far = 4000.0;
        set_camera(&camera);

        clear_background(Color::new(0.53, 0.81, 0.92, 1.0));
        for mesh in &self.track.meshes {
            draw_mesh(mesh);
        }
        for index in 0..self.world.cars.len() {
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

    fn draw_hud(&self, now: f64, laps: u32) {
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
    }
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
    println!(
        "  {} cars, {} career events, {} deluxe events, {} quick-race tracks",
        cars.len(),
        career.len(),
        deluxe.len(),
        quick.len()
    );

    let save_path = PathBuf::from(
        std::env::var("KORA_SAVE").unwrap_or_else(|_| "kora-save.txt".to_string()),
    );
    let mut progress = Progress::load(&save_path);
    if let Ok(name) = std::env::var("KORA_CAR") {
        if let Some(index) = cars.iter().position(|car| format!("cars/{}", car.file) == name) {
            progress.car = index;
        }
    }

    // `KORA_SKIP_MENU=1` boots straight into a race, which is handy from a
    // shell and keeps the environment overrides meaningful.
    let mut screen = Screen::Main;
    let mut cursor = 0usize;
    let mut stat_cursor = 0usize;
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
            running = start_race(&resources, &dir, &event, &file, &progress, Screen::Quick);
            if running.is_some() {
                screen = Screen::Race;
            }
        }
    }

    loop {
        let dt = get_frame_time().min(0.05);
        clear_background(menu::BACKDROP);

        match screen {
            Screen::Main => {
                menu::draw_main(&progress, cursor);
                if !message.is_empty() {
                    text::draw_shadow(&message, 16.0, screen_height() - 44.0, 19.0, WHITE);
                }
                let items = menu::MAIN_ITEMS.len();
                if is_key_pressed(KeyCode::Up) {
                    cursor = (cursor + items - 1) % items;
                }
                if is_key_pressed(KeyCode::Down) {
                    cursor = (cursor + 1) % items;
                }
                if is_key_pressed(KeyCode::Enter) {
                    message.clear();
                    match cursor {
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
                            cursor = progress.car.min(cars.len().saturating_sub(1));
                            stat_cursor = 0;
                            screen = Screen::Garage;
                        }
                        _ => break,
                    }
                }
            }

            Screen::Career | Screen::Deluxe | Screen::Quick => {
                let (events, title) = match screen {
                    Screen::Career => (&career, "CAREER"),
                    Screen::Deluxe => (&deluxe, "DELUXE"),
                    _ => (&quick, "QUICK RACE"),
                };
                if events.is_empty() {
                    screen = Screen::Main;
                } else {
                    cursor = cursor.min(events.len() - 1);
                    menu::draw_events(&progress, events, cursor, title);
                    if is_key_pressed(KeyCode::Up) {
                        cursor = cursor.saturating_sub(1);
                    }
                    if is_key_pressed(KeyCode::Down) {
                        cursor = (cursor + 1).min(events.len() - 1);
                    }
                    if is_key_pressed(KeyCode::Escape) {
                        cursor = 0;
                        screen = Screen::Main;
                    }
                    if is_key_pressed(KeyCode::Enter) {
                        let event = events[cursor].clone();
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
                            running =
                                start_race(&resources, &dir, &event, &file, &progress, screen);
                            if running.is_some() {
                                screen = Screen::Race;
                            } else {
                                message = labels::format("load_failed", &[&event.map.to_uppercase()]);
                            }
                        }
                    }
                }
            }

            Screen::Cars => {
                if cars.is_empty() {
                    screen = Screen::Main;
                } else {
                    cursor = cursor.min(cars.len() - 1);
                    menu::draw_cars(&cars, &progress, cursor, None, "");
                    if is_key_pressed(KeyCode::Up) {
                        cursor = cursor.saturating_sub(1);
                    }
                    if is_key_pressed(KeyCode::Down) {
                        cursor = (cursor + 1).min(cars.len() - 1);
                    }
                    if is_key_pressed(KeyCode::Escape) {
                        cursor = 0;
                        screen = Screen::Main;
                    }
                    if is_key_pressed(KeyCode::Enter) {
                        progress.car = cursor;
                        progress.save(&save_path);
                        message = labels::format("car_selected", &[&cars[cursor].name]);
                        cursor = 0;
                        screen = Screen::Main;
                    }
                }
            }

            Screen::Garage => {
                if cars.is_empty() {
                    screen = Screen::Main;
                } else {
                    cursor = cursor.min(cars.len() - 1);
                    let stat = stat_cursor.min(3);
                    menu::draw_cars(&cars, &progress, cursor, Some(stat), &message);

                    if is_key_pressed(KeyCode::Up) {
                        cursor = cursor.saturating_sub(1);
                    }
                    if is_key_pressed(KeyCode::Down) {
                        cursor = (cursor + 1).min(cars.len() - 1);
                    }
                    if is_key_pressed(KeyCode::Left) {
                        stat_cursor = stat.saturating_sub(1);
                    }
                    if is_key_pressed(KeyCode::Right) {
                        stat_cursor = (stat + 1).min(3);
                    }
                    if is_key_pressed(KeyCode::Escape) {
                        cursor = 0;
                        message.clear();
                        screen = Screen::Main;
                    }
                    if is_key_pressed(KeyCode::Enter) {
                        let file = cars[cursor].file.clone();
                        let base = cars[cursor].stats;
                        let name = cars[cursor].name.clone();
                        match progress.buy_upgrade(&file, base, stat) {
                            Some(cost) => {
                                progress.save(&save_path);
                                message = labels::format(
                                    "garage_upgraded",
                                    &[
                                        &name,
                                        &labels::stat_name(stat).to_lowercase(),
                                        &cost.to_string(),
                                    ],
                                );
                            }
                            None => {
                                message = match progress.next_upgrade_cost(&file, base, stat) {
                                    None => labels::format(
                                        "garage_max",
                                        &[labels::stat_name(stat)],
                                    ),
                                    Some(cost) => labels::format(
                                        "garage_poor",
                                        &[&cost.to_string()],
                                    ),
                                };
                            }
                        }
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
                    let finished = run.update(dt, laps);
                    run.draw_world(dt);
                    run.draw_hud(get_time(), laps);
                    if let Some(outcome) = finished {
                        // Award the medal once, and only for an improvement.
                        let (medal, gained) =
                            progress.record(&run.event.key, outcome.place, run.event.award);
                        progress.save(&save_path);
                        if let Some(run) = running.as_mut() {
                            if let Some(outcome) = run.outcome.as_mut() {
                                outcome.medal = medal;
                                outcome.gained = gained;
                            }
                        }
                        screen = Screen::Results;
                    }
                }
            }

            Screen::Paused => {
                clear_background(menu::BACKDROP);
                menu::draw_pause(cursor);
                if is_key_pressed(KeyCode::Up) {
                    cursor = (cursor + 2) % 3;
                }
                if is_key_pressed(KeyCode::Down) {
                    cursor = (cursor + 1) % 3;
                }
                if is_key_pressed(KeyCode::Escape) {
                    if let Some(run) = running.as_mut() {
                        let laps = env_number("KORA_LAPS", 99).unwrap_or(run.event.laps).max(1);
                        run.restart(laps);
                    }
                    screen = Screen::Race;
                }
                if is_key_pressed(KeyCode::Enter) {
                    match cursor {
                        0 => screen = Screen::Race,
                        1 => {
                            if let Some(run) = running.as_mut() {
                                let laps =
                                    env_number("KORA_LAPS", 99).unwrap_or(run.event.laps).max(1);
                                run.restart(laps);
                            }
                            screen = Screen::Race;
                        }
                        _ => {
                            running = None;
                            cursor = 0;
                            screen = Screen::Main;
                        }
                    }
                }
            }

            Screen::Results => {
                if let Some(run) = running.as_ref() {
                    menu::draw_results(&run.event, &progress, run.outcome.as_ref().expect("outcome"));
                }
                if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Escape) {
                    let back = running.as_ref().map(|run| run.back).unwrap_or(Screen::Main);
                    cursor = 0;
                    running = None;
                    screen = back;
                }
            }
        }

        next_frame().await;
    }
}
