use clankerdiff_core::{
    DiffDocument, DiffScope, RepositoryAction, Review, SourceUnavailable, StageState,
    testing::DocumentBuilder,
};
use clankerdiff_protocol::{
    client::{ClientCommand, DocumentCache},
    server::{SentDocument, ServerMessage},
    shared::{
        DiffSnapshot, DocumentUpdate, Event, FileEntry, LIVE_PROTOCOL_VERSION, ProtocolError,
        RemoteError, RemoteErrorCode,
    },
};
use std::{error::Error, sync::Arc};

#[test]
fn commands_and_events_round_trip() -> Result<(), Box<dyn Error>> {
    for message in [
        ClientCommand::Initialize {
            protocol_version: LIVE_PROTOCOL_VERSION,
            scope: DiffScope::Staged,
        },
        ClientCommand::SetScope(DiffScope::Unstaged),
        ClientCommand::Apply(RepositoryAction::Commit {
            message: "🦀\n".to_owned(),
        }),
        ClientCommand::Refresh,
        ClientCommand::Submit(Review::default().submission()),
        ClientCommand::Cancel,
    ] {
        let decoded = ClientCommand::decode(&message.encode()?)?;
        assert_eq!(format!("{decoded:?}"), format!("{message:?}"));
    }

    let update = SentDocument::default().encode(&DiffSnapshot {
        scope: DiffScope::Both,
        document: DocumentBuilder::new()
            .changed("a", "old\r\n", "new🦀")
            .build(),
    });
    for message in [
        Event::Initialize {
            protocol_version: LIVE_PROTOCOL_VERSION,
            repository_root: "/remote/repo".to_owned(),
        },
        ServerMessage::Document(update),
        Event::RequestResult(Err(RemoteError::new(RemoteErrorCode::Busy, "busy"))),
        Event::RequestResult(Ok(())),
        Event::Health { error: None },
        Event::Error(RemoteError::new(RemoteErrorCode::Watcher, "stopped")),
    ] {
        let decoded = ServerMessage::decode(&message.encode()?)?;
        assert_eq!(format!("{decoded:?}"), format!("{message:?}"));
    }
    Ok(())
}

#[test]
fn round_trip_preserves_metadata_custom_patches_and_order() -> Result<(), Box<dyn Error>> {
    let mut pair = SyncPair::default();
    let first = DocumentBuilder::new()
        .changed("a", "old\r\n", "new🦀\n")
        .untracked("b", "added\n")
        .build();
    assert_eq!(pair.publish(first.clone())?.received.document, first);

    let mut second = DocumentBuilder::new()
        .deleted("c", "gone")
        .renamed("a", "d", "old\r\n", "new\r\n")
        .binary("b")
        .build();
    let files = &mut Arc::make_mut(&mut second).files;
    files[0].hunks[0].header = "custom header".to_owned();
    files[0].no_newline_at_end = false;
    files[1].staged = StageState::PartiallyStaged;
    files[1].omitted_bytes = Some(32);
    files[2].old_source = Err(SourceUnavailable::TooLarge { bytes: 900_000_000 });
    assert_eq!(pair.publish(second.clone())?.received.document, second);
    Ok(())
}

#[test]
fn unchanged_files_are_not_resent() -> Result<(), Box<dyn Error>> {
    let mut pair = SyncPair::default();
    let document = DocumentBuilder::new()
        .changed("a", "old", "new")
        .changed("b", "old", "new")
        .build();
    pair.publish(document.clone())?;

    let mut edited = document.clone();
    Arc::make_mut(&mut edited).files[0] = clankerdiff_core::FileDiff::from_texts("a", "old", "y")?;
    let published = pair.publish(edited)?;
    assert!(matches!(
        published.update.files.as_slice(),
        [FileEntry::Changed(_), FileEntry::Unchanged(_)]
    ));

    let published = pair.publish(document)?;
    assert!(matches!(
        published.update.files.as_slice(),
        [FileEntry::Changed(_), FileEntry::Unchanged(_)]
    ));
    Ok(())
}

#[test]
fn a_scope_change_alone_reuses_every_file() -> Result<(), Box<dyn Error>> {
    let mut pair = SyncPair::default();
    let document = DocumentBuilder::new().changed("a", "old", "new").build();
    pair.publish(document.clone())?;
    let published = pair.publish_scope(DiffScope::Staged, document)?;
    assert_eq!(published.received.scope, DiffScope::Staged);
    assert!(matches!(
        published.update.files.as_slice(),
        [FileEntry::Unchanged(_)]
    ));
    Ok(())
}

#[test]
fn updates_referring_to_absent_or_repeated_files_are_rejected() {
    let document = DocumentBuilder::new().changed("a", "old", "new").build();
    let missing = update(vec![FileEntry::Unchanged(document.files[0].path.clone())]);
    assert!(DocumentCache::default().apply(&missing).is_err());

    let file = Arc::new(document.files[0].clone());
    let repeated = update(vec![
        FileEntry::Changed(Arc::clone(&file)),
        FileEntry::Changed(file),
    ]);
    assert!(DocumentCache::default().apply(&repeated).is_err());
}

