//! Additive source schema and owned image files; legacy `{task}.png` is untouched.
use super::*;
use crate::source::SourceSegment;
use std::collections::HashSet;

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS source_revisions(task_id TEXT NOT NULL REFERENCES records(id) ON DELETE CASCADE,source_revision INTEGER NOT NULL,data TEXT NOT NULL,PRIMARY KEY(task_id,source_revision)); CREATE TABLE IF NOT EXISTS source_assets(asset_id TEXT PRIMARY KEY,task_id TEXT NOT NULL REFERENCES records(id) ON DELETE CASCADE,segment_id TEXT NOT NULL,path TEXT NOT NULL); CREATE TABLE IF NOT EXISTS source_asset_cleanup(path TEXT PRIMARY KEY);").map_err(db_error)
}
pub(super) fn save(connection: &Connection, task: &Task) -> Result<(), String> {
    let mut retained = task.source_history.clone();
    retained.push(task.source_snapshot());
    for snapshot in &retained {
        connection.execute("INSERT OR REPLACE INTO source_revisions(task_id,source_revision,data) VALUES(?1,?2,?3)",params![task.id,snapshot.source_revision,serde_json::to_string(snapshot).map_err(db_error)?]).map_err(db_error)?;
    }
    let mut versions: Vec<_> = retained.iter().map(|s| s.source_revision).collect();
    // Completed and partial current results can outlive the bounded draft
    // history. A draft without output has no result whose source needs pinning.
    if !task.translation.is_empty() {
        versions.extend(task.translation_source_revision);
    }
    versions.extend(task.past_attempts.iter().filter_map(|a| a.source_revision));
    // Assistant retries can retain a source version beyond the bounded draft
    // history. Keep both their current snapshot and every archived attempt.
    {
        let mut query=connection.prepare(
            "SELECT json_extract(data,'$.record.source_revision') FROM assistant_operations
             WHERE task_id=?1 AND json_type(data,'$.record.source_revision')='integer'
             UNION
             SELECT json_extract(attempt.value,'$.source_revision')
             FROM assistant_operations AS operation,json_each(operation.data,'$.record.attempts') AS attempt
             WHERE operation.task_id=?1 AND json_type(attempt.value,'$.source_revision')='integer'"
        ).map_err(db_error)?;
        let referenced = query
            .query_map([&task.id], |row| row.get::<_, u64>(0))
            .map_err(db_error)?;
        versions.extend(
            referenced
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(db_error)?,
        );
    }
    let mut query = connection
        .prepare("SELECT source_revision FROM source_revisions WHERE task_id=?1")
        .map_err(db_error)?;
    let obsolete = query
        .query_map([&task.id], |r| r.get::<_, u64>(0))
        .map_err(db_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(db_error)?;
    drop(query);
    for version in obsolete {
        if !versions.contains(&version) {
            connection
                .execute(
                    "DELETE FROM source_revisions WHERE task_id=?1 AND source_revision=?2",
                    params![task.id, version],
                )
                .map_err(db_error)?;
        }
    }
    let retained_snapshots = {
        let mut query = connection
            .prepare("SELECT data FROM source_revisions WHERE task_id=?1")
            .map_err(db_error)?;
        let rows = query
            .query_map([&task.id], |row| row.get::<_, String>(0))
            .map_err(db_error)?;
        rows.map(|row| {
            serde_json::from_str::<crate::source::SourceSnapshot>(&row.map_err(db_error)?)
                .map_err(db_error)
        })
        .collect::<Result<Vec<_>, String>>()?
    };
    let segments: Vec<&SourceSegment> = task
        .source_segments
        .iter()
        .chain(task.source_history.iter().flat_map(|s| s.segments.iter()))
        .chain(task.source_undo.iter().flat_map(|s| s.segments.iter()))
        .chain(retained_snapshots.iter().flat_map(|s| s.segments.iter()))
        .collect();
    let mut ids = HashSet::new();
    for segment in segments {
        if let Some(asset) = &segment.asset_id {
            checked_id(asset)?;
            ids.insert(asset.clone());
            let path = asset_name(&task.id, asset)?;
            connection.execute("INSERT INTO source_assets(asset_id,task_id,segment_id,path) VALUES(?1,?2,?3,?4) ON CONFLICT(asset_id) DO UPDATE SET segment_id=excluded.segment_id WHERE source_assets.task_id=excluded.task_id",params![asset,task.id,segment.id,path]).map_err(db_error)?;
        }
    }
    let mut query = connection
        .prepare("SELECT asset_id,path FROM source_assets WHERE task_id=?1")
        .map_err(db_error)?;
    let rows = query
        .query_map([&task.id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(db_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(db_error)?;
    drop(query);
    for (asset, path) in rows {
        if !ids.contains(asset.as_str()) {
            connection
                .execute(
                    "INSERT OR IGNORE INTO source_asset_cleanup(path) VALUES(?1)",
                    [path],
                )
                .map_err(db_error)?;
            connection
                .execute("DELETE FROM source_assets WHERE asset_id=?1", [asset])
                .map_err(db_error)?;
        }
    }
    Ok(())
}
pub(super) fn queue_removal(connection: &Connection, id: &str) -> Result<(), String> {
    connection.execute("INSERT OR IGNORE INTO source_asset_cleanup SELECT path FROM source_assets WHERE task_id=?1",[id]).map_err(db_error)?;
    Ok(())
}
pub(super) fn asset_name(task: &str, asset: &str) -> Result<String, String> {
    checked_id(task)?;
    checked_id(asset)?;
    Ok(if task == asset {
        format!("{task}.png")
    } else {
        format!("{task}--{asset}.png")
    })
}
pub(super) fn clean_files(db: &mut Database) {
    let paths = (|| -> rusqlite::Result<Vec<String>> {
        let mut stmt = db
            .connection
            .prepare("SELECT path FROM source_asset_cleanup LIMIT 128")?;
        stmt.query_map([], |r| r.get(0))?.collect()
    })()
    .unwrap_or_default();
    for name in paths {
        if !safe_asset_name(&name) {
            continue;
        }
        let path = db.root.join("images").join(&name);
        if fs::remove_file(path).is_ok_and(|_| true) || !db.root.join("images").join(&name).exists()
        {
            let _ = db
                .connection
                .execute("DELETE FROM source_asset_cleanup WHERE path=?1", [name]);
        }
    }
}
fn safe_asset_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".png") else {
        return false;
    };
    let ids: Vec<_> = stem.split("--").collect();
    (ids.len() == 1 || ids.len() == 2) && ids.iter().all(|id| checked_id(id).is_ok())
}
pub(super) fn remove_orphans(db: &Database) -> Result<(), String> {
    for entry in fs::read_dir(db.root.join("images")).map_err(db_error)? {
        let entry = entry.map_err(db_error)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.contains("--")
            || !safe_asset_name(name)
            || !entry.file_type().map_err(db_error)?.is_file()
        {
            continue;
        }
        let referenced: bool = db
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM source_assets WHERE path=?1)",
                [name],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if !referenced {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}
impl Storage {
    pub async fn save_source_asset(
        &self,
        task_id: String,
        asset_id: String,
        bytes: Arc<Vec<u8>>,
    ) -> Result<bool, String> {
        self.call(move |db| {
            crate::source::validate_png(&bytes)?;
            if db.settings.retention == Retention::Disabled
                || db.tombstoned(&task_id)?
                || db.task(&task_id)?.is_none()
            {
                return Ok(false);
            }
            let name = asset_name(&task_id, &asset_id)?;
            let exists: bool = db
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM source_assets WHERE task_id=?1 AND asset_id=?2)",
                    params![task_id, asset_id],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            if !exists {
                return Ok(false);
            }
            write_atomic(&db.root.join("images").join(name), &bytes)?;
            Ok(true)
        })
        .await
    }
    pub async fn source_asset(
        &self,
        task_id: String,
        asset_id: String,
    ) -> Result<Option<Vec<u8>>, String> {
        self.call(move |db| {
            if db.task(&task_id)?.is_none() {
                return Ok(None);
            }
            let path = asset_name(&task_id, &asset_id)?;
            let allowed: bool = db
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM source_assets WHERE task_id=?1 AND asset_id=?2)",
                    params![task_id, asset_id],
                    |r| r.get(0),
                )
                .map_err(db_error)?;
            if !allowed && task_id != asset_id {
                return Ok(None);
            }
            let file = db.root.join("images").join(path);
            match fs::metadata(&file) {
                Ok(meta) if meta.len() <= 32 * 1024 * 1024 => {
                    fs::read(file).map(Some).map_err(db_error)
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                _ => Err("截图文件无效或过大。".into()),
            }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::encounters::{lookup_keys, matches, token_ranges};

    async fn stored_source_versions(storage: &Storage, task_id: &str) -> Vec<u64> {
        let task_id = task_id.to_owned();
        storage
            .call(move |db| {
                let mut query = db
                    .connection
                    .prepare("SELECT source_revision FROM source_revisions WHERE task_id=?1")
                    .map_err(db_error)?;
                query
                    .query_map([task_id], |row| row.get::<_, u64>(0))
                    .map_err(db_error)?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(db_error)
            })
            .await
            .unwrap()
    }

    async fn current_translation_asset_retention(status: Status) {
        use crate::{
            config::Profile,
            tasks::{Source, TaskBook},
        };

        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().into(), Arc::new(|| 100))
            .await
            .unwrap();
        let book = TaskBook::new(Retention::Forever, 12);
        let initial = book
            .create(Source::Selection, Profile::default(), "zh-Hans".into(), 0)
            .unwrap();
        book.recognized(&initial.id, "Synthetic initial selection.".into(), 0, 1)
            .unwrap();
        let initial = book.ready(&initial.id, Some("en".into()), 2).unwrap();
        let asset = uuid::Uuid::new_v4().to_string();
        let mut appended = initial.source_snapshot();
        appended
            .original_text
            .push_str("\n\nSynthetic screenshot text.");
        appended.segments.push(SourceSegment {
            id: uuid::Uuid::new_v4().to_string(),
            kind: Source::Screenshot,
            original_text: "Synthetic screenshot text.".into(),
            captured_at: 3,
            asset_id: Some(asset.clone()),
            ocr_layout: None,
        });
        let appended = book
            .apply_source(
                &initial.id,
                initial.source_revision,
                appended,
                true,
                false,
                3,
            )
            .unwrap();
        storage.save_task(appended.clone()).await.unwrap();
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(2u32.to_be_bytes());
        png.extend(2u32.to_be_bytes());
        assert!(
            storage
                .save_source_asset(appended.id.clone(), asset.clone(), Arc::new(png))
                .await
                .unwrap()
        );

        book.submit_ready(&appended.id, None, 4).unwrap().unwrap();
        book.start(&appended.id, 1, 0, 5).unwrap();
        book.append(&appended.id, 1, "Synthetic retained translation.", 0, 6)
            .unwrap();
        let translated = match status {
            Status::Completed => book.finish(&appended.id, 1, 0, 7),
            Status::Failed => book.fail(&appended.id, 1, "Synthetic failure.".into(), 7),
            Status::Stopped => book.stop(&appended.id, 7),
            _ => unreachable!(),
        }
        .unwrap();
        let translated_revision = translated.translation_source_revision.unwrap();
        storage.save_task(translated.clone()).await.unwrap();

        let mut current = translated;
        for step in 0..13 {
            let mut snapshot = current.source_snapshot();
            snapshot.original_text = initial.original_text.clone();
            snapshot.segments = initial.source_segments.clone();
            snapshot.edited_text = Some(format!("Synthetic edited selection {step}."));
            current = book
                .apply_source(
                    &current.id,
                    current.source_revision,
                    snapshot,
                    true,
                    false,
                    8 + step,
                )
                .unwrap();
            storage.save_task(current.clone()).await.unwrap();
        }
        assert_eq!(current.status, status);
        assert_eq!(
            current.translation_source_revision,
            Some(translated_revision)
        );
        assert!(!current.translation.is_empty());
        assert!(
            current
                .source_history
                .iter()
                .chain(&current.source_undo)
                .all(|snapshot| snapshot.source_revision > translated_revision)
        );
        let retained = stored_source_versions(&storage, &current.id).await;
        assert!(
            retained.contains(&translated_revision),
            "{status:?} translation lost its source version"
        );
        assert!(retained.len() <= 8);
        assert!(
            storage
                .source_asset(current.id.clone(), asset.clone())
                .await
                .unwrap()
                .is_some()
        );

        // Reopening drops the in-memory undo stack, while a retained result still
        // needs the same source and image. A new attempt then archives that result.
        let restored = Storage::open(dir.path().into(), Arc::new(|| 100))
            .await
            .unwrap();
        let reopened = restored.task(current.id.clone()).await.unwrap().unwrap();
        assert!(reopened.source_undo.is_empty());
        restored.save_task(reopened).await.unwrap();
        assert!(
            stored_source_versions(&restored, &current.id)
                .await
                .contains(&translated_revision)
        );
        assert!(
            restored
                .source_asset(current.id.clone(), asset.clone())
                .await
                .unwrap()
                .is_some()
        );

        for retry in 0..6 {
            let queued = book.retry(&current.id, None, 30 + retry).unwrap().unwrap();
            book.start(&current.id, queued.attempt, 0, 30 + retry)
                .unwrap();
            book.append(
                &current.id,
                queued.attempt,
                "Synthetic newer translation.",
                0,
                30 + retry,
            )
            .unwrap();
            current = book
                .finish(&current.id, queued.attempt, 0, 30 + retry)
                .unwrap();
            restored.save_task(current.clone()).await.unwrap();
            let referenced = retry < 5;
            assert_eq!(
                stored_source_versions(&restored, &current.id)
                    .await
                    .contains(&translated_revision),
                referenced
            );
            assert_eq!(
                restored
                    .source_asset(current.id.clone(), asset.clone())
                    .await
                    .unwrap()
                    .is_some(),
                referenced
            );
        }
        assert_eq!(current.past_attempts.len(), 5);
        assert!(stored_source_versions(&restored, &current.id).await.len() <= 7);
        assert_eq!(
            std::fs::read_dir(dir.path().join("images"))
                .unwrap()
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn completed_translation_retains_source_asset_until_its_attempt_is_evicted() {
        current_translation_asset_retention(Status::Completed).await;
    }

    #[tokio::test]
    async fn failed_partial_translation_retains_source_asset_until_its_attempt_is_evicted() {
        current_translation_asset_retention(Status::Failed).await;
    }

    #[tokio::test]
    async fn stopped_partial_translation_retains_source_asset_until_its_attempt_is_evicted() {
        current_translation_asset_retention(Status::Stopped).await;
    }

    #[tokio::test]
    async fn indexed_encounters_with_ten_thousand_saved_entries() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().into(), Arc::new(|| 100))
            .await
            .unwrap();
        storage
            .call(|db| {
                let tx = db.connection.transaction().map_err(db_error)?;
                {
                    let mut insert = tx
                        .prepare(
                            "INSERT INTO vocabulary(id,word,updated_at,data) VALUES(?1,?1,100,?2)",
                        )
                        .map_err(db_error)?;
                    for i in 0..10000 {
                        let word = format!("term{i:05}");
                        let item = crate::vocabulary::VocabularyItem {
                            id: word.clone(),
                            word: word.clone(),
                            phonetic: String::new(),
                            pos: String::new(),
                            dict_meaning: String::new(),
                            note: String::new(),
                            difficulty: String::new(),
                            status: if i < 2000 { "learning" } else { "mastered" }.into(),
                            review_count: 7,
                            contexts: vec![],
                            created_at: 100,
                            updated_at: 100,
                        };
                        insert
                            .execute(params![
                                word,
                                serde_json::to_string(&item).map_err(db_error)?
                            ])
                            .map_err(db_error)?;
                    }
                }
                tx.commit().map_err(db_error)?;
                Ok(())
            })
            .await
            .unwrap();
        for count in [2000, 4999] {
            let text = (0..count)
                .map(|i| format!("term{:05} ", i % 2000))
                .collect::<String>();
            assert!(text.chars().count() <= 50000);
            let start = std::time::Instant::now();
            let tokens = token_ranges(&text, |word| word.into());
            let keys = lookup_keys(&text, &tokens);
            let item_count = keys.len();
            let items = storage.vocabulary_lookup(keys).await.unwrap();
            let matched = matches(&text, &tokens, &items);
            assert_eq!(matched.len(), count);
            assert_eq!(items.len(), 2000);
            assert!(items.iter().all(|i| i.review_count == 7));
            eprintln!(
                "indexed SQLite encounter benchmark: {} chars, {} tokens, 10000 stored entries, {} lookup keys, {:?}",
                text.len(),
                tokens.len(),
                item_count,
                start.elapsed()
            );
        }
    }
    #[tokio::test]
    async fn future_schema_is_rejected_without_rewriting_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.sqlite3");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("PRAGMA user_version=999; CREATE TABLE future_data(value TEXT); INSERT INTO future_data VALUES('synthetic')").unwrap();
        drop(connection);
        assert!(
            Storage::open(dir.path().into(), Arc::new(|| 100))
                .await
                .is_err()
        );
        let connection = Connection::open(path).unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            999
        );
        assert_eq!(
            connection
                .query_row("SELECT value FROM future_data", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "synthetic"
        );
    }
    #[tokio::test]
    async fn assistant_current_and_attempt_sources_keep_old_assets_until_operation_is_removed() {
        use crate::{
            config::Profile,
            operations::{
                BackTranslationState, ContextScope, OperationAttempt, OperationKind,
                OperationRecord, OperationRequest, OperationStatus, ReplyTone, SavedOperation,
            },
            tasks::{Source, TaskBook},
        };
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path().into(), Arc::new(|| 100))
            .await
            .unwrap();
        let book = TaskBook::new(Retention::Forever, 12);
        let initial = book
            .create(Source::Screenshot, Profile::default(), "zh-Hans".into(), 0)
            .unwrap();
        book.recognized(&initial.id, "First screenshot source.".into(), 0, 1)
            .unwrap();
        let first = book.ready(&initial.id, Some("en".into()), 2).unwrap();
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(2u32.to_be_bytes());
        png.extend(2u32.to_be_bytes());
        let png = Arc::new(png);
        storage.save_task(first.clone()).await.unwrap();
        storage
            .save_image(first.id.clone(), png.clone())
            .await
            .unwrap();
        let asset = uuid::Uuid::new_v4().to_string();
        let mut second = first.source_snapshot();
        second.original_text = "Second screenshot source.".into();
        second.segments = vec![SourceSegment {
            id: uuid::Uuid::new_v4().to_string(),
            kind: Source::Screenshot,
            original_text: second.original_text.clone(),
            captured_at: 3,
            asset_id: Some(asset.clone()),
            ocr_layout: None,
        }];
        let second = book
            .apply_source(&first.id, first.source_revision, second, false, false, 3)
            .unwrap();
        storage.save_task(second.clone()).await.unwrap();
        assert!(
            storage
                .save_source_asset(second.id.clone(), asset.clone(), png)
                .await
                .unwrap()
        );
        let operation_id = uuid::Uuid::new_v4().to_string();
        let request_id = uuid::Uuid::new_v4().to_string();
        let request = OperationRequest {
            request_id: request_id.clone(),
            kind: OperationKind::Reply,
            source_text: second.original_text.clone(),
            task_id: Some(second.id.clone()),
            source_revision: Some(second.source_revision),
            profile_id: None,
            source_language: Some("en".into()),
            target_language: "zh-Hans".into(),
            selection_start: None,
            selection_end: None,
            context_scope: ContextScope::Paragraph,
            intent: "Ask for more details.".into(),
            tone: ReplyTone::Default,
            reply_language: Some("en".into()),
            include_back_translation: false,
            previous_operation_id: None,
        };
        let record = OperationRecord {
            id: operation_id.clone(),
            request_id,
            task_id: Some(second.id.clone()),
            source_revision: second.source_revision,
            kind: OperationKind::Reply,
            status: OperationStatus::Completed,
            primary_text: "Please share more details.".into(),
            back_translation: String::new(),
            back_translation_state: BackTranslationState::None,
            profile: Profile::default(),
            attempt: 2,
            attempts: vec![OperationAttempt {
                number: 1,
                profile: Profile::default(),
                status: OperationStatus::Completed,
                error: None,
                failure: None,
                reason: "manual_retry".into(),
                partial_text: "Could you elaborate?".into(),
                source_revision: first.source_revision,
                back_translation: String::new(),
            }],
            error: None,
            failure: None,
            sensitive: None,
            output_language: "en".into(),
            reference_language: "zh-Hans".into(),
            created_at: 4,
            updated_at: 4,
            expires_at: None,
            revision: 1,
            memory_only: false,
            save_error: None,
        };
        assert_eq!(
            storage
                .save_operation(SavedOperation {
                    record,
                    request,
                    past_requests: vec![]
                })
                .await
                .unwrap(),
            SaveOutcome::Saved
        );
        let mut current = second.clone();
        for step in 0..12 {
            let mut snapshot = current.source_snapshot();
            snapshot.original_text = format!("Replacement source version {step}.");
            snapshot.segments = vec![SourceSegment {
                id: uuid::Uuid::new_v4().to_string(),
                kind: Source::Manual,
                original_text: snapshot.original_text.clone(),
                captured_at: 5 + step,
                asset_id: None,
                ocr_layout: None,
            }];
            current = book
                .apply_source(
                    &current.id,
                    current.source_revision,
                    snapshot,
                    false,
                    false,
                    5 + step,
                )
                .unwrap();
            storage.save_task(current.clone()).await.unwrap();
        }
        assert!(
            current
                .source_history
                .iter()
                .all(|snapshot| snapshot.source_revision > second.source_revision)
        );
        let task_id = current.id.clone();
        let versions = [first.source_revision, second.source_revision];
        let retained = storage
            .call(move |db| {
                let mut query = db
                    .connection
                    .prepare("SELECT source_revision FROM source_revisions WHERE task_id=?1")
                    .map_err(db_error)?;
                query
                    .query_map([task_id], |row| row.get::<_, u64>(0))
                    .map_err(db_error)?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(db_error)
            })
            .await
            .unwrap();
        assert!(versions.iter().all(|revision| retained.contains(revision)));
        assert!(
            storage
                .source_asset(current.id.clone(), first.id.clone())
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            storage
                .source_asset(current.id.clone(), asset.clone())
                .await
                .unwrap()
                .is_some()
        );
        storage.remove_operation(operation_id).await.unwrap();
        storage.save_task(current.clone()).await.unwrap();
        let task_id = current.id.clone();
        let retained = storage
            .call(move |db| {
                let mut query = db
                    .connection
                    .prepare("SELECT source_revision FROM source_revisions WHERE task_id=?1")
                    .map_err(db_error)?;
                query
                    .query_map([task_id], |row| row.get::<_, u64>(0))
                    .map_err(db_error)?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(db_error)
            })
            .await
            .unwrap();
        assert!(versions.iter().all(|revision| !retained.contains(revision)));
        assert!(
            storage
                .source_asset(current.id.clone(), first.id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            storage
                .source_asset(current.id, asset)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            std::fs::read_dir(dir.path().join("images"))
                .unwrap()
                .count(),
            0
        );
    }
}
