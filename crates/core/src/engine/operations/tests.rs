use super::*;
use crate::operations::*;
use std::sync::atomic::AtomicUsize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

struct EmptyCredentials;
impl Credentials for EmptyCredentials {
    fn exists(&self, _: &str) -> Result<bool, String> {
        Ok(false)
    }
    fn read(&self, _: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn write(&self, _: &str, _: Option<&str>) -> Result<(), String> {
        Ok(())
    }
}
fn request(kind: OperationKind) -> OperationRequest {
    OperationRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        kind,
        source_text: "A patient reader asks a question.".into(),
        task_id: None,
        source_revision: None,
        profile_id: None,
        source_language: Some("en".into()),
        target_language: "zh-Hans".into(),
        selection_start: Some(2),
        selection_end: Some(16),
        context_scope: ContextScope::Paragraph,
        intent: "请先发送议程".into(),
        tone: ReplyTone::Professional,
        reply_language: Some("en".into()),
        include_back_translation: true,
        previous_operation_id: None,
    }
}
async fn fixture(
    root: PathBuf,
    output: &'static str,
    delay: u64,
) -> (Arc<Engine>, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let received = count.clone();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let received = received.clone();
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                loop {
                    let mut chunk = [0u8; 4096];
                    let n = socket.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(boundary) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..boundary]);
                        let size: usize = headers
                            .lines()
                            .find_map(|line| {
                                line.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse().unwrap())
                            })
                            .unwrap();
                        if bytes.len() >= boundary + 4 + size {
                            let body: serde_json::Value =
                                serde_json::from_slice(&bytes[boundary + 4..boundary + 4 + size])
                                    .unwrap();
                            assert!(
                                body["messages"][0]["content"]
                                    .as_str()
                                    .unwrap()
                                    .contains("quoted")
                            );
                            break;
                        }
                    }
                }
                received.fetch_add(1, Ordering::SeqCst);
                if socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").await.is_err(){return;}
                for part in [output, "参考译文。"] {
                    let event = format!(
                        "data: {}\n\n",
                        serde_json::json!({"choices":[{"delta":{"content":part},"finish_reason":null}]})
                    );
                    if socket.write_all(event.as_bytes()).await.is_err() {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
                let end = b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n";
                let _ = socket.write_all(end).await;
            });
        }
    });
    let engine = Engine::open(root, Arc::new(EmptyCredentials), storage::system_clock())
        .await
        .unwrap();
    let mut settings = engine.configuration().await.settings;
    settings.profiles[0].endpoint = format!("http://{address}/v1");
    settings.profiles[0].model = "synthetic-model".into();
    engine
        .save_configuration(settings, BTreeMap::new())
        .await
        .unwrap();
    (engine, count)
}
async fn until(
    engine: &Engine,
    id: &str,
    predicate: impl Fn(&OperationRecord) -> bool,
) -> OperationRecord {
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut events = engine.operations.subscribe();
        loop {
            if let Some(record) = engine.operations.get(id)
                && predicate(&record)
            {
                return record;
            }
            let _ = events.recv().await;
        }
    })
    .await
    .expect("assistant operation did not reach expected state")
}

#[tokio::test]
async fn explanation_is_explicit_deduplicated_and_does_not_replace_main_translation() {
    let root = tempfile::tempdir().unwrap();
    let (engine, count) = fixture(root.path().into(), "A contextual meaning. ", 10).await;
    let handle = engine.begin(Source::Selection).await.unwrap();
    engine.accept_detected_text(
        &handle,
        "A patient reader asks a question.".into(),
        1,
        Some("en".into()),
    );
    let mut input = request(OperationKind::Explain);
    input.task_id = Some(handle.id.clone());
    input.source_revision = Some(engine.book.get(&handle.id).unwrap().source_revision);
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let a = engine.submit_operation(input.clone()).await.unwrap();
    let b = engine.submit_operation(input).await.unwrap();
    assert_eq!(a.id, b.id);
    until(&engine, &a.id, |r| r.status == OperationStatus::Completed).await;
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let parent = engine.book.get(&handle.id).unwrap();
    assert_eq!(parent.status, Status::Ready);
    assert!(parent.translation.is_empty());
    engine.shutdown().await;
}

