use clankerdiff_ratatui::{composite_color, layered_style, page_color};
use clankerdiff_theme::{ReviewTheme, Rgba};
use ratatui::style::Color;

#[test]
fn transparent_foreground_preserves_the_page_color() {
    let background = Rgba::new(20, 40, 60, 255);
    assert_eq!(
        composite_color(Rgba::new(255, 0, 0, 0), background),
        Color::Rgb(20, 40, 60)
    );
    let theme = ReviewTheme::default();
    assert_eq!(
        page_color(&theme, theme.diff.foreground),
        composite_color(theme.diff.foreground, theme.diff.background)
    );
}

#[test]
fn layered_alpha_is_composited_once_per_layer() {
    let foreground = Rgba::new(200, 30, 50, 128);
    let background = Rgba::new(30, 200, 50, 128);
    let canvas = Rgba::new(30, 50, 200, 128);
    let opaque_canvas = canvas.over(Rgba::new(0, 0, 0, 255));
    let opaque_background = background.over(opaque_canvas);
    let style = layered_style(foreground, background, canvas);
    assert_eq!(style.bg, Some(composite_color(background, canvas)));
    assert_eq!(
        style.fg,
        Some(composite_color(foreground, opaque_background))
    );
}
