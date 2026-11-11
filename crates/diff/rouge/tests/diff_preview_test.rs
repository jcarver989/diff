use clankerdiff_rouge::PatchPreview;
use clankerdiff_ui_core::ReviewTheme;
use rouge_core::testing::{TestResult, TestUiBuilder};
use std::{cell::Cell, io, rc::Rc};

#[test]
fn long_previews_are_bounded_and_open_the_original_document() -> TestResult {
    let _runtime = rouge_reactive::Runtime::new().enter();
    let opened = Rc::new(Cell::new(0));
    let preview = |lines| {
        let opened = opened.clone();
        TestUiBuilder::default().size(800.0, 4_000.0).build(|| {
            PatchPreview::parse(&patch(lines))
                .render("preview", &ReviewTheme::default(), move |doc| opened.set(doc.line_stat().added))
        })
    };
    let mut long = preview(300);
    let mut longer = preview(3_000);
    let height = |ui: &mut rouge_core::testing::TestUi| {
        ui.bounds_of("preview").map(|r| r.size.height).ok_or_else(|| io::Error::other("missing preview"))
    };
    assert_eq!(height(&mut long)?, height(&mut longer)?);
    longer.click_on("preview-more")?;
    assert_eq!(opened.get(), 3_000);
    assert!(preview(10).bounds_of("preview-more").is_none());
    Ok(())
}

fn patch(lines: usize) -> String {
    let body = "+<p>line</p>\n".repeat(lines);
    format!(
        "diff --git a/page.html b/page.html\nnew file mode 100644\n--- /dev/null\n+++ b/page.html\n@@ -0,0 +1,{lines} @@\n{body}"
    )
}
