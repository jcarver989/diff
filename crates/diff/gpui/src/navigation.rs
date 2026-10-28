use crate::{
    DiffViewer,
    comment_editor::init_input,
    definition_card::Hold,
    style,
    ui::{
        menu::{MenuItem, with_menu},
        prelude::{Button, ButtonVariant, ControlSize, IconButton, IconName, Typography, TypographyExt, rem},
        tokens::{RADIUS_CONTROL, RADIUS_INNER, ROW_HEIGHT},
    },
    viewer::{
        CancelNameHints, CloseDefinition, GoToDefinition, NavigateBack, NavigateForward, NextDefinition, PickName,
    },
};
use clankerdiff_core::{DiffReviewCommand, SourceLocation, TargetLocation};
use clankerdiff_symbols::{Mark, SymbolReference};
use gpui::{
    AnyElement, Context, Div, HighlightStyle, InteractiveText, KeyDownEvent, Modifiers, ModifiersChangedEvent,
    MouseButton, SharedString, StyledText, UnderlineStyle, WeakEntity, Window, combine_highlights, div, prelude::*, px,
};
use std::{ops::Range, rc::Rc};

pub(crate) const HISTORY_BUTTONS: usize = 2;

impl DiffViewer {
    pub(crate) fn navigate(&mut self, command: DiffReviewCommand, cx: &mut Context<Self>) -> bool {
        if self.session_mut().apply_navigation(command) != Some(true) {
            return false;
        }
        self.update_names(cx, |names, session, _, enabled| names.navigated(session, enabled));
        self.reveal_navigation(cx);
        true
    }

    pub(crate) fn reveal_navigation(&mut self, cx: &mut Context<Self>) {
        self.sync_diff_list();
        self.reveal_selected_row();
        cx.notify();
    }

    pub(crate) fn render_reference_status(&self, message: String) -> Div {
        div()
            .debug_selector(|| "reference-status".to_owned())
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .typography(Typography::Body)
            .text_color(self.ui_theme().colors.text_secondary)
            .child(message)
    }

    pub(crate) fn name_hint_row(&self) -> Option<usize> {
        self.presentation().row_showing_source(&self.names.hints()?.origin)
    }

