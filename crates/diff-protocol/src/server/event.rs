use crate::shared::{DiffSnapshot, DocumentUpdate, Event};
use std::sync::Arc;

pub type ServerEvent = Event<Arc<DiffSnapshot>>;
pub type ServerMessage = Event<DocumentUpdate>;
