use crate::{ReviewHandle, style};
use clankerdiff_core::{DiffReviewCommand as C, SourceLocation};
use clankerdiff_symbols::SymbolReference;
use rouge_core::{Modifiers, Styled, Theme, View, div, dynamic, scroll, text, view};
use rouge_reactive::{Signal, memo};
use rouge_widgets::{ControlSize, UiStyles, button};
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

pub(crate) fn at_line(handle: ReviewHandle, location: SourceLocation) -> impl View {
    let theme = Theme::current();
    let card = memo(move || {
        handle.definition_revision().get();
        handle.with(|s| s.definition_card().filter(|card| card.origin == location).cloned())
    });
    dynamic(move || {
        let card = card.get()?;
        let review_theme = handle.theme().get();
        let (body, can_jump, can_cycle) = handle.with_syntax(|_, syntax| {
            let Some(index) = syntax.ready_symbols() else {
                return (div(), false, false);
            };
            let content = card.content(&index);
            let code = content.code.filter(|_| content.message.is_none());
            let body = view! {
                <div flex col gap=4.0>
                    {content.message.map(|message| view! { <text body secondary>{message}</text> })}
                    {code.map(|code| view! {
                        <div test_id="definition-card-signature" flex col overflow_hidden>
                            {..code.map_lines(|line, spans| {
                                style::code(line, spans, &review_theme.syntax).color(theme.text)
                            })}
                        </div>
                    })}
                    {content.docs.map(|docs| view! {
                        <scroll test_id="definition-card-docs" max_h=168.0>
                            <text body primary>{docs.to_owned()}</text>
                        </scroll>
                    })}
                    {content.place.map(|place| view! { <text caption muted>{place}</text> })}
                    <text caption muted>{content.uses}</text>
                </div>
            };
            (body, content.can_jump, content.can_cycle)
        });
        let action = |id: &str, label: &str, command: C| {
            view! {
                <button id label size=ControlSize::Small
                        on_click={move || {
                            handle.handle_command(command.clone());
                        }}/>
            }
        };
        Some(view! {
            <div test_id="definition-card" flex col gap=4.0 p=8.0 min_w=0.0 bg=theme.elevated border=1.0
                 border_color=theme.border on_hover={move |hovered| handle.set_definition_hovered(false, hovered)}>
                <text body_strong primary>{card.name}</text>
                {body}
                <div flex row gap=4.0>
                    {can_jump.then(|| action("definition-jump", "Go to definition", C::GoToDefinition))}
                    {can_cycle.then(|| action("definition-next", "Next", C::NextDefinition))}
                    {action("definition-close", "Close", C::CloseDefinition)}
                </div>
            </div>
        })
    })
}
