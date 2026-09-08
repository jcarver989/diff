//! Bundled fonts shared by the native and web GPUI hosts.

use gpui::{App, Result};
use std::borrow::Cow;

/// The bundled font family used by the diff viewer and its hosts.
pub const DEFAULT_FONT_FAMILY: &str = "Lilex";

/// The default viewer font size in logical pixels for a roomy display.
pub const DEFAULT_FONT_SIZE: f32 = 16.0;

/// The narrowest viewport width that keeps the default font size.
pub const FULL_SIZE_VIEWPORT_WIDTH: f32 = 1800.0;

/// The smallest font size automatic display scaling selects.
pub const MIN_AUTO_FONT_SIZE: f32 = 13.0;

/// Returns the default viewer font size for a viewport width in logical pixels.
///
/// Narrower viewports (laptop displays) start smaller so text occupies a
/// similar fraction of the screen as the default size on large monitors.
/// Viewports at or above [`FULL_SIZE_VIEWPORT_WIDTH`] keep [`DEFAULT_FONT_SIZE`].
#[must_use]
pub fn default_font_size_for_viewport_width(viewport_width: f32) -> f32 {
    if !viewport_width.is_finite() {
        return DEFAULT_FONT_SIZE;
    }
    let ratio = (viewport_width / FULL_SIZE_VIEWPORT_WIDTH).clamp(0.0, 1.0);
    (MIN_AUTO_FONT_SIZE + (DEFAULT_FONT_SIZE - MIN_AUTO_FONT_SIZE) * ratio * ratio)
        .clamp(MIN_AUTO_FONT_SIZE, DEFAULT_FONT_SIZE)
}

const LILEX_REGULAR: &[u8] = include_bytes!("../assets/fonts/lilex/Lilex-Regular.ttf");
const LILEX_BOLD: &[u8] = include_bytes!("../assets/fonts/lilex/Lilex-Bold.ttf");
const LILEX_ITALIC: &[u8] = include_bytes!("../assets/fonts/lilex/Lilex-Italic.ttf");
const LILEX_BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/lilex/Lilex-BoldItalic.ttf");

const MONASPACE_ARGON_REGULAR: &[u8] =
    include_bytes!("../assets/fonts/monaspace/MonaspaceArgon-Regular.otf");
const MONASPACE_ARGON_BOLD: &[u8] =
    include_bytes!("../assets/fonts/monaspace/MonaspaceArgon-Bold.otf");
const MONASPACE_ARGON_ITALIC: &[u8] =
    include_bytes!("../assets/fonts/monaspace/MonaspaceArgon-Italic.otf");
const MONASPACE_ARGON_BOLD_ITALIC: &[u8] =
    include_bytes!("../assets/fonts/monaspace/MonaspaceArgon-BoldItalic.otf");
const MONASPACE_NEON_REGULAR: &[u8] =
    include_bytes!("../assets/fonts/monaspace/MonaspaceNeon-Regular.otf");
const MONASPACE_NEON_BOLD: &[u8] =
    include_bytes!("../assets/fonts/monaspace/MonaspaceNeon-Bold.otf");
const MONASPACE_NEON_ITALIC: &[u8] =
    include_bytes!("../assets/fonts/monaspace/MonaspaceNeon-Italic.otf");
const MONASPACE_NEON_BOLD_ITALIC: &[u8] =
    include_bytes!("../assets/fonts/monaspace/MonaspaceNeon-BoldItalic.otf");

/// Loads the bundled Lilex, Monaspace Argon, and Monaspace Neon faces into GPUI's text system.
///
/// Hosts must call this once during application startup before opening a viewer window.
///
/// # Errors
///
/// Returns an error when the platform text system cannot parse or register a bundled font.
pub fn load_default_fonts(cx: &mut App) -> Result<()> {
    cx.text_system().add_fonts(vec![
        Cow::Borrowed(LILEX_REGULAR),
        Cow::Borrowed(LILEX_BOLD),
        Cow::Borrowed(LILEX_ITALIC),
        Cow::Borrowed(LILEX_BOLD_ITALIC),
        Cow::Borrowed(MONASPACE_ARGON_REGULAR),
        Cow::Borrowed(MONASPACE_ARGON_BOLD),
        Cow::Borrowed(MONASPACE_ARGON_ITALIC),
        Cow::Borrowed(MONASPACE_ARGON_BOLD_ITALIC),
        Cow::Borrowed(MONASPACE_NEON_REGULAR),
        Cow::Borrowed(MONASPACE_NEON_BOLD),
        Cow::Borrowed(MONASPACE_NEON_ITALIC),
        Cow::Borrowed(MONASPACE_NEON_BOLD_ITALIC),
    ])
}
