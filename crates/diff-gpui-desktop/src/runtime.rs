use gpui::{App, AppContext, Global, Task};
use std::future::Future;
use tokio::{
    runtime::{Builder, Runtime},
    task::{AbortHandle, JoinError},
};

struct DesktopRuntime(Option<Runtime>);

impl Global for DesktopRuntime {}

impl Drop for DesktopRuntime {
    fn drop(&mut self) {
        if let Some(runtime) = self.0.take() {
            runtime.shutdown_background();
        }
    }
}

struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) fn init(cx: &mut App) {
    let runtime = Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("failed to initialize desktop Tokio runtime");
    cx.set_global(DesktopRuntime(Some(runtime)));
}

pub(crate) fn spawn<T>(
    cx: &impl AppContext,
    operation: impl Future<Output = T> + Send + 'static,
) -> Task<Result<T, JoinError>>
where
    T: Send + 'static,
{
    cx.read_global(|runtime: &DesktopRuntime, cx| {
        let task = runtime
            .0
            .as_ref()
            .expect("desktop runtime initialized")
            .spawn(operation);
        let abort = AbortOnDrop(task.abort_handle());
        cx.background_spawn(async move {
            let result = task.await;
            drop(abort);
            result
        })
    })
}
