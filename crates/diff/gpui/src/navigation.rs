use crate::{
    DiffViewer,
    comment_editor::init_input,
    definition_card::Hold,
    style,
    ui::{
        menu::{MenuItem, with_menu},
        prelude::{Button, ButtonVariant, ControlSize, IconButton, IconName, Typography, TypographyExt, rem},
        tokens::{self, RADIUS_CONTROL, RADIUS_INNER, ROW_HEIGHT},
    },
    viewer::{
        CancelNameHints, CloseDefinition, GoToDefinition, NavigateBack, NavigateForward, NextDefinition, PickName,
    },
};
use clankerdiff_core::{DiffReviewCommand, DiffSide, SourceLocation};
use clankerdiff_symbols::{Definition, Lookup, SymbolReference};
use gpui::{
    AnyElement, Context, Div, HighlightStyle, Hsla, InteractiveText, KeyDownEvent, Modifiers, ModifiersChangedEvent,
    MouseButton, SharedString, StyledText, UnderlineStyle, WeakEntity, Window, combine_highlights, div, prelude::*, px,
};
use std::{ops::Range, rc::Rc};

impl DiffViewer {
    pub(crate) fn navigate(&mut self, command: DiffReviewCommand, cx: &mut Context<Self>) -> bool {
        if self.session_mut().apply_navigation(command) != Some(true) {
            return false;
        }
        self.name_hints = None;
        self.sync_diff_list();
        self.reveal_selected_row();
        cx.notify();
        true
    }

    pub(crate) fn name_hint_row(&self) -> Option<usize> {
        self.presentation().row_showing_source(&self.name_hints.as_ref()?.origin)
    }

    pub(crate) fn pick_name(&mut self, _: &PickName, _: &mut Window, cx: &mut Context<Self>) {
        self.clear_name_hints(cx);
        self.close_card(cx);
        self.show_definition_at_selection(cx);
    }

