//! Renderer-neutral colors, semantic diff palettes, and syntax themes.

use crate::{DiffTone, Fingerprint};
use serde::{Deserialize, Serialize};
use std::{fmt, io::Cursor};
use syntect::{
    highlighting::{Color, Theme as SyntectTheme, ThemeSet},
    parsing::Scope,
};

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
            reason = "the weighted average of two u8 channels is at most u8::MAX"
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

    const fn from_syntect(color: Color) -> Self {
        Self::new(color.r, color.g, color.b, color.a)
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
    pub bold: bool,
    pub italic: bool,
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

/// A highlighted, UTF-8-safe byte range into the source passed to the highlighter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightSpan {
    pub range: std::ops::Range<usize>,
    pub foreground: Rgba,
    pub font_style: FontStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToneColors {
    pub foreground: Rgba,
    pub background: Rgba,
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
            foreground: Rgba::new(212, 221, 214, 255),
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
    Sage,
    #[default]
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

/// A syntax theme plus semantic colors for the diff UI.
#[derive(Debug, Clone)]
pub struct DiffTheme {
    id: ThemeId,
    palette: DiffPalette,
    syntax: SyntectTheme,
    revision: Fingerprint,
}

impl DiffTheme {
    /// Parses a `TextMate` theme and derives its semantic palette.
    ///
    /// # Errors
    /// Returns an error when the theme bytes are invalid.
    pub fn from_bytes(id: ThemeId, bytes: &[u8]) -> Result<Self, ThemeError> {
        let syntax = ThemeSet::load_from_reader(&mut Cursor::new(bytes)).map_err(|source| {
            ThemeError::Parse {
                message: source.to_string(),
            }
        })?;
        let mut theme = Self::from_syntect(id, syntax);
        theme.revision = Fingerprint::of([bytes]);
        Ok(theme)
    }

    #[must_use]
    pub fn from_syntect(id: ThemeId, syntax: SyntectTheme) -> Self {
        let fallback = DiffPalette::default();
        let settings = &syntax.settings;
        let derive =
            |color: Option<Color>, default: Rgba| color.map_or(default, Rgba::from_syntect);
        let background = derive(settings.background, fallback.background);
        let addition = scope_foreground(&syntax, "markup.inserted", fallback.addition);
        let deletion = scope_foreground(&syntax, "markup.deleted", fallback.deletion);
        let palette = DiffPalette {
            foreground: derive(settings.foreground, fallback.foreground),
            background,
            accent: derive(settings.accent, fallback.accent),
            gutter: derive(settings.gutter_foreground, fallback.gutter),
            selection: derive(settings.selection, fallback.selection),
            addition,
            deletion,
            addition_background: diff_background(addition, background),
            deletion_background: diff_background(deletion, background),
            ..fallback
        };
        let revision = parsed_theme_revision(&id, &palette, &syntax);
        Self {
            id,
            palette,
            syntax,
            revision,
        }
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
    pub const fn syntax(&self) -> &SyntectTheme {
        &self.syntax
    }

    #[must_use]
    pub const fn revision(&self) -> Fingerprint {
        self.revision
    }

    pub fn sage() -> Result<Self, ThemeError> {
        Self::from_bytes(ThemeId::Sage, include_bytes!("../assets/sage.tmTheme"))
    }

    pub fn ayu() -> Result<Self, ThemeError> {
        Self::from_bytes(ThemeId::Ayu, include_bytes!("../assets/ayu-dark.tmTheme"))
    }
}

const DIFF_BACKGROUND_ALPHA: u8 = 31;

fn scope_foreground(theme: &SyntectTheme, scope: &str, fallback: Rgba) -> Rgba {
    let Ok(scope) = Scope::new(scope) else {
        return fallback;
    };
    theme
        .scopes
        .iter()
        .find(|item| {
            item.scope
                .selectors
                .iter()
                .any(|selector| selector.extract_single_scope() == Some(scope))
        })
        .and_then(|item| item.style.foreground)
        .map_or(fallback, Rgba::from_syntect)
}

const fn diff_background(foreground: Rgba, background: Rgba) -> Rgba {
    Rgba::new(
        foreground.r,
        foreground.g,
        foreground.b,
        DIFF_BACKGROUND_ALPHA,
    )
    .over(background)
}

fn parsed_theme_revision(
    id: &ThemeId,
    palette: &DiffPalette,
    syntax: &SyntectTheme,
) -> Fingerprint {
    let name = id.to_string();
    let syntax = format!("{syntax:?}");
    let mut channels = Vec::with_capacity(palette.colors().len() * 4);
    for color in palette.colors() {
        channels.extend_from_slice(&color.to_bytes());
    }
    Fingerprint::of([name.as_bytes(), channels.as_slice(), syntax.as_bytes()])
}

impl Default for DiffTheme {
    fn default() -> Self {
        Self::ayu().expect("bundled Ayu Dark theme must parse")
    }
}

/// Errors produced while loading a `.tmTheme`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    #[error("failed to parse theme: {message}")]
    Parse { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_themes_parse_with_distinct_revisions() {
        let sage = DiffTheme::sage().unwrap();
        let ayu = DiffTheme::default();
        assert_eq!(ThemeId::default(), ThemeId::Ayu);
        assert_eq!(ayu.id(), &ThemeId::Ayu);
        assert_eq!(sage.id(), &ThemeId::Sage);
        assert_ne!(sage.revision(), ayu.revision());
    }

    #[test]
    fn parsed_themes_with_different_rules_have_different_revisions() {
        let first = DiffTheme::default();
        let mut syntax = first.syntax().clone();
        syntax.scopes[0].style.foreground = Some(Color {
            r: 1,
            g: 2,
            b: 3,
            a: 255,
        });
        let second = DiffTheme::from_syntect(first.id().clone(), syntax);
        assert_eq!(first.palette(), second.palette());
        assert_ne!(first.revision(), second.revision());
    }

    #[test]
    fn tone_mapping_is_shared_by_adapters() {
        let palette = DiffPalette::default();
        assert_eq!(palette.tone(DiffTone::Added).foreground, palette.addition);
        assert_eq!(
            palette.tone(DiffTone::Meta).background,
            palette.tone(DiffTone::Context).background
        );
    }

    #[test]
    fn diff_backgrounds_are_subtle_opaque_theme_tints() {
        let palette = DiffPalette::default();
        assert_eq!(palette.addition_background, Rgba::new(40, 52, 39, 255));
        assert_eq!(palette.deletion_background, Rgba::new(46, 40, 42, 255));

        let ayu = DiffTheme::default();
        assert_eq!(ayu.palette().addition, Rgba::new(194, 217, 76, 255));
        assert_eq!(ayu.palette().deletion, Rgba::new(255, 51, 51, 255));
        assert_eq!(ayu.palette().addition_background.a, 255);
        assert_eq!(ayu.palette().deletion_background.a, 255);

        let sage = DiffTheme::sage().unwrap();
        assert_eq!(sage.palette().addition, Rgba::new(167, 192, 128, 255));
        assert_eq!(sage.palette().deletion, Rgba::new(230, 126, 128, 255));
    }

    #[test]
    fn font_style_serializes_as_named_flags() {
        let style = FontStyle {
            bold: true,
            italic: true,
            underline: false,
        };
        assert!(!style.is_plain());
        assert_eq!(
            serde_json::to_string(&style).unwrap(),
            r#"{"bold":true,"italic":true,"underline":false}"#
        );
        assert!(FontStyle::none().is_plain());
    }
}
