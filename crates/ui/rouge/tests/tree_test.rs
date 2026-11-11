use clankerdiff_rouge_ui::{
    prelude::*,
    testing::{TestResult, component_test},
};
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::{Styled, div, view};
use rouge_reactive::signal;

#[test]
fn nested_tree_keeps_full_hit_width_and_separates_toggle_from_selection() -> TestResult {
    let runtime = rouge_reactive::Runtime::new();
    let _guard = runtime.enter();
    let (mut ui, (size, counts)) = component_test().build_with(|| {
        let _ = ActiveTheme::install(ReviewTheme::default());
        let size = InterfaceSize::install(16.0);
        let counts = signal((0, 0));
        let root = view! {
            <div flex col w_full>
                <tree_row id="root" label="Root" expanded=true/>
                <tree_row id="child" label="Child" depth=2 expanded=false icon=IconName::Folder
                          state=SelectionState::Selected on_click={move || counts.update(|v| v.0 += 1)}
                          on_toggle={move || counts.update(|v| v.1 += 1)}/>
            </div>
        };
        (root, (size, counts))
    });
    assert!(ui.center_of("child-toggle")?.x > ui.center_of("root-toggle")?.x + 20.0);
    ui.click_on("child-toggle")?;
    let child = ui.center_of("child")?;
    ui.click_at(190.0, child.y);
    assert_eq!(counts.get(), (1, 1));
    let height = ui.center_of("child")?.y;
    size.set(24.0);
    assert!(ui.center_of("child")?.y > height);
    Ok(())
}