#[test]
fn a_failed_update_leaves_the_previous_document_installed() -> Result<(), Box<dyn Error>> {
    let mut cache = DocumentCache::default();
    let document = DocumentBuilder::new().changed("a", "old", "new").build();
    let installed = cache.apply(&bootstrap(&document))?;
    let mut broken = bootstrap(&document);
    broken
        .files
        .push(FileEntry::Unchanged(document.files[0].path.clone()));
    assert!(cache.apply(&broken).is_err());
    assert_eq!(cache.apply(&bootstrap(&document))?, installed);
    Ok(())
}

#[test]
fn truncated_and_foreign_payloads_are_rejected() -> Result<(), Box<dyn Error>> {
    let text = ClientCommand::Refresh.encode()?;
    assert!(ClientCommand::decode(&text[..text.len() - 1]).is_err());
    assert!(ServerMessage::decode("").is_err());
    assert!(ClientCommand::decode("{\"Unknown\":1}").is_err());
    Ok(())
}

#[derive(Debug, Default)]
struct SyncPair {
    sent: SentDocument,
    cache: DocumentCache,
}

impl SyncPair {
    fn publish(&mut self, document: Arc<DiffDocument>) -> Result<Publication, ProtocolError> {
        self.publish_scope(DiffScope::Both, document)
    }

    fn publish_scope(
        &mut self,
        scope: DiffScope,
        document: Arc<DiffDocument>,
    ) -> Result<Publication, ProtocolError> {
        let update = self.sent.encode(&DiffSnapshot { scope, document });
        let message = ServerMessage::Document(update.clone()).encode()?;
        let Event::Document(received) = self.cache.decode_event(&message)? else {
            unreachable!()
        };
        Ok(Publication { update, received })
    }
}

struct Publication {
    update: DocumentUpdate,
    received: Arc<DiffSnapshot>,
}

fn update(files: Vec<FileEntry>) -> DocumentUpdate {
    DocumentUpdate {
        scope: DiffScope::Both,
        repo_root: "/repo".to_owned(),
        files,
    }
}

fn bootstrap(document: &DiffDocument) -> DocumentUpdate {
    update(
        document
            .files
            .iter()
            .map(|file| FileEntry::Changed(Arc::new(file.clone())))
            .collect(),
    )
}

#[test]
fn document_updates_reuse_unchanged_files() -> Result<(), Box<dyn Error>> {
    let document = DocumentBuilder::new()
        .changed("a", "old", "new")
        .changed("b", "old", "new")
        .build();
    let mut sent = SentDocument::default();
    let mut cache = DocumentCache::default();
    let first = sent.encode(&DiffSnapshot {
        scope: DiffScope::Both,
        document: document.clone(),
    });
    let message: ServerMessage = Event::Document(first);
    let Event::Document(received) = cache.decode_event(&message.encode()?)? else {
        return Err("expected document event".into());
    };
    assert_eq!(received.document, document);

    let second = sent.encode(&DiffSnapshot {
        scope: DiffScope::Staged,
        document,
    });
    assert!(
        second
            .files
            .iter()
            .all(|entry| matches!(entry, FileEntry::Unchanged(_)))
    );
    let received = cache.apply(&second)?;
    assert_eq!(received.scope, DiffScope::Staged);
    assert_eq!(received.document.files.len(), 2);
    assert_eq!(LIVE_PROTOCOL_VERSION, 2);
    Ok(())
}

#[test]
fn encoded_events_rebuild_through_the_cache() -> Result<(), Box<dyn Error>> {
    let mut sent = SentDocument::default();
    let mut cache = DocumentCache::default();
    let document = DocumentBuilder::new()
        .changed("a", "old", "new")
        .changed("b", "old", "new")
        .build();
    let snapshot = |scope| {
        Event::Document(Arc::new(DiffSnapshot {
            scope,
            document: document.clone(),
        }))
    };

    let Event::Document(first) = cache.apply_event(sent.encode_event(snapshot(DiffScope::Both)))?
    else {
        return Err("expected document event".into());
    };
    assert_eq!(first.document, document);

    let second = sent.encode_event(snapshot(DiffScope::Staged));
    let Event::Document(update) = &second else {
        return Err("expected document event".into());
    };
    assert!(
        update
            .files
            .iter()
            .all(|entry| matches!(entry, FileEntry::Unchanged(_)))
    );
    let Event::Document(second) = cache.apply_event(second)? else {
        return Err("expected document event".into());
    };
    assert_eq!(second.scope, DiffScope::Staged);
    assert_eq!(second.document, document);

    assert!(matches!(
        cache.apply_event(sent.encode_event(Event::RequestResult(Ok(()))))?,
        Event::RequestResult(Ok(()))
    ));
    Ok(())
}
