use clankerdiff_server::{DiffServer, ServerOptions};
use std::{env, error::Error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args().nth(1).unwrap_or_else(|| ".".to_owned());
    let server = DiffServer::open(path, ServerOptions::default()).await?;
    let listener = server.listen("127.0.0.1:7331".parse()?).await?;
    eprintln!("Listening on ws://{}/ws", listener.local_addr());
    tokio::signal::ctrl_c().await?;
    server.shutdown().await?;
    Ok(())
}
