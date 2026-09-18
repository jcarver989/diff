#![cfg(all(feature = "websocket", not(target_arch = "wasm32")))]

use clankerdiff_client::{
    ClientOptions, ConnectionHeader, ConnectionState, DiffClient,
    protocol::{
        server::ServerMessage,
        shared::{DocumentUpdate, FileEntry, LIVE_PROTOCOL_VERSION},
    },
};
use clankerdiff_core::DiffScope;
use futures_util::{SinkExt, StreamExt};
use std::{error::Error, time::Duration};
use tokio::{net::TcpListener, time::timeout};
use tokio_tungstenite::{
    WebSocketStream, accept_async, accept_hdr_async,
    tungstenite::{
        Message,
        handshake::server::{Request, Response},
    },
};

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
        let socket = accept_initialized_with_headers(&listener).await?;
        drop(socket);

        let mut socket = timeout(
            Duration::from_secs(2),
            accept_initialized_with_headers(&listener),
        )
        .await??;
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

    let headers = vec![
        "X-aws-proxy-auth: token:a:b".parse::<ConnectionHeader>()?,
        "X-aws-proxy-port: 8081".parse::<ConnectionHeader>()?,
        "X-Repeat: first".parse::<ConnectionHeader>()?,
        "X-Repeat: second".parse::<ConnectionHeader>()?,
    ];
    let client = DiffClient::connect_with_headers(
        &format!("ws://{address}/ws"),
        ClientOptions::default(),
        headers,
    )
    .await?;
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
    initialize(accept_async(socket).await?).await
}

#[allow(clippy::result_large_err)]
async fn accept_initialized_with_headers(
    listener: &TcpListener,
) -> Result<Socket, Box<dyn Error + Send + Sync>> {
    let (socket, _) = listener.accept().await?;
    let socket = accept_hdr_async(socket, |request: &Request, response: Response| {
        assert_eq!(request.headers()["x-aws-proxy-auth"], "token:a:b");
        assert_eq!(request.headers()["x-aws-proxy-port"], "8081");
        let repeated = request
            .headers()
            .get_all("x-repeat")
            .iter()
            .map(http::HeaderValue::as_bytes)
            .collect::<Vec<_>>();
        assert_eq!(repeated, [b"first".as_slice(), b"second".as_slice()]);
        Ok(response)
    })
    .await?;
    initialize(socket).await
}

async fn initialize(mut socket: Socket) -> Result<Socket, Box<dyn Error + Send + Sync>> {
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
