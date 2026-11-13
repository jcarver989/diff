mod appearance;
mod selection;
mod terminal;
mod ui;
pub use appearance::{ThemePair, ThemePreference};
pub use rouge_syntax::{FontStyle, HighlightId, HighlightSpan, SyntaxStyle, SyntaxTheme};
pub use rouge_theme::{
    Appearance, AppearanceMode, ButtonVariant, ControlSize, ControlState, InteractionState, MarkdownPalette, ModalSize,
    NoticeTone, PickerOption, PickerRow, PickerState, Rgba, SCRIM_ALPHA, SelectionState, SemanticStyle, UiPalette,
};
pub use selection::{ThemeChoice, ThemeSelection};
pub use terminal::TerminalPalette;

use arborium_theme::{HIGHLIGHTS, ThemeSlot, builtin, slot_to_highlight_index};
pub use clankerdiff_fingerprint::{Fingerprint, FingerprintError};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, sync::LazyLock};

const THEME_VERSION: u32 = 3;

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
            DiffTone::Added => ToneColors { foreground: self.addition, background: self.addition_background },
            DiffTone::Removed => ToneColors { foreground: self.deletion, background: self.deletion_background },
            DiffTone::Context | DiffTone::Meta => {
                ToneColors { foreground: self.foreground, background: self.background }
            }
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
    #[serde(deserialize_with = "ui::palette")]
    ui: UiPalette,
    syntax: BTreeMap<String, SyntaxStyle>,
    markdown: MarkdownPalette,
}

#[derive(Debug, Clone)]
pub struct ReviewTheme {
    id: ThemeId,
    pub diff: DiffPalette,
    pub ui: UiPalette,
    pub syntax: SyntaxTheme,
    pub markdown: MarkdownPalette,
}

