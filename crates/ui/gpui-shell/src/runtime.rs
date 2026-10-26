use gpui::{App, AppContext, Global, Task};
use std::future::Future;
use tokio::{
    runtime::{Builder, Handle, Runtime},
    task::{AbortHandle, JoinError},
};

#[must_use]
pub fn build() -> Runtime {
    Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("failed to start the desktop Tokio runtime")
}

pub fn install(cx: &mut App, handle: Handle) {
    cx.set_global(DesktopRuntime(handle));
}

/// Runs `operation` on the installed Tokio runtime. Dropping the returned task aborts it.
pub fn spawn<T>(
    cx: &impl AppContext,
    operation: impl Future<Output = T> + Send + 'static,
) -> Task<Result<T, JoinError>>
where
    T: Send + 'static,
{
    cx.read_global(|runtime: &DesktopRuntime, cx| {
        let task = runtime.0.spawn(operation);
        let abort = AbortOnDrop(task.abort_handle());
        cx.background_spawn(async move {
            let result = task.await;
            drop(abort);
            result
        })
    })
}

struct DesktopRuntime(Handle);

impl Global for DesktopRuntime {}

struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
