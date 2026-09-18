use crate::{server::ServerEvent, shared::LocalEnd};

use super::ClientCommand;

pub type LocalClientTransport = LocalEnd<ClientCommand, ServerEvent>;
