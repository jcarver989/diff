use std::{future::Future, time::Duration};

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn(future: impl Future<Output = ()> + Send + 'static) {
    tokio::spawn(future);
}

#[cfg(target_arch = "wasm32")]
pub fn spawn(future: impl Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(future);
}

#[cfg(feature = "websocket")]
pub fn reconnect_delay(base_ms: u64) -> Duration {
    #[cfg(not(target_arch = "wasm32"))]
    let fraction = rand::random::<f64>();
    #[cfg(target_arch = "wasm32")]
    let fraction = js_sys::Math::random();
    Duration::from_millis(base_ms).mul_f64(0.8 + 0.2 * fraction)
}

pub async fn sleep(duration: Duration) {
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(duration).await;
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::TimeoutFuture::new(
        u32::try_from(duration.as_millis()).unwrap_or(u32::MAX),
    )
    .await;
}
