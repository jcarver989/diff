use clankerdiff_gpui::DiffPreviewCache;
use clankerdiff_gpui::ui::testing::ComponentHarnessBuilder;
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_ui_core::ReviewTheme;
use gpui::{AnyElement, App, Pixels, TestAppContext, Window};
use std::cell::Cell;
use std::rc::Rc;

#[gpui::test]
fn a_long_patch_previews_the_same_rows_however_long_it_grows(cx: &mut TestAppContext) {
    let long = preview_height(cx, added_file(300));
    let longer = preview_height(cx, added_file(3000));

    assert!(long.is_some(), "the preview renders");
    assert_eq!(long, longer);
}

#[gpui::test]
fn a_long_patch_opens_in_full_from_its_last_preview_row(cx: &mut TestAppContext) {
    let opened = Rc::new(Cell::new(None));

    tall_window().run(cx, preview(added_file(3000), opened.clone()), |harness, cx| {
        harness.click(cx, "preview-more")?;

        assert_eq!(opened.get(), Some(3000));
        Ok(())
    });
}

#[gpui::test]
fn a_short_patch_shows_every_row(cx: &mut TestAppContext) {
    let harness = tall_window().build(cx, preview(added_file(10), Rc::default()));

    assert!(harness.bounds(cx, "preview").is_some());
    assert!(harness.bounds(cx, "preview-more").is_none());
}

fn preview(patch: String, opened: Rc<Cell<Option<usize>>>) -> impl Fn(&mut Window, &mut App) -> AnyElement + 'static {
    move |_, _| {
        let opened = opened.clone();
        DiffPreviewCache::default().render(
            "preview",
            &patch,
            &ReviewTheme::default(),
            &mut SyntaxHighlighter::default(),
            move |document, _, _| opened.set(Some(document.line_stat().added)),
        )
    }
}

fn preview_height(cx: &mut TestAppContext, patch: String) -> Option<Pixels> {
    tall_window().build(cx, preview(patch, Rc::default())).bounds(cx, "preview").map(|bounds| bounds.size.height)
}

fn tall_window() -> ComponentHarnessBuilder {
    ComponentHarnessBuilder { height: 20_000.0, ..ComponentHarnessBuilder::default() }
}

fn added_file(lines: usize) -> String {
    let body = "+<p>line</p>\n".repeat(lines);
    format!(
        "diff --git a/page.html b/page.html\nnew file mode 100644\n--- /dev/null\n+++ b/page.html\n@@ -0,0 +1,{lines} @@\n{body}"
    )
}
