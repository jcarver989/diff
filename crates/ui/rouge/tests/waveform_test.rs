use clankerdiff_rouge_ui::{
    prelude::*,
    testing::{TestResult, component_test},
};
use rouge_core::{Color, Styled, div, show};
use rouge_reactive::{Runtime, signal};

#[test]
fn measured_waveform_responds_to_levels_without_requesting_animation_frames() -> TestResult {
    let runtime = Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, (levels, mounted)) = component_test().build_with(|| {
        let _layer = IndicatorLayer::install();
        let levels = signal([0.0; 5]);
        let mounted = signal(true);
        let root = div()
            .child(show(move || mounted.get(), move || Waveform::new("wave", Color::WHITE).size(48.0).levels(levels)));
        (root, (levels, mounted))
    });
    let before = ui.pixel(14, 10)?;
    levels.set([1.0; 5]);
    ui.advance(0.0);
    assert_ne!(before, ui.pixel(14, 10)?);
    assert!(!ui.is_animating());
    mounted.set(false);
    ui.advance(0.0);
    assert!(ui.bounds_of("wave").is_none());
    Ok(())
}

#[test]
fn animated_waveform_uses_compositor_without_relaying_out_its_host() {
    let runtime = Runtime::new();
    let _guard = runtime.enter();
    let mut ui = component_test().build(|| div().size_full().child(Waveform::new("wave", Color::WHITE)));
    let before = ui.passes();
    ui.run(0.8);
    assert_eq!(before, ui.passes());
    assert!(ui.is_animating());
}
