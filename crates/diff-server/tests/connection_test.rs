mod support;

use clankerdiff_client::{ClientOptions, DiffClient};
use clankerdiff_core::DiffScope;
use clankerdiff_protocol::{
    client::{ClientCommand, DocumentCache},
    server::ServerMessage,
    shared::{Event, LIVE_PROTOCOL_VERSION},
};
use futures_util::{SinkExt, StreamExt};
use std::error::Error;
use support::{TestServer, WAIT, wait_for_text};
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

#[tokio::test]
async fn a_slow_consumer_converges_without_blocking_another_client() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    let (listener, url) = fixture.listen().await?;

    let (mut paused, _) = connect_async(&url).await?;
    send(&mut paused, &hello()).await?;
    assert!(matches!(
        timeout(WAIT, receive(&mut paused)).await??,
        Event::Initialize { .. }
    ));

    let client = DiffClient::connect(&url, ClientOptions::default()).await?;
    let mut updates = client.subscribe();
    for source in ["first edit\n", "latest edit\n"] {
        fixture.repo.write("a", source);
        client.refresh().await?;
        wait_for_text(&mut updates, source).await?;
    }

    let mut cache = DocumentCache::default();
    let mut latest = String::new();
    timeout(WAIT, async {
        while latest != "latest edit\n" {
            if let Event::Document(update) = receive(&mut paused).await? {
                let snapshot = cache.apply(&update)?;
                if let Some(file) = snapshot.document.files.first() {
                    latest = file
                        .new_source
                        .as_ref()
                        .map_err(Clone::clone)?
                        .text()
                        .to_owned();
                }
            }
        }
        Ok::<_, Box<dyn Error>>(())
    })
    .await??;

    paused.close(None).await?;
    client.close().await?;
    listener.shutdown().await?;
    fixture.server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn unsupported_versions_and_malformed_frames_are_rejected() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    let (listener, url) = fixture.listen().await?;

    let (mut socket, _) = connect_async(&url).await?;
    send(
        &mut socket,
        &ClientCommand::Initialize {
            protocol_version: LIVE_PROTOCOL_VERSION + 1,
            scope: DiffScope::Both,
        },
    )
    .await?;
    assert!(matches!(
        timeout(WAIT, receive(&mut socket)).await??,
        Event::Error(_)
    ));

    let (mut socket, _) = connect_async(&url).await?;
    socket.send(Message::text("not a message")).await?;
    assert!(matches!(
        timeout(WAIT, receive(&mut socket)).await??,
        Event::Error(_)
    ));

    let healthz = format!("http://{}/healthz", listener.local_addr());
    assert!(fetch(&healthz).await?.contains("ok"));

    listener.shutdown().await?;
    fixture.server.shutdown().await?;
    Ok(())
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

fn hello() -> ClientCommand {
    ClientCommand::Initialize {
        protocol_version: LIVE_PROTOCOL_VERSION,
        scope: DiffScope::Both,
    }
}

async fn send(socket: &mut Socket, message: &ClientCommand) -> Result<(), Box<dyn Error>> {
    socket.send(Message::text(message.encode()?)).await?;
    Ok(())
}

async fn receive(socket: &mut Socket) -> Result<ServerMessage, Box<dyn Error>> {
    loop {
        match socket.next().await.ok_or("socket closed")?? {
            Message::Text(text) => return Ok(ServerMessage::decode(&text)?),
            Message::Close(_) => return Err("socket closed".into()),
            _ => {}
        }
    }
}

async fn fetch(url: &str) -> Result<String, Box<dyn Error>> {
    let url = url.strip_prefix("http://").ok_or("http url")?;
    let (address, path) = url.split_once('/').ok_or("http path")?;
    let mut stream = TcpStream::connect(address).await?;
    let request = format!("GET /{path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    tokio::io::AsyncWriteExt::write_all(&mut stream, request.as_bytes()).await?;
    let mut response = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut response).await?;
    Ok(String::from_utf8(response)?)
}
