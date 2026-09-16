//! Text drawing for the menus and the HUD.
//!
//! Everything on screen is set in **Contrail One**, bundled in `fonts/` and
//! rasterised by macroquad's own text renderer (fontdue) at whatever pixel size
//! is asked for, so the interface stays sharp at every scale.
//!
//! Contrail One is Copyright (c) 2011 Sorkin Type Co, released under the SIL
//! Open Font License 1.1; the licence ships alongside it in `fonts/OFL.txt`.
//! The reserved font names are "Contrail" and "Contrail One", so the file is
//! redistributed unmodified and keeps its name.
//!
//! The pack's own bitmap fonts (`/fonts/<name>` + `.tab` + `.png`) are still
//! fully decoded and documented - `python3 -m kora font` prints a font's
//! metrics and character map and `tools/README.md` has the layouts - but they
//! cannot be used at runtime: macroquad's `Font` is fontdue-based and takes
//! TrueType outlines, so a texture atlas has no route in.

use std::sync::LazyLock;

use macroquad::prelude::*;
use macroquad::text::TextParams;

const FONT_BYTES: &[u8] = include_bytes!("../fonts/ContrailOne-Regular.ttf");

/// Loaded on first use, which is always inside the frame loop and therefore
/// after the window exists - rasterising a glyph needs a live graphics context.
/// `None` falls back to macroquad's default face.
static FONT: LazyLock<Option<Font>> = LazyLock::new(|| {
    match load_ttf_font_from_bytes(FONT_BYTES) {
        Ok(font) => Some(font),
        Err(error) => {
            eprintln!("could not load the bundled font, falling back: {error}");
            None
        }
    }
});

/// The loaded face, for anything that needs to rasterise with it - the UI
/// toolkit builds its own styles from a `Font`.
pub fn face() -> Option<&'static Font> {
    FONT.as_ref()
}

/// Draw `label` with its top-left at `(x, y)`, returning its width.
pub fn draw(label: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    let metrics = measure_text(label, face(), size as u16, 1.0);
    draw_text_ex(
        label,
        x,
        // `draw_text_ex` places the baseline, so shift by the offset to keep
        // the callers, which all think in terms of the top of a line, honest.
        y + metrics.offset_y,
        TextParams {
            font: face(),
            font_size: size as u16,
            font_scale: 1.0,
            color,
            ..Default::default()
        },
    );
    metrics.width
}

/// Width of `label` at `size`, for laying a row out.
pub fn width(label: &str, size: f32) -> f32 {
    measure_text(label, face(), size as u16, 1.0).width
}

/// Draw with a small drop shadow, as the MIDlet does for its own text.
pub fn draw_shadow(label: &str, x: f32, y: f32, size: f32, color: Color) {
    let offset = (size * 0.06).max(1.0);
    draw(label, x + offset, y + offset, size, Color::new(0.0, 0.0, 0.0, 0.65));
    draw(label, x, y, size, color);
}
