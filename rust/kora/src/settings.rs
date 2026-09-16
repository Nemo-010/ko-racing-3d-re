//! Settings: what the OPTIONS screen changes, and where it lives.
//!
//! The MIDlet keeps its settings in a record store called `KORa_1.1.1` - the
//! version is part of the name - with record 1 holding a Java
//! `DataOutputStream` blob: seventeen ints, fifteen booleans, four bytes and
//! five strings, in the fixed order `al.b()` writes them.  Career progress is a
//! *different* store, `KORa_record`, and the online tour two more.  A desktop
//! port has no record stores, so this is a text file, and the fields below are
//! the part of that blob which still means something outside a handset.
//!
//! Every value names the game's own label for it, so the options screen shows
//! the original's words rather than the port's.
//!
//! One thing the original's blob carries is deliberately absent: the player
//! name (`101 YOUR NAME:`).  The MIDlet uploads it with a leaderboard entry and
//! shows it on the career screen; the port has neither, so a stored name would
//! be the one setting that changes nothing.

use std::fs;
use std::path::Path;

use macroquad::prelude::KeyCode;

/// How much of a track to build: `16 LOW`, `19 MEDIUM`, `15 HIGH` in ui.txt.
/// The MIDlet calls this the graphics detail (`al.d()`), and it is what gates
/// the mid and high detail layers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    Low,
    Medium,
    High,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Low, Quality::Medium, Quality::High];

    pub fn key(self) -> &'static str {
        match self {
            Quality::Low => "low",
            Quality::Medium => "medium",
            Quality::High => "high",
        }
    }

    pub fn next(self) -> Quality {
        match self {
            Quality::Low => Quality::Medium,
            Quality::Medium => Quality::High,
            Quality::High => Quality::Low,
        }
    }

    pub fn previous(self) -> Quality {
        match self {
            Quality::Low => Quality::High,
            Quality::Medium => Quality::Low,
            Quality::High => Quality::Medium,
        }
    }
}

/// `214 BEHIND CAR`, `22 FAR CAMERA`, `21 INSIDE CAR`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Camera {
    Chase,
    Far,
    Inside,
}

impl Camera {
    pub fn key(self) -> &'static str {
        match self {
            Camera::Chase => "camera_behind",
            Camera::Far => "camera_far",
            Camera::Inside => "camera_inside",
        }
    }

    pub fn next(self) -> Camera {
        match self {
            Camera::Chase => Camera::Far,
            Camera::Far => Camera::Inside,
            Camera::Inside => Camera::Chase,
        }
    }

    pub fn previous(self) -> Camera {
        match self {
            Camera::Chase => Camera::Inside,
            Camera::Far => Camera::Chase,
            Camera::Inside => Camera::Far,
        }
    }

    /// How far back and how high the camera sits, and how far it can see.
    pub fn placement(self) -> (f32, f32) {
        match self {
            Camera::Chase => (5.0, 2.2),
            Camera::Far => (9.0, 4.2),
            Camera::Inside => (0.0, 0.42),
        }
    }
}

/// `20 NEAR`, `19 MEDIUM`, `18 FAR` - how far the camera draws.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Visibility {
    Near,
    Medium,
    Far,
}

impl Visibility {
    pub fn key(self) -> &'static str {
        match self {
            Visibility::Near => "near",
            Visibility::Medium => "medium",
            Visibility::Far => "far",
        }
    }

    pub fn next(self) -> Visibility {
        match self {
            Visibility::Near => Visibility::Medium,
            Visibility::Medium => Visibility::Far,
            Visibility::Far => Visibility::Near,
        }
    }

    pub fn previous(self) -> Visibility {
        match self {
            Visibility::Near => Visibility::Far,
            Visibility::Medium => Visibility::Near,
            Visibility::Far => Visibility::Medium,
        }
    }

    pub fn far_plane(self) -> f32 {
        match self {
            Visibility::Near => 900.0,
            Visibility::Medium => 2000.0,
            Visibility::Far => 4000.0,
        }
    }
}

/// `27 CLASSIC`, `28 LEFT-HANDED`, `29 RIGHT-HANDED`.
///
/// On a handset the scheme picks which side of the keypad you drive with.  On a
/// keyboard the same idea maps onto three groups, and the scheme selects them
/// outright rather than quietly accepting all of them - a setting that changes
/// nothing is not a setting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scheme {
    Classic,
    LeftHanded,
    RightHanded,
}

