use clankerdiff_rouge_ui::{fonts::bundled_fonts, prelude::*};
use clankerdiff_ui_core::{ReviewTheme, ThemeChoice};
use rouge::{App, AppError, WindowOptions};
use rouge_core::{Styled, View, component, div, show, text, view};
use rouge_reactive::{Signal, signal};

fn main() -> Result<(), AppError> {
    App::new()
        .fonts(bundled_fonts())
        .window(WindowOptions::default().title("Diff shared component gallery").size(920.0, 680.0), gallery)
        .run()
}

fn gallery() -> impl View {
    let active = ActiveTheme::install(ReviewTheme::default());
    let size = InterfaceSize::install(16.0);
    let _indicators = IndicatorLayer::install();
    let theme = active.ui();
    let themes_open = signal(false);
    let dialog_open = signal(false);
    let status = signal(String::from("Choose a control"));
    let edit = signal(String::from("Editable text"));
    let width = signal(16.0);
    let count = view! { <badge id="count" label="2"/> };
    size.bind(view! {
        <div size_full flex col p=16.0 gap=12.0 bg=theme.canvas>
            <text heading primary>"Shared Rouge components"</text>
            <controls size themes_open dialog_open status/>
            <div flex gap=8.0 flex_1>
                <resizable_panel id="sidebar" side=PanelSide::Left width_in_rem=width min_width=10.0 max_width=28.0>
                    <surface size_full>
                        <section_header id="files" title="Files" expanded=true badge=count.into_any()/>
                        <tree_row id="src" label="src" expanded=true icon=IconName::FolderOpen/>
                        <tree_row id="lib" label="lib.rs" depth=1 icon=IconName::File state=SelectionState::Selected
                                  on_click={move || status.set("Selected lib.rs".into())}/>
                    </surface>
                </resizable_panel>
                <surface padded raised flex_1 min_w=0.0 gap=10.0>
                    <segmented_control id="view">
                        <segment id="unified" label="Unified" selected/>
                        <segment id="split" label="Split" icon=IconName::Columns2/>
                        <segment id="disabled-segment" label="Disabled" disabled/>
                    </segmented_control>
                    <notification message="Review theme tokens and bundled typefaces" tone=NoticeTone::Info/>
                    <inline_edit value=edit on_commit={move |value: String| status.set(value)}/>
                    <empty_state icon=IconName::FileDiff title="No changes" detail="The working tree is clean"/>
                    <action_bar>
                        <text caption primary>{move || status.get()}</text>
                        {Waveform::new("activity", theme.accent)}
                    </action_bar>
                </surface>
            </div>
            <show when={move || themes_open.get()}>{
                let handle = theme_picker(&active.review(), ThemeChoice::catalog(), move |event| match event {
                    ThemePickerEvent::Preview(review) => active.set(review),
                    ThemePickerEvent::Commit(review) | ThemePickerEvent::Cancel(review) => {
                        active.set(review);
                        themes_open.set(false);
                    }
                });
                view! { <theme_picker_overlay id="themes" handle/> }
            }</show>
            <show when={move || dialog_open.get()}>
                <confirm_dialog id="confirm" title="Confirm action"
                                message="This dialog uses the shared modal and action bar." confirm="Confirm"
                                alternate="Alternate"
                                on_event={move |event: ConfirmDialogEvent| {
                                    status.set(format!("{event:?}"));
                                    dialog_open.set(false);
                                }}/>
            </show>
        </div>
    })
}

#[component]
fn controls(
    size: InterfaceSize,
    themes_open: Signal<bool>,
    dialog_open: Signal<bool>,
    status: Signal<String>,
) -> impl View {
    let items = [
        MenuItem::new("Larger", move || size.set(size.get() + 1.0)).icon(IconName::Plus),
        MenuItem::new("Smaller", move || size.set(size.get() - 1.0)).icon(IconName::Minus),
        MenuItem::separator(),
        MenuItem::new("Reset", move || size.set(16.0)),
    ];
    view! {
        <div flex gap=8.0>
            <button id="primary" label="Theme picker" variant=ButtonVariant::Primary keyboard_focusable
                    on_click={move || themes_open.set(true)}/>
            <button id="secondary" label="Confirm dialog" variant=ButtonVariant::Secondary keyboard_focusable
                    on_click={move || dialog_open.set(true)}/>
            <button id="destructive" label="Destructive" variant=ButtonVariant::Destructive
                    on_click={move || status.set("Destructive selected".into())}/>
            <button id="disabled" label="Disabled" disabled/>
            <with_menu items><button id="menu" label="Interface size"/></with_menu>
        </div>
    }
}
