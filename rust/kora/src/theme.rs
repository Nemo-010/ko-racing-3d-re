//! The interface skin, built with macroquad's own UI toolkit.
//!
//! Colours and shapes are taken from the MIDlet's drawing code, which is all
//! hand-rolled `fillRect` calls: a light grey body (`0xDDDDDD`), a grey gradient
//! bar for a header (`0x999999` into `0x666666`), grey rows (`0x888888`) with a
//! lighter one under the cursor (`0xAAAAAA`), dark red (`0xAA0000`) for the one
//! you are about to choose, and black text on the light panels.
//!
//! The typeface is Contrail One, the face the rest of the port draws with.
//! Build this once, after the window exists and before the loop: it takes the
//! UI lock.

use macroquad::math::RectOffset;
use macroquad::prelude::*;
use macroquad::texture::Image;
use macroquad::ui::{root_ui, Skin, Style};

/// The dark the menus sit on, behind the panels.
pub const BACKDROP: Color = Color::new(0.05, 0.07, 0.12, 1.0);

/// `0x000000`, the shadow and the text on the light panels.
pub const INK: Color = Color::new(0.0, 0.0, 0.0, 1.0);
/// `0xDDDDDD`, the panel body.
pub const PANEL: Color = Color::new(0.867, 0.867, 0.867, 1.0);
/// `0x999999` into `0x666666`, the header gradient.
pub const BAR_TOP: Color = Color::new(0.600, 0.600, 0.600, 1.0);
pub const BAR_BOTTOM: Color = Color::new(0.400, 0.400, 0.400, 1.0);
/// `0x888888`, an unselected row.
pub const ROW: Color = Color::new(0.533, 0.533, 0.533, 1.0);
/// `0xAAAAAA`, the row under the cursor.
pub const ROW_SELECTED: Color = Color::new(0.667, 0.667, 0.667, 1.0);
/// `0xAA0000`, the choice about to be made.
pub const SPECIAL: Color = Color::new(0.667, 0.0, 0.0, 1.0);
/// `0x666666`, an item that is not available yet.
pub const DIM: Color = Color::new(0.400, 0.400, 0.400, 1.0);

fn shade(color: Color, amount: f32) -> Color {
    Color::new(
        (color.r + amount).clamp(0.0, 1.0),
        (color.g + amount).clamp(0.0, 1.0),
        (color.b + amount).clamp(0.0, 1.0),
        color.a,
    )
}

/// macroquad's UI paints a style from an image, so a flat colour is a
/// one-pixel image and a gradient is a small one.
fn flat(color: Color) -> Image {
    Image::gen_image_color(8, 8, color)
}

fn gradient(top: Color, bottom: Color) -> Image {
    let (width, height) = (8u16, 24u16);
    let mut image = Image::gen_image_color(width, height, top);
    for y in 0..height {
        let t = y as f32 / (height - 1) as f32;
        let color = Color::new(
            top.r + (bottom.r - top.r) * t,
            top.g + (bottom.g - top.g) * t,
            top.b + (bottom.b - top.b) * t,
            top.a + (bottom.a - top.a) * t,
        );
        for x in 0..width {
            image.set_pixel(x as u32, y as u32, color);
        }
    }
    image
}

/// The three states an item can be in.
pub struct Theme {
    pub normal: Skin,
    pub selected: Skin,
    pub locked: Skin,
}

pub fn build() -> Theme {
    let font = crate::text::face().cloned();

    let styled = |background: Image, text: Color, size: u16, margin: f32| -> Style {
        let builder = root_ui()
            .style_builder()
            .background(background)
            .text_color(text)
            .font_size(size)
            .margin(RectOffset::new(margin, margin, margin * 0.6, margin * 0.6));
        let builder = match &font {
            Some(font) => builder
                .with_font(font)
                .expect("the bundled face loads into the ui atlas"),
            None => builder,
        };
        builder.build()
    };

    let label = styled(flat(PANEL), INK, 20, 2.0);
    let window_style = styled(flat(PANEL), INK, 20, 12.0);
    let titlebar = styled(gradient(BAR_TOP, BAR_BOTTOM), WHITE, 22, 4.0);

    let button = |top: Color, bottom: Color, text: Color| -> Style {
        let builder = root_ui()
            .style_builder()
            .background(gradient(top, bottom))
            .background_hovered(gradient(shade(top, 0.07), shade(bottom, 0.07)))
            .background_clicked(gradient(shade(top, -0.10), shade(bottom, -0.10)))
            .background_margin(RectOffset::new(2.0, 2.0, 2.0, 2.0))
            .margin(RectOffset::new(10.0, 10.0, 4.0, 4.0))
            .text_color(text)
            .font_size(22);
        let builder = match &font {
            Some(font) => builder
                .with_font(font)
                .expect("the bundled face loads into the ui atlas"),
            None => builder,
        };
        builder.build()
    };

    // Rows are the panel grey; the cursor takes the red the MIDlet uses for the
    // level about to be started, and an item out of reach is dimmed.
    let normal = button(PANEL, shade(PANEL, -0.12), INK);
    let selected = button(shade(SPECIAL, 0.16), SPECIAL, WHITE);
    let locked = button(ROW, shade(ROW, -0.12), DIM);

    let skin = |button_style: Style| Skin {
        label_style: label.clone(),
        button_style,
        window_style: window_style.clone(),
        window_titlebar_style: titlebar.clone(),
        margin: 10.0,
        title_height: 30.0,
        scroll_width: 12.0,
        scroll_multiplier: 24.0,
        ..root_ui().default_skin()
    };

    Theme {
        normal: skin(normal),
        selected: skin(selected),
        locked: skin(locked),
    }
}
