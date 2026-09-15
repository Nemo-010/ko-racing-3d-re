//! K.O. Racing 3D - Rust port of the Jollybox J2ME racer.
//!
//! This library exposes the asset formats (`format`), the resource archive
//! reader (`pack`) and the engine pieces (`scene`, `physics`, `text`) so the
//! geometry pipeline can be tested without a GPU.  The binary in
//! `src/main.rs` is the playable macroquad front-end.

#![allow(dead_code)]

pub mod format;
pub mod pack;
pub mod physics;
pub mod scene;
pub mod text;
