//! Conversion from renderer-neutral diff styles to GPUI styles.

use diff_core::{FontStyle, Rgba};
use gpui::{FontStyle as GpuiFontStyle, FontWeight, HighlightStyle, Hsla, UnderlineStyle, px};

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
        font_weight: style.bold.then_some(FontWeight::BOLD),
        font_style: style.italic.then_some(GpuiFontStyle::Italic),
        underline: style.underline.then(|| UnderlineStyle {
            color: Some(color(foreground)),
            thickness: px(1.0),
            wavy: false,
        }),
        ..Default::default()
    }
}
