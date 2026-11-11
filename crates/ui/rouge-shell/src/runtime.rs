use rouge::reactive::{Task, expect_context, provide_context, spawn_on};
use std::future::Future;
use tokio::runtime::{Builder, Handle, Runtime};

#[must_use]
pub fn build() -> Runtime {
    Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("failed to start the desktop Tokio runtime")
}

#[derive(Clone)]
struct DesktopRuntime(Handle);

pub fn install(handle: Handle) {
    provide_context(DesktopRuntime(handle));
}

pub fn spawn<T: Send + 'static>(operation: impl Future<Output = T> + Send + 'static) -> Task<T> {
    spawn_on(expect_context::<DesktopRuntime>().0, operation)
}
