//! Renderer-neutral colors, semantic diff palettes, and syntax themes.

use crate::{DiffTone, Fingerprint};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

/// An sRGB color with an explicit alpha channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Composites this color over an opaque background.
    #[must_use]
    pub const fn over(self, background: Self) -> Self {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "weighted u8 average fits in u8"
        )]
        const fn channel(foreground: u8, background: u8, alpha: u8) -> u8 {
            let foreground = foreground as u32 * alpha as u32;
            let background = background as u32 * (u8::MAX - alpha) as u32;
            ((foreground + background + 127) / 255) as u8
        }
        Self::new(
            channel(self.r, background.r, self.a),
            channel(self.g, background.g, self.a),
            channel(self.b, background.b, self.a),
            u8::MAX,
        )
    }

    #[must_use]
    pub const fn to_bytes(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
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
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
}

impl FontStyle {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
        }
    }

    #[must_use]
    pub const fn is_plain(self) -> bool {
        !self.bold && !self.italic && !self.underline
    }
}

/// A highlighted, UTF-8-safe byte range into the supplied source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightSpan {
    pub range: std::ops::Range<usize>,
    pub foreground: Rgba,
    pub font_style: FontStyle,
}

/// Style assigned to a Tree-sitter capture name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxStyle {
    pub foreground: Rgba,
    #[serde(default)]
    pub font_style: FontStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToneColors {
    pub foreground: Rgba,
    pub background: Rgba,
}

/// Colors used by diff renderers.
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

impl DiffPalette {
    #[must_use]
    pub const fn tone(&self, tone: DiffTone) -> ToneColors {
        match tone {
            DiffTone::Added => ToneColors {
                foreground: self.addition,
                background: self.addition_background,
            },
            DiffTone::Removed => ToneColors {
                foreground: self.deletion,
                background: self.deletion_background,
            },
            DiffTone::Context | DiffTone::Meta => ToneColors {
                foreground: self.foreground,
                background: self.background,
            },
        }
    }

    #[must_use]
    pub const fn colors(&self) -> [Rgba; 11] {
        [
            self.background,
            self.foreground,
            self.gutter,
            self.addition,
            self.deletion,
            self.addition_background,
            self.deletion_background,
            self.selection,
            self.accent,
            self.muted,
            self.border,
        ]
    }
}

impl Default for DiffPalette {
    fn default() -> Self {
        let background = Rgba::new(21, 29, 31, 255);
        let addition = Rgba::new(179, 215, 98, 255);
        let deletion = Rgba::new(223, 120, 122, 255);
        Self {
            background,
            foreground: Rgba::default(),
            gutter: Rgba::new(80, 96, 91, 255),
            addition,
            deletion,
            addition_background: diff_background(addition, background),
            deletion_background: diff_background(deletion, background),
            selection: Rgba::new(143, 188, 176, 45),
            accent: Rgba::new(143, 188, 176, 255),
            muted: Rgba::new(125, 143, 136, 255),
            border: Rgba::new(57, 73, 73, 255),
        }
    }
}

/// Stable identifiers for embedded and host-provided themes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThemeId {
    #[default]
    Sage,
    Ayu,
    Custom(String),
}

impl fmt::Display for ThemeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sage => f.write_str("sage"),
            Self::Ayu => f.write_str("ayu-dark"),
            Self::Custom(name) => f.write_str(name),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeDocument {
    version: u32,
    palette: DiffPalette,
    syntax: BTreeMap<String, SyntaxStyle>,
}

/// A capture-based syntax theme plus semantic colors for the diff UI.
#[derive(Debug, Clone)]
pub struct DiffTheme {
    id: ThemeId,
    palette: DiffPalette,
    syntax: BTreeMap<String, SyntaxStyle>,
    revision: Fingerprint,
}

