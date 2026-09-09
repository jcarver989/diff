//! Renderer-neutral colors, semantic diff palettes, and syntax themes.

use arborium_theme::{HIGHLIGHTS, ThemeSlot, builtin, slot_to_highlight_index};
pub use clankerdiff_fingerprint::{Fingerprint, FingerprintError};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, sync::LazyLock};

const THEME_VERSION: u32 = 2;

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

/// Semantic tint for a diff cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DiffTone {
    Context,
    Added,
    Removed,
    Meta,
}

impl DiffTone {
    #[must_use]
    pub const fn marker(self) -> char {
        match self {
            Self::Added => '+',
            Self::Removed => '-',
            Self::Context | Self::Meta => ' ',
        }
    }
}

/// Foreground and background colors resolved for one [`DiffTone`].
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

/// Alpha applied to the canvas color when it backs a modal scrim.
pub const SCRIM_ALPHA: u8 = 184;

/// Renderer-neutral application colors derived from a diff palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiPalette {
    pub canvas: Rgba,
    pub surface: Rgba,
    pub surface_hover: Rgba,
    pub surface_selected: Rgba,
    pub text: Rgba,
    pub text_muted: Rgba,
    pub border: Rgba,
    pub accent: Rgba,
    pub accent_foreground: Rgba,
    pub positive: Rgba,
    pub destructive: Rgba,
    pub scrim: Rgba,
}

/// Semantic intent shared by renderer-native action controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Destructive,
    #[default]
    Ghost,
}

/// Interaction state shared by renderer-native controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InteractionState {
    #[default]
    Rest,
    Hovered,
    Disabled,
}

/// Complete semantic state for an action control.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ControlState {
    pub interaction: InteractionState,
    pub selected: bool,
}

impl ControlState {
    #[must_use]
    pub const fn new(interaction: InteractionState) -> Self {
        Self {
            interaction,
            selected: false,
        }
    }

    #[must_use]
    pub const fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

/// Semantic size shared by renderer-native controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ControlSize {
    Small,
    #[default]
    Medium,
}

/// Semantic state for selectable content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectionState {
    #[default]
    None,
    Selected,
    Focused,
    Disabled,
}

/// Semantic tone for notices and status messages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NoticeTone {
    #[default]
    Info,
    Positive,
    Warning,
    Error,
}

/// Cross-renderer modal size category.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ModalSize {
    Compact,
    #[default]
    Medium,
    Wide,
}

/// Renderer-neutral visual result for one semantic component state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticStyle {
    pub foreground: Rgba,
    pub background: Option<Rgba>,
    pub emphasized: bool,
}

impl UiPalette {
    /// Resolves an action control without renderer-specific state ordering.
    #[must_use]
    pub const fn control_style(self, variant: ButtonVariant, state: ControlState) -> SemanticStyle {
        if matches!(state.interaction, InteractionState::Disabled) {
            return SemanticStyle {
                foreground: self.text_muted,
                background: Some(self.surface_selected),
                emphasized: false,
            };
        }
        let (foreground, background) = match (variant, state.interaction, state.selected) {
            (ButtonVariant::Primary, _, _) => (self.accent_foreground, Some(self.accent)),
            (ButtonVariant::Destructive, _, _) => (self.accent_foreground, Some(self.destructive)),
            (ButtonVariant::Ghost, _, true) => (self.accent, Some(self.surface_selected)),
            (ButtonVariant::Secondary | ButtonVariant::Ghost, InteractionState::Hovered, _) => {
                (self.text, Some(self.surface_hover))
            }
            (ButtonVariant::Secondary, _, _) => (self.text, Some(self.surface_selected)),
            (ButtonVariant::Ghost, _, false) => (self.text_muted, None),
        };
        SemanticStyle {
            foreground,
            background,
            emphasized: state.selected,
        }
    }

    /// Resolves selectable content consistently across renderers.
    #[must_use]
    pub const fn selection_style(self, state: SelectionState) -> SemanticStyle {
        match state {
            SelectionState::None => SemanticStyle {
                foreground: self.text,
                background: Some(self.surface),
                emphasized: false,
            },
            SelectionState::Selected => SemanticStyle {
                foreground: self.accent,
                background: Some(self.surface_selected),
                emphasized: false,
            },
            SelectionState::Focused => SemanticStyle {
                foreground: self.accent_foreground,
                background: Some(self.accent),
                emphasized: true,
            },
            SelectionState::Disabled => SemanticStyle {
                foreground: self.text_muted,
                background: Some(self.surface),
                emphasized: false,
            },
        }
    }