#[tokio::test]
async fn sensitive_composite_needs_own_consent_and_never_persists() {
    let root = tempfile::tempdir().unwrap();
    let (engine, count) = fixture(
        root.path().into(),
        "A synthetic reply.\n---BACK_TRANSLATION---\n",
        10,
    )
    .await;
    let mut input = request(OperationKind::Reply);
    input.source_text = ["abandon"; 6].join(" ");
    input.intent = ["abandon"; 6].join(" ");
    let blocked = engine.submit_operation(input).await.unwrap();
    assert_eq!(blocked.status, OperationStatus::Blocked);
    assert!(blocked.primary_text.is_empty());
    assert_eq!(count.load(Ordering::SeqCst), 0);
    engine
        .resolve_operation_sensitive(&blocked.id, true)
        .await
        .unwrap();
    let done = until(&engine, &blocked.id, |r| {
        r.status == OperationStatus::Completed
    })
    .await;
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(done.memory_only);
    assert!(
        engine
            .storage
            .operation_history(None)
            .await
            .unwrap()
            .is_empty()
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn reply_body_is_ready_before_reference_and_stop_preserves_it() {
    let root = tempfile::tempdir().unwrap();
    let (engine, count) = fixture(
        root.path().into(),
        "Please send the agenda.\n---BACK_TRANSLATION---\n",
        150,
    )
    .await;
    let started = engine
        .submit_operation(request(OperationKind::Reply))
        .await
        .unwrap();
    let visible = until(&engine, &started.id, |r| {
        r.back_translation_state == BackTranslationState::Pending
    })
    .await;
    assert_eq!(visible.status, OperationStatus::Generating);
    assert_eq!(visible.primary_text, "Please send the agenda.\n");
    assert!(!visible.primary_text.contains(BACK_TRANSLATION_MARKER));
    engine.cancel_operation(&started.id).await.unwrap();
    let stopped = engine.operations.get(&started.id).unwrap();
    assert_eq!(stopped.status, OperationStatus::Stopped);
    assert_eq!(
        stopped.back_translation_state,
        BackTranslationState::Cancelled
    );
    assert_eq!(stopped.primary_text, visible.primary_text);
    assert_eq!(count.load(Ordering::SeqCst), 1);
    engine.shutdown().await;
}

#[tokio::test]
async fn deleting_parent_cancels_child_and_prevents_late_history_writes() {
    let root = tempfile::tempdir().unwrap();
    let (engine, _) = fixture(root.path().into(), "First part. ", 100).await;
    let handle = engine.begin(Source::Selection).await.unwrap();
    engine.accept_detected_text(
        &handle,
        "A patient reader asks a question.".into(),
        1,
        Some("en".into()),
    );
    let mut input = request(OperationKind::Explain);
    input.task_id = Some(handle.id.clone());
    input.source_revision = Some(engine.book.get(&handle.id).unwrap().source_revision);
    let child = engine.submit_operation(input).await.unwrap();
    until(&engine, &child.id, |r| !r.primary_text.is_empty()).await;
    engine.remove(&handle.id).await.unwrap();
    assert!(engine.operations.get(&child.id).is_none());
    assert!(
        engine
            .storage
            .operation_history(Some(handle.id))
            .await
            .unwrap()
            .is_empty()
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn concurrent_sensitive_approval_sends_only_one_request() {
    let root = tempfile::tempdir().unwrap();
    let (engine, count) =
        fixture(root.path().into(), "A reply.\n---BACK_TRANSLATION---\n", 10).await;
    let mut input = request(OperationKind::Reply);
    input.intent = ["abandon"; 12].join(" ");
    let blocked = engine.submit_operation(input).await.unwrap();
    let (a, b) = tokio::join!(
        engine.resolve_operation_sensitive(&blocked.id, true),
        engine.resolve_operation_sensitive(&blocked.id, true)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    until(&engine, &blocked.id, |r| {
        r.status == OperationStatus::Completed
    })
    .await;
    assert_eq!(count.load(Ordering::SeqCst), 1);
    engine.shutdown().await;
}

#[tokio::test]
async fn deletion_queued_before_admission_prevents_late_remote_dispatch() {
    let root = tempfile::tempdir().unwrap();
    let (engine, count) = fixture(root.path().into(), "An explanation. ", 10).await;
    let handle = engine.begin(Source::Selection).await.unwrap();
    engine.accept_detected_text(
        &handle,
        "A patient reader asks a question.".into(),
        1,
        Some("en".into()),
    );
    let mut input = request(OperationKind::Explain);
    input.task_id = Some(handle.id.clone());
    input.source_revision = Some(engine.book.get(&handle.id).unwrap().source_revision);
    let guard = engine.mutations.lock().await;
    let remover = engine.clone();
    let id = handle.id.clone();
    let remove = tokio::spawn(async move { remover.remove(&id).await });
    tokio::task::yield_now().await;
    let sender = engine.clone();
    let submit = tokio::spawn(async move { sender.submit_operation(input).await });
    tokio::task::yield_now().await;
    drop(guard);
    remove.await.unwrap().unwrap();
    assert!(submit.await.unwrap().is_err());
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert!(engine.operations.records(None, engine.now()).is_empty());
    engine.shutdown().await;
}

#[tokio::test]
async fn reenabled_guard_rechecks_queued_sensitive_input() {
    let root = tempfile::tempdir().unwrap();
    let (engine, count) =
        fixture(root.path().into(), "A reply.\n---BACK_TRANSLATION---\n", 10).await;
    let mut settings = engine.configuration().await.settings;
    settings.sensitive_guard = false;
    engine
        .save_configuration(settings, BTreeMap::new())
        .await
        .unwrap();
    let permit = engine.permits.clone().acquire_many_owned(2).await.unwrap();
    let mut input = request(OperationKind::Reply);
    input.intent = ["abandon"; 12].join(" ");
    let operation = engine.submit_operation(input).await.unwrap();
    engine.sensitive_guard.store(true, Ordering::Relaxed);
    drop(permit);
    until(&engine, &operation.id, |r| {
        r.status == OperationStatus::Blocked
    })
    .await;
    assert_eq!(count.load(Ordering::SeqCst), 0);
    engine.shutdown().await;
}

#[tokio::test]
async fn blocked_deadline_cannot_be_extended_by_retention_changes() {
    let root = tempfile::tempdir().unwrap();
    let (engine, count) = fixture(root.path().into(), "A reply. ", 10).await;
    let mut input = request(OperationKind::Reply);
    input.intent = ["abandon"; 12].join(" ");
    let blocked = engine.submit_operation(input).await.unwrap();
    let expiry = blocked.expires_at.unwrap();
    assert!(expiry <= engine.now() + 600_000);
    let mut settings = engine.configuration().await.settings;
    settings.retention = Retention::Forever;
    engine
        .save_configuration(settings, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(
        engine.operations.get(&blocked.id).unwrap().expires_at,
        Some(expiry)
    );
    engine
        .operations
        .change(&blocked.id, 1, engine.now() + 10_000, |_| {});
    engine.cleanup_operations().await.unwrap();
    assert_eq!(
        engine.operations.get(&blocked.id).unwrap().expires_at,
        Some(expiry)
    );
    engine.operations.expire(expiry);
    assert!(
        engine
            .resolve_operation_sensitive(&blocked.id, true)
            .await
            .is_err()
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
    engine.shutdown().await;
}

#[tokio::test]
async fn disabling_history_cancels_queued_operations_without_dispatch() {
    let root = tempfile::tempdir().unwrap();
    let (engine, count) = fixture(root.path().into(), "An explanation. ", 10).await;
    let permit = engine.permits.clone().acquire_many_owned(2).await.unwrap();
    let operation = engine
        .submit_operation(request(OperationKind::Explain))
        .await
        .unwrap();
    let token = engine.operations.token(&operation.id).unwrap();
    let mut settings = engine.configuration().await.settings;
    settings.retention = Retention::Disabled;
    engine
        .save_configuration(settings, BTreeMap::new())
        .await
        .unwrap();
    assert!(token.is_cancelled());
    drop(permit);
    tokio::task::yield_now().await;
    assert!(engine.operations.get(&operation.id).is_none());
    assert!(
        engine
            .storage
            .operation_history(None)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
    engine.shutdown().await;
}

#[tokio::test]
async fn retained_attempts_share_the_memory_budget_with_current_outputs() {
    let root = tempfile::tempdir().unwrap();
    let (engine, _) = fixture(root.path().into(), "An explanation. ", 1).await;
    let operation = engine
        .submit_operation(request(OperationKind::Explain))
        .await
        .unwrap();
    let template = until(&engine, &operation.id, |r| {
        r.status == OperationStatus::Completed
    })
    .await;
    let hub = OperationHub::default();
    let chunk = "x".repeat(1024 * 1024);
    let mut first = None;
    for _ in 0..30 {
        let input = request(OperationKind::Explain);
        let mut record = template.clone();
        record.id = uuid::Uuid::new_v4().to_string();
        record.request_id = input.request_id.clone();
        record.primary_text.clear();
        record.status = OperationStatus::Generating;
        let admission = hub.admit(input, record).unwrap();
        hub.append(&admission.record.id, 1, &chunk, "", false, engine.now())
            .unwrap();
        hub.change(&admission.record.id, 1, engine.now(), |r| {
            r.status = OperationStatus::Completed
        });
        first.get_or_insert(admission.record.id);
    }
    let id = first.unwrap();
    for attempt in 2..=3 {
        let mut input = request(OperationKind::Explain);
        input.previous_operation_id = Some(id.clone());
        let mut record = template.clone();
        record.request_id = input.request_id.clone();
        record.primary_text.clear();
        record.status = OperationStatus::Generating;
        let admission = hub.admit(input, record).unwrap();
        assert_eq!(admission.record.attempt, attempt);
        let result = hub.append(&id, attempt, &chunk, "", false, engine.now());
        if attempt == 2 {
            assert!(result.is_ok());
            hub.change(&id, attempt, engine.now(), |r| {
                r.status = OperationStatus::Completed
            });
        } else {
            assert!(
                result.is_err(),
                "old attempts must count against the aggregate cap"
            );
        }
    }
    engine.shutdown().await;
}

#[tokio::test]
async fn cleared_standalone_operations_cannot_be_resurrected_by_late_saves() {
    let root = tempfile::tempdir().unwrap();
    let (engine, _) = fixture(root.path().into(), "An explanation. ", 1).await;
    let operation = engine
        .submit_operation(request(OperationKind::Explain))
        .await
        .unwrap();
    until(&engine, &operation.id, |r| {
        r.status == OperationStatus::Completed
    })
    .await;
    let saved = engine.operations.saved(&operation.id).unwrap();
    engine.clear().await.unwrap();
    assert_eq!(
        engine.storage.save_operation(saved).await.unwrap(),
        storage::SaveOutcome::Dropped
    );
    assert!(engine.operation_list(None).await.unwrap().is_empty());
    engine.shutdown().await;
}

#[tokio::test]
async fn loading_saved_history_can_replace_terminal_session_entries_at_capacity() {
    let root = tempfile::tempdir().unwrap();
    let (engine, _) = fixture(root.path().into(), "An explanation. ", 1).await;
    let operation = engine
        .submit_operation(request(OperationKind::Explain))
        .await
        .unwrap();
    let template = until(&engine, &operation.id, |r| {
        r.status == OperationStatus::Completed
    })
    .await;
    let saved = engine.operations.saved(&operation.id).unwrap();
    let hub = OperationHub::default();
    for _ in 0..MAX_OPERATIONS {
        let input = request(OperationKind::Explain);
        let mut record = template.clone();
        record.id = uuid::Uuid::new_v4().to_string();
        record.request_id = input.request_id.clone();
        record.memory_only = true;
        hub.admit(input, record).unwrap();
    }
    hub.restore(vec![saved]);
    assert_eq!(hub.records(None, engine.now()).len(), MAX_OPERATIONS);
    assert!(hub.get(&operation.id).is_some());
    engine.shutdown().await;
}

#[tokio::test]
async fn queued_main_translation_precedes_an_earlier_queued_assistant() {
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::open(
        root.path().into(),
        Arc::new(EmptyCredentials),
        storage::system_clock(),
    )
    .await
    .unwrap();
    let held = engine.permits.clone().acquire_many_owned(2).await.unwrap();
    let cancellation = CancellationToken::new();
    let assistant_engine = engine.clone();
    let assistant_cancel = cancellation.clone();
    let assistant = tokio::spawn(async move {
        assistant_engine
            .operation_permit(
                2,
                tokio::time::Instant::now() + Duration::from_secs(2),
                &assistant_cancel,
            )
            .await
    });
    tokio::task::yield_now().await;
    let handle = engine
        .book
        .create(
            Source::Manual,
            Profile::default(),
            "zh-Hans".into(),
            engine.now(),
        )
        .unwrap();
    engine
        .book
        .recognized(
            &handle.id,
            "A queued main translation.".into(),
            0,
            engine.now(),
        )
        .unwrap();
    let main_permits = engine.permits.clone();
    let main = tokio::spawn(async move { main_permits.acquire_many_owned(2).await.unwrap() });
    drop(held);
    let main_permit = tokio::time::timeout(Duration::from_secs(1), main)
        .await
        .expect("main translation must not wait behind the assistant")
        .unwrap();
    assert!(!assistant.is_finished());
    engine.book.start(&handle.id, 1, 0, engine.now());
    drop(main_permit);
    assert!(
        tokio::time::timeout(Duration::from_secs(1), assistant)
            .await
            .unwrap()
            .unwrap()
            .is_ok()
    );
    engine.shutdown().await;
}