    pub(crate) fn pick_name(&mut self, _: &PickName, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::ShowDefinition, window, cx);
    }

    pub(crate) fn show_definition_at_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(origin) = self.session().selected_source_line() else {
            return false;
        };
        let opened =
            self.update_names(cx, |names, session, index, enabled| names.open_at(origin, session, index, enabled));
        self.hold_card(Hold::Pinned);
        opened
    }

    pub(crate) fn cancel_name_hints(&mut self, _: &CancelNameHints, _: &mut Window, cx: &mut Context<Self>) {
        self.close_names(cx);
    }

    pub(crate) fn name_hint_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.names.hints().is_none() || event.keystroke.modifiers.modified() {
            return;
        }
        let Ok(key) = event.keystroke.key.parse() else {
            return;
        };
        if self.update_names(cx, |names, session, _, enabled| names.pick_hint(key, session, enabled)) {
            cx.stop_propagation();
            self.hold_card(Hold::Pinned);
        }
    }

    pub(crate) fn close_definition(&mut self, _: &CloseDefinition, _: &mut Window, cx: &mut Context<Self>) {
        self.close_names(cx);
    }

    pub(crate) fn go_to_definition(&mut self, _: &GoToDefinition, _: &mut Window, cx: &mut Context<Self>) {
        self.jump_to_card_choice(cx);
    }

    pub(crate) fn next_definition(&mut self, _: &NextDefinition, _: &mut Window, cx: &mut Context<Self>) {
        self.cycle_card_choice(cx);
    }

    pub(crate) fn navigate_back(&mut self, _: &NavigateBack, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::NavigateBack, window, cx);
    }

    pub(crate) fn navigate_forward(&mut self, _: &NavigateForward, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_command(DiffReviewCommand::NavigateForward, window, cx);
    }

    pub(crate) fn modifiers_changed(&mut self, event: &ModifiersChangedEvent, _: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.modifiers;
        let visible = modifiers.secondary() && !modifiers.shift && !modifiers.alt;
        if self.links_visible != visible {
            self.links_visible = visible;
            cx.notify();
        }
    }

    pub(crate) fn render_line_text(
        &mut self,
        location: SourceLocation,
        text: SharedString,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let marked = self.names.pick().is_some_and(|pick| pick.origin() == &location);
        if !self.links_visible && !marked {
            return StyledText::new(text).with_highlights(highlights).into_any_element();
        }
        let references = self.line_references(&location, &text);
        let names = self.names.marks(&location, &references);
        let card_here = self.card_at(&location);
        let styled = StyledText::new(text.clone())
            .with_highlights(combine_highlights(highlights, self.mark_highlights(&references, &names)));
        let anchor = card_here.then(|| self.render_card_anchor(&text, cx)).flatten();
        let key = line_key(&location);
        let interactive = line_links(&key, location.clone(), styled, references, cx);
        div()
            .id(SharedString::from(format!("line-text:{key}")))
            .relative()
            .on_hover(cx.listener(move |viewer, hovered: &bool, _, cx| {
                if !*hovered && viewer.card_at(&location) {
                    viewer.card_token_hovered(false, cx);
                }
            }))
            .child(interactive)
            .children(anchor)
            .into_any_element()
    }

    fn mark_highlights(
        &self,
        references: &[SymbolReference],
        names: &[(Range<usize>, Mark)],
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let theme = self.theme();
        let accent = style::color(theme.diff.accent);
        let (hinted, carded) = (style::color(theme.hinted_name()), style::color(theme.carded_name()));
        let underline =
            self.links_visible.then_some(UnderlineStyle { color: Some(accent), thickness: px(1.0), wavy: false });
        references
            .iter()
            .filter_map(|reference| {
                let background =
                    names.iter().find(|(range, _)| range == &reference.span.columns).map(|(_, mark)| match mark {
                        Mark::Hinted => hinted,
                        Mark::Card => carded,
                    });
                (background.is_some() || underline.is_some()).then(|| {
                    let style = HighlightStyle { background_color: background, underline, ..HighlightStyle::default() };
                    (reference.span.columns.clone(), style)
                })
            })
            .collect()
    }

    fn line_references(&mut self, location: &SourceLocation, text: &str) -> Rc<Vec<SymbolReference>> {
        Rc::new(
            self.symbols()
                .references_at(location)
                .iter()
                .filter(|reference| text.get(reference.span.columns.clone()).is_some())
                .cloned()
                .collect(),
        )
    }

    fn card_at(&self, location: &SourceLocation) -> bool {
        self.names.card().is_some_and(|card| &card.origin == location)
    }

    fn hover_line(
        &mut self,
        location: &SourceLocation,
        references: &[SymbolReference],
        index: Option<usize>,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) {
        let hit = index.and_then(|index| references.iter().find(|reference| reference.span.columns.contains(&index)));
        match hit {
            Some(reference) if self.names.card().is_some_and(|card| card.shows(location, &reference.span.columns)) => {
                self.card_token_hovered(true, cx);
            }
            Some(reference) if modifiers.secondary() => {
                self.show_definition(location.clone(), reference, Hold::Hover, cx);
            }
            _ if self.card_at(location) => {
                self.card_token_hovered(false, cx);
            }
            _ => {}
        }
    }

    pub(crate) fn render_name_hints(&self) -> Option<AnyElement> {
        let hints = self.names.hints()?;
        let theme = self.ui_theme();
        let colors = theme.colors;
        let palette = &self.theme().diff;
        let entries = hints.keyed().map(|(key, SymbolReference { name, .. })| {
            div()
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .px_1()
                        .rounded(rem(RADIUS_INNER))
                        .bg(colors.warning)
                        .text_color(colors.canvas)
                        .typography(Typography::Code)
                        .child(key.to_string()),
                )
                .child(div().typography(Typography::Code).text_color(colors.text).child(name.clone()))
        });
        Some(
            div()
                .debug_selector(|| "name-hints".to_owned())
                .w_full()
                .px_3()
                .pb_2()
                .bg(style::color(palette.background))
                .child(
                    div()
                        .h(rem(ROW_HEIGHT))
                        .px_3()
                        .flex()
                        .items_center()
                        .gap_4()
                        .rounded(rem(RADIUS_CONTROL))
                        .border_1()
                        .border_color(colors.border)
                        .bg(colors.surface_selected)
                        .typography(Typography::Caption)
                        .child(div().text_color(colors.text_secondary).child("Show definition"))
                        .children(entries)
                        .child(div().flex_1())
                        .child(div().text_color(colors.text_secondary).child("Esc cancel")),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn render_symbol_path(&mut self, file_index: usize, cx: &mut Context<Self>) -> Option<Div> {
        let location = self.top_source_location(file_index)?;
        let index = self.symbols();
        let names = index.path_names(&location);
        if names.is_empty() {
            return None;
        }
        init_input(cx);
        let file_path = self.presentation().document().files.get(file_index)?.path.clone();
        let items: Vec<(String, SourceLocation)> = index
            .file_definitions(&file_path, location.side)
            .into_iter()
            .map(|definition| (definition.label(), definition.location.clone()))
            .collect();
        let crumbs: Vec<Div> = names
            .into_iter()
            .enumerate()
            .map(|(depth, name)| self.render_symbol_crumb(depth, name, items.clone(), cx))
            .collect();
        Some(
            div()
                .debug_selector(|| "symbol-path".to_owned())
                .flex_none()
                .flex()
                .items_center()
                .gap_0p5()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(crumbs),
        )
    }

    fn render_symbol_crumb(
        &self,
        depth: usize,
        name: String,
        items: Vec<(String, SourceLocation)>,
        cx: &Context<Self>,
    ) -> Div {
        let theme = self.ui_theme();
        let viewer = cx.entity().downgrade();
        let selector = format!("symbol-path-{depth}");
        let button = Button::new(SharedString::from(selector.clone()), name, theme)
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Small);
        let menu = with_menu(&selector, button, MouseButton::Left, move |_, _| symbol_menu(items, &viewer), &theme);
        div().flex().items_center().gap_0p5().child(div().text_color(theme.colors.text_muted).child("›")).child(menu)
    }

    pub(crate) fn render_history_buttons(&self, cx: &Context<Self>) -> [IconButton; HISTORY_BUTTONS] {
        let label = |direction: &str, key: char, target: Option<&TargetLocation>| -> SharedString {
            let Some(target) = target else {
                return format!("{direction} (Ctrl-{key})").into();
            };
            format!("{direction} to {}:{} (Ctrl-{key})", target.path.display_path().file_name(), target.line_number)
                .into()
        };
        [
            self.history_button(
                "navigate-back",
                IconName::ArrowLeft,
                label("Back", 'O', self.session().back_location()),
                DiffReviewCommand::NavigateBack,
                cx,
            ),
            self.history_button(
                "navigate-forward",
                IconName::ArrowRight,
                label("Forward", 'I', self.session().forward_location()),
                DiffReviewCommand::NavigateForward,
                cx,
            ),
        ]
    }

    fn history_button(
        &self,
        id: &'static str,
        icon: IconName,
        label: SharedString,
        command: DiffReviewCommand,
        cx: &Context<Self>,
    ) -> IconButton {
        let enabled = self.command_enabled(&command);
        IconButton::new(id, icon, label, self.ui_theme()).disabled(!enabled).on_click(cx.listener(
            move |viewer, _, window, cx| {
                viewer.handle_command(command.clone(), window, cx);
            },
        ))
    }

    fn top_source_location(&self, file_index: usize) -> Option<SourceLocation> {
        let top = self.session().visible_range().start + self.diff_list_state.logical_scroll_top().item_ix + 1;
        self.presentation().source_location_from(file_index, top, self.session().selected_side())
    }
}

