use clankerdiff_core::{DiffPresentation, DiffSide, PresentationOptions, Review, ViewMode, testing::DocumentBuilder};
use serde_json::json;
use std::{env, error::Error, fs, path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn Error>> {
    let output = env::args_os().nth(1).map(PathBuf::from).ok_or("expected an output directory")?;
    fs::create_dir_all(&output)?;
    let old = "fn greeting(name: &str) -> String {\n\tformat!(\"Hello, {name}\")\n}\n";
    let new = "fn greeting(name: &str) -> String {\n\tformat!(\"Hello, {name}! café e\u{301} 日本語\")\n}\n";
    let builder = DocumentBuilder::new()
        .repo_root("/migration-fixture")
        .changed("src/greeting.rs", old, new)
        .renamed("src/old.rs", "src/renamed.rs", "let old = 1;\n", "let new = 2;\n")
        .added("src/added.rs", "pub const ADDED: bool = true;\n")
        .deleted("src/deleted.rs", "pub const REMOVED: bool = true;\n")
        .untracked("notes.txt", "A new file without a trailing newline")
        .binary("assets/image.png");
    let small = builder.build();
    let mut replacement = (*small).clone();
    replacement.files[0] =
        DocumentBuilder::new().changed("src/greeting.rs", old, &new.replace("Hello", "Howdy")).build().files[0].clone();
    let documents = [
        ("small", small.clone()),
        ("same-count-refresh", Arc::new(replacement)),
        ("large", DocumentBuilder::new().repo_root("/migration-large").generated("src/large.rs", 100_000).build()),
        ("many-files", DocumentBuilder::new().repo_root("/migration-many").generated_files(100, 60).build()),
        (
            "long-lines",
            DocumentBuilder::new()
                .repo_root("/migration-long")
                .changed(
                    "src/long.rs",
                    &format!("{}\n", "old ".repeat(2048)),
                    &format!("{}\n", "新 e\u{301}\t".repeat(2048)),
                )
                .build(),
        ),
        ("empty", DocumentBuilder::new().repo_root("/migration-empty").build()),
    ];
    let mut fixtures = Vec::new();
    for (name, document) in documents {
        let bytes = serde_json::to_vec_pretty(document.as_ref())?;
        fs::write(output.join(format!("{name}.json")), bytes)?;
        let rows = |view_mode| {
            DiffPresentation::new(document.clone(), PresentationOptions { view_mode, ..Default::default() }).row_count()
        };
        fixtures.push(json!({
            "name": name,
            "files": document.files.len(),
            "unified_rows": rows(ViewMode::Unified),
            "split_rows": rows(ViewMode::Split),
        }));
    }
    let presentation = DiffPresentation::new(small, PresentationOptions::default());
    let anchor = (0..presentation.row_count())
        .find_map(|row| presentation.anchor_at(row, DiffSide::New))
        .ok_or("small fixture must have a commentable new-side line")?;
    let mut review = Review::default();
    review.add_comment(
        anchor,
        "Check Unicode, tabs, anchors, and wrapped review comments.\nA second line must survive export.",
    );
    fs::write(output.join("review.json"), serde_json::to_vec_pretty(&review.submission())?)?;
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&json!({ "schema_version": 1, "documents": fixtures }))?,
    )?;
    Ok(())
}
