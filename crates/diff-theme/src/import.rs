use crate::{
    DiffPalette, DiffTheme, FontStyle, MarkdownPalette, ReviewTheme, Rgba, SyntaxStyle, ThemeError,
};
use std::io::{BufReader, Cursor};
use syntect::{
    highlighting::{FontStyle as SyntectFontStyle, Highlighter, Theme, ThemeSet},
    parsing::Scope,
};

pub(crate) fn from_tm_theme_bytes(id: &str, bytes: &[u8]) -> Result<ReviewTheme, ThemeError> {
    let cursor = Cursor::new(bytes);
    let mut reader = BufReader::new(cursor);
    let source = ThemeSet::load_from_reader(&mut reader).map_err(|error| ThemeError::Parse {
        message: error.to_string(),
    })?;
    convert(id, &source)
}

fn convert(id: &str, source: &Theme) -> Result<ReviewTheme, ThemeError> {
    let background = source
        .settings
        .background
        .map_or(Rgba::new(21, 29, 31, 255), rgba);
    let foreground = source.settings.foreground.map_or(Rgba::default(), rgba);
    let accent = source
        .settings
        .accent
        .map_or(Rgba::new(143, 188, 176, 255), rgba);
    let highlighter = Highlighter::new(source);
    let resolve = |scope: &str| -> Option<SyntaxStyle> {
        let scope = Scope::new(scope).ok()?;
        let style = highlighter.style_mod_for_stack(&[scope]);
        Some(SyntaxStyle {
            foreground: style.foreground.map_or(foreground, rgba),
            font_style: style.font_style.map_or_else(FontStyle::none, font_style),
        })
    };
    let addition = resolve("markup.inserted.diff")
        .map_or(Rgba::new(128, 190, 120, 255), |style| style.foreground);
    let deletion = resolve("markup.deleted.diff")
        .map_or(Rgba::new(225, 115, 115, 255), |style| style.foreground);
    let palette = DiffPalette {
        background,
        foreground,
        gutter: source
            .settings
            .gutter_foreground
            .map_or(Rgba::new(80, 96, 91, 255), rgba),
        addition,
        deletion,
        addition_background: translucent_over(addition, background),
        deletion_background: translucent_over(deletion, background),
        selection: source.settings.selection.map_or(
            Rgba::new(accent.r, accent.g, accent.b, 45).over(background),
            rgba,
        ),
        accent,
        muted: mix(foreground, background, 62),
        border: mix(foreground, background, 25),
    };
    let mappings = [
        ("comment", "comment"),
        ("string", "string"),
        ("number", "constant.numeric"),
        ("boolean", "constant.language.boolean"),
        ("constant", "constant"),
        ("keyword", "keyword"),
        ("operator", "keyword.operator"),
        ("function", "entity.name.function"),
        ("function.method", "entity.name.function"),
        ("type", "entity.name.type"),
        ("constructor", "entity.name.class"),
        ("variable", "variable"),
        ("property", "variable.other.property"),
        ("tag", "entity.name.tag"),
        ("attribute", "entity.other.attribute-name"),
        ("markup.heading", "markup.heading"),
        ("markup.bold", "markup.bold"),
        ("markup.italic", "markup.italic"),
        ("markup.link", "markup.underline.link"),
        ("markup.raw", "markup.raw"),
    ];
    let mut builder = DiffTheme::builder(id).palette(palette.clone());
    for (capture, scope) in mappings {
        if let Some(style) = resolve(scope) {
            builder = builder.capture(capture, style);
        }
    }
    let syntax = builder.build()?;
    let markdown = MarkdownPalette {
        heading: syntax
            .style("markup.heading")
            .map_or(accent, |style| style.foreground),
        link: syntax
            .style("markup.link")
            .map_or(accent, |style| style.foreground),
        quote: palette.muted,
        code: syntax
            .style("markup.raw")
            .map_or(foreground, |style| style.foreground),
    };
    Ok(ReviewTheme {
        syntax,
        markdown,
        diff: palette,
    })
}

const fn rgba(color: syntect::highlighting::Color) -> Rgba {
    Rgba::new(color.r, color.g, color.b, color.a)
}

fn font_style(style: SyntectFontStyle) -> FontStyle {
    FontStyle {
        bold: style.contains(SyntectFontStyle::BOLD),
        italic: style.contains(SyntectFontStyle::ITALIC),
        underline: style.contains(SyntectFontStyle::UNDERLINE),
    }
}

const fn mix(foreground: Rgba, background: Rgba, alpha: u8) -> Rgba {
    Rgba::new(foreground.r, foreground.g, foreground.b, alpha).over(background)
}

const fn translucent_over(foreground: Rgba, background: Rgba) -> Rgba {
    Rgba::new(foreground.r, foreground.g, foreground.b, 31).over(background)
}
