mod support;

use clankerdiff_client::{ClientOptions, DiffClient};
use clankerdiff_server::StreamTransport;
use std::error::Error;
use support::{TestServer, wait_for_text};
use tokio::io::{duplex, split};

#[tokio::test]
async fn a_client_reviews_the_repository_over_a_pair_of_byte_streams_until_it_hangs_up() -> Result<(), Box<dyn Error>> {
    let fixture = TestServer::start().await?;
    let (client_end, server_end) = duplex(STREAM_BUFFER);
    let (server_read, server_write) = split(server_end);
    let (client_read, client_write) = split(client_end);
    let reviewing = async {
        let client =
            DiffClient::from_messages(StreamTransport::new(client_read, client_write), ClientOptions::default())
                .await?;
        let mut state = client.subscribe();
        fixture.repo.write("a", "new\nlines\n");
        wait_for_text(&mut state, "new\nlines\n").await?;
        client.close().await?;
        Ok::<_, Box<dyn Error>>(())
    };

    let (served, reviewed) =
        tokio::join!(fixture.server.serve(StreamTransport::new(server_read, server_write)), reviewing);

    reviewed?;
    served?;
    fixture.server.shutdown().await?;
    Ok(())
}

const STREAM_BUFFER: usize = 64 * 1024;