fn line_links(
    key: &str,
    location: SourceLocation,
    styled: StyledText,
    references: Rc<Vec<SymbolReference>>,
    cx: &Context<DiffViewer>,
) -> InteractiveText {
    let viewer = cx.entity().downgrade();
    let ranges = references.iter().map(|reference| reference.span.columns.clone()).collect();
    InteractiveText::new(SharedString::from(format!("links:{key}")), styled)
        .on_click(ranges, {
            let (viewer, references, location) = (viewer.clone(), Rc::clone(&references), location.clone());
            move |index, window, cx| {
                let Some(reference) = references.get(index).filter(|_| window.modifiers().secondary()) else {
                    return;
                };
                let _ = viewer.update(cx, |viewer, cx| viewer.follow_definition(location.clone(), reference, cx));
            }
        })
        .on_hover(move |index, event, _, cx| {
            let _ = viewer.update(cx, |viewer, cx| {
                viewer.hover_line(&location, &references, index, event.modifiers, cx);
            });
        })
}

fn symbol_menu(items: Vec<(String, SourceLocation)>, viewer: &WeakEntity<DiffViewer>) -> Vec<MenuItem> {
    items
        .into_iter()
        .enumerate()
        .map(|(index, (label, location))| {
            let viewer = viewer.clone();
            MenuItem::new(format!("symbol-path-item-{index}"), label, move |window, cx| {
                let _ = viewer.update(cx, |viewer, cx| {
                    viewer.handle_command(DiffReviewCommand::JumpTo(location.clone().into()), window, cx);
                });
            })
        })
        .collect()
}

pub(crate) fn line_key(location: &SourceLocation) -> String {
    format!("{}:{}:{}", location.path, location.side.as_str(), location.line_number)
}
