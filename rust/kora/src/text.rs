//! Text drawing for the menus and the HUD.
//!
//! This goes through macroquad's own text renderer, which rasterises glyphs
//! with `fontdue` at whatever pixel size is asked for, so the interface stays
//! sharp at every scale instead of magnifying a fixed bitmap.
//!
//! The pack's bitmap fonts (`/fonts/<name>` + `.tab` + `.png`) are still fully
//! decoded and documented - `tools/README.md` has the layouts and
//! `python3 -m kora fontimg` renders them - but the running game does not need
//! them: macroquad's `Font` is fontdue-based and takes TrueType outlines, so a
//! texture atlas cannot be loaded into it.

use macroquad::prelude::*;

/// Draw `text` with its top-left at `(x, y)`, returning its width.
pub fn draw(text: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    let metrics = measure_text(text, None, size as u16, 1.0);
    // `draw_text` places the baseline, so shift down by the offset for the
    // callers, which all think in terms of the top of a line.
    draw_text(text, x, y + metrics.offset_y, size, color);
    metrics.width
}

/// Width of `text` at `size`, for laying a row out.
pub fn width(text: &str, size: f32) -> f32 {
    measure_text(text, None, size as u16, 1.0).width
}

/// Draw with a small drop shadow, as the MIDlet does for its own text.
pub fn draw_shadow(label: &str, x: f32, y: f32, size: f32, color: Color) {
    let offset = (size * 0.06).max(1.0);
    draw(label, x + offset, y + offset, size, Color::new(0.0, 0.0, 0.0, 0.65));
    draw(label, x, y, size, color);
}
