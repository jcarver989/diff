use crate::{ReviewHandle, style};
use clankerdiff_core::{DiffReviewCommand as C, SourceLocation};
use clankerdiff_markdown::{MarkdownDocument, MarkdownOptions};
use clankerdiff_symbols::{DefinitionCard, SymbolReference};
use rouge_core::{Cursor, Div, Modifiers, Rect, Role, Styled, TextHit, View, div, dynamic, scroll, text, view};
use rouge_reactive::{Signal, effect, memo, signal};
use rouge_widgets::{ControlSize, UiStyles, button, hover_card, markdown, tokens::POPOVER_WIDTH};
use std::ops::Range;

const CARD_MIN_WIDTH: f32 = 320.0;
const DOCS_MAX_HEIGHT: f32 = 240.0;

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

pub(crate) fn click(handle: &ReviewHandle, row: usize, origin: &SourceLocation, hit: TextHit, modifiers: Modifiers) {
    if links(modifiers)
        && let Some(reference) = reference(handle, origin, hit.index)
    {
        handle.select_cell(row, origin.side);
        handle.hover_definition(origin.clone(), &reference);
        handle.handle_command(C::GoToDefinition);
    }
}

fn reference(handle: &ReviewHandle, origin: &SourceLocation, byte: usize) -> Option<SymbolReference> {
    handle.with_syntax(|_, syntax| {
        syntax.ready_symbols()?.references_at(origin).iter().find(|r| r.span.columns.contains(&byte)).cloned()
    })
}

pub(crate) fn popover(handle: ReviewHandle, anchor: Signal<Rect>) -> impl View {
    let card = memo(move || {
        handle.definition_revision().get();
        handle.with(|s| s.definition_card().cloned())
    });
    let kept = signal(None::<DefinitionCard>);
    let placed = signal(Rect::ZERO);
    effect(move || {
        if let Some(card) = card.get() {
            kept.set(Some(card));
        }
    });
    effect(move || {
        let anchor = anchor.get();
        if anchor != Rect::ZERO {
            placed.set(anchor);
        }
    });
    hover_card(
        move || card.with(Option::is_some) && anchor.get() != Rect::ZERO,
        placed,
        move || {
            view! {
                <div test_id="definition-card" flex col min_w=CARD_MIN_WIDTH max_w=POPOVER_WIDTH overflow_hidden
                     cursor=Cursor::Default on_hover={move |hovered| handle.set_definition_hovered(false, hovered)}>
                    {dynamic(move || kept.get().map(|card| body(handle, &card)))}
                </div>
            }
        },
    )
}

fn body(handle: ReviewHandle, card: &DefinitionCard) -> Div {
    let theme = handle.theme();
    let review_theme = theme.get_untracked();
    let (head, docs, meta, can_jump, can_cycle) = handle.with_syntax(|_, syntax| {
        let Some(index) = syntax.ready_symbols() else {
            return (None, None, None, false, false);
        };
        let content = card.content(&index);
        let head = match (content.message, content.code) {
            (Some(message), _) => Some(view! { <text body secondary>{message}</text> }.into_any()),
            (None, Some(code)) => Some(
                view! {
                    <div test_id="definition-card-signature" flex col overflow_hidden>
                        {..code.map_lines(|line, spans| {
                            style::code(line, spans, &review_theme.syntax).color(Role::Text)
                        })}
                    </div>
                }
                .into_any(),
            ),
            (None, None) => None,
        };
        let docs = content.docs.map(|docs| {
            let document = MarkdownDocument::parse_with(docs, MarkdownOptions { intra_doc_links: true });
            view! {
                <scroll test_id="definition-card-docs" max_h=DOCS_MAX_HEIGHT>
                    {markdown(document)
                        .palette(move || theme.with(|t| t.markdown))
                        .syntax(move || theme.with(|t| t.syntax.clone()))
                        .compact()}
                </scroll>
            }
        });
        let meta = view! {
            <div flex row items_center gap=12.0>
                <div flex_1 min_w=0.0 overflow_hidden>
                    {content.place.map(|place| view! { <text caption muted no_wrap>{place}</text> })}
                </div>
                <text caption muted no_wrap flex_none>{content.uses}</text>
            </div>
        };
        (head, docs, Some(meta), content.can_jump, content.can_cycle)
    });
    let action = |id: &str, label: &str, command: C| {
        view! {
            <button test_id=id label control_size=ControlSize::Small
                    on_click={move || {
                        handle.handle_command(command.clone());
                    }}/>
        }
    };
    let divider = head.is_some().then(|| view! { <div border_t=1.0 border_color=Role::BorderSubtle/> });
    view! {
        <div flex col>
            <div flex col gap=4.0 px=12.0 py=8.0>
                <text body_strong primary>{card.name.clone()}</text>
                {head}
            </div>
            {divider}
            <div flex col gap=8.0 px=12.0 py=8.0>
                {docs}
                {meta}
                <div flex row gap=4.0>
                    {can_jump.then(|| action("definition-jump", "Go to definition", C::GoToDefinition))}
                    {can_cycle.then(|| action("definition-next", "Next", C::NextDefinition))}
                    {action("definition-close", "Close", C::CloseDefinition)}
                </div>
            </div>
        </div>
    }
}
