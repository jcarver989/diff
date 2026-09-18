use clankerdiff_client::{ClientOptions, DiffClient};
use std::{env, error::Error, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let url = env::args()
        .nth(1)
        .ok_or("usage: remote_review ws://host:7331/ws")?;
    let client = DiffClient::connect(&url, ClientOptions::default()).await?;
    let state = client
        .subscribe()
        .wait_until(Duration::from_secs(30), |state| state.snapshot.is_some())
        .await?;
    let snapshot = state.snapshot.as_ref().ok_or("snapshot")?;
    println!(
        "{}: {} changed files",
        snapshot.document.repo_root,
        snapshot.document.files.len()
    );
    client.close().await?;
    Ok(())
}
