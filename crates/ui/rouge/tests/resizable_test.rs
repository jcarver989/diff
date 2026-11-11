use clankerdiff_rouge_ui::{
    prelude::*,
    testing::{TestResult, component_test},
};
use rouge_core::{Point, Styled, div, view};
use rouge_reactive::signal;

#[test]
fn resizes_both_edges_in_rem_clamps_and_resets() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    for side in [PanelSide::Left, PanelSide::Right] {
        let (mut ui, (width, size, resets, reported)) = component_test().size(640.0, 200.0).build_with(move || {
            let size = InterfaceSize::install(16.0);
            let width = signal(10.0_f32);
            let resets = signal(0);
            let reported = signal(0.0_f32);
            let panel = view! {
                <resizable_panel id="panel" side width_in_rem=width min_width=6.0 max_width=14.0
                                 on_resize={move |value: f32| reported.set(value)}
                                 on_reset={move || resets.update(|n| *n += 1)}>
                    <div h=160.0/>
                </resizable_panel>
            };
            let root = div().size_full().flex();
            let root = if side == PanelSide::Right { root.justify_end() } else { root };
            (root.child(panel), (width, size, resets, reported))
        });
        let initial = ui.center_of("panel-resize-handle")?;
        let direction = if side == PanelSide::Left { 1.0 } else { -1.0 };
        ui.drag_on("panel-resize-handle", Point::new(initial.x + 100.0 * direction, initial.y))?;
        assert!((width.get() - 14.0).abs() < 0.01);
        assert!((reported.get() - 14.0).abs() < 0.01);
        let handle = ui.center_of("panel-resize-handle")?;
        ui.advance(1.0);
        ui.click(handle);
        ui.click(handle);
        ui.advance(0.0);
        assert_eq!(resets.get(), 1);
        assert!((width.get() - 10.0).abs() < 0.01);
        let before = ui.center_of("panel")?;
        size.set(20.0);
        assert!((ui.center_of("panel")?.x - before.x - 20.0 * direction).abs() < 0.1);
    }
    Ok(())
}
