use crate::{
    DiffViewer,
    markdown_viewer::{content::MarkdownContent, inline::InlineColors, rows::document_rows},
    style,
    ui::{
        prelude::{CODE_FONT, SurfaceExt, Typography, TypographyExt, rem},
        tokens::{CODE_ROW_HEIGHT, POPOVER_WIDTH},
    },
};
use clankerdiff_core::{
    DiffReviewCommand, DiffReviewEvent, LookupReply, ReviewSession, SourceLocation, TargetLocation,
};
use clankerdiff_markdown::{MarkdownOptions, parse_markdown};
use clankerdiff_symbols::{Activation, DefinitionCard, Hold, NameLookup, Settled, SymbolIndex, SymbolReference};
use clankerdiff_syntax::LanguageHint;
use gpui::{
    AnyElement, App, Context, Div, MouseDownEvent, Pixels, StyledText, anchored, deferred, div, font, prelude::*, px,
};
use std::time::Duration;

const CARD_HIDE_DELAY: Duration = Duration::from_millis(200);
const CARD_MIN_WIDTH: f32 = 320.0;
const DOCS_MAX_HEIGHT: f32 = 240.0;

impl DiffViewer {
    pub(crate) fn update_names<T>(
        &mut self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut NameLookup, &mut ReviewSession, &SymbolIndex, bool) -> T,
    ) -> T {
        let index = self.symbols();
        let result = update(&mut self.names, &mut self.session, &index, self.capabilities.lookups);
        for lookup in self.names.take_lookups() {
            cx.emit(DiffReviewEvent::Lookup(lookup));
        }
        cx.notify();
        result
    }

    pub(crate) fn show_definition(
        &mut self,
        origin: SourceLocation,
        reference: &SymbolReference,
        hold: Hold,
        cx: &mut Context<Self>,
    ) {
        let card = DefinitionCard::new(origin, reference);
        self.card_timer = None;
        self.update_names(cx, |names, session, _, enabled| names.show(card, session, enabled, hold));
    }

    pub fn lookup_finished(&mut self, reply: LookupReply, cx: &mut Context<Self>) {
        match self.update_names(cx, |names, session, index, _| names.finish(reply, session, index)) {
            Settled::Ignored | Settled::Redraw => {}
            Settled::Opened => self.reveal_navigation(cx),
            Settled::Jump(target) => {
                self.jump_to(target, cx);
            }
        }
    }

    pub(crate) fn close_names(&mut self, cx: &mut Context<Self>) -> bool {
        self.card_timer = None;
        if self.names.card().is_none() {
            return false;
        }
        self.update_names(cx, |names, _, _, _| names.close())
    }

    pub(crate) fn card_token_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.names.hover_token(hovered);
        self.schedule_card_hide(cx);
    }

    fn card_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        self.names.hover_card(hovered);
        self.schedule_card_hide(cx);
    }

    fn schedule_card_hide(&mut self, cx: &mut Context<Self>) {
        if !self.names.lingering() {
            self.card_timer = None;
            return;
        }
        self.card_timer = Some(cx.spawn(async move |viewer, cx| {
            cx.background_executor().timer(CARD_HIDE_DELAY).await;
            let _ = viewer.update(cx, |viewer, cx| {
                viewer.card_timer = None;
                if viewer.names.lingering() {
                    viewer.close_names(cx);
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
        if self.names.card().is_some_and(|card| card.shows(&origin, &reference.span.columns)) {
            self.names.pin();
        } else {
            self.show_definition(origin, reference, Hold::Pinned, cx);
        }
        self.jump_to_card_choice(cx);
    }

    pub(crate) fn jump_to_card_choice(&mut self, cx: &mut Context<Self>) -> bool {
        match self.update_names(cx, |names, _, index, _| names.activate(index)) {
            Activation::Idle => false,
            Activation::Wait => true,
            Activation::Jump(target) => self.jump_to(target, cx),
        }
    }

    fn jump_to(&mut self, target: TargetLocation, cx: &mut Context<Self>) -> bool {
        let command = DiffReviewCommand::JumpTo(target);
        if !self.command_enabled(&command) {
            return false;
        }
        self.close_names(cx);
        self.navigate(command, cx)
    }

    pub(crate) fn cycle_card_choice(&mut self, cx: &mut Context<Self>) -> bool {
        self.update_names(cx, |names, _, index, _| names.cycle(index))
    }

    #[expect(clippy::cast_precision_loss, reason = "a column offset is far below f32's exact integer range")]
    pub(crate) fn render_card_anchor(&mut self, text: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let card = self.names.card()?.clone();
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
        let content = card.content(&index);
        let head = match (&content.message, &content.code) {
            (Some(message), _) => Some(
                div()
                    .typography(Typography::Body)
                    .text_color(colors.text_secondary)
                    .child(message.clone())
                    .into_any_element(),
            ),
            (None, Some((path, code))) => Some(
                self.render_code(LanguageHint::Path(path.as_str()), code)
                    .debug_selector(|| "definition-card-signature".to_owned())
                    .into_any_element(),
            ),
            (None, None) => None,
        };
        let docs = content.docs.map(|docs| {
            let rows = document_rows(parse_markdown(docs, MarkdownOptions { intra_doc_links: true }).blocks());
            let mut markdown = MarkdownContent {
                theme: &self.theme,
                ui: &theme,
                inline: InlineColors::new(&self.theme, &theme),
                highlighter: &mut self.highlighter,
                copy: false,
                compact: true,
            };
            div()
                .id("definition-card-docs")
                .debug_selector(|| "definition-card-docs".to_owned())
                .max_h(rem(DOCS_MAX_HEIGHT))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .typography(Typography::Body)
                .text_color(colors.text)
                .children(markdown.rows("definition-card-doc", &rows))
        });
        let head_divider = head.is_some();
        let meta = div()
            .flex()
            .items_center()
            .gap_3()
            .typography(Typography::Caption)
            .text_color(colors.text_secondary)
            .child(div().min_w_0().flex_1().overflow_hidden().whitespace_nowrap().children(content.place.clone()))
            .child(div().flex_shrink_0().child(content.uses.clone()));
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
                    viewer.close_names(cx);
                }
            }))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .children(head.map(|head| div().px_3().py_2p5().child(head)))
            .children(head_divider.then(|| div().h(px(1.0)).bg(colors.border_subtle)))
            .child(div().px_3().py_2p5().flex().flex_col().gap_2p5().children(docs).child(meta))
            .into_any_element()
    }

    fn render_code(&mut self, language: LanguageHint<'_>, code: &str) -> Div {
        let lines: Vec<StyledText> = self
            .highlighter
            .with_theme(&self.theme.syntax)
            .highlight_lines(language, code)
            .into_iter()
            .map(|(line, spans)| style::code_text(line.to_owned().into(), &spans))
            .collect();
        div().flex().flex_col().typography(Typography::Code).children(lines)
    }
}
