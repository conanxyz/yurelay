//! Assistant artifacts share the parent history's retention and deletion boundary.
use super::*;
use crate::operations::{MAX_OPERATIONS, OperationStatus, SavedOperation};
pub(crate) const MAX_OPERATION_STORAGE_BYTES: usize = 32 * 1024 * 1024;

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS assistant_operations(
            id TEXT PRIMARY KEY, task_id TEXT REFERENCES records(id) ON DELETE CASCADE,
            created_at INTEGER NOT NULL, expires_at INTEGER, revision INTEGER NOT NULL,
            status TEXT NOT NULL, data TEXT NOT NULL);
         CREATE INDEX IF NOT EXISTS assistant_parent ON assistant_operations(task_id,created_at);",
        )
        .map_err(db_error)
}

/// Run once when opening the process-owned database, never while reading live
/// history: requests from a previous process cannot still be active.
pub(super) fn recover_interrupted(connection: &Connection, now: i64) -> Result<(), String> {
    connection.execute("UPDATE assistant_operations SET status='\"stopped\"',revision=revision+1,data=json_set(data,'$.record.status','stopped','$.record.revision',revision+1,'$.record.updated_at',?1,'$.record.error','应用已重新启动，操作已停止，可手动重新生成。','$.record.back_translation_state',CASE WHEN json_extract(data,'$.record.back_translation_state')='pending' THEN 'cancelled' ELSE json_extract(data,'$.record.back_translation_state') END) WHERE trim(status,'\"') IN ('queued','generating')",[now]).map_err(db_error)?;
    Ok(())
}

