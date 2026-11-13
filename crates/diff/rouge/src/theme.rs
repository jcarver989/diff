use clankerdiff_ui_core::{Appearance, ReviewTheme, TerminalPalette, ThemeChoice, ThemePreference, ThemeSelection};
use rouge_core::{Div, Theme, View, appearance, component, view};
use rouge_reactive::{ReadSignal, Signal, batch, expect_context, provide_context, signal};
use rouge_widgets::{Material, PickerItem, SearchPickerEvent, SearchPickerHandle, backdrop, search_picker};
use std::{cell::RefCell, sync::Arc};

#[derive(Clone, Copy)]
pub struct ActiveReviewTheme {
    review: Signal<ReviewTheme>,
    ui: Theme,
}

impl ActiveReviewTheme {
    #[must_use]
    pub fn install(review: ReviewTheme) -> Self {
        let ui = Theme::provide(review.ui);
        let theme = Self { review: signal(review), ui };
        provide_context(theme);
        theme
    }

    #[must_use]
    pub fn current() -> Self {
        expect_context()
    }

    pub fn apply(self, preference: &ThemePreference) {
        self.set(preference.theme(appearance().get()));
    }

    pub fn set(self, review: ReviewTheme) {
        if self.review.with_untracked(|old| old.revision() == review.revision()) {
            return;
        }
        batch(|| {
            self.ui.set(review.ui);
            self.review.set(review);
        });
    }

    #[must_use]
    pub fn review(self) -> ReviewTheme {
        self.review.get()
    }

    #[must_use]
    pub fn review_signal(self) -> ReadSignal<ReviewTheme> {
        self.review.read_only()
    }

    #[must_use]
    pub const fn ui(self) -> Theme {
        self.ui
    }

    pub fn terminal(self) -> TerminalPalette {
        self.review.with(ReviewTheme::terminal_palette)
    }
}

#[derive(Debug, Clone)]
pub enum ThemePickerEvent {
    Preview(ReviewTheme),
    Commit(ReviewTheme),
    Cancel(ReviewTheme),
}

pub fn theme_picker(
    current: &ReviewTheme,
    themes: Arc<[ThemeChoice]>,
    emit: impl Fn(ThemePickerEvent) + 'static,
) -> SearchPickerHandle {
    let selection = ThemeSelection::new(current, themes);
    let items = selection
        .iter()
        .flat_map(ThemeSelection::themes)
        .map(|choice| {
            PickerItem::new(choice.theme.id().to_string(), choice.name.clone())
                .detail(match choice.theme.appearance() {
                    Appearance::Light => "Light",
                    Appearance::Dark => "Dark",
                })
                .checked(choice.theme.id() == current.id())
        })
        .collect();
    let index = selection.as_ref().map_or(0, ThemeSelection::selected);
    let selection = RefCell::new(selection);
    SearchPickerHandle::new(items, move |event| {
        let event = match event {
            SearchPickerEvent::Highlighted(index) => {
                selection.borrow_mut().as_mut().and_then(|s| s.select(index)).map(ThemePickerEvent::Preview)
            }
            SearchPickerEvent::Chose(index) => selection.borrow_mut().take().map(|mut s| {
                s.select(index);
                ThemePickerEvent::Commit(s.commit())
            }),
            SearchPickerEvent::Dismissed => selection.borrow_mut().take().map(|s| ThemePickerEvent::Cancel(s.cancel())),
            SearchPickerEvent::Query(_) => None,
        };
        if let Some(event) = event {
            emit(event);
        }
    })
    .highlighted(index)
}

#[must_use]
#[component]
pub fn theme_picker_view(
    #[prop(into)] id: String,
    handle: SearchPickerHandle,
    #[prop(default)] material: Material,
) -> Div {
    view! {
        <search_picker id handle material searchable="Search themes…" empty="No matching themes"
                       footer="↑↓ Preview    ↵ Apply    Esc Cancel"/>
    }
}

#[must_use]
#[component]
pub fn theme_picker_overlay(#[prop(into)] id: String, handle: SearchPickerHandle) -> impl View {
    view! {
        <backdrop id="theme-picker-backdrop" on_dismiss={move || handle.dismiss()}>
            <theme_picker_view id handle/>
        </backdrop>
    }
}
