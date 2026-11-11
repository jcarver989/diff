use clankerdiff_symbols::DocumentSyntax;
use rouge_reactive::{Signal, untrack};
use std::sync::Arc;

pub(crate) async fn prepare(syntax: Arc<DocumentSyntax>, progress: Signal<u64>, focus: Signal<Option<usize>>) {
    let mut done = vec![false; syntax.document().files.len()];
    while let Some(file) = next(&done, untrack(|| focus.try_get().flatten())) {
        done[file] = true;
        prepare_file(&syntax, file).await;
        if progress.try_update(|revision| *revision = revision.wrapping_add(1)).is_none() {
            return;
        }
    }
    index(&syntax).await;
    progress.try_update(|revision| *revision = revision.wrapping_add(1));
}

fn next(done: &[bool], focus: Option<usize>) -> Option<usize> {
    let start = focus.unwrap_or(0).min(done.len().saturating_sub(1));
    (start..done.len()).chain(0..start).find(|file| !done[*file])
}

#[cfg(not(target_arch = "wasm32"))]
async fn prepare_file(syntax: &Arc<DocumentSyntax>, file: usize) {
    let syntax = Arc::clone(syntax);
    rouge_reactive::background(async move { syntax.prepare(file) }).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn index(syntax: &Arc<DocumentSyntax>) {
    let syntax = Arc::clone(syntax);
    rouge_reactive::background(async move { drop(syntax.symbols()) }).await;
}

#[cfg(target_arch = "wasm32")]
async fn prepare_file(syntax: &Arc<DocumentSyntax>, file: usize) {
    syntax.prepare(file);
    rouge_reactive::sleep(std::time::Duration::ZERO).await;
}

#[cfg(target_arch = "wasm32")]
async fn index(syntax: &Arc<DocumentSyntax>) {
    drop(syntax.symbols());
}
