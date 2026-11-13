use crate::{Rgba, ThemeError, UiPalette};
use serde::{Deserialize, Deserializer};
use std::{collections::BTreeMap, sync::LazyLock};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UiPaletteDocument {
    canvas: Rgba,
    surface: Rgba,
    surface_hover: Rgba,
    surface_selected: Rgba,
    chrome: Option<Rgba>,
    elevated: Option<Rgba>,
    input: Option<Rgba>,
    text: Rgba,
    text_secondary: Rgba,
    text_muted: Rgba,
    border: Rgba,
    border_subtle: Option<Rgba>,
    accent: Rgba,
    accent_foreground: Rgba,
    info: Rgba,
    positive: Rgba,
    warning: Rgba,
    destructive: Rgba,
    destructive_foreground: Rgba,
    scrim: Rgba,
}

impl From<UiPaletteDocument> for UiPalette {
    fn from(document: UiPaletteDocument) -> Self {
        Self {
            chrome: document.chrome.unwrap_or(document.surface),
            elevated: document.elevated.unwrap_or(document.surface_hover),
            input: document.input.unwrap_or(document.canvas),
            border_subtle: document.border_subtle.unwrap_or_else(|| document.border.mix(document.canvas, 128)),
            canvas: document.canvas,
            surface: document.surface,
            surface_hover: document.surface_hover,
            surface_selected: document.surface_selected,
            text: document.text,
            text_secondary: document.text_secondary,
            text_muted: document.text_muted,
            border: document.border,
            accent: document.accent,
            accent_foreground: document.accent_foreground,
            info: document.info,
            positive: document.positive,
            warning: document.warning,
            destructive: document.destructive,
            destructive_foreground: document.destructive_foreground,
            scrim: document.scrim,
        }
    }
}

pub(crate) fn palette<'de, D: Deserializer<'de>>(deserializer: D) -> Result<UiPalette, D::Error> {
    UiPaletteDocument::deserialize(deserializer).map(UiPalette::from)
}

pub(crate) const fn colors(ui: &UiPalette) -> [Rgba; 20] {
    [
        ui.canvas,
        ui.surface,
        ui.surface_hover,
        ui.surface_selected,
        ui.chrome,
        ui.elevated,
        ui.input,
        ui.text,
        ui.text_secondary,
        ui.text_muted,
        ui.border,
        ui.border_subtle,
        ui.accent,
        ui.accent_foreground,
        ui.info,
        ui.positive,
        ui.warning,
        ui.destructive,
        ui.destructive_foreground,
        ui.scrim,
    ]
}

pub(crate) const fn bytes(color: Rgba) -> [u8; 4] {
    [color.r, color.g, color.b, color.a]
}

static BUILTINS: LazyLock<Result<BTreeMap<String, UiPalette>, ThemeError>> = LazyLock::new(|| {
    serde_json::from_slice::<BTreeMap<String, UiPaletteDocument>>(include_bytes!("../assets/ui.json"))
        .map(|palettes| palettes.into_iter().map(|(id, palette)| (id, palette.into())).collect())
        .map_err(|error| ThemeError::Parse { message: error.to_string() })
});

pub(crate) fn builtin(id: &str) -> Result<UiPalette, ThemeError> {
    BUILTINS
        .as_ref()
        .map_err(Clone::clone)?
        .get(id)
        .copied()
        .ok_or_else(|| ThemeError::Parse { message: format!("built-in theme `{id}` has no UI palette") })
}
