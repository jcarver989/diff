use std::future::Future;

/// Watches the filesystem for changes
pub trait FileWatcher: Send + 'static {
    fn recv(&mut self) -> impl Future<Output = Option<()>> + Send;
}
