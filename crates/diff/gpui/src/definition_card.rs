use crate::{
    DiffViewer,
    markdown_viewer::inline::{InlineColors, inline_text},
    style,
    ui::{
        prelude::{CODE_FONT, SurfaceExt, Typography, TypographyExt, rem},
        tokens::{CODE_ROW_HEIGHT, POPOVER_WIDTH, RADIUS_INNER},
    },
};
use clankerdiff_core::{DiffReviewCommand, SourceDocument, SourceLocation};
use clankerdiff_markdown::{MarkdownBlock, MarkdownBlockKind, parse_markdown, rendered_text};
use clankerdiff_symbols::{Definition, DefinitionCard, SymbolReference};
use gpui::{
    AnyElement, App, Context, MouseDownEvent, Pixels, SharedString, StyledText, anchored, deferred, div, font,
    prelude::*, px,
};
use std::{ops::Range, time::Duration};

const CARD_HIDE_DELAY: Duration = Duration::from_millis(200);
const CARD_MIN_WIDTH: f32 = 320.0;
const DOCS_MAX_HEIGHT: f32 = 240.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hold {
    Hover,
    Pinned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HoverCard {
    pub(crate) card: DefinitionCard,
    hold: Hold,
    token_hovered: bool,
    card_hovered: bool,
}

impl HoverCard {
    fn lingering(&self) -> bool {
        self.hold == Hold::Hover && !self.token_hovered && !self.card_hovered
    }

    pub(crate) fn shows(&self, origin: &SourceLocation, columns: &Range<usize>) -> bool {
        self.card.shows(origin, columns)
    }
}

impl DiffViewer {
    pub(crate) fn show_definition(
        &mut self,
        origin: SourceLocation,
        reference: &SymbolReference,
        hold: Hold,
        cx: &mut Context<Self>,
    ) {
        self.show_card(DefinitionCard::new(origin, reference), hold, cx);
    }

    pub(crate) fn show_card(&mut self, card: DefinitionCard, hold: Hold, cx: &mut Context<Self>) {
        self.card_timer = None;
        self.card = Some(HoverCard { card, hold, token_hovered: hold == Hold::Hover, card_hovered: false });
        cx.notify();
    }

    pub(crate) fn close_card(&mut self, cx: &mut Context<Self>) -> bool {
        self.card_timer = None;
        if self.card.take().is_none() {
            return false;
        }
        cx.notify();
        true
    }

    pub(crate) fn card_token_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.card {
            card.token_hovered = hovered;
        }
        self.schedule_card_hide(cx);
    }

    fn card_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.card {
            card.card_hovered = hovered;
        }
        self.schedule_card_hide(cx);
    }

    fn schedule_card_hide(&mut self, cx: &mut Context<Self>) {
        if !self.card.as_ref().is_some_and(HoverCard::lingering) {
            self.card_timer = None;
            return;
        }
        self.card_timer = Some(cx.spawn(async move |viewer, cx| {
            cx.background_executor().timer(CARD_HIDE_DELAY).await;
            let _ = viewer.update(cx, |viewer, cx| {
                viewer.card_timer = None;
                if viewer.card.as_ref().is_some_and(HoverCard::lingering) {
                    viewer.close_card(cx);
                }
            });
        }));
    }

    pub(crate) fn follow_definition(
        &mut self,
        origin: SourceLocation,
        reference: &SymbolReference,
        cx: &mut Context<Self>,
    ) {
        match &mut self.card {
            Some(card) if card.shows(&origin, &reference.span.columns) => card.hold = Hold::Pinned,
            _ => self.show_definition(origin, reference, Hold::Pinned, cx),
        }
        self.jump_to_card_choice(cx);
    }

    pub(crate) fn jump_to_card_choice(&mut self, cx: &mut Context<Self>) -> bool {
        let index = self.symbols();
        let Some(target) = self.card.as_ref().and_then(|hover| hover.card.target(&index)) else {
            return false;
        };
        let command = DiffReviewCommand::JumpTo(target);
        if !self.command_enabled(&command) {
            return false;
        }
        self.clear_name_hints(cx);
        self.close_card(cx);
        self.navigate(command, cx)
    }

    pub(crate) fn cycle_card_choice(&mut self, cx: &mut Context<Self>) {
        let index = self.symbols();
        if let Some(hover) = self.card.as_mut()
            && hover.card.cycle(&index)
        {
            hover.hold = Hold::Pinned;
            cx.notify();
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "a column offset is far below f32's exact integer range")]
    pub(crate) fn render_card_anchor(&mut self, text: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let card = self.card.as_ref()?.card.clone();
        let preceding = text.get(..card.columns.start).map_or(0, |prefix| prefix.chars().count());
        let left = self.code_advance(cx) * preceding as f32;
        let line_height = rem(CODE_ROW_HEIGHT);
        let body = self.render_card(&card, cx);
        Some(
            div()
                .absolute()
                .top(line_height / 2.0)
                .left(left)
                .child(
                    deferred(
                        anchored().snap_to_window_with_margin(px(8.0)).child(div().py(line_height / 2.0).child(body)),
                    )
                    .with_priority(1),
                )
                .into_any_element(),
        )
    }

    fn code_advance(&self, cx: &App) -> Pixels {
        let size = rem(Typography::Code.size()).to_pixels(self.rem_size());
        let text_system = cx.text_system();
        let font_id = text_system.resolve_font(&font(CODE_FONT));
        text_system.advance(font_id, size, 'm').map_or(size * 0.6, |advance| advance.width)
    }

    fn render_card(&mut self, card: &DefinitionCard, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.ui_theme();
        let colors = theme.colors;
        let index = self.symbols();
        let choice = card.choice(&index);
        let uses = format!("{} in this diff", uses_label(index.uses(&card.name).len()));
        let head = match choice {
            Some(choice) => self.render_signature(choice.definition),
            None => div()
                .typography(Typography::Body)
                .text_color(colors.text_secondary)
                .child(format!("No definition of {} in this diff", card.name))
                .into_any_element(),
        };
        let docs = choice.and_then(|choice| choice.definition.docs.as_deref()).map(|docs| {
            let colors = InlineColors::new(self.theme(), &theme);
            let document = parse_markdown(docs);
            div()
                .id("definition-card-docs")
                .debug_selector(|| "definition-card-docs".to_owned())
                .max_h(rem(DOCS_MAX_HEIGHT))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2()
                .typography(Typography::Body)
                .text_color(theme.colors.text)
                .children(
                    document
                        .blocks()
                        .iter()
                        .enumerate()
                        .map(|(index, block)| doc_block(index, block, colors, theme.colors.surface_hover)),
                )
        });
        let place = choice.map(|choice| {
            let location = choice.definition.location.clone();
            let location = format!("{}:{}", location.path, location.line_number);
            match choice.count {
                0 | 1 => location,
                count => format!("{location} · {} of {count}", choice.position + 1),
            }
        });
        let meta = div()
            .flex()
            .items_center()
            .gap_3()
            .typography(Typography::Caption)
            .text_color(colors.text_secondary)
            .child(div().min_w_0().flex_1().overflow_hidden().whitespace_nowrap().children(place))
            .child(div().flex_shrink_0().child(uses));
        div()
            .id("definition-card")
            .debug_selector(|| "definition-card".to_owned())
            .occlude()
            .min_w(rem(CARD_MIN_WIDTH))
            .max_w(rem(POPOVER_WIDTH))
            .floating(theme)
            .overflow_hidden()
            .flex()
            .flex_col()
            .cursor_default()
            .on_hover(cx.listener(|viewer, hovered: &bool, _, cx| viewer.card_hovered(*hovered, cx)))
            .on_mouse_down_out(cx.listener(|viewer, event: &MouseDownEvent, _, cx| {
                if !event.modifiers.secondary() {
                    viewer.close_card(cx);
                }
            }))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(div().px_3().py_2p5().child(head))
            .child(div().h(px(1.0)).bg(colors.border_subtle))
            .child(div().px_3().py_2p5().flex().flex_col().gap_2p5().children(docs).child(meta))
            .into_any_element()
    }

    fn render_signature(&mut self, definition: &Definition) -> AnyElement {
        let path = definition.location.path.as_str().to_owned();
        let lines: Vec<AnyElement> =
            match SourceDocument::new(&definition.signature) {
                Ok(source) => (1..=source.line_count())
                    .map(|line_number| {
                        let text: SharedString = source.line(line_number).unwrap_or_default().to_owned().into();
                        let highlights = self.highlight_source_line(&path, &source, line_number);
                        StyledText::new(text)
                            .with_highlights(highlights.iter().map(|span| {
                                (span.range.clone(), style::highlight_style(span.foreground, span.font_style))
                            }))
                            .into_any_element()
                    })
                    .collect(),
                Err(_) => vec![definition.signature.clone().into_any_element()],
            };
        div()
            .debug_selector(|| "definition-card-signature".to_owned())
            .flex()
            .flex_col()
            .typography(Typography::Code)
            .children(lines)
            .into_any_element()
    }
}