impl Storage {
    pub async fn save_operation(&self, mut saved: SavedOperation) -> Result<SaveOutcome, String> {
        self.call(move |db| {
            if db.settings.retention == Retention::Disabled || saved.record.memory_only
                || saved.record.sensitive.is_some() || saved.record.status == OperationStatus::Blocked {
                return Ok(SaveOutcome::MemoryOnly);
            }
            checked_id(&saved.record.id)?;
            if db.tombstoned(&saved.record.id)? { return Ok(SaveOutcome::Dropped); }
            saved.record.expires_at = if let Some(id) = &saved.record.task_id {
                if db.tombstoned(id)? { return Ok(SaveOutcome::Dropped); }
                let Some(parent) = db.task(id)? else { return Ok(SaveOutcome::Dropped); };
                parent.expires_at
            } else { db.settings.retention.expires_at(saved.record.created_at) };
            if saved.record.expires_at.is_some_and(|expiry|expiry <= (db.clock)()) { return Ok(SaveOutcome::Dropped); }
            // A persisted operation is accepted only with structurally valid input
            // and no sensitive material, even when a caller mislabels memory_only.
            saved.request.prepare_for_profile(&saved.record.profile,Some(&saved.record.output_language))?;
            if saved.request.sensitive_kind().is_some() || saved.past_requests.iter().any(|r|r.sensitive_kind().is_some()) { return Ok(SaveOutcome::MemoryOnly); }
            let data=serde_json::to_string(&saved).map_err(db_error)?;
            if data.len()>MAX_OPERATION_STORAGE_BYTES {return Ok(SaveOutcome::MemoryOnly);}
            let status=serde_json::to_string(&saved.record.status).map_err(db_error)?;
            let tx=db.connection.transaction().map_err(db_error)?;
            let existing:Option<u64>=tx.query_row("SELECT revision FROM assistant_operations WHERE id=?1",[&saved.record.id],|row|row.get(0)).optional().map_err(db_error)?;
            if existing.is_some_and(|revision|revision>saved.record.revision){return Ok(SaveOutcome::Dropped);}
            let Some(victims)=capacity_plan(&tx,Some(&saved.record.id),1,data.len())? else {return Ok(SaveOutcome::MemoryOnly);};
            for id in victims {tx.execute("DELETE FROM assistant_operations WHERE id=?1",[id]).map_err(db_error)?;}
            tx.execute(
                "INSERT INTO assistant_operations(id,task_id,created_at,expires_at,revision,status,data) VALUES(?1,?2,?3,?4,?5,?6,?7)
                 ON CONFLICT(id) DO UPDATE SET expires_at=excluded.expires_at,revision=excluded.revision,status=excluded.status,data=excluded.data
                 WHERE excluded.revision>=assistant_operations.revision",
                params![saved.record.id,saved.record.task_id,saved.record.created_at,saved.record.expires_at,saved.record.revision,status,data]
            ).map_err(db_error)?;
            tx.commit().map_err(db_error)?;
            Ok(SaveOutcome::Saved)
        }).await
    }
    pub async fn operation_history(
        &self,
        task_id: Option<String>,
    ) -> Result<Vec<SavedOperation>, String> {
        if let Some(id) = &task_id {
            checked_id(id)?;
        }
        self.call(move|db| {
            db.prune_operations()?;
            let mut statement=db.connection.prepare("SELECT data,expires_at FROM assistant_operations WHERE (?1 IS NULL OR task_id=?1) ORDER BY created_at DESC,id DESC LIMIT ?2").map_err(db_error)?;
            let rows=statement.query_map(params![task_id,MAX_OPERATIONS],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<i64>>(1)?))).map_err(db_error)?;
            rows.map(|row| {
                let (data,expiry)=row.map_err(db_error)?;
                let mut item:SavedOperation=serde_json::from_str(&data).map_err(db_error)?;
                item.record.expires_at=expiry;
                Ok(item)
            }).collect()
        }).await
    }
    pub async fn prune_operations(&self) -> Result<(), String> {
        self.call(|db| db.prune_operations()).await
    }
    pub async fn remove_operation(&self, id: String) -> Result<(), String> {
        checked_id(&id)?;
        self.call(move |db| {
            let tx = db.connection.transaction().map_err(db_error)?;
            tx.execute("INSERT OR IGNORE INTO tombstones VALUES(?1)", [&id])
                .map_err(db_error)?;
            tx.execute("DELETE FROM assistant_operations WHERE id=?1", [id])
                .map_err(db_error)?;
            tx.commit().map_err(db_error)?;
            Ok(())
        })
        .await
    }
    pub async fn clear_operations(&self, inflight_ids: Vec<String>) -> Result<(), String> {
        for id in &inflight_ids {
            checked_id(id)?;
        }
        self.call(move |db| {
            let tx = db.connection.transaction().map_err(db_error)?;
            tx.execute(
                "INSERT OR IGNORE INTO tombstones SELECT id FROM assistant_operations",
                [],
            )
            .map_err(db_error)?;
            for id in inflight_ids {
                tx.execute("INSERT OR IGNORE INTO tombstones VALUES(?1)", [id])
                    .map_err(db_error)?;
            }
            tx.execute("DELETE FROM assistant_operations", [])
                .map_err(db_error)?;
            tx.commit().map_err(db_error)?;
            Ok(())
        })
        .await
    }
}
// Plan eviction first: an oversized active working set must not evict useful
// terminal history and then fail admission. Active rows are never candidates.
fn capacity_plan(
    connection: &Connection,
    excluded: Option<&str>,
    reserve_count: usize,
    reserve_bytes: usize,
) -> Result<Option<Vec<String>>, String> {
    let (count,bytes):(usize,usize)=connection.query_row("SELECT count(*),COALESCE(sum(length(CAST(data AS BLOB))),0) FROM assistant_operations WHERE (?1 IS NULL OR id<>?1)",[excluded],|row|Ok((row.get(0)?,row.get(1)?))).map_err(db_error)?;
    let mut count = count.saturating_add(reserve_count);
    let mut bytes = bytes.saturating_add(reserve_bytes);
    let mut victims = vec![];
    if count <= MAX_OPERATIONS && bytes <= MAX_OPERATION_STORAGE_BYTES {
        return Ok(Some(victims));
    }
    let mut statement=connection.prepare("SELECT id,length(CAST(data AS BLOB)) FROM assistant_operations WHERE (?1 IS NULL OR id<>?1) AND trim(status,'\"') IN ('completed','stopped','failed','blocked') ORDER BY created_at,id").map_err(db_error)?;
    let rows = statement
        .query_map([excluded], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
        })
        .map_err(db_error)?;
    for row in rows {
        let (id, size) = row.map_err(db_error)?;
        victims.push(id);
        count = count.saturating_sub(1);
        bytes = bytes.saturating_sub(size);
        if count <= MAX_OPERATIONS && bytes <= MAX_OPERATION_STORAGE_BYTES {
            return Ok(Some(victims));
        }
    }
    Ok(None)
}
impl Database {
    fn prune_operations(&mut self) -> Result<(), String> {
        if self.settings.retention == Retention::Disabled {
            self.connection
                .execute("DELETE FROM assistant_operations", [])
                .map_err(db_error)?;
            return Ok(());
        }
        self.connection.execute("UPDATE assistant_operations SET expires_at=(SELECT expires_at FROM records WHERE id=assistant_operations.task_id) WHERE task_id IS NOT NULL",[]).map_err(db_error)?;
        match self.settings.retention {
            Retention::Days(days) => {
                self.connection.execute("UPDATE assistant_operations SET expires_at=created_at+?1 WHERE task_id IS NULL",[i64::from(days)*86_400_000]).map_err(db_error)?;
            }
            Retention::Forever => {
                self.connection
                    .execute(
                        "UPDATE assistant_operations SET expires_at=NULL WHERE task_id IS NULL",
                        [],
                    )
                    .map_err(db_error)?;
            }
            Retention::Disabled => {}
        }
        self.connection.execute("DELETE FROM assistant_operations WHERE (expires_at IS NOT NULL AND expires_at<=?1) OR (task_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM records WHERE id=assistant_operations.task_id))",[(self.clock)()]).map_err(db_error)?;
        let tx = self.connection.transaction().map_err(db_error)?;
        if let Some(victims) = capacity_plan(&tx, None, 0, 0)? {
            for id in victims {
                tx.execute("DELETE FROM assistant_operations WHERE id=?1", [id])
                    .map_err(db_error)?;
            }
        }
        tx.commit().map_err(db_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::*;
    fn fixture(at: i64) -> SavedOperation {
        let request = OperationRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            kind: OperationKind::Reply,
            source_text: "Can we discuss the draft?".into(),
            task_id: None,
            source_revision: None,
            profile_id: None,
            source_language: Some("en".into()),
            target_language: "zh-Hans".into(),
            selection_start: None,
            selection_end: None,
            context_scope: ContextScope::Paragraph,
            intent: "请先发送议程".into(),
            tone: ReplyTone::Professional,
            reply_language: Some("en".into()),
            include_back_translation: false,
            previous_operation_id: None,
        };
        let now = at;
        let record = OperationRecord {
            id: uuid::Uuid::new_v4().to_string(),
            request_id: request.request_id.clone(),
            task_id: None,
            source_revision: 0,
            kind: OperationKind::Reply,
            status: OperationStatus::Completed,
            primary_text: "Please send the agenda first.".into(),
            back_translation: String::new(),
            back_translation_state: BackTranslationState::None,
            profile: crate::config::Profile::default(),
            attempt: 1,
            attempts: vec![],
            error: None,
            failure: None,
            sensitive: None,
            output_language: "en".into(),
            reference_language: "zh-Hans".into(),
            created_at: now,
            updated_at: now,
            expires_at: None,
            revision: 1,
            memory_only: false,
            save_error: None,
        };
        SavedOperation {
            record,
            request,
            past_requests: vec![],
        }
    }
    #[tokio::test]
    async fn artifact_reopen_and_disabled_retention_do_not_resurrect_data() {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::open(root.path().into(), system_clock())
            .await
            .unwrap();
        let saved = fixture(system_clock()());
        assert_eq!(
            storage.save_operation(saved.clone()).await.unwrap(),
            SaveOutcome::Saved
        );
        storage.flush().await.unwrap();
        drop(storage);
        let storage = Storage::open(root.path().into(), system_clock())
            .await
            .unwrap();
        assert_eq!(
            storage.operation_history(None).await.unwrap()[0]
                .record
                .primary_text,
            saved.record.primary_text
        );
        let mut settings = storage.settings().await.unwrap();
        settings.retention = Retention::Disabled;
        storage.save_settings(settings, vec![]).await.unwrap();
        assert!(storage.operation_history(None).await.unwrap().is_empty());
        assert_eq!(
            storage.save_operation(saved).await.unwrap(),
            SaveOutcome::MemoryOnly
        );
    }
    #[tokio::test]
    async fn capacity_evicts_oldest_terminal_and_survives_reopen_without_losing_active() {
        let root = tempfile::tempdir().unwrap();
        let clock: Clock = Arc::new(|| 1000);
        let storage = Storage::open(root.path().into(), clock.clone())
            .await
            .unwrap();
        let mut active = fixture(100);
        active.record.status = OperationStatus::Generating;
        storage.save_operation(active.clone()).await.unwrap();
        let mut oldest = None;
        for at in 101..151 {
            let item = fixture(at);
            if oldest.is_none() {
                oldest = Some(item.record.id.clone());
            }
            assert_eq!(
                storage.save_operation(item).await.unwrap(),
                SaveOutcome::Saved
            );
        }
        let history = storage.operation_history(None).await.unwrap();
        assert_eq!(history.len(), MAX_OPERATIONS);
        assert!(history.iter().any(|s| s.record.id == active.record.id));
        assert!(
            !history
                .iter()
                .any(|s| Some(&s.record.id) == oldest.as_ref())
        );
        storage.flush().await.unwrap();
        drop(storage);
        let reopened = Storage::open(root.path().into(), clock).await.unwrap();
        let history = reopened.operation_history(None).await.unwrap();
        assert_eq!(history.len(), MAX_OPERATIONS);
        assert!(history.iter().any(|s| s.record.id == active.record.id));
    }
    #[tokio::test]
    async fn active_capacity_and_single_oversize_remain_memory_only_without_partial_eviction() {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::open(root.path().into(), Arc::new(|| 1000))
            .await
            .unwrap();
        for at in 0..MAX_OPERATIONS {
            let mut item = fixture(at as i64);
            item.record.status = OperationStatus::Queued;
            assert_eq!(
                storage.save_operation(item).await.unwrap(),
                SaveOutcome::Saved
            );
        }
        let extra = fixture(100);
        assert_eq!(
            storage.save_operation(extra).await.unwrap(),
            SaveOutcome::MemoryOnly
        );
        assert_eq!(
            storage.operation_history(None).await.unwrap().len(),
            MAX_OPERATIONS
        );
        let mut oversized = fixture(101);
        oversized.record.primary_text = "x".repeat(MAX_OPERATION_STORAGE_BYTES);
        assert_eq!(
            storage.save_operation(oversized).await.unwrap(),
            SaveOutcome::MemoryOnly
        );
        assert_eq!(
            storage.operation_history(None).await.unwrap().len(),
            MAX_OPERATIONS
        );
    }
    #[tokio::test]
    async fn total_capacity_counts_utf8_bytes_and_stale_updates_cannot_evict() {
        let root = tempfile::tempdir().unwrap();
        let storage = Storage::open(root.path().into(), Arc::new(|| 1000))
            .await
            .unwrap();
        let mut newest = None;
        for at in 0..4 {
            let mut item = fixture(at);
            item.record.primary_text = "界".repeat(3 * 1024 * 1024);
            item.record.revision = 5;
            assert_eq!(
                storage.save_operation(item.clone()).await.unwrap(),
                SaveOutcome::Saved
            );
            newest = Some(item);
        }
        let (count, bytes) = storage
            .call(|db| {
                db.connection
                    .query_row(
                        "SELECT count(*),sum(length(CAST(data AS BLOB))) FROM assistant_operations",
                        [],
                        |row| Ok((row.get::<_, usize>(0)?, row.get::<_, usize>(1)?)),
                    )
                    .map_err(db_error)
            })
            .await
            .unwrap();
        assert_eq!(count, 3);
        assert!(bytes <= MAX_OPERATION_STORAGE_BYTES);
        let mut stale = newest.unwrap();
        stale.record.revision = 4;
        assert_eq!(
            storage.save_operation(stale).await.unwrap(),
            SaveOutcome::Dropped
        );
        assert_eq!(storage.operation_history(None).await.unwrap().len(), 3);
    }
    #[tokio::test]
    async fn interrupted_process_rows_become_stopped_once_and_no_longer_pin_capacity() {
        let root = tempfile::tempdir().unwrap();
        let clock: Clock = Arc::new(|| 1000);
        let storage = Storage::open(root.path().into(), clock.clone())
            .await
            .unwrap();
        let mut first = None;
        for at in 0..MAX_OPERATIONS {
            let mut item = fixture(at as i64);
            item.record.status = OperationStatus::Generating;
            item.record.primary_text = "Preserved partial output".into();
            item.record.back_translation_state = BackTranslationState::Pending;
            if at == 0 {
                first = Some(item.record.id.clone());
            }
            storage.save_operation(item).await.unwrap();
        }
        assert!(
            storage
                .operation_history(None)
                .await
                .unwrap()
                .iter()
                .all(|saved| saved.record.status == OperationStatus::Generating)
        );
        storage.flush().await.unwrap();
        drop(storage);
        let storage = Storage::open(root.path().into(), clock).await.unwrap();
        let restored = storage.operation_history(None).await.unwrap();
        assert_eq!(restored.len(), MAX_OPERATIONS);
        assert!(
            restored
                .iter()
                .all(|saved| saved.record.status == OperationStatus::Stopped
                    && saved.record.back_translation_state == BackTranslationState::Cancelled
                    && saved.record.primary_text == "Preserved partial output")
        );
        let mut new = fixture(500);
        new.record.status = OperationStatus::Generating;
        let id = new.record.id.clone();
        assert_eq!(
            storage.save_operation(new).await.unwrap(),
            SaveOutcome::Saved
        );
        let history = storage.operation_history(None).await.unwrap();
        assert!(
            history
                .iter()
                .any(|s| s.record.id == id && s.record.status == OperationStatus::Generating)
        );
        assert!(!history.iter().any(|s| Some(&s.record.id) == first.as_ref()));
    }
}