impl ReviewTheme {
    pub fn builder(id: impl Into<String>) -> ReviewThemeBuilder {
        ReviewThemeBuilder {
            id: ThemeId::Custom(id.into()),
            diff: DiffPalette::default(),
            ui: UiPalette::default(),
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
            serde_json::from_slice(bytes).map_err(|source| ThemeError::Parse { message: source.to_string() })?;
        if document.version != THEME_VERSION {
            return Err(ThemeError::UnsupportedVersion { version: document.version });
        }
        Ok(Self {
            id,
            ui: document.ui,
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
    pub const fn carded_name(&self) -> Rgba {
        Rgba { a: 56, ..self.diff.accent }
    }

    #[must_use]
    pub fn appearance(&self) -> Appearance {
        let canvas = self.ui.canvas;
        let luminance = 0.2126 * f32::from(canvas.r) + 0.7152 * f32::from(canvas.g) + 0.0722 * f32::from(canvas.b);
        if luminance < f32::from(u8::MAX) / 2.0 { Appearance::Dark } else { Appearance::Light }
    }

    #[must_use]
    pub fn revision(&self) -> Fingerprint {
        let diff = self.diff.colors().map(ui::bytes);
        let markdown = self.markdown.colors().map(ui::bytes);
        let ui = ui::colors(&self.ui).map(ui::bytes);
        Fingerprint::of(
            std::iter::once(syntax_revision(&self.syntax).as_bytes().as_slice())
                .chain(diff.iter().map(<[u8; 4]>::as_slice))
                .chain(markdown.iter().map(<[u8; 4]>::as_slice))
                .chain(ui.iter().map(<[u8; 4]>::as_slice)),
        )
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, ThemeError> {
        serde_json::to_vec(&ThemeDocument {
            version: THEME_VERSION,
            palette: self.diff.clone(),
            ui: self.ui,
            syntax: self.syntax.captures().clone(),
            markdown: self.markdown,
        })
        .map_err(|error| ThemeError::Parse { message: error.to_string() })
    }

    /// Returns all selectable built-in themes, with stable kebab-case identifiers.
    #[must_use]
    pub fn catalog() -> Vec<ThemeDescriptor> {
        let mut themes =
            vec![ThemeDescriptor { id: "sage".to_owned(), name: "Sage".to_owned(), is_dark: true, source_url: None }];
        themes.extend(builtin::all().into_iter().map(|theme| ThemeDescriptor {
            id: theme_slug(&theme.name),
            name: theme.name,
            is_dark: theme.is_dark,
            source_url: theme.source_url,
        }));
        themes
            .sort_by(|left, right| left.is_dark.cmp(&right.is_dark).reverse().then_with(|| left.name.cmp(&right.name)));
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
            .ok_or_else(|| ThemeError::UnknownTheme { name: name.to_owned() })?;
        let ui = ui::builtin(&id)?;
        Ok(Self::from_arborium(ThemeId::Builtin(id), &source, ui))
    }

    fn from_arborium(id: ThemeId, source: &arborium_theme::Theme, ui: UiPalette) -> Self {
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
        let markdown = markdown_palette(&syntax, &palette);
        Self { id, diff: palette, ui, syntax, markdown }
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
        Self::from_bytes(ThemeId::Ayu, include_bytes!("../assets/themes/ayu-dark.json"))
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
            if theme.is_dark { Rgba::new(21, 29, 31, 255) } else { Rgba::new(250, 250, 250, 255) }
        },
        rgba,
    );
    let foreground = theme.foreground.map_or_else(
        || {
            if theme.is_dark { Rgba::new(220, 220, 220, 255) } else { Rgba::new(35, 35, 35, 255) }
        },
        rgba,
    );
    let style_color =
        |slot| slot_to_highlight_index(slot).and_then(|index| theme.style(index)).and_then(|style| style.fg).map(rgba);
    let addition = style_color(ThemeSlot::DiffAdd)
        .unwrap_or_else(|| if theme.is_dark { Rgba::new(128, 190, 120, 255) } else { Rgba::new(40, 125, 55, 255) });
    let deletion = style_color(ThemeSlot::DiffDelete)
        .unwrap_or_else(|| if theme.is_dark { Rgba::new(225, 115, 115, 255) } else { Rgba::new(185, 45, 45, 255) });
    let accent = style_color(ThemeSlot::Link)
        .or_else(|| style_color(ThemeSlot::Function))
        .or_else(|| style_color(ThemeSlot::Keyword))
        .unwrap_or_else(|| foreground.mix(background, 72));
    DiffPalette {
        background,
        foreground,
        gutter: foreground.mix(background, 45),
        addition,
        deletion,
        addition_background: diff_background(addition, background),
        deletion_background: diff_background(deletion, background),
        selection: accent.mix(background, 28),
        accent,
        muted: foreground.mix(background, 62),
        border: foreground.mix(background, 25),
    }
}

const DIFF_BACKGROUND_ALPHA: u8 = 31;
const fn diff_background(foreground: Rgba, background: Rgba) -> Rgba {
    foreground.mix(background, DIFF_BACKGROUND_ALPHA)
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
    ui: UiPalette,
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
    pub fn ui(mut self, palette: UiPalette) -> Self {
        self.ui = palette;
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
        let markdown = self.markdown.unwrap_or_else(|| markdown_palette(&syntax, &self.diff));
        ReviewTheme { id: self.id, ui: self.ui, diff: self.diff, syntax, markdown }
    }
}

fn syntax_revision(syntax: &SyntaxTheme) -> Fingerprint {
    let mut fields = Vec::with_capacity(syntax.captures().len() * 3);
    for (capture, style) in syntax.captures() {
        fields.push(capture.as_bytes().to_vec());
        fields.push(ui::bytes(style.foreground).to_vec());
        fields.push(vec![
            u8::from(style.font_style.bold),
            u8::from(style.font_style.italic),
            u8::from(style.font_style.underline),
        ]);
    }
    Fingerprint::of(fields)
}

fn markdown_palette(syntax: &SyntaxTheme, palette: &DiffPalette) -> MarkdownPalette {
    let capture = |name, fallback| syntax.style(name).map_or(fallback, |style| style.foreground);
    MarkdownPalette {
        heading: capture("markup.heading", palette.accent),
        link: capture("markup.link", palette.accent),
        quote: capture("markup.quote", palette.muted),
        inline_code: capture("markup.raw.inline", palette.foreground),
        inline_code_background: palette.border,
        code: capture("markup.raw.block", palette.foreground),
        code_background: palette.background,
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
