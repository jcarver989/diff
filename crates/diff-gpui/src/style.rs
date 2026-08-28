//! Conversion from renderer-neutral diff styles to GPUI styles.

use diff_core::{DiffPalette, DiffTone, FontStyle, Rgba};
use gpui::{FontStyle as GpuiFontStyle, FontWeight, HighlightStyle, Hsla};

/// Converts a renderer-neutral sRGB color to GPUI's HSLA color.
#[must_use]
pub fn color(value: Rgba) -> Hsla {
    gpui::rgba(
        (u32::from(value.r) << 24)
            | (u32::from(value.g) << 16)
            | (u32::from(value.b) << 8)
            | u32::from(value.a),
    )
    .into()
}

/// Converts a syntax-highlight modifier and color to a GPUI highlight style.
#[must_use]
pub fn highlight_style(foreground: Rgba, style: FontStyle) -> HighlightStyle {
    HighlightStyle {
        color: Some(color(foreground)),
        font_weight: style.contains(FontStyle::BOLD).then_some(FontWeight::BOLD),
        font_style: style
            .contains(FontStyle::ITALIC)
            .then_some(GpuiFontStyle::Italic),
        underline: style
            .contains(FontStyle::UNDERLINE)
            .then_some(gpui::UnderlineStyle {
                color: Some(color(foreground)),
                thickness: gpui::px(1.0),
                wavy: false,
            }),
        ..Default::default()
    }
}

/// Returns the semantic foreground for a diff cell.
#[must_use]
pub const fn tone_foreground(palette: &DiffPalette, tone: DiffTone) -> Rgba {
    match tone {
        DiffTone::Added => palette.addition,
        DiffTone::Removed => palette.deletion,
        DiffTone::Context | DiffTone::Meta => palette.foreground,
    }
}

/// Returns the semantic background for a diff cell.
#[must_use]
pub const fn tone_background(palette: &DiffPalette, tone: DiffTone) -> Rgba {
    match tone {
        DiffTone::Added => palette.addition_background,
        DiffTone::Removed => palette.deletion_background,
        DiffTone::Context | DiffTone::Meta => palette.background,
    }
}
