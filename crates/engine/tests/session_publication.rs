//! Durable Chat publication stays in the profile outbox across host lifetimes.
use std::sync::Arc;

use loro::{ExportMode, LoroDoc, VersionVector};
use zeron_doc::SessionDoc;
use zeron_engine::chat2_host::EngineChatSink;
use zeron_sync::{ChatDocSink, DocsStore};

#[test]
fn engine_sink_replays_legacy_doc_updates_after_store_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(DocsStore::open(dir.path()).unwrap());
    let source = SessionDoc::init("publication").unwrap();
    source
        .doc()
        .get_text("body")
        .insert(0, "durable publication")
        .unwrap();
    source.doc().commit();
    let updates = vec![
        source
            .doc()
            .export(ExportMode::updates(&VersionVector::default()))
            .unwrap(),
    ];
    assert!(!updates.is_empty());
    store
        .initialize_chat_outbox("publication", &updates)
        .unwrap();

    let pending_before = store.pending_chat_updates("publication").unwrap();
    drop(store);

    let reopened_store = Arc::new(DocsStore::open(dir.path()).unwrap());
    let target = Arc::new(SessionDoc::from_doc(LoroDoc::new()));
    let sink = EngineChatSink::new(&target, reopened_store.clone(), "publication");
    let pending = sink.pending_updates().unwrap();
    assert_eq!(pending, pending_before);
    for (_, bytes) in &pending {
        target.doc().import(bytes).unwrap();
    }
    assert_eq!(
        target.doc().get_text("body").to_string(),
        "durable publication"
    );

    for (batch_id, _) in pending {
        sink.acknowledge_update(&batch_id).unwrap();
    }
    assert!(
        reopened_store
            .pending_chat_updates("publication")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn rejected_publication_waits_for_checkpoint_ack() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(DocsStore::open(dir.path()).unwrap());
    store
        .enqueue_chat_update("publication", "too-large", b"payload")
        .unwrap();
    let target = Arc::new(SessionDoc::init("publication").unwrap());
    let sink = EngineChatSink::new(&target, store.clone(), "publication");
    sink.reject_update("too-large").unwrap();
    assert!(sink.pending_updates().unwrap().is_empty());
    assert_eq!(store.rejected_chat_updates("publication").unwrap().len(), 1);
    sink.acknowledge_update("too-large").unwrap();
    assert!(
        store
            .rejected_chat_updates("publication")
            .unwrap()
            .is_empty()
    );
}