impl Scheme {
    pub fn key(self) -> &'static str {
        match self {
            Scheme::Classic => "scheme_classic",
            Scheme::LeftHanded => "scheme_left",
            Scheme::RightHanded => "scheme_right",
        }
    }

    pub fn next(self) -> Scheme {
        match self {
            Scheme::Classic => Scheme::LeftHanded,
            Scheme::LeftHanded => Scheme::RightHanded,
            Scheme::RightHanded => Scheme::Classic,
        }
    }

    pub fn previous(self) -> Scheme {
        match self {
            Scheme::Classic => Scheme::RightHanded,
            Scheme::LeftHanded => Scheme::Classic,
            Scheme::RightHanded => Scheme::LeftHanded,
        }
    }

    /// `(accelerate, brake)`.
    pub fn throttle_keys(self) -> (KeyCode, KeyCode) {
        match self {
            Scheme::Classic => (KeyCode::Up, KeyCode::Down),
            Scheme::LeftHanded => (KeyCode::W, KeyCode::S),
            Scheme::RightHanded => (KeyCode::Kp8, KeyCode::Kp2),
        }
    }

    /// `(left, right)`.
    pub fn steer_keys(self) -> (KeyCode, KeyCode) {
        match self {
            Scheme::Classic => (KeyCode::Left, KeyCode::Right),
            Scheme::LeftHanded => (KeyCode::A, KeyCode::D),
            Scheme::RightHanded => (KeyCode::Kp4, KeyCode::Kp6),
        }
    }
}

pub struct Settings {
    pub quality: Quality,
    pub camera: Camera,
    pub visibility: Visibility,
    /// The `.bck` sky, or a flat colour if off (`10 BACKGROUND`, `42 OFF`).
    pub background: bool,
    /// `13 HUD`.
    pub hud: bool,
    pub scheme: Scheme,
    /// `26 AUTO-THROTTLE`: the car accelerates on its own and you only steer.
    pub auto_throttle: bool,
    /// `197 MUSIC`.
    pub music: bool,
    /// `50 VOLUME`, 0.0 to 1.0.
    pub volume: f32,
}

pub const MAX_VOLUME_STEPS: i32 = 10;

impl Default for Settings {
    fn default() -> Self {
        Settings {
            quality: Quality::High,
            camera: Camera::Chase,
            visibility: Visibility::Far,
            background: true,
            hud: true,
            scheme: Scheme::Classic,
            auto_throttle: false,
            music: true,
            volume: 0.35,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        let mut settings = Settings::default();
        let Ok(text) = fs::read_to_string(path) else {
            return settings;
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once(' ') else {
                continue;
            };
            let flag = value.trim() == "on";
            match key.trim() {
                "quality" => {
                    settings.quality = match value.trim() {
                        "low" => Quality::Low,
                        "medium" => Quality::Medium,
                        _ => Quality::High,
                    }
                }
                "camera" => {
                    settings.camera = match value.trim() {
                        "far" => Camera::Far,
                        "inside" => Camera::Inside,
                        _ => Camera::Chase,
                    }
                }
                "visibility" => {
                    settings.visibility = match value.trim() {
                        "near" => Visibility::Near,
                        "medium" => Visibility::Medium,
                        _ => Visibility::Far,
                    }
                }
                "scheme" => {
                    settings.scheme = match value.trim() {
                        "left" => Scheme::LeftHanded,
                        "right" => Scheme::RightHanded,
                        _ => Scheme::Classic,
                    }
                }
                "background" => settings.background = flag,
                "hud" => settings.hud = flag,
                "auto_throttle" => settings.auto_throttle = flag,
                "music" => settings.music = flag,
                "volume" => {
                    settings.volume = value.trim().parse().unwrap_or(settings.volume).clamp(0.0, 1.0)
                }
                _ => {}
            }
        }
        settings
    }

    pub fn save(&self, path: &Path) {
        let flag = |on: bool| if on { "on" } else { "off" };
        let mut text = String::new();
        text.push_str(&format!("quality {}\n", self.quality.key()));
        text.push_str(&format!(
            "camera {}\n",
            match self.camera {
                Camera::Chase => "chase",
                Camera::Far => "far",
                Camera::Inside => "inside",
            }
        ));
        text.push_str(&format!(
            "visibility {}\n",
            match self.visibility {
                Visibility::Near => "near",
                Visibility::Medium => "medium",
                Visibility::Far => "far",
            }
        ));
        text.push_str(&format!(
            "scheme {}\n",
            match self.scheme {
                Scheme::Classic => "classic",
                Scheme::LeftHanded => "left",
                Scheme::RightHanded => "right",
            }
        ));
        text.push_str(&format!("background {}\n", flag(self.background)));
        text.push_str(&format!("hud {}\n", flag(self.hud)));
        text.push_str(&format!("auto_throttle {}\n", flag(self.auto_throttle)));
        text.push_str(&format!("music {}\n", flag(self.music)));
        text.push_str(&format!("volume {:.2}\n", self.volume));
        if let Err(error) = fs::write(path, text) {
            eprintln!("could not save settings to {}: {error}", path.display());
        }
    }

    /// The volume in steps, for a menu that counts rather than slides.
    pub fn volume_steps(&self) -> i32 {
        (self.volume * MAX_VOLUME_STEPS as f32).round() as i32
    }

    pub fn set_volume_steps(&mut self, steps: i32) {
        self.volume = (steps.clamp(0, MAX_VOLUME_STEPS) as f32) / MAX_VOLUME_STEPS as f32;
    }

    pub fn cycle_volume(&mut self, forward: bool) {
        let steps = self.volume_steps() + if forward { 1 } else { -1 };
        self.set_volume_steps(if steps > MAX_VOLUME_STEPS {
            0
        } else if steps < 0 {
            MAX_VOLUME_STEPS
        } else {
            steps
        });
    }
}
