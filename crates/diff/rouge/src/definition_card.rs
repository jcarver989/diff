use crate::{ReviewHandle, style};
use clankerdiff_core::{DiffReviewCommand as C, SourceLocation};
use clankerdiff_rouge_ui::{
    components::{Button, ControlSize},
    theme::UiTheme,
    tokens::rem,
    typography::{Typography, TypographyExt},
};
use clankerdiff_symbols::SymbolReference;
use rouge_core::{Modifiers, Styled, View, div, dynamic, text};
use rouge_reactive::{Signal, memo};
use std::ops::Range;

pub(crate) fn links(modifiers: Modifiers) -> bool {
    let secondary = modifiers.command();
    secondary && !modifiers.shift && !modifiers.alt
}

pub(crate) fn hover(
    handle: &ReviewHandle,
    origin: &SourceLocation,
    byte: Option<usize>,
    modifiers: Modifiers,
    hovered: Signal<Option<Range<usize>>>,
) {
    let shown =
        handle.with(|s| s.definition_card().filter(|card| card.origin == *origin).map(|card| card.columns.clone()));
    let target = byte.and_then(|byte| {
        if links(modifiers) {
            reference(handle, origin, byte).map(|r| r.span.columns)
        } else {
            shown.as_ref().filter(|range| range.contains(&byte)).cloned()
        }
    });
    if hovered.get_untracked() == target {
        return;
    }
    hovered.set(target.clone());
    match (target, shown, byte) {
        (Some(target), Some(shown), _) if target == shown => handle.set_definition_hovered(true, true),
        (Some(_), _, Some(byte)) if let Some(reference) = reference(handle, origin, byte) => {
            handle.hover_definition(origin.clone(), &reference);
        }
        _ => handle.set_definition_hovered(true, false),
    }
}

pub(crate) fn click(handle: &ReviewHandle, origin: &SourceLocation, byte: Option<usize>, modifiers: Modifiers) {
    if links(modifiers)
        && let Some(reference) = byte.and_then(|byte| reference(handle, origin, byte))
    {
        handle.hover_definition(origin.clone(), &reference);
        handle.handle_command(C::GoToDefinition);
    }
}

fn reference(handle: &ReviewHandle, origin: &SourceLocation, byte: usize) -> Option<SymbolReference> {
    handle.with_syntax(|_, syntax| {
        syntax.ready_symbols()?.references_at(origin).iter().find(|r| r.span.columns.contains(&byte)).cloned()
    })
}

pub(crate) fn at_line(handle: ReviewHandle, location: SourceLocation, theme: UiTheme) -> impl View {
    let card = memo({
        let handle = handle.clone();
        move || {
            handle.definition_revision().get();
            handle.with(|s| s.definition_card().filter(|card| card.origin == location).cloned())
        }
    });
    dynamic(move || {
        let Some(card) = card.get() else {
            return div().into_any();
        };
        let review_theme = handle.theme().get();
        let (body, can_jump, can_cycle) = handle.with_syntax(|_, syntax| {
            let Some(index) = syntax.ready_symbols() else {
                return (div(), false, false);
            };
            let content = card.content(&index);
            let mut body = div().flex().col().gap(rem(4.0));
            if let Some(message) = content.message {
                body = body.child(text(message).typography(Typography::Body).color(theme.paint(|c| c.text_secondary)));
            } else if let Some(code) = content.code {
                body = body.child(div().test_id("definition-card-signature").flex().col().overflow_hidden().children(
                    code.map_lines(|line, spans| {
                        style::code(line, spans, &review_theme.syntax).color(theme.paint(|c| c.text))
                    }),
                ));
            }
            if let Some(docs) = content.docs {
                body = body.child(
                    rouge_core::scroll()
                        .test_id("definition-card-docs")
                        .max_h(rem(168.0))
                        .child(text(docs.to_owned()).typography(Typography::Body).color(theme.paint(|c| c.text))),
                );
            }
            if let Some(place) = content.place {
                body = body.child(text(place).typography(Typography::Caption).color(theme.paint(|c| c.text_muted)));
            }
            body = body.child(text(content.uses).typography(Typography::Caption).color(theme.paint(|c| c.text_muted)));
            (body, content.can_jump, content.can_cycle)
        });
        let button = |id, label, command: C| {
            Button::new(id, label, theme).size(ControlSize::Small).on_click({
                let handle = handle.clone();
                move || {
                    handle.handle_command(command.clone());
                }
            })
        };
        div()
            .test_id("definition-card")
            .flex()
            .col()
            .gap(rem(4.0))
            .p(rem(8.0))
            .min_w(0.0)
            .bg(theme.paint(|c| c.elevated))
            .border(rem(1.0))
            .border_color(theme.paint(|c| c.border))
            .on_hover({
                let handle = handle.clone();
                move |hovered| handle.set_definition_hovered(false, hovered)
            })
            .child(text(card.name).typography(Typography::BodyStrong).color(theme.paint(|c| c.text)))
            .child(body)
            .child(
                div()
                    .flex()
                    .row()
                    .gap(rem(4.0))
                    .children(can_jump.then(|| button("definition-jump", "Go to definition", C::GoToDefinition)))
                    .children(can_cycle.then(|| button("definition-next", "Next", C::NextDefinition)))
                    .child(button("definition-close", "Close", C::CloseDefinition)),
            )
            .into_any()
    })
}
