//! HUD text drawn with the game's own bitmap font.
//!
//! `/fonts/<name>` holds the advance widths and cell height, `<name>.tab`
//! maps characters to glyph slots and `<name>.png` is the atlas (see
//! `tools/README.md` for the byte layout).  Glyph rectangles are recovered by
//! wrapping the atlas at the image width.

use macroquad::prelude::*;

use crate::format;
use crate::pack::Resources;

pub struct GameFont {
    font: format::Font,
    rects: Vec<(u32, u32, u32, u32)>,
    texture: Texture2D,
}

impl GameFont {
    pub fn load(res: &Resources, name: &str) -> Option<GameFont> {
        let metrics = res.get(&format!("fonts/{name}"))?;
        let table = res
            .get(&format!("fonts/{name}.tab"))
            .and_then(|bytes| format::FontTable::parse(bytes));
        let font = format::Font::parse(metrics, table)?;
        let texture = Texture2D::from_file_with_format(
            res.get(&format!("fonts/{name}.png"))?,
            Some(ImageFormat::Png),
        );
        texture.set_filter(FilterMode::Linear);
        let rects = font.layout(texture.width() as u32);
        Some(GameFont {
            font,
            rects,
            texture,
        })
    }

    pub fn width(&self, text: &str, scale: f32) -> f32 {
        self.font.text_width(text) * scale
    }

    /// Draw *text* with its top-left at `(x, y)`.
    pub fn draw(&self, text: &str, x: f32, y: f32, scale: f32, color: Color) -> f32 {
        let mut pen = x;
        for ch in text.chars() {
            let glyph = self.font.glyph_for(ch);
            if let Some(&(gx, gy, gw, gh)) = self.rects.get(glyph) {
                if gw > 0 && gh > 0 {
                    draw_texture_ex(
                        &self.texture,
                        pen,
                        y,
                        color,
                        DrawTextureParams {
                            dest_size: Some(vec2(gw as f32 * scale, gh as f32 * scale)),
                            source: Some(Rect::new(gx as f32, gy as f32, gw as f32, gh as f32)),
                            ..Default::default()
                        },
                    );
                }
            }
            pen += self.font.width(ch) as f32 * scale;
        }
        pen - x
    }

    /// Draw with a one-pixel dark drop shadow, as the MIDlet does.
    pub fn draw_shadow(&self, text: &str, x: f32, y: f32, scale: f32, color: Color) {
        self.draw(text, x + scale, y + scale, scale, Color::new(0.0, 0.0, 0.0, 0.6));
        self.draw(text, x, y, scale, color);
    }
}