    pub(crate) fn show_definition_at_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(origin) = self.session().selected_source_line() else {
            return false;
        };
        match Lookup::at(&self.symbols(), origin) {
            Some(Lookup::Card(card)) => self.show_card(card, Hold::Pinned, cx),
            Some(Lookup::Hints(hints)) => {
                self.name_hints = Some(hints);
                self.remeasure_row(self.name_hint_row());
                cx.notify();
            }
            None => return false,
        }
        true
    }

    pub(crate) fn cancel_name_hints(&mut self, _: &CancelNameHints, _: &mut Window, cx: &mut Context<Self>) {
        self.clear_name_hints(cx);
    }

    pub(crate) fn clear_name_hints(&mut self, cx: &mut Context<Self>) {
        let row = self.name_hint_row();
        if self.name_hints.take().is_some() {
            self.remeasure_row(row);
            cx.notify();
        }
    }

    pub(crate) fn name_hint_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(hints) = &self.name_hints else {
            return;
        };
        if event.keystroke.modifiers.modified() {
            return;
        }
        let Some(card) = event.keystroke.key.parse().ok().and_then(|key| hints.pick(key)) else {
            return;
        };
        cx.stop_propagation();
        self.clear_name_hints(cx);
        self.show_card(card, Hold::Pinned, cx);
    }

    pub(crate) fn close_definition(&mut self, _: &CloseDefinition, _: &mut Window, cx: &mut Context<Self>) {
        self.close_card(cx);
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
        let marks = self.line_marks(&location);
        if marks.is_empty() {
            return StyledText::new(text).with_highlights(highlights).into_any_element();
        }
        let references = self.line_references(&location, &text);
        let styled = StyledText::new(text.clone())
            .with_highlights(combine_highlights(highlights, marks.highlights(&references)));
        let anchor = marks.card.is_some().then(|| self.render_card_anchor(&text, cx)).flatten();
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

    fn line_marks(&self, location: &SourceLocation) -> LineMarks {
        let hinted = self.name_hints.as_ref().filter(|hints| &hints.origin == location);
        LineMarks {
            card: self
                .card
                .as_ref()
                .filter(|hover| &hover.card.origin == location)
                .map(|hover| hover.card.columns.clone()),
            hinted: hinted
                .map(|hints| hints.names.iter().map(|reference| reference.name.clone()).collect())
                .unwrap_or_default(),
            links: self.links_visible,
            accent: style::color(self.theme().diff.accent),
            warning: self.ui_theme().colors.warning,
        }
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
        self.card.as_ref().is_some_and(|hover| &hover.card.origin == location)
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
            Some(reference) if self.card.as_ref().is_some_and(|card| card.shows(location, &reference.span.columns)) => {
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
        let hints = self.name_hints.as_ref()?;
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
        let path = self.symbol_path(file_index);
        let navigation = self.render_history_buttons(cx);
        if path.is_empty() && navigation.is_none() {
            return None;
        }
        init_input(cx);
        let file_path = self.document().files.get(file_index)?.path.clone();
        let side = path.last().map_or(DiffSide::New, |definition| definition.location.side);
        let items: Vec<(String, SourceLocation)> = self
            .symbols()
            .file_definitions(&file_path, side)
            .into_iter()
            .map(|definition| (definition.label(), definition.location.clone()))
            .collect();
        let mut names: Vec<String> = path.into_iter().map(|definition| definition.name).collect();
        names.dedup();
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
                .children(navigation)
                .children(crumbs),
        )
    }

    fn symbol_path(&mut self, file_index: usize) -> Vec<Definition> {
        let Some(location) = self.top_source_location(file_index) else {
            return Vec::new();
        };
        self.symbols().path_at(&location).into_iter().cloned().collect()
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

    fn render_history_buttons(&self, cx: &Context<Self>) -> Option<Div> {
        let back = self.session().back_location().is_some();
        let forward = self.session().forward_location().is_some();
        if !back && !forward {
            return None;
        }
        Some(
            div()
                .flex()
                .items_center()
                .gap_0p5()
                .child(self.history_button(
                    "navigate-back",
                    IconName::ChevronLeft,
                    "Back (Ctrl-O)",
                    back,
                    DiffReviewCommand::NavigateBack,
                    cx,
                ))
                .child(self.history_button(
                    "navigate-forward",
                    IconName::ChevronRight,
                    "Forward (Ctrl-I)",
                    forward,
                    DiffReviewCommand::NavigateForward,
                    cx,
                )),
        )
    }

    fn history_button(
        &self,
        id: &'static str,
        icon: IconName,
        label: &'static str,
        enabled: bool,
        command: DiffReviewCommand,
        cx: &Context<Self>,
    ) -> IconButton {
        IconButton::new(id, icon, label, self.ui_theme())
            .icon_size(tokens::ICON_SIZE_SMALL)
            .disabled(!enabled)
            .on_click(cx.listener(move |viewer, _, window, cx| {
                viewer.handle_command(command.clone(), window, cx);
            }))
    }

    fn top_source_location(&self, file_index: usize) -> Option<SourceLocation> {
        let range = self.presentation().file_range(file_index)?;
        let top = self.session().visible_range().start + self.diff_list_state.logical_scroll_top().item_ix + 1;
        (top.max(range.start)..range.end).find_map(|index| {
            let row = self.presentation().row(index)?;
            let cell = row.preferred_cell(self.session().selected_side()).or_else(|| row.primary_cell())?;
            self.presentation().source_location(row, cell)
        })
    }
}

struct LineMarks {
    card: Option<Range<usize>>,
    hinted: Vec<String>,
    links: bool,
    accent: Hsla,
    warning: Hsla,
}

impl LineMarks {
    fn is_empty(&self) -> bool {
        !self.links && self.card.is_none() && self.hinted.is_empty()
    }

    fn highlights(&self, references: &[SymbolReference]) -> Vec<(Range<usize>, HighlightStyle)> {
        references
            .iter()
            .filter_map(|reference| Some((reference.span.columns.clone(), self.style(reference)?)))
            .collect()
    }

    fn style(&self, reference: &SymbolReference) -> Option<HighlightStyle> {
        let background = if self.hinted.contains(&reference.name) {
            Some(self.warning.opacity(0.28))
        } else if self.card.as_ref() == Some(&reference.span.columns) {
            Some(self.accent.opacity(0.22))
        } else {
            None
        };
        let underline =
            self.links.then_some(UnderlineStyle { color: Some(self.accent), thickness: px(1.0), wavy: false });
        (background.is_some() || underline.is_some()).then(|| HighlightStyle {
            background_color: background,
            underline,
            ..HighlightStyle::default()
        })
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
                    viewer.handle_command(DiffReviewCommand::JumpTo(location.clone()), window, cx);
                });
            })
        })
        .collect()
}

pub(crate) fn line_key(location: &SourceLocation) -> String {
    format!("{}:{}:{}", location.path, location.side.as_str(), location.line_number)
}
