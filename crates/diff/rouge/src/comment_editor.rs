use crate::{
    ReviewHandle,
    ui::{
        components::{ButtonVariant, ControlSize, SurfaceExt, button},
        fonts::UI_FONT,
        theme::UiTheme,
        tokens::RADIUS_CONTROL,
        typography::UiStyles,
    },
};
use rouge_core::{Cursor, FocusHandle, Styled, View, div, dynamic, editable, focus_handle, px, text, view};
use rouge_reactive::{Callback, Memo, effect, memo, provide_context, signal, use_context};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, PartialEq)]
struct DraftFocus {
    handle: FocusHandle,
    opened: Rc<Cell<bool>>,
}

pub(crate) fn install(handle: &ReviewHandle) {
    let draft = handle.draft_placement();
    provide_context(memo(move || {
        draft.with(|draft| {
            draft.as_ref().map(|_| DraftFocus { handle: focus_handle(), opened: Rc::new(Cell::new(false)) })
        })
    }));
}

pub(crate) fn composer(
    title: String,
    body: String,
    editing: bool,
    on_change: impl Fn(String) + 'static,
    on_submit: impl Fn(&str) + 'static,
    on_cancel: impl Fn() + 'static,
) -> impl View {
    let theme = UiTheme::current();
    let value = signal(body);
    let submit: Callback<(String,)> = (move |body: String| on_submit(&body)).into();
    effect(move || on_change(value.get()));
    let draft = use_context::<Memo<Option<DraftFocus>>>().and_then(|focus| focus.get_untracked());
    let autofocus = draft.as_ref().is_none_or(|draft| !draft.opened.replace(true));
    let focus = draft.map_or_else(focus_handle, |draft| draft.handle);
    let empty = memo(move || value.with(|body| body.trim().is_empty()));
    view! {
        <div test_id="comment-composer" flex col gap=12.0 p=12.0 surface bg=theme.surface_selected>
            <text body_strong primary>{title}</text>
            <div min_h=96.0 w_full p=12.0 rounded=RADIUS_CONTROL border=px(1.0)
                 border_color=theme.paint(move |c| if focus.is_focused() { c.accent } else { c.border })
                 bg=theme.canvas cursor=Cursor::Text>
                <editable value test_id="comment-editor" when:autofocus=autofocus track_focus=focus multiline
                          min_rows=3 max_rows=12 font=UI_FONT text_size=13.0 line_height=1.5 color=theme.text w_full
                          submit_on_enter=true on_submit={move |body| submit.call(body.to_owned())}/>
            </div>
            <div w_full flex row justify_end items_center gap=8.0>
                <button id="cancel-comment" label="Cancel" variant=ButtonVariant::Ghost size=ControlSize::Small
                        on_click=on_cancel/>
                <dynamic>
                    <button id="save-comment" label={if editing { "Save" } else { "Comment" }}
                            size=ControlSize::Small variant=ButtonVariant::Primary disabled={empty.get()}
                            on_click={move || submit.call(value.get_untracked())}/>
                </dynamic>
            </div>
        </div>
    }
}
