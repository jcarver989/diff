use clankerdiff_rouge_ui::{prelude::*, testing::component_test};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Color, Rect, Styled, div, show, view};
use rouge_reactive::{Runtime, signal};

#[test]
fn backdrop_suppresses_underlying_animation_without_hiding_its_own_indicators() {
    let runtime = Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, (shown, overlay, inside)) = component_test().size(440.0, 300.0).build_with(|| {
        let layer = IndicatorLayer::install();
        let _ = ActiveTheme::install(ReviewTheme::default());
        let overlay = signal(false);
        let inside = signal(false);
        let root = view! {
            <div size_full>
                {Waveform::new("base", Color::WHITE)}
                <show when={move || overlay.get()}>
                    <backdrop id="backdrop">
                        <show when={move || inside.get()}>{Waveform::new("inside", Color::WHITE)}</show>
                    </backdrop>
                </show>
            </div>
        };
        (root, (layer.shown(), overlay, inside))
    });
    let base = shown.get();
    assert_eq!(base.len(), 1);
    assert!(ui.is_animating());
    overlay.set(true);
    ui.advance(0.0);
    assert_eq!(shown.get(), Vec::new());
    assert!(!ui.is_animating());
    inside.set(true);
    ui.advance(0.0);
    assert_eq!(shown.get().len(), 1);
    assert_ne!(shown.get(), base);
    assert!(ui.is_animating());
    overlay.set(false);
    ui.advance(0.0);
    assert_eq!(shown.get(), base);
    assert!(ui.is_animating());
}

#[test]
fn indicator_bounds_follow_clipping_and_partial_overlap_hides_the_indicator() {
    let runtime = Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, (shown, x, cover, mounted)) = component_test().build_with(|| {
        let layer = IndicatorLayer::install();
        let x = signal(4.0);
        let cover = signal(false);
        let mounted = signal(true);
        let root = div()
            .size_full()
            .child(div().relative().w(20.0).h(20.0).overflow_hidden().child(show(
                move || mounted.get(),
                move || IndicatorMark::slot("slot", div).absolute().left(move || x.get()).top(0.0).w(16.0).h(16.0),
            )))
            .child(show(
                move || cover.get(),
                || IndicatorMark::occluder("cover").absolute().left(17.0).top(0.0).w(2.0).h(2.0),
            ));
        (root, (layer.shown(), x, cover, mounted))
    });
    assert_eq!(shown.get(), vec![Rect::new(4.0, 0.0, 16.0, 16.0)]);
    x.set(16.0);
    ui.advance(0.0);
    assert_eq!(shown.get(), vec![Rect::new(16.0, 0.0, 4.0, 16.0)]);
    cover.set(true);
    ui.advance(0.0);
    assert_eq!(shown.get(), Vec::new());
    cover.set(false);
    mounted.set(false);
    ui.advance(0.0);
    assert_eq!(shown.get(), Vec::new());
}