impl DiffTheme {
    /// Parses a versioned Diff theme JSON document.
    ///
    /// Capture lookup first tries an exact name and then progressively removes
    /// dot-separated suffixes. The full parsed palette and map affect revision.
    ///
    /// # Errors
    /// Returns an error for malformed JSON or an unsupported schema version.
    pub fn from_bytes(id: ThemeId, bytes: &[u8]) -> Result<Self, ThemeError> {
        let document: ThemeDocument =
            serde_json::from_slice(bytes).map_err(|source| ThemeError::Parse {
                message: source.to_string(),
            })?;
        if document.version != 1 {
            return Err(ThemeError::UnsupportedVersion {
                version: document.version,
            });
        }
        let canonical = serde_json::to_vec(&document).map_err(|source| ThemeError::Parse {
            message: source.to_string(),
        })?;
        let revision = Fingerprint::of([id.to_string().as_bytes(), canonical.as_slice()]);
        Ok(Self {
            id,
            palette: document.palette,
            syntax: document.syntax,
            revision,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &ThemeId {
        &self.id
    }

    #[must_use]
    pub const fn palette(&self) -> &DiffPalette {
        &self.palette
    }

    #[must_use]
    pub const fn revision(&self) -> Fingerprint {
        self.revision
    }

    /// Resolves an exact capture or its nearest dot-separated parent.
    #[must_use]
    pub fn style(&self, capture: &str) -> Option<SyntaxStyle> {
        let mut candidate = capture;
        loop {
            if let Some(style) = self.syntax.get(candidate) {
                return Some(*style);
            }
            let (parent, _) = candidate.rsplit_once('.')?;
            candidate = parent;
        }
    }

    /// Loads the bundled Sage theme.
    ///
    /// # Errors
    /// Returns an error if the checked-in theme JSON is invalid.
    pub fn sage() -> Result<Self, ThemeError> {
        Self::from_bytes(ThemeId::Sage, include_bytes!("../assets/themes/sage.json"))
    }
    /// Loads the bundled Ayu Dark theme.
    ///
    /// # Errors
    /// Returns an error if the checked-in theme JSON is invalid.
    pub fn ayu() -> Result<Self, ThemeError> {
        Self::from_bytes(
            ThemeId::Ayu,
            include_bytes!("../assets/themes/ayu-dark.json"),
        )
    }
}

const DIFF_BACKGROUND_ALPHA: u8 = 31;
const fn diff_background(foreground: Rgba, background: Rgba) -> Rgba {
    Rgba::new(
        foreground.r,
        foreground.g,
        foreground.b,
        DIFF_BACKGROUND_ALPHA,
    )
    .over(background)
}

impl Default for DiffTheme {
    fn default() -> Self {
        Self::sage().expect("bundled Sage theme JSON must parse")
    }
}

/// Errors produced while loading theme JSON.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    #[error("failed to parse theme JSON: {message}")]
    Parse { message: String },
    #[error("unsupported theme JSON version {version}")]
    UnsupportedVersion { version: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_themes_parse_with_distinct_revisions() {
        let sage = DiffTheme::default();
        let ayu = DiffTheme::ayu().unwrap();
        assert_eq!(sage.id(), &ThemeId::Sage);
        assert_eq!(ayu.id(), &ThemeId::Ayu);
        assert_ne!(sage.revision(), ayu.revision());
    }

    #[test]
    fn capture_style_uses_parent_fallback() {
        let theme = DiffTheme::default();
        assert_eq!(
            theme.style("function.method.builtin"),
            theme.style("function.method")
        );
        assert!(theme.style("not-a-capture").is_none());
    }

    #[test]
    fn invalid_json_and_version_are_rejected() {
        assert!(matches!(
            DiffTheme::from_bytes(ThemeId::Custom("x".into()), b"{"),
            Err(ThemeError::Parse { .. })
        ));
        let bytes = include_bytes!("../assets/themes/sage.json");
        let changed =
            String::from_utf8_lossy(bytes).replacen("\"version\": 1", "\"version\": 2", 1);
        assert_eq!(
            DiffTheme::from_bytes(ThemeId::Sage, changed.as_bytes()).unwrap_err(),
            ThemeError::UnsupportedVersion { version: 2 }
        );
    }

    #[test]
    fn semantic_palette_values_remain_stable() {
        let sage = DiffTheme::default();
        let ayu = DiffTheme::ayu().unwrap();
        assert_eq!(sage.palette().addition, Rgba::new(167, 192, 128, 255));
        assert_eq!(sage.palette().deletion, Rgba::new(230, 126, 128, 255));
        assert_eq!(ayu.palette().addition, Rgba::new(194, 217, 76, 255));
        assert_eq!(ayu.palette().deletion, Rgba::new(255, 51, 51, 255));
    }

    #[test]
    fn font_style_serializes_as_named_flags() {
        let style = FontStyle {
            bold: true,
            italic: true,
            underline: false,
        };
        assert_eq!(
            serde_json::to_string(&style).unwrap(),
            r#"{"bold":true,"italic":true,"underline":false}"#
        );
    }
}