    /// Resolves the foreground used to communicate a notice tone.
    #[must_use]
    pub const fn notice_style(self, tone: NoticeTone) -> SemanticStyle {
        let foreground = match tone {
            NoticeTone::Info => self.text_muted,
            NoticeTone::Positive => self.positive,
            NoticeTone::Warning => self.accent,
            NoticeTone::Error => self.destructive,
        };
        SemanticStyle {
            foreground,
            background: None,
            emphasized: matches!(tone, NoticeTone::Warning | NoticeTone::Error),
        }
    }
}

impl From<&DiffPalette> for UiPalette {
    fn from(palette: &DiffPalette) -> Self {
        let mut scrim = palette.background;
        scrim.a = SCRIM_ALPHA;
        Self {
            canvas: palette.background,
            surface: palette.background,
            surface_hover: palette.selection,
            surface_selected: palette.selection,
            text: palette.foreground,
            text_muted: palette.muted,
            border: palette.border,
            accent: palette.accent,
            accent_foreground: palette.background,
            positive: palette.addition,
            destructive: palette.deletion,
            scrim,
        }
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
    Builtin(String),
    Custom(String),
}

impl fmt::Display for ThemeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sage => f.write_str("sage"),
            Self::Ayu => f.write_str("ayu-dark"),
            Self::Builtin(name) | Self::Custom(name) => f.write_str(name),
        }
    }
}

/// Summary metadata for a theme offered by the built-in catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeDescriptor {
    pub id: String,
    pub name: String,
    pub is_dark: bool,
    pub source_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeDocument {
    version: u32,
    palette: DiffPalette,
    syntax: BTreeMap<String, SyntaxStyle>,
    markdown: MarkdownPalette,
}

#[derive(Debug, Clone)]
pub struct ReviewTheme {
    id: ThemeId,
    pub diff: DiffPalette,
    pub syntax: SyntaxTheme,
    pub markdown: MarkdownPalette,
}

