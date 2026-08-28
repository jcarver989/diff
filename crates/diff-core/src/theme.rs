//! Renderer-neutral colors, semantic diff palettes, and syntax themes.

use serde::{Deserialize, Serialize};
use std::{
    fmt,
    io::Cursor,
    ops::{BitOr, BitOrAssign},
};
use syntect::highlighting::{Color, Theme as SyntectTheme, ThemeSet};

/// An sRGB color with an explicit alpha channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    fn from_syntect(c: Color) -> Self {
        Self::new(c.r, c.g, c.b, c.a)
    }
}
impl Default for Rgba {
    fn default() -> Self {
        Self::new(212, 221, 214, 255)
    }
}

/// Text modifiers returned by the syntax engine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FontStyle {
    bits: u8,
}
impl FontStyle {
    pub const BOLD: Self = Self { bits: 1 };
    pub const UNDERLINE: Self = Self { bits: 2 };
    pub const ITALIC: Self = Self { bits: 4 };
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }
    pub const fn bits(self) -> u8 {
        self.bits
    }
    pub const fn contains(self, other: Self) -> bool {
        self.bits & other.bits == other.bits
    }
    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !7 == 0 {
            Some(Self { bits })
        } else {
            None
        }
    }
}
impl BitOr for FontStyle {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self {
            bits: self.bits | rhs.bits,
        }
    }
}
impl BitOrAssign for FontStyle {
    fn bitor_assign(&mut self, rhs: Self) {
        self.bits |= rhs.bits;
    }
}

/// A highlighted, UTF-8-safe byte range into the source passed to the highlighter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightSpan {
    pub range: std::ops::Range<usize>,
    pub foreground: Rgba,
    pub font_style: FontStyle,
}

/// Colors used by diff renderers. Frontends are responsible for converting these values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffPalette {
    pub background: Rgba,
    pub foreground: Rgba,
    pub gutter: Rgba,
    pub addition: Rgba,
    pub deletion: Rgba,
    pub addition_background: Rgba,
    pub deletion_background: Rgba,
    pub selection: Rgba,
    pub accent: Rgba,
    pub muted: Rgba,
    pub border: Rgba,
}
impl Default for DiffPalette {
    fn default() -> Self {
        Self {
            background: Rgba::new(21, 29, 31, 255),
            foreground: Rgba::new(212, 221, 214, 255),
            gutter: Rgba::new(80, 96, 91, 255),
            addition: Rgba::new(143, 188, 176, 255),
            deletion: Rgba::new(191, 97, 106, 255),
            addition_background: Rgba::new(41, 70, 61, 180),
            deletion_background: Rgba::new(77, 42, 48, 180),
            selection: Rgba::new(143, 188, 176, 45),
            accent: Rgba::new(143, 188, 176, 255),
            muted: Rgba::new(125, 143, 136, 255),
            border: Rgba::new(57, 73, 73, 255),
        }
    }
}

/// Stable identifiers for embedded and host-provided themes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThemeId {
    Sage,
    Ayu,
    Custom(String),
}
impl ThemeId {
    pub const SAGE: Self = Self::Sage;
    pub const AYU: Self = Self::Ayu;
    pub fn custom(name: impl Into<String>) -> Self {
        Self::Custom(name.into())
    }
}
impl Default for ThemeId {
    fn default() -> Self {
        Self::Sage
    }
}
impl fmt::Display for ThemeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sage => f.write_str("sage"),
            Self::Ayu => f.write_str("ayu-dark"),
            Self::Custom(s) => f.write_str(s),
        }
    }
}

/// A syntax theme plus semantic colors for the diff UI.
#[derive(Debug, Clone)]
pub struct DiffTheme {
    pub id: ThemeId,
    pub palette: DiffPalette,
    pub syntax: SyntectTheme,
    revision: [u8; 32],
}
impl DiffTheme {
    pub fn from_bytes(id: ThemeId, bytes: &[u8]) -> Result<Self, ThemeError> {
        let syntax = ThemeSet::load_from_reader(&mut Cursor::new(bytes)).map_err(|source| {
            ThemeError::Parse {
                message: source.to_string(),
            }
        })?;
        let mut theme = Self::from_syntect(id, syntax);
        theme.revision = *blake3::hash(bytes).as_bytes();
        Ok(theme)
    }
    /// Parses a Sublime Text `.tmTheme` payload.
    pub fn from_tm_theme_bytes(id: ThemeId, bytes: &[u8]) -> Result<Self, ThemeError> {
        Self::from_bytes(id, bytes)
    }
    /// Returns the embedded Sage theme.
    pub fn default_sage() -> Result<Self, ThemeError> {
        Self::sage()
    }
    /// Returns the embedded Ayu Dark theme.
    pub fn ayu_dark() -> Result<Self, ThemeError> {
        Self::ayu()
    }
    pub fn from_syntect(id: ThemeId, syntax: SyntectTheme) -> Self {
        let fallback = DiffPalette::default();
        let s = syntax.settings.clone();
        let fg = s
            .foreground
            .map(Rgba::from_syntect)
            .unwrap_or(fallback.foreground);
        let bg = s
            .background
            .map(Rgba::from_syntect)
            .unwrap_or(fallback.background);
        let accent = s.accent.map(Rgba::from_syntect).unwrap_or(fallback.accent);
        let revision = *blake3::hash(format!("{syntax:?}").as_bytes()).as_bytes();
        Self {
            id,
            syntax,
            revision,
            palette: DiffPalette {
                foreground: fg,
                background: bg,
                accent,
                gutter: s
                    .gutter_foreground
                    .map(Rgba::from_syntect)
                    .unwrap_or(fallback.gutter),
                selection: s
                    .selection
                    .map(Rgba::from_syntect)
                    .unwrap_or(fallback.selection),
                ..fallback
            },
        }
    }
    pub(crate) const fn revision(&self) -> [u8; 32] {
        self.revision
    }

    pub fn sage() -> Result<Self, ThemeError> {
        Self::from_bytes(ThemeId::Sage, include_bytes!("../assets/sage.tmTheme"))
    }
    pub fn ayu() -> Result<Self, ThemeError> {
        Self::from_bytes(ThemeId::Ayu, include_bytes!("../assets/ayu-dark.tmTheme"))
    }
}
impl Default for DiffTheme {
    fn default() -> Self {
        Self::sage().expect("bundled Sage theme must parse")
    }
}

/// Errors produced while loading a `.tmTheme`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    #[error("failed to parse theme: {message}")]
    Parse { message: String },
    #[error("theme has no syntax settings")]
    MissingSettings,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_themes_parse() {
        assert_eq!(DiffTheme::default().id, ThemeId::Sage);
        assert!(DiffTheme::ayu().is_ok());
    }
    #[test]
    fn flags_serialize() {
        let f = FontStyle::BOLD | FontStyle::ITALIC;
        assert!(f.contains(FontStyle::BOLD));
        assert_eq!(serde_json::to_string(&f).unwrap(), "{\"bits\":5}");
    }
}
