//! Where the save and the settings live.
//!
//! The MIDlet has record stores and does not care where the handset puts them.
//! A desktop port does: on Linux the convention is the XDG directories, with
//! configuration and data kept apart -
//!
//! ```text
//! $XDG_CONFIG_HOME/kora/settings.txt    (~/.config/kora/settings.txt)
//! $XDG_DATA_HOME/kora/save.txt          (~/.local/share/kora/save.txt)
//! ```
//!
//! - and on macOS and Windows the equivalent per-user locations.
//!
//! **macroquad does not provide any of this.**  Its `load_file` is read-only,
//! and its "storage" module is an in-memory map.  On Android that matters:
//! `load_file` reads the APK's assets through `AAssetManager`, nothing in the
//! stack calls `getFilesDir()` and nothing changes the working directory, so a
//! relative path there points at `/` and fails.  An app may only write to its
//! own directory, which is what [`data_dir`] works out from the package name.
//! On the web there is no filesystem at all; `load_file` is an HTTP fetch, and
//! persistence means `localStorage` or IndexedDB.
//!
//! Everything here can be overridden with `KORA_CONFIG_DIR` and
//! `KORA_DATA_DIR`, which is also how a launcher hands a sandboxed build a
//! directory it is allowed to write to.

use std::fs;
use std::path::PathBuf;

/// Where a file belongs: configuration, or the data the program produces.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Config,
    Data,
}

/// The platform rules, as a value so that every one of them can be checked on
/// any machine rather than only on the one it applies to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// Linux and the other unices: XDG.
    Unix,
    MacOs,
    Windows,
    Android,
    Web,
}

pub const APP: &str = "kora";

/// The platform this was compiled for.
#[cfg(target_os = "android")]
pub const fn target() -> Target {
    Target::Android
}
#[cfg(target_arch = "wasm32")]
pub const fn target() -> Target {
    Target::Web
}
#[cfg(target_os = "macos")]
pub const fn target() -> Target {
    Target::MacOs
}
#[cfg(target_os = "windows")]
pub const fn target() -> Target {
    Target::Windows
}
#[cfg(not(any(
    target_os = "android",
    target_os = "macos",
    target_os = "windows",
    target_arch = "wasm32"
)))]
pub const fn target() -> Target {
    Target::Unix
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.is_empty())
}

fn override_name(dir: Dir) -> &'static str {
    match dir {
        Dir::Config => "KORA_CONFIG_DIR",
        Dir::Data => "KORA_DATA_DIR",
    }
}

/// Resolve the directory for `dir` on `target`, reading the environment through
/// `get`.  `None` means the platform has nowhere to put it.
pub fn resolve(dir: Dir, target: Target, get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    if let Some(explicit) = non_empty(get(override_name(dir))) {
        return Some(PathBuf::from(explicit));
    }

    match target {
        Target::Unix => {
            // The XDG spec: an empty variable means unset, and the fallbacks
            // are relative to $HOME.
            let (variable, fallback) = match dir {
                Dir::Config => ("XDG_CONFIG_HOME", ".config"),
                Dir::Data => ("XDG_DATA_HOME", ".local/share"),
            };
            let base = non_empty(get(variable))
                .map(PathBuf::from)
                .or_else(|| non_empty(get("HOME")).map(|home| PathBuf::from(home).join(fallback)))?;
            Some(base.join(APP))
        }
        Target::MacOs => non_empty(get("HOME"))
            .map(|home| PathBuf::from(home).join("Library/Application Support").join(APP)),
        Target::Windows => {
            non_empty(get("APPDATA")).map(|appdata| PathBuf::from(appdata).join(APP))
        }
        Target::Android => android_dir(get),
        // No filesystem to write to: macroquad's own file access is an HTTP
        // fetch, so this would need localStorage or IndexedDB instead.
        Target::Web => None,
    }
}

/// An app on Android may only write to its own directory, and the only
/// dependency-free way to find it is the package name the platform puts in
/// `/proc/self/cmdline`.  The private files directory follows from it.
///
/// Nothing in macroquad or miniquad exposes this, so if the read fails the
/// caller falls back to the working directory - which is not writable there,
/// hence the `KORA_DATA_DIR` escape hatch.
fn android_dir(_get: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        let command = fs::read("/proc/self/cmdline").ok()?;
        let first = command.split(|byte| *byte == 0).next()?;
        let package = std::str::from_utf8(first).ok()?;
        // A secondary process is named "<package>:<suffix>".
        let package = package.split(':').next()?;
        if package.is_empty() {
            return None;
        }
        let dir = PathBuf::from(format!("/data/data/{package}")).join("files").join(APP);
        let _ = fs::create_dir_all(&dir);
        return Some(dir);
    }
    #[cfg(not(target_os = "android"))]
    {
        // Only reached by the tests, which exercise the Android rule on a host.
        None
    }
}

/// The directory for `dir` on this machine, if it has one.
pub fn dir(kind: Dir) -> Option<PathBuf> {
    resolve(kind, target(), &|name| std::env::var(name).ok())
}

/// The file to use for `name`, creating the directory if needed.
///
/// A file already sitting beside the working directory wins, so that the move
/// to a per-user directory does not orphan a save that predates it.  Without
/// anywhere to put it - Android without a readable package name, or the web -
/// this stays relative, which is what the port did before.
pub fn file(kind: Dir, name: &str) -> PathBuf {
    let beside = PathBuf::from(name);
    if beside.exists() {
        return beside;
    }
    match dir(kind) {
        Some(directory) => {
            if let Err(error) = fs::create_dir_all(&directory) {
                eprintln!("could not create {}: {error}", directory.display());
                return beside;
            }
            directory.join(name)
        }
        None => beside,
    }
}
