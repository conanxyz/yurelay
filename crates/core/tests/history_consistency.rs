use std::{
    collections::BTreeMap,
    future::Future,
    sync::Arc,
    task::{Context, Poll, Waker},
    time::Duration,
};
use yurelay_core::{
    config::{Profile, Retention},
    engine::{Credentials, Engine},
    operations::{OperationRecord, SavedOperation},
    source::{SourceRequest, UpdateRequest},
    storage,
    tasks::{Source, Task, TaskBook},
};

struct FakeCredentials;
impl Credentials for FakeCredentials {
    fn read(&self, _: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn write(&self, _: &str, _: Option<&str>) -> Result<(), String> {
        Ok(())
    }
}

async fn engine() -> (tempfile::TempDir, Arc<Engine>) {
    let directory = tempfile::tempdir().unwrap();
    let engine = Engine::open(
        directory.path().into(),
        Arc::new(FakeCredentials),
        Arc::new(|| 1000),
    )
    .await
    .unwrap();
    let mut settings = engine.configuration().await.settings;
    settings.profiles[0].endpoint = "https://example.test/v1".into();
    settings.profiles[0].model = "synthetic-model".into();
    settings.auto_vocabulary = false;
    settings.auto_clean = false;
    engine
        .save_configuration(settings, BTreeMap::new())
        .await
        .unwrap();
    (directory, engine)
}

fn completed(profile: Profile, source: &str, translation: &str, created_at: i64) -> Task {
    let book = TaskBook::new(Retention::Days(7), 12);
    let task = book
        .create(Source::Selection, profile, "zh-Hans".into(), created_at)
        .unwrap();
    let queued = book
        .recognized(&task.id, source.into(), 0, created_at)
        .unwrap()
        .unwrap();
    book.cached_result(&task.id, queued.attempt, translation.into(), created_at)
        .unwrap()
}

async fn completed_parent(engine: &Engine) -> Task {
    let profile = engine.configuration().await.settings.profiles[0].clone();
    let task = completed(profile, "The old source has apples.", "苹果的旧译文", 1000);
    engine.storage.save_task(task.clone()).await.unwrap();
    engine.book.restore(vec![task.clone()], engine.now());
    engine.book.get(&task.id).unwrap()
}

#[tokio::test]
async fn source_edit_preserves_old_translation_without_caching_it_for_new_text() {
    let (_directory, engine) = engine().await;
    let original = completed_parent(&engine).await;
    let edited = engine
        .source_update(UpdateRequest {
            source: SourceRequest {
                task_id: original.id.clone(),
                expected_revision: original.source_revision,
            },
            text: "The new source has oranges.".into(),
            source_language: None,
            target_language: None,
        })
        .await
        .unwrap();
    assert_eq!(edited.translation, original.translation);
    assert_ne!(
        edited.translation_source_revision,
        Some(edited.source_revision)
    );
    let new_request_key = storage::cache_key(&edited).unwrap();
    assert!(
        engine
            .storage
            .cached(new_request_key)
            .await
            .unwrap()
            .is_none(),
        "An old translation must not become a cache hit for edited source text"
    );
    assert_eq!(
        engine
            .storage
            .task(edited.id)
            .await
            .unwrap()
            .unwrap()
            .translation,
        original.translation,
        "The stale result remains available for comparison"
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn source_language_change_does_not_cache_the_previous_language_result() {
    let (_directory, engine) = engine().await;
    let original = completed_parent(&engine).await;
    let edited = engine
        .source_update(UpdateRequest {
            source: SourceRequest {
                task_id: original.id.clone(),
                expected_revision: original.source_revision,
            },
            text: original.query_text().into(),
            source_language: Some("en".into()),
            target_language: Some("ja".into()),
        })
        .await
        .unwrap();
    assert_eq!(edited.query_text(), original.query_text());
    assert!(
        engine
            .storage
            .cached(storage::cache_key(&edited).unwrap())
            .await
            .unwrap()
            .is_none()
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn previously_polluted_cache_rows_are_rejected_and_do_not_hide_valid_ones() {
    let (directory, engine) = engine().await;
    let profile = engine.configuration().await.settings.profiles[0].clone();
    let valid = completed(profile.clone(), "A new source.", "新的译文", 100);
    let mut stale = completed(profile, "An old source.", "旧的译文", 200);
    stale.edited_text = Some(valid.query_text().into());
    stale.source_revision += 1;
    let key = storage::cache_key(&stale).unwrap();
    assert_eq!(storage::cache_key(&valid).as_ref(), Some(&key));

    // Reproduce a row written before the fix, including its incorrect cache key.
    engine.storage.save_task(stale.clone()).await.unwrap();
    engine.storage.flush().await.unwrap();
    let connection = rusqlite::Connection::open(directory.path().join("history.sqlite3")).unwrap();
    connection
        .execute(
            "UPDATE records SET cache_key=?1 WHERE id=?2",
            rusqlite::params![key, stale.id],
        )
        .unwrap();
    drop(connection);
    assert!(engine.storage.cached(key.clone()).await.unwrap().is_none());

    engine.storage.save_task(valid.clone()).await.unwrap();
    let found = engine.storage.cached(key).await.unwrap().unwrap();
    assert_eq!(found.id, valid.id);
    assert_eq!(found.translation, valid.translation);
    engine.shutdown().await;
}

#[tokio::test]
async fn unmodified_legacy_cache_survives_but_an_edited_legacy_result_is_not_reused() {
    let (_directory, engine) = engine().await;
    let profile = engine.configuration().await.settings.profiles[0].clone();
    let mut value =
        serde_json::to_value(completed(profile, "Legacy source.", "旧版译文", 100)).unwrap();
    for field in [
        "source_revision",
        "source_segments",
        "source_history",
        "translation_source_revision",
        "translation_source_text",
    ] {
        value.as_object_mut().unwrap().remove(field);
    }
    let legacy: Task = serde_json::from_value(value).unwrap();
    assert_eq!(legacy.source_revision, 0);
    assert!(legacy.translation_source_revision.is_none());
    engine.storage.save_task(legacy.clone()).await.unwrap();
    assert_eq!(
        engine
            .storage
            .cached(storage::cache_key(&legacy).unwrap())
            .await
            .unwrap()
            .unwrap()
            .translation,
        legacy.translation
    );

    engine.book.restore(vec![legacy.clone()], engine.now());
    let edited = engine
        .source_update(UpdateRequest {
            source: SourceRequest {
                task_id: legacy.id,
                expected_revision: 0,
            },
            text: "Edited legacy source.".into(),
            source_language: None,
            target_language: None,
        })
        .await
        .unwrap();
    assert!(
        engine
            .storage
            .cached(storage::cache_key(&edited).unwrap())
            .await
            .unwrap()
            .is_none()
    );
    engine.shutdown().await;
}

async fn saved_reply(engine: &Engine, parent: Option<&Task>) -> SavedOperation {
    let id = uuid::Uuid::new_v4().to_string();
    let request_id = uuid::Uuid::new_v4().to_string();
    let profile = engine.configuration().await.settings.profiles[0].clone();
    let saved: SavedOperation = serde_json::from_value(serde_json::json!({
        "record": {
            "id": id, "request_id": request_id, "task_id": parent.map(|p| &p.id),
            "source_revision": parent.map_or(0, |p| p.source_revision),
            "kind": "reply", "status": "completed", "primary_text": "Synthetic reply",
            "back_translation": "", "back_translation_state": "none", "profile": profile,
            "attempt": 1, "attempts": [], "error": null, "sensitive": null,
            "output_language": "en", "reference_language": "zh-Hans",
            "created_at": 1000, "updated_at": 1000, "expires_at": null, "revision": 1,
            "memory_only": false, "save_error": null
        },
        "request": {
            "request_id": request_id, "kind": "reply", "task_id": parent.map(|p| &p.id),
            "source_text": "Synthetic source", "source_language": "en",
            "target_language": "zh-Hans", "intent": "Acknowledge this",
            "include_back_translation": false
        },
        "past_requests": []
    }))
    .unwrap();
    engine.storage.save_operation(saved.clone()).await.unwrap();
    engine.operations.restore(vec![saved.clone()]);
    saved
}

async fn pending_list(
    engine: &Engine,
) -> std::pin::Pin<Box<impl Future<Output = Result<Vec<OperationRecord>, String>> + '_>> {
    let mut listing = Box::pin(engine.operation_list(None));
    let mut context = Context::from_waker(Waker::noop());
    assert!(matches!(listing.as_mut().poll(&mut context), Poll::Pending));
    // The worker has completed the read; its response has not been consumed yet.
    engine.storage.flush().await.unwrap();
    listing
}

async fn complete_read_and_mutation(
    listing: impl Future<Output = Result<Vec<OperationRecord>, String>>,
    mutation: impl Future<Output = Result<(), String>>,
) {
    let mut mutation = Box::pin(mutation);
    // Let an unsynchronized mutation finish before resuming the old read. A
    // synchronized implementation may instead wait for that read to complete.
    let early = tokio::time::timeout(Duration::from_secs(1), mutation.as_mut()).await;
    listing.await.unwrap();
    match early {
        Ok(result) => result.unwrap(),
        Err(_) => mutation.await.unwrap(),
    }
}

async fn assert_operation_removed(engine: &Engine, saved: &SavedOperation) {
    assert!(engine.operations.get(&saved.record.id).is_none());
    assert!(
        engine
            .storage
            .operation_history(None)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(engine.operation_list(None).await.unwrap().is_empty());
    // The disk tombstone also continues to reject callbacks holding old rows.
    assert_eq!(
        engine.storage.save_operation(saved.clone()).await.unwrap(),
        storage::SaveOutcome::Dropped
    );
}

#[tokio::test]
async fn operation_read_cannot_restore_history_after_clear() {
    let (_directory, engine) = engine().await;
    let saved = saved_reply(&engine, None).await;
    let listing = pending_list(&engine).await;
    complete_read_and_mutation(listing, engine.clear()).await;
    assert_operation_removed(&engine, &saved).await;
    engine.shutdown().await;
}

#[tokio::test]
async fn operation_read_cannot_restore_history_after_parent_deletion() {
    let (_directory, engine) = engine().await;
    let parent = completed_parent(&engine).await;
    let saved = saved_reply(&engine, Some(&parent)).await;
    let listing = pending_list(&engine).await;
    complete_read_and_mutation(listing, engine.remove(&parent.id)).await;
    assert_operation_removed(&engine, &saved).await;
    engine.shutdown().await;
}

#[tokio::test]
async fn operation_read_cannot_restore_persisted_history_after_saving_is_disabled() {
    let (_directory, engine) = engine().await;
    let saved = saved_reply(&engine, None).await;
    // Historical rows need not already be loaded into the session hub.
    engine.operations.clear();
    let mut settings = engine.configuration().await.settings;
    settings.retention = Retention::Disabled;
    let listing = pending_list(&engine).await;
    complete_read_and_mutation(listing, async {
        engine
            .save_configuration(settings, BTreeMap::new())
            .await
            .map(|_| ())
    })
    .await;
    assert!(
        engine
            .storage
            .operation_history(None)
            .await
            .unwrap()
            .is_empty()
    );
    let restored = engine.operations.get(&saved.record.id);
    assert!(
        restored.is_none_or(|record| record.memory_only),
        "A late read must not restore a persistent record after history is disabled"
    );
    engine.shutdown().await;
}
