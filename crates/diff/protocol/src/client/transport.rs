use crate::{
    server::{ServerEvent, ServerMessage},
    shared::LocalEnd,
};

use super::ClientCommand;

pub type LocalClientTransport = LocalEnd<ClientCommand, ServerEvent>;
pub type LocalClientMessageTransport = LocalEnd<ClientCommand, ServerMessage>;