impl ReviewTheme {
    pub fn builder(id: impl Into<String>) -> ReviewThemeBuilder {
        ReviewThemeBuilder {
            id: ThemeId::Custom(id.into()),
            diff: DiffPalette::default(),
            syntax: BTreeMap::new(),
            markdown: None,
        }
    }

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
        if document.version != THEME_VERSION {
            return Err(ThemeError::UnsupportedVersion {
                version: document.version,
            });
        }
        Ok(Self {
            id,
            diff: document.palette,
            syntax: SyntaxTheme::new(document.syntax),
            markdown: document.markdown,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &ThemeId {
        &self.id
    }

    #[must_use]
    pub fn revision(&self) -> Fingerprint {
        let diff = self.diff.colors().map(Rgba::to_bytes);
        let markdown = self.markdown.colors().map(Rgba::to_bytes);
        Fingerprint::of(
            std::iter::once(self.syntax.revision().as_bytes().as_slice())
                .chain(diff.iter().map(<[u8; 4]>::as_slice))
                .chain(markdown.iter().map(<[u8; 4]>::as_slice)),
        )
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, ThemeError> {
        serde_json::to_vec(&ThemeDocument {
            version: THEME_VERSION,
            palette: self.diff.clone(),
            syntax: self.syntax.captures.clone(),
            markdown: self.markdown.clone(),
        })
        .map_err(|error| ThemeError::Parse {
            message: error.to_string(),
        })
    }

    /// Returns all selectable built-in themes, with stable kebab-case identifiers.
    #[must_use]
    pub fn catalog() -> Vec<ThemeDescriptor> {
        let mut themes = vec![ThemeDescriptor {
            id: "sage".to_owned(),
            name: "Sage".to_owned(),
            is_dark: true,
            source_url: None,
        }];
        themes.extend(builtin::all().into_iter().map(|theme| ThemeDescriptor {
            id: theme_slug(&theme.name),
            name: theme.name,
            is_dark: theme.is_dark,
            source_url: theme.source_url,
        }));
        themes.sort_by(|left, right| {
            left.is_dark
                .cmp(&right.is_dark)
                .reverse()
                .then_with(|| left.name.cmp(&right.name))
        });
        themes
    }

    /// Loads a selectable built-in theme by its stable identifier.
    ///
    /// Aliases are matched case-insensitively and with spaces or underscores
    /// normalized to dashes.
    ///
    /// # Errors
    /// Returns an error when no built-in theme has the supplied identifier.
    pub fn builtin(name: &str) -> Result<Self, ThemeError> {
        let id = theme_slug(name);
        if id == "sage" {
            return Self::sage();
        }
        if id == "ayu" || id == "ayu-dark" {
            return Self::ayu();
        }
        let source = builtin::all()
            .into_iter()
            .find(|theme| theme_slug(&theme.name) == id)
            .ok_or_else(|| ThemeError::UnknownTheme {
                name: name.to_owned(),
            })?;
        Ok(Self::from_arborium(ThemeId::Builtin(id), &source))
    }

    fn from_arborium(id: ThemeId, source: &arborium_theme::Theme) -> Self {
        let palette = palette_from_arborium(source);
        let syntax = HIGHLIGHTS
            .iter()
            .zip(source.styles.iter())
            .filter_map(|(highlight, style)| {
                let foreground = style.fg?;
                Some((
                    highlight.name.to_owned(),
                    SyntaxStyle {
                        foreground: rgba(foreground),
                        font_style: FontStyle {
                            bold: style.modifiers.bold,
                            italic: style.modifiers.italic,
                            underline: style.modifiers.underline,
                        },
                    },
                ))
            })
            .collect::<BTreeMap<_, _>>();
        let syntax = SyntaxTheme::new(syntax);
        let markdown = MarkdownPalette::from_syntax(&syntax, &palette);
        Self {
            id,
            diff: palette,
            syntax,
            markdown,
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

fn theme_slug(name: &str) -> String {
    name.trim()
        .chars()
        .map(|character| match character {
            ' ' | '_' => '-',
            character => character.to_ascii_lowercase(),
        })
        .collect()
}

const fn rgba(color: arborium_theme::Color) -> Rgba {
    Rgba::new(color.r, color.g, color.b, u8::MAX)
}

fn palette_from_arborium(theme: &arborium_theme::Theme) -> DiffPalette {
    let background = theme.background.map_or_else(
        || {
            if theme.is_dark {
                Rgba::new(21, 29, 31, 255)
            } else {
                Rgba::new(250, 250, 250, 255)
            }
        },
        rgba,
    );
    let foreground = theme.foreground.map_or_else(
        || {
            if theme.is_dark {
                Rgba::new(220, 220, 220, 255)
            } else {
                Rgba::new(35, 35, 35, 255)
            }
        },
        rgba,
    );
    let style_color = |slot| {
        slot_to_highlight_index(slot)
            .and_then(|index| theme.style(index))
            .and_then(|style| style.fg)
            .map(rgba)
    };
    let addition = style_color(ThemeSlot::DiffAdd).unwrap_or_else(|| {
        if theme.is_dark {
            Rgba::new(128, 190, 120, 255)
        } else {
            Rgba::new(40, 125, 55, 255)
        }
    });
    let deletion = style_color(ThemeSlot::DiffDelete).unwrap_or_else(|| {
        if theme.is_dark {
            Rgba::new(225, 115, 115, 255)
        } else {
            Rgba::new(185, 45, 45, 255)
        }
    });
    let accent = style_color(ThemeSlot::Link)
        .or_else(|| style_color(ThemeSlot::Function))
        .or_else(|| style_color(ThemeSlot::Keyword))
        .unwrap_or_else(|| mix(foreground, background, 72));
    DiffPalette {
        background,
        foreground,
        gutter: mix(foreground, background, 45),
        addition,
        deletion,
        addition_background: diff_background(addition, background),
        deletion_background: diff_background(deletion, background),
        selection: mix(accent, background, 28),
        accent,
        muted: mix(foreground, background, 62),
        border: mix(foreground, background, 25),
    }
}

const fn mix(foreground: Rgba, background: Rgba, amount: u8) -> Rgba {
    Rgba::new(foreground.r, foreground.g, foreground.b, amount).over(background)
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

static DEFAULT_THEME: LazyLock<ReviewTheme> =
    LazyLock::new(|| ReviewTheme::sage().expect("bundled Sage theme JSON must parse"));

impl Default for ReviewTheme {
    fn default() -> Self {
        DEFAULT_THEME.clone()
    }
}

pub struct ReviewThemeBuilder {
    id: ThemeId,
    diff: DiffPalette,
    syntax: BTreeMap<String, SyntaxStyle>,
    markdown: Option<MarkdownPalette>,
}

impl ReviewThemeBuilder {
    #[must_use]
    pub fn capture(mut self, name: impl Into<String>, style: SyntaxStyle) -> Self {
        self.syntax.insert(name.into(), style);
        self
    }

    #[must_use]
    pub fn diff(mut self, palette: DiffPalette) -> Self {
        self.diff = palette;
        self
    }

    #[must_use]
    pub fn markdown(mut self, palette: MarkdownPalette) -> Self {
        self.markdown = Some(palette);
        self
    }

    #[must_use]
    pub fn build(self) -> ReviewTheme {
        let syntax = SyntaxTheme::new(self.syntax);
        let markdown = self
            .markdown
            .unwrap_or_else(|| MarkdownPalette::from_syntax(&syntax, &self.diff));
        ReviewTheme {
            id: self.id,
            diff: self.diff,
            syntax,
            markdown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxTheme {
    captures: BTreeMap<String, SyntaxStyle>,
    revision: Fingerprint,
}

impl SyntaxTheme {
    #[must_use]
    pub fn new(captures: BTreeMap<String, SyntaxStyle>) -> Self {
        let mut fields = Vec::with_capacity(captures.len() * 3);
        for (capture, style) in &captures {
            fields.push(capture.as_bytes().to_vec());
            fields.push(style.foreground.to_bytes().to_vec());
            fields.push(vec![
                u8::from(style.font_style.bold),
                u8::from(style.font_style.italic),
                u8::from(style.font_style.underline),
            ]);
        }
        Self {
            captures,
            revision: Fingerprint::of(fields),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> Fingerprint {
        self.revision
    }

    #[must_use]
    pub fn style(&self, capture: &str) -> Option<SyntaxStyle> {
        let mut candidate = capture;
        loop {
            if let Some(style) = self.captures.get(candidate) {
                return Some(*style);
            }
            let (parent, _) = candidate.rsplit_once('.')?;
            candidate = parent;
        }
    }
}

impl Default for SyntaxTheme {
    fn default() -> Self {
        DEFAULT_THEME.syntax.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkdownPalette {
    pub heading: Rgba,
    pub link: Rgba,
    pub quote: Rgba,
    pub inline_code: Rgba,
    pub inline_code_background: Rgba,
    pub code: Rgba,
    pub code_background: Rgba,
}

impl MarkdownPalette {
    #[must_use]
    pub fn from_syntax(syntax: &SyntaxTheme, palette: &DiffPalette) -> Self {
        let capture = |name, fallback| {
            syntax
                .style(name)
                .map_or(fallback, |style| style.foreground)
        };
        Self {
            heading: capture("markup.heading", palette.accent),
            link: capture("markup.link", palette.accent),
            quote: capture("markup.quote", palette.muted),
            inline_code: capture("markup.raw.inline", palette.foreground),
            inline_code_background: palette.border,
            code: capture("markup.raw.block", palette.foreground),
            code_background: palette.background,
        }
    }

    #[must_use]
    pub const fn colors(&self) -> [Rgba; 7] {
        [
            self.heading,
            self.link,
            self.quote,
            self.inline_code,
            self.inline_code_background,
            self.code,
            self.code_background,
        ]
    }
}

impl Default for MarkdownPalette {
    fn default() -> Self {
        DEFAULT_THEME.markdown.clone()
    }
}

/// Errors produced while loading theme JSON.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    #[error("unknown built-in theme `{name}`")]
    UnknownTheme { name: String },
    #[error("failed to parse theme JSON: {message}")]
    Parse { message: String },
    #[error("unsupported theme JSON version {version}")]
    UnsupportedVersion { version: u32 },
}
