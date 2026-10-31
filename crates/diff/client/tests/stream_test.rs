#![cfg(not(target_arch = "wasm32"))]

use clankerdiff_client::{
    ClientOptions, ConnectionState, DiffClient,
    protocol::{
        client::ClientCommand,
        server::ServerMessage,
        shared::{Event, LIVE_PROTOCOL_VERSION},
    },
    transport::StreamTransport,
};
use std::{error::Error, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, duplex, split};

#[tokio::test]
async fn a_malformed_line_fails_the_connection() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (client_end, server_end) = duplex(STREAM_BUFFER);
    let serving = tokio::spawn(async move {
        let (read, mut write) = split(server_end);
        let mut lines = BufReader::new(read).lines();
        let hello = lines.next_line().await?.ok_or("the client says hello")?;
        assert!(matches!(ClientCommand::decode(&hello)?, ClientCommand::Initialize { .. }));
        let initialize: ServerMessage =
            Event::Initialize { protocol_version: LIVE_PROTOCOL_VERSION, repository_root: "/remote".to_owned() };
        write.write_all(format!("{}\nnot a message\n", initialize.encode()?).as_bytes()).await?;
        lines.next_line().await?;
        Ok::<_, Box<dyn Error + Send + Sync>>(())
    });
    let (read, write) = split(client_end);

    let client = DiffClient::from_messages(StreamTransport::new(read, write), ClientOptions::default()).await?;

    let mut updates = client.subscribe();
    updates.wait_until(WAIT, |state| matches!(state.connection, ConnectionState::Failed(_))).await?;
    assert!(client.state().snapshot.is_none());
    drop(client);
    serving.await??;
    Ok(())
}

const STREAM_BUFFER: usize = 64 * 1024;
const WAIT: Duration = Duration::from_secs(10);
