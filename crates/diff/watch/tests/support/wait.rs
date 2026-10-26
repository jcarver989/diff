use std::{future::Future, time::Duration};
use tokio::time::{sleep, timeout};

const WAIT_TIMEOUT: Duration = Duration::from_secs(10);
const OBSERVATION_WINDOW: Duration = Duration::from_millis(1_500);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub async fn wait_for<T: Future>(description: &str, future: T) -> T::Output {
    timeout(WAIT_TIMEOUT, future)
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {description} after {WAIT_TIMEOUT:?}"))
}

pub async fn assert_pending<T: Future>(description: &str, future: T) {
    assert!(
        timeout(OBSERVATION_WINDOW, future).await.is_err(),
        "{description}: completed unexpectedly within {OBSERVATION_WINDOW:?}"
    );
}

pub async fn observe() {
    sleep(OBSERVATION_WINDOW).await;
}

pub async fn wait_until(description: &str, mut predicate: impl FnMut() -> bool) {
    wait_for(description, async {
        while !predicate() {
            sleep(POLL_INTERVAL).await;
        }
    })
    .await;
}
