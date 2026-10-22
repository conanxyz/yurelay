//! Synthetic reproduction of the actual history list database/serialization path.
use super::*;
#[tokio::test]
async fn history_list_payload_does_not_scale_with_unopened_document_versions() {
    use crate::{
        config::Profile,
        source::{SourceSegment, SourceSnapshot},
        tasks::{PastAttempt, Source, TaskBook, Timings},
    };
    let root = tempfile::tempdir().unwrap();
    let storage = Storage::open(root.path().into(), Arc::new(|| 1000))
        .await
        .unwrap();
    let book = TaskBook::new(Retention::Forever, 12);
    let initial = book
        .create(Source::Selection, Profile::default(), "zh-Hans".into(), 0)
        .unwrap();
    book.recognized(&initial.id, "Synthetic baseline source.".into(), 0, 1)
        .unwrap();
    let mut template = book.ready(&initial.id, Some("en".into()), 2).unwrap();
    let source = "Synthetic history source with distinct ordinary prose. ".repeat(850);
    let translated = "A synthetic translation paragraph.\n\n".repeat(1600);
    template.original_text = source.clone();
    template.edited_text = Some(source.clone());
    template.translation = translated.clone();
    template.status = Status::Completed;
    template.source_segments = vec![SourceSegment {
        id: uuid::Uuid::new_v4().to_string(),
        kind: Source::Selection,
        original_text: source.clone(),
        captured_at: 0,
        asset_id: None,
        ocr_layout: None,
    }];
    template.source_history = (0..6)
        .map(|version| SourceSnapshot {
            source_revision: version,
            original_text: source.clone(),
            edited_text: Some(source.clone()),
            source_language: Some("en".into()),
            target_language: "zh-Hans".into(),
            segments: template.source_segments.clone(),
        })
        .collect();
    template.past_attempts = (0..5)
        .map(|number| PastAttempt {
            number,
            source_text: Some(source.clone()),
            source_revision: Some(number as u64),
            translation: translated.clone(),
            profile: Profile::default(),
            status: Status::Completed,
            error: None,
            failure: None,
            reason: "manual_retry".into(),
            timings: Timings::default(),
            preserve_structure: false,
        })
        .collect();
    storage.call(move|db|{let tx=db.connection.transaction().map_err(db_error)?;for at in 0..60{let mut task=template.clone();task.id=uuid::Uuid::new_v4().to_string();task.created_at=at;tx.execute("INSERT INTO records(id,created_at,expires_at,revision,status,data) VALUES(?1,?2,NULL,1,'completed',?3)",params![task.id,at,serde_json::to_string(&task).map_err(db_error)?]).map_err(db_error)?;}tx.commit().map_err(db_error)?;Ok(())}).await.unwrap();
    let start = std::time::Instant::now();
    let list = storage
        .search_history(String::new(), String::new(), 0)
        .await
        .unwrap();
    let db_ms = start.elapsed().as_secs_f64() * 1000.0;
    let serialize_start = std::time::Instant::now();
    let payload = serde_json::to_vec(&list).unwrap();
    let serialize_ms = serialize_start.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "history list baseline: records={} payload_bytes={} db_ms={db_ms:.3} serialize_ms={serialize_ms:.3}",
        list.len(),
        payload.len()
    );
    assert_eq!(list.len(), 30);
    assert!(
        payload.len() < 64 * 1024,
        "history first page carried {} bytes of unopened document data",
        payload.len()
    );
}

#[tokio::test]
async fn ordinary_history_cleanup_preserves_live_records() {
    use crate::{
        config::Profile,
        tasks::{Source, TaskBook},
    };
    let root = tempfile::tempdir().unwrap();
    let storage = Storage::open(root.path().into(), Arc::new(|| 1000))
        .await
        .unwrap();
    let book = TaskBook::new(Retention::Forever, 12);
    let task = book
        .create(Source::Selection, Profile::default(), "zh-Hans".into(), 0)
        .unwrap();
    book.recognized(&task.id, "Synthetic short history entry.".repeat(30), 0, 1)
        .unwrap();
    let mut template = book.ready(&task.id, Some("en".into()), 2).unwrap();
    template.translation = "Synthetic saved translation.".repeat(30);
    template.status = Status::Completed;
    storage.call(move|db|{let tx=db.connection.transaction().map_err(db_error)?;for at in 0..172{let mut task=template.clone();task.id=uuid::Uuid::new_v4().to_string();tx.execute("INSERT INTO records(id,created_at,expires_at,revision,status,data) VALUES(?1,?2,NULL,1,'completed',?3)",params![task.id,at,serde_json::to_string(&task).map_err(db_error)?]).map_err(db_error)?;}tx.commit().map_err(db_error)?;Ok(())}).await.unwrap();
    let start = std::time::Instant::now();
    storage.cleanup().await.unwrap();
    storage.prune_operations().await.unwrap();
    let cleanup = start.elapsed();
    let query = std::time::Instant::now();
    let rows = storage
        .search_history(String::new(), String::new(), 0)
        .await
        .unwrap();
    eprintln!(
        "ordinary history: stored=172 page={} cleanup_ms={:.3} list_ms={:.3}",
        rows.len(),
        cleanup.as_secs_f64() * 1000.0,
        query.elapsed().as_secs_f64() * 1000.0
    );
    assert_eq!(rows.len(), 30);
    assert_eq!(storage.stats().await.unwrap().records, 172);
}
