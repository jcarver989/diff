use crate::{
    client::{ClientCommand, LocalClientMessageTransport, LocalClientTransport},
    shared::LocalEnd,
};

use super::{ServerEvent, ServerMessage};
use async_channel::bounded;

pub type LocalServerTransport = LocalEnd<ServerEvent, ClientCommand>;
pub type LocalServerMessageTransport = LocalEnd<ServerMessage, ClientCommand>;

#[must_use]
pub fn local_transport_pair(capacity: usize) -> (LocalClientTransport, LocalServerTransport) {
    let (commands_tx, commands_rx) = bounded(capacity.max(1));
    let (events_tx, events_rx) = bounded(capacity.max(1));
    (
        LocalEnd::new(commands_tx, events_rx),
        LocalEnd::new(events_tx, commands_rx),
    )
}

#[must_use]
pub fn local_message_transport_pair(
    capacity: usize,
) -> (LocalClientMessageTransport, LocalServerMessageTransport) {
    let (commands_tx, commands_rx) = bounded(capacity.max(1));
    let (messages_tx, messages_rx) = bounded(capacity.max(1));
    (
        LocalEnd::new(commands_tx, messages_rx),
        LocalEnd::new(messages_tx, commands_rx),
    )
}
