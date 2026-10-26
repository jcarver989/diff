use clankerdiff_theme::{ReviewTheme, Rgba};
use ratatui::style::{Color, Style};

#[must_use]
pub const fn composite_color(value: Rgba, background: Rgba) -> Color {
    let value = value.over(background.over(Rgba::new(0, 0, 0, 255)));
    Color::Rgb(value.r, value.g, value.b)
}

#[must_use]
pub const fn page_color(theme: &ReviewTheme, value: Rgba) -> Color {
    composite_color(value, theme.diff.background)
}

#[must_use]
pub fn layered_style(foreground: Rgba, background: Rgba, canvas: Rgba) -> Style {
    let canvas = canvas.over(Rgba::new(0, 0, 0, 255));
    let background = background.over(canvas);
    Style::new()
        .fg(composite_color(foreground, background))
        .bg(composite_color(background, canvas))
}

pub(crate) const fn native_color(value: Rgba, background: Color) -> Color {
    let rgb = match background {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Red => (128, 0, 0),
        Color::Green => (0, 128, 0),
        Color::Yellow => (128, 128, 0),
        Color::Blue => (0, 0, 128),
        Color::Magenta => (128, 0, 128),
        Color::Cyan => (0, 128, 128),
        Color::Gray => (192, 192, 192),
        Color::DarkGray => (128, 128, 128),
        Color::LightRed => (255, 0, 0),
        Color::LightGreen => (0, 255, 0),
        Color::LightYellow => (255, 255, 0),
        Color::LightBlue => (0, 0, 255),
        Color::LightMagenta => (255, 0, 255),
        Color::LightCyan => (0, 255, 255),
        Color::White => (255, 255, 255),
        Color::Black | Color::Reset => (0, 0, 0),
        Color::Indexed(index) => return indexed_color(value, index),
    };
    composite_color(value, Rgba::new(rgb.0, rgb.1, rgb.2, 255))
}

const fn indexed_color(value: Rgba, index: u8) -> Color {
    const BASIC: [Color; 16] = [
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::Gray,
        Color::DarkGray,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
        Color::White,
    ];
    const LEVEL: [u8; 6] = [0, 95, 135, 175, 215, 255];
    if index < 16 {
        return native_color(value, BASIC[index as usize]);
    }
    if index >= 232 {
        let gray = 8 + (index - 232) * 10;
        return composite_color(value, Rgba::new(gray, gray, gray, 255));
    }
    let index = (index - 16) as usize;
    composite_color(
        value,
        Rgba::new(
            LEVEL[index / 36],
            LEVEL[(index / 6) % 6],
            LEVEL[index % 6],
            255,
        ),
    )
}
