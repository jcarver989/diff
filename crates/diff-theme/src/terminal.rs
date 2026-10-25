use crate::{Appearance, ReviewTheme, Rgba};
use std::{collections::HashMap, sync::LazyLock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalPalette {
    pub foreground: Rgba,
    pub background: Rgba,
    pub cursor: Rgba,
    pub cursor_text: Rgba,
    pub selection: Rgba,
    pub ansi: [Rgba; 16],
}

impl TerminalPalette {
    #[must_use]
    pub fn indexed(&self, index: u8) -> Rgba {
        match index {
            0..16 => self.ansi[usize::from(index)],
            16..232 => {
                let index = index - 16;
                let level = |value: u8| if value == 0 { 0 } else { value * 40 + 55 };
                opaque(level(index / 36), level((index / 6) % 6), level(index % 6))
            }
            232..=255 => {
                let gray = (index - 232) * 10 + 8;
                opaque(gray, gray, gray)
            }
        }
    }

    #[must_use]
    pub const fn dim(&self, color: Rgba) -> Rgba {
        color.mix(self.background, 153)
    }
}

impl Default for TerminalPalette {
    fn default() -> Self {
        Self {
            foreground: opaque(0xd8, 0xde, 0xe9),
            background: opaque(0x16, 0x18, 0x1d),
            cursor: opaque(0xd8, 0xde, 0xe9),
            cursor_text: opaque(0x16, 0x18, 0x1d),
            selection: opaque(0x3b, 0x42, 0x52),
            ansi: [
                opaque(0x2e, 0x34, 0x40),
                opaque(0xbf, 0x61, 0x6a),
                opaque(0xa3, 0xbe, 0x8c),
                opaque(0xeb, 0xcb, 0x8b),
                opaque(0x81, 0xa1, 0xc1),
                opaque(0xb4, 0x8e, 0xad),
                opaque(0x88, 0xc0, 0xd0),
                opaque(0xe5, 0xe9, 0xf0),
                opaque(0x4c, 0x56, 0x6a),
                opaque(0xd0, 0x87, 0x70),
                opaque(0xb5, 0xd0, 0x9e),
                opaque(0xf0, 0xd8, 0xa8),
                opaque(0x9a, 0xb8, 0xd7),
                opaque(0xc8, 0xa8, 0xc5),
                opaque(0x8f, 0xbc, 0xbb),
                opaque(0xec, 0xef, 0xf4),
            ],
        }
    }
}

impl ReviewTheme {
    #[must_use]
    pub fn terminal_palette(&self) -> TerminalPalette {
        let ui = &self.ui;
        let background = solid(ui.canvas);
        TerminalPalette {
            foreground: solid(ui.text),
            background,
            cursor: solid(ui.accent),
            cursor_text: background,
            selection: solid(self.diff.selection),
            ansi: CURATED
                .get(self.id().to_string().as_str())
                .copied()
                .unwrap_or_else(|| derived_ansi(self)),
        }
    }
}

static CURATED: LazyLock<HashMap<String, [Rgba; 16]>> = LazyLock::new(|| {
    let palettes: HashMap<String, [String; 16]> =
        serde_json::from_str(include_str!("../assets/terminal.json"))
            .expect("the bundled terminal palettes are valid");
    palettes
        .into_iter()
        .map(|(id, colors)| {
            let colors = colors.map(|color| hex(&color).expect("the bundled colors are #rrggbb"));
            (id, colors)
        })
        .collect()
});

fn derived_ansi(theme: &ReviewTheme) -> [Rgba; 16] {
    let ui = &theme.ui;
    let dark = theme.appearance() == Appearance::Dark;
    let syntax = |names: &[&str], fallback: Rgba| {
        names
            .iter()
            .find_map(|name| theme.syntax.style(name))
            .map_or(solid(fallback), |style| solid(style.foreground))
    };
    let canvas = solid(ui.canvas);
    let text = solid(ui.text);
    let red = solid(ui.destructive);
    let green = solid(ui.positive);
    let yellow = solid(ui.warning);
    let blue = solid(ui.info);
    let magenta = syntax(&["keyword", "keyword.function", "constant"], ui.accent);
    let cyan = syntax(
        &["property", "string.special", "type", "constructor"],
        ui.info,
    );
    let (black, white, bright_black, bright_white) = if dark {
        (
            text.mix(canvas, 46),
            solid(ui.text_secondary),
            solid(ui.text_muted),
            text,
        )
    } else {
        (
            text,
            text.mix(canvas, 64),
            solid(ui.text_muted),
            text.mix(canvas, 26),
        )
    };
    let brighten = |color: Rgba| {
        if dark {
            opaque(255, 255, 255).mix(color, 46)
        } else {
            opaque(0, 0, 0).mix(color, 38)
        }
    };
    [
        black,
        red,
        green,
        yellow,
        blue,
        magenta,
        cyan,
        white,
        bright_black,
        brighten(red),
        brighten(green),
        brighten(yellow),
        brighten(blue),
        brighten(magenta),
        brighten(cyan),
        bright_white,
    ]
}

const fn opaque(r: u8, g: u8, b: u8) -> Rgba {
    Rgba::new(r, g, b, u8::MAX)
}

const fn solid(color: Rgba) -> Rgba {
    opaque(color.r, color.g, color.b)
}

fn hex(value: &str) -> Option<Rgba> {
    let hex = value.strip_prefix('#').filter(|hex| hex.len() == 6)?;
    let channel = |range| u8::from_str_radix(hex.get(range)?, 16).ok();
    Some(opaque(channel(0..2)?, channel(2..4)?, channel(4..6)?))
}