fn doc_block(index: usize, block: &MarkdownBlock, colors: InlineColors, code_background: gpui::Hsla) -> AnyElement {
    let id = ("definition-card-doc", index);
    match &block.kind {
        MarkdownBlockKind::Heading { content, .. }
        | MarkdownBlockKind::Paragraph { content }
        | MarkdownBlockKind::HtmlFallback { content } => inline_text(id, content, colors),
        MarkdownBlockKind::List { items, .. } => div()
            .flex()
            .flex_col()
            .gap_1()
            .children(items.iter().enumerate().map(|(item, entry)| {
                div().flex().gap_1p5().child("•").child(inline_text(
                    (id.0, index * 1000 + item),
                    &entry.content,
                    colors,
                ))
            }))
            .into_any_element(),
        MarkdownBlockKind::BlockQuote { blocks } => div()
            .flex()
            .flex_col()
            .gap_2()
            .children(
                blocks
                    .iter()
                    .enumerate()
                    .map(|(nested, block)| doc_block(index * 1000 + nested, block, colors, code_background)),
            )
            .into_any_element(),
        MarkdownBlockKind::CodeBlock(code) => div()
            .px_2()
            .py_1()
            .rounded(rem(RADIUS_INNER))
            .bg(code_background)
            .typography(Typography::Code)
            .children(code.lines.iter().map(|line| div().child(line.text.clone())))
            .into_any_element(),
        MarkdownBlockKind::Table(table) => {
            div()
                .flex()
                .flex_col()
                .children(table.rows.iter().map(|row| {
                    row.cells.iter().map(|cell| rendered_text(&cell.content)).collect::<Vec<_>>().join(" | ")
                }))
                .into_any_element()
        }
        MarkdownBlockKind::Rule => div().h(px(1.0)).bg(code_background).into_any_element(),
    }
}

fn uses_label(count: usize) -> String {
    if count == 1 { "1 use".to_owned() } else { format!("{count} uses") }
}
