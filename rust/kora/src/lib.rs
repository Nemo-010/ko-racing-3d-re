//! K.O. Racing 3D - Rust port of the Jollybox J2ME racer.
//!
//! This library exposes the asset formats (`format`), the resource archive
//! reader (`pack`) and the engine pieces (`scene`, `physics`, `text`) so the
//! geometry pipeline can be tested without a GPU.  The binary in
//! `src/main.rs` is the playable macroquad front-end.

#![allow(dead_code)]

pub mod ai;
pub mod campaign;
pub mod format;
pub mod grid;
pub mod hud;
pub mod labels;
pub mod map;
pub mod menu;
pub mod music;
pub mod pack;
pub mod paths;
pub mod physics;
pub mod progress;
pub mod race;
pub mod scene;
pub mod settings;
pub mod sky;
pub mod space;
pub mod text;
pub mod theme;
