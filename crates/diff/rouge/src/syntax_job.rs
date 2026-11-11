use clankerdiff_symbols::DocumentSyntax;
use rouge_reactive::{Scope, Signal, Task, spawn};
use std::sync::Arc;

pub(crate) struct SyntaxJob {
    target: *const DocumentSyntax,
    scope: Scope,
    task: Task,
}

impl SyntaxJob {
    pub(crate) fn start(
        syntax: &Arc<DocumentSyntax>,
        revision: Signal<u64>,
        focus: impl Fn() -> Option<usize> + 'static,
    ) -> Self {
        let target = Arc::as_ptr(syntax);
        let syntax = Arc::clone(syntax);
        let scope = Scope::root();
        let task = scope.run(|| {
            spawn(async move {
                let count = syntax.document().files.len();
                let mut done = vec![false; count];
                while let Some(file) = next(&done, focus()) {
                    done[file] = true;
                    prepare(&syntax, file).await;
                    if !revision.is_alive() {
                        return;
                    }
                    revision.update(|revision| *revision = revision.wrapping_add(1));
                }
                index(&syntax).await;
                if revision.is_alive() {
                    revision.update(|revision| *revision = revision.wrapping_add(1));
                }
            })
        });
        Self { target, scope, task }
    }

    pub(crate) fn targets(&self, syntax: &Arc<DocumentSyntax>) -> bool {
        std::ptr::eq(self.target, Arc::as_ptr(syntax))
    }
}

impl Drop for SyntaxJob {
    fn drop(&mut self) {
        self.task.cancel();
        self.scope.dispose();
    }
}

fn next(done: &[bool], focus: Option<usize>) -> Option<usize> {
    let start = focus.unwrap_or(0).min(done.len().saturating_sub(1));
    (start..done.len()).chain(0..start).find(|file| !done[*file])
}

#[cfg(not(target_arch = "wasm32"))]
async fn prepare(syntax: &Arc<DocumentSyntax>, file: usize) {
    let syntax = Arc::clone(syntax);
    rouge_reactive::background(async move { syntax.prepare(file) }).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn index(syntax: &Arc<DocumentSyntax>) {
    let syntax = Arc::clone(syntax);
    rouge_reactive::background(async move { drop(syntax.symbols()) }).await;
}

#[cfg(target_arch = "wasm32")]
async fn prepare(syntax: &Arc<DocumentSyntax>, file: usize) {
    syntax.prepare(file);
    rouge_reactive::sleep(std::time::Duration::ZERO).await;
}

#[cfg(target_arch = "wasm32")]
async fn index(syntax: &Arc<DocumentSyntax>) {
    drop(syntax.symbols());
}
