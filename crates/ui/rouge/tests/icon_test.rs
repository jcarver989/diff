use clankerdiff_rouge_ui::{
    fonts::bundled_fonts,
    prelude::*,
    testing::{TestResult, component_test},
};
use rouge_core::{Color, Styled, div};
use rouge_reactive::Runtime;

#[test]
fn bundled_faces_and_lucide_paths_render() -> TestResult {
    let runtime = Runtime::new();
    let _guard = runtime.enter();
    assert_eq!(bundled_fonts().count(), 16);
    let mut ui = component_test().build(|| div().child(Icon::new(IconName::Plus, Color::WHITE).size(48.0)));
    assert_eq!(ui.pixel(24, 24)?, [255; 4]);
    assert_eq!(ui.pixel(4, 4)?, [0; 4]);
    Ok(())
}

#[test]
fn spinner_animates_without_relayout() {
    let runtime = Runtime::new();
    let _guard = runtime.enter();
    let mut ui =
        component_test().build(|| div().size_full().child(Icon::new(IconName::LoaderCircle, Color::WHITE).spin()));
    let before = ui.passes();
    ui.run(0.6);
    assert_eq!(before, ui.passes());
    assert!(ui.is_animating());
}

#[test]
fn every_icon_stays_inside_its_box() -> TestResult {
    let runtime = Runtime::new();
    let _guard = runtime.enter();
    for name in IconName::ALL {
        let mut ui = component_test()
            .size(96.0, 96.0)
            .build(|| div().size_full().p(24.0).child(Icon::new(name, Color::WHITE).size(48.0)));
        let image = ui.render()?;
        let outside = (0..96)
            .flat_map(|y| (0..96).map(move |x| (x, y)))
            .filter(|&(x, y)| !(20..76).contains(&x) || !(20..76).contains(&y))
            .find(|&(x, y)| image.pixel(x, y)[3] != 0);
        assert_eq!(outside, None, "{name:?} paints outside its box");
    }
    Ok(())
}
