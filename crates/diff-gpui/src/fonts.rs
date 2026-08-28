//! Bundled fonts shared by the native and web GPUI hosts.

use gpui::{App, Result};
use std::borrow::Cow;

/// The bundled font family used by the diff viewer and its hosts.
pub const DEFAULT_FONT_FAMILY: &str = "Lilex";

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
