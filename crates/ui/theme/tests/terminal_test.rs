use clankerdiff_theme::{ReviewTheme, Rgba, TerminalPalette};
use std::error::Error;

#[test]
fn a_theme_with_a_canonical_terminal_palette_uses_it() -> Result<(), Box<dyn Error>> {
    let palette = ReviewTheme::builtin("dracula")?.terminal_palette();

    assert_eq!(palette.ansi[1], Rgba::new(0xff, 0x55, 0x55, 255));
    Ok(())
}

#[test]
fn other_themes_derive_their_ansi_colors_from_the_ui() -> Result<(), Box<dyn Error>> {
    let theme = ReviewTheme::builtin("alabaster")?;
    let palette = theme.terminal_palette();

    let red = theme.ui.destructive;
    assert_eq!(palette.ansi[1], Rgba::new(red.r, red.g, red.b, 255));
    assert_eq!(palette.background, Rgba::new(theme.ui.canvas.r, theme.ui.canvas.g, theme.ui.canvas.b, 255));
    Ok(())
}

#[test]
fn indexed_colors_follow_the_xterm_table() {
    let palette = TerminalPalette::default();

    assert_eq!(palette.indexed(3), palette.ansi[3]);
    assert_eq!(palette.indexed(196), Rgba::new(255, 0, 0, 255));
    assert_eq!(palette.indexed(232), Rgba::new(8, 8, 8, 255));
}

#[test]
fn dimmed_colors_move_toward_the_background() {
    let palette = TerminalPalette::default();

    let dimmed = palette.dim(palette.foreground);

    assert!(dimmed.r < palette.foreground.r && dimmed.r > palette.background.r);
}
