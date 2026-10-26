use crate::shared::{DocumentUpdate, Event};
use clankerdiff_core::DiffDocument;
use std::sync::Arc;

pub type ServerEvent = Event<Arc<DiffDocument>>;
pub type ServerMessage = Event<DocumentUpdate>;
