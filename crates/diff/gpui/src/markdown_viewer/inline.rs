use crate::ui::prelude::{CODE_FONT, UiTheme, color};
use clankerdiff_markdown::MarkdownInline;
use clankerdiff_theme::ReviewTheme;
use gpui::{
    AnyElement, ElementId, FontStyle, FontWeight, HighlightStyle, Hsla, InteractiveText, IntoElement, SharedString,
    StrikethroughStyle, StyledText, UnderlineStyle, px,
};
use std::ops::Range;

#[derive(Debug, Clone, Copy)]
pub(super) struct InlineColors {
    link: Hsla,
    code: Hsla,
    code_background: Hsla,
    muted: Hsla,
}

impl InlineColors {
    pub(super) fn new(theme: &ReviewTheme, ui: &UiTheme) -> Self {
        let markdown = &theme.markdown;
        Self {
            link: color(markdown.link),
            code: color(markdown.inline_code),
            code_background: color(markdown.inline_code_background),
            muted: ui.colors.text_muted,
        }
    }
}

pub(super) fn inline_text(id: impl Into<ElementId>, content: &[MarkdownInline], colors: InlineColors) -> AnyElement {
    let mut builder =
        Builder { colors, text: String::new(), highlights: Vec::new(), code: Vec::new(), links: Vec::new() };
    builder.children(content, Marks::NONE);
    let text = StyledText::new(SharedString::from(builder.text))
        .with_highlights(builder.highlights)
        .with_font_family_overrides(builder.code.into_iter().map(|range| (range, SharedString::new_static(CODE_FONT))));
    if builder.links.is_empty() {
        return text.into_any_element();
    }
    let (ranges, urls): (Vec<_>, Vec<_>) = builder.links.into_iter().unzip();
    InteractiveText::new(id, text)
        .on_click(ranges, move |index, _, cx| {
            if let Some(url) = urls.get(index) {
                cx.open_url(url);
            }
        })
        .into_any_element()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Marks(u8);

impl Marks {
    const NONE: Self = Self(0);
    const STRONG: Self = Self(1);
    const EMPHASIS: Self = Self(1 << 1);
    const STRIKETHROUGH: Self = Self(1 << 2);
    const CODE: Self = Self(1 << 3);
    const LINK: Self = Self(1 << 4);
    const IMAGE: Self = Self(1 << 5);

    const fn with(self, mark: Self) -> Self {
        Self(self.0 | mark.0)
    }

    const fn has(self, mark: Self) -> bool {
        self.0 & mark.0 != 0
    }

    fn style(self, colors: InlineColors) -> Option<HighlightStyle> {
        if self == Self::NONE {
            return None;
        }
        let color = if self.has(Self::LINK) {
            Some(colors.link)
        } else if self.has(Self::CODE) {
            Some(colors.code)
        } else if self.has(Self::IMAGE) {
            Some(colors.muted)
        } else {
            None
        };
        Some(HighlightStyle {
            color,
            font_weight: self.has(Self::STRONG).then_some(FontWeight::SEMIBOLD),
            font_style: (self.has(Self::EMPHASIS) || self.has(Self::IMAGE)).then_some(FontStyle::Italic),
            background_color: self.has(Self::CODE).then_some(colors.code_background),
            underline: self.has(Self::LINK).then_some(UnderlineStyle {
                thickness: px(1.0),
                color: Some(colors.link),
                wavy: false,
            }),
            strikethrough: self
                .has(Self::STRIKETHROUGH)
                .then_some(StrikethroughStyle { thickness: px(1.0), color: None }),
            ..HighlightStyle::default()
        })
    }
}

struct Builder {
    colors: InlineColors,
    text: String,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    code: Vec<Range<usize>>,
    links: Vec<(Range<usize>, String)>,
}

impl Builder {
    fn push(&mut self, inline: &MarkdownInline, marks: Marks) {
        match inline {
            MarkdownInline::Text(text) => self.leaf(text, marks),
            MarkdownInline::Code(text) => self.leaf(text, marks.with(Marks::CODE)),
            MarkdownInline::ImageAlt(text) => self.leaf(text, marks.with(Marks::IMAGE)),
            MarkdownInline::SoftBreak => self.leaf(" ", marks),
            MarkdownInline::HardBreak => self.leaf("\n", marks),
            MarkdownInline::Strong(children) => self.children(children, marks.with(Marks::STRONG)),
            MarkdownInline::Emphasis(children) => {
                self.children(children, marks.with(Marks::EMPHASIS));
            }
            MarkdownInline::Strikethrough(children) => {
                self.children(children, marks.with(Marks::STRIKETHROUGH));
            }
            MarkdownInline::Link { destination, content, .. } => {
                let start = self.text.len();
                self.children(content, marks.with(Marks::LINK));
                if self.text.len() > start {
                    self.links.push((start..self.text.len(), destination.clone()));
                }
            }
        }
    }

    fn children(&mut self, children: &[MarkdownInline], marks: Marks) {
        for child in children {
            self.push(child, marks);
        }
    }

    fn leaf(&mut self, text: &str, marks: Marks) {
        if text.is_empty() {
            return;
        }
        let range = self.text.len()..self.text.len() + text.len();
        self.text.push_str(text);
        if let Some(style) = marks.style(self.colors) {
            self.highlights.push((range.clone(), style));
        }
        if marks.has(Marks::CODE) {
            self.code.push(range);
        }
    }
}
