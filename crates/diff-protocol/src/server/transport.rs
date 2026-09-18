use crate::{
    client::{ClientCommand, LocalClientTransport},
    shared::LocalEnd,
};

use super::ServerEvent;
use async_channel::bounded;

pub type LocalServerTransport = LocalEnd<ServerEvent, ClientCommand>;

#[must_use]
pub fn local_transport_pair(capacity: usize) -> (LocalClientTransport, LocalServerTransport) {
    let (commands_tx, commands_rx) = bounded(capacity.max(1));
    let (events_tx, events_rx) = bounded(capacity.max(1));
    (
        LocalEnd::new(commands_tx, events_rx),
        LocalEnd::new(events_tx, commands_rx),
    )
}
