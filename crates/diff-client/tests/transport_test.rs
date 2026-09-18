#![cfg(all(feature = "websocket", not(target_arch = "wasm32")))]

use clankerdiff_client::{
    ClientOptions, ConnectionState, DiffClient,
    protocol::{
        server::ServerMessage,
        shared::{DocumentUpdate, FileEntry, LIVE_PROTOCOL_VERSION},
    },
};
use clankerdiff_core::DiffScope;
use futures_util::{SinkExt, StreamExt};
use std::{error::Error, time::Duration};
use tokio::{net::TcpListener, time::timeout};
use tokio_tungstenite::{WebSocketStream, accept_async, tungstenite::Message};

#[tokio::test]
async fn a_malformed_frame_is_terminal_not_a_reconnect_loop()
-> Result<(), Box<dyn Error + Send + Sync>> {
    assert_failed_without_retry(Message::text("{\"NotAMessage\":true}")).await
}

#[tokio::test]
async fn an_unmaterializable_update_is_terminal_not_a_reconnect_loop()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let update = DocumentUpdate {
        scope: DiffScope::Both,
        repo_root: "/remote".to_owned(),
        files: vec![FileEntry::Unchanged("never-sent".try_into()?)],
    };
    assert_failed_without_retry(Message::text(ServerMessage::Document(update).encode()?)).await
}

#[tokio::test]
async fn socket_eof_reconnects_and_installs_the_next_document()
-> Result<(), Box<dyn Error + Send + Sync>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let serving = tokio::spawn(async move {
        let socket = accept_initialized(&listener).await?;
        drop(socket);

        let mut socket = timeout(Duration::from_secs(2), accept_initialized(&listener)).await??;
        socket
            .send(Message::text(
                ServerMessage::Document(DocumentUpdate {
                    scope: DiffScope::Both,
                    repo_root: "/remote".to_owned(),
                    files: Vec::new(),
                })
                .encode()?,
            ))
            .await?;
        socket.next().await;
        Ok::<_, Box<dyn Error + Send + Sync>>(())
    });

    let client =
        DiffClient::connect(&format!("ws://{address}/ws"), ClientOptions::default()).await?;
    client
        .subscribe()
        .wait_until(Duration::from_secs(3), |state| {
            matches!(state.connection, ConnectionState::Connected) && state.snapshot.is_some()
        })
        .await?;
    client.close().await?;
    serving.await??;
    Ok(())
}

type Socket = WebSocketStream<tokio::net::TcpStream>;

async fn accept_initialized(
    listener: &TcpListener,
) -> Result<Socket, Box<dyn Error + Send + Sync>> {
    let (socket, _) = listener.accept().await?;
    let mut socket = accept_async(socket).await?;
    socket.next().await.ok_or("missing Hello")??;
    socket
        .send(Message::text(
            ServerMessage::Initialize {
                protocol_version: LIVE_PROTOCOL_VERSION,
                repository_root: "/remote".to_owned(),
            }
            .encode()?,
        ))
        .await?;
    Ok(socket)
}

/// Serves one initialization followed by `poison`, then asserts the client fails
/// permanently rather than reconnecting.
async fn assert_failed_without_retry(poison: Message) -> Result<(), Box<dyn Error + Send + Sync>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let serving = tokio::spawn(async move {
        let mut socket = accept_initialized(&listener).await?;
        socket.send(poison).await?;
        assert!(
            timeout(Duration::from_millis(750), listener.accept())
                .await
                .is_err()
        );
        Ok::<_, Box<dyn Error + Send + Sync>>(())
    });

    let client =
        DiffClient::connect(&format!("ws://{address}/ws"), ClientOptions::default()).await?;
    let mut updates = client.subscribe();
    updates
        .wait_until(Duration::from_secs(2), |state| {
            matches!(state.connection, ConnectionState::Failed(_))
        })
        .await?;
    assert!(client.state().snapshot.is_none());
    assert!(client.state().status().is_some());
    serving.await??;
    Ok(())
}
