//! One database/file worker, bounded messages, per-record updates and tombstones.

mod history;
#[cfg(test)]
mod history_performance;
mod onboarding;
pub mod operations;
pub mod reliability;
mod source;
pub mod vocabulary;
pub use history::HistorySummary;

use crate::{
    config::{Retention, Settings},
    tasks::{SaveState, Status, Task},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{mpsc, oneshot};

pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;
pub fn system_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(i64::MAX as u128) as i64
    })
}

const STORAGE_ERROR: &str = "无法读写本地数据，请检查磁盘空间和目录权限。";
type Job = Box<dyn FnOnce(&mut Database) + Send>;

#[derive(Clone)]
pub struct Storage {
    jobs: mpsc::Sender<Job>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Stats {
    pub records: u64,
    pub pending_cleanup: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved,
    WaitingForImage,
    PartialImageFailure,
    Dropped,
    MemoryOnly,
}

impl Storage {
    #[cfg(test)]
    pub(crate) async fn reject_writes_for_test(&self) {
        self.call(|db| {
            db.connection
                .execute_batch("PRAGMA query_only=ON;")
                .map_err(db_error)
        })
        .await
        .unwrap();
    }
    pub async fn open(root: PathBuf, clock: Clock) -> Result<Self, String> {
        let (tx, mut rx) = mpsc::channel::<Job>(32);
        let (ready, initialized) = oneshot::channel();
        std::thread::Builder::new()
            .name("yurelay-storage".into())
            .spawn(move || match Database::open(root, clock) {
                Ok(mut db) => {
                    let _ = ready.send(Ok(()));
                    while let Some(job) = rx.blocking_recv() {
                        job(&mut db);
                    }
                    let _ = db
                        .connection
                        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
                }
                Err(error) => {
                    let _ = ready.send(Err(error));
                }
            })
            .map_err(|_| STORAGE_ERROR.to_string())?;
        initialized.await.map_err(|_| STORAGE_ERROR.to_string())??;
        Ok(Self { jobs: tx })
    }

    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Database) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let (tx, rx) = oneshot::channel();
        self.jobs
            .send(Box::new(move |db| {
                let _ = tx.send(f(db));
            }))
            .await
            .map_err(|_| STORAGE_ERROR.to_string())?;
        rx.await.map_err(|_| STORAGE_ERROR.to_string())?
    }

    pub async fn settings(&self) -> Result<Settings, String> {
        self.call(|db| Ok(db.settings.clone())).await
    }
    pub async fn save_settings(
        &self,
        settings: Settings,
        known_ids: Vec<String>,
    ) -> Result<(), String> {
        self.call(move |db| db.save_settings(settings, known_ids))
            .await
    }
    pub async fn save_task(&self, task: Task) -> Result<SaveOutcome, String> {
        self.call(move |db| db.save_task(task)).await
    }
    pub async fn save_image(&self, id: String, bytes: Arc<Vec<u8>>) -> Result<bool, String> {
        self.call(move |db| db.save_image(&id, &bytes)).await
    }
    pub async fn image(&self, id: String) -> Result<Option<Vec<u8>>, String> {
        self.call(move |db| db.image(&id)).await
    }
    pub async fn task(&self, id: String) -> Result<Option<Task>, String> {
        self.call(move |db| db.task(&id)).await
    }
    pub async fn history(&self, limit: usize, offset: usize) -> Result<Vec<Task>, String> {
        self.call(move |db| db.history(limit.min(100), offset))
            .await
    }
    pub async fn cached(&self, key: String) -> Result<Option<Task>, String> {
        self.call(move |db| db.cached(&key)).await
    }
    pub async fn remove(&self, ids: Vec<String>) -> Result<(), String> {
        self.call(move |db| db.remove(&ids)).await
    }
    pub async fn clear(&self, extra_ids: Vec<String>) -> Result<(), String> {
        self.call(move |db| db.clear(extra_ids)).await
    }
    pub async fn cleanup(&self) -> Result<Vec<String>, String> {
        self.call(|db| db.cleanup()).await
    }
    pub async fn stats(&self) -> Result<Stats, String> {
        self.call(|db| {
            let now = (db.clock)();
            Ok(Stats {
                records: db
                    .connection
                    .query_row(
                        "SELECT count(*) FROM records WHERE expires_at IS NULL OR expires_at > ?1",
                        [now],
                        |r| r.get(0),
                    )
                    .map_err(db_error)?,
                pending_cleanup: db
                    .connection
                    .query_row("SELECT count(*) FROM cleanup_jobs", [], |r| r.get(0))
                    .map_err(db_error)?,
            })
        })
        .await
    }
    pub async fn flush(&self) -> Result<(), String> {
        self.call(|db| {
            db.connection
                .execute_batch("PRAGMA wal_checkpoint(PASSIVE);")
                .map_err(db_error)
        })
        .await
    }
}

struct Database {
    root: PathBuf,
    connection: Connection,
    settings: Settings,
    clock: Clock,
}

fn db_error(_: impl std::fmt::Display) -> String {
    STORAGE_ERROR.into()
}

fn checked_id(id: &str) -> Result<(), String> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| "记录标识无效。")?;
    if parsed.to_string() != id {
        return Err("记录标识无效。".into());
    }
    Ok(())
}

pub fn cache_key(task: &Task) -> Option<String> {
    if matches!(
        task.vocabulary_state,
        crate::tasks::VocabularyState::Extracting
            | crate::tasks::VocabularyState::Cancelled
            | crate::tasks::VocabularyState::Failed
    ) {
        return None;
    }
    let profile = &task.profile;
    let url = profile.request_url().ok()?;
    let bytes = serde_json::to_vec(&(
        task.prompt_version,
        task.extract_vocabulary,
        task.preserve_structure,
        task.query_text(),
        &task.target_language,
        &task.source_language,
        &profile.id,
        profile.revision,
        url.as_str(),
        &profile.model,
        &profile.custom_prompt,
        profile.temperature.to_bits(),
    ))
    .ok()?;
    Some(crate::hex::lower(&Sha256::digest(bytes)))
}

fn cacheable_result(task: &Task) -> bool {
    if task.status != Status::Completed || task.translation.trim().is_empty() {
        return false;
    }
    match (
        task.translation_source_revision,
        task.translation_source_text.as_deref(),
    ) {
        (Some(revision), Some(text)) => {
            revision == task.source_revision && text == task.query_text()
        }
        // Before source drafts existed, editing required a new translation.
        // Those unchanged records remain valid; any subsequent edit increments
        // source_revision and must not associate their old result with new input.
        (None, None) => task.source_revision == 0,
        _ => false,
    }
}

impl Database {
    fn open(root: PathBuf, clock: Clock) -> Result<Self, String> {
        fs::create_dir_all(root.join("images")).map_err(db_error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(db_error)?;
            fs::set_permissions(root.join("images"), fs::Permissions::from_mode(0o700))
                .map_err(db_error)?;
        }
        let connection = Connection::open(root.join("history.sqlite3")).map_err(db_error)?;
        connection
            .busy_timeout(std::time::Duration::from_secs(2))
            .map_err(db_error)?;
        let schema: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(db_error)?;
        if schema > 1 {
            return Err("本地数据库来自较新版本，已停止写入。".into());
        }
        if schema == 0
            && root
                .join("history.sqlite3")
                .metadata()
                .is_ok_and(|m| m.len() > 0)
            && !root.join("history.pre-source-v1.sqlite3").exists()
        {
            connection
                .execute(
                    "VACUUM INTO ?1",
                    [root
                        .join("history.pre-source-v1.sqlite3")
                        .to_string_lossy()
                        .as_ref()],
                )
                .map_err(db_error)?;
        }
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA secure_delete=ON; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS vocabulary(id TEXT PRIMARY KEY, word TEXT NOT NULL UNIQUE, updated_at INTEGER NOT NULL, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS preferences(key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS records(id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, expires_at INTEGER, revision INTEGER NOT NULL, status TEXT NOT NULL, cache_key TEXT, data TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS records_created ON records(created_at DESC);
            CREATE INDEX IF NOT EXISTS records_expiry ON records(expires_at);
            CREATE INDEX IF NOT EXISTS records_cache ON records(cache_key);
            CREATE TABLE IF NOT EXISTS tombstones(id TEXT PRIMARY KEY);
            CREATE TABLE IF NOT EXISTS cleanup_jobs(id TEXT PRIMARY KEY);").map_err(db_error)?;
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(db_error)?;
        let migration = (|| {
            source::migrate(&connection)?;
            reliability::migrate(&connection)?;
            operations::migrate(&connection)?;
            connection
                .execute_batch("PRAGMA user_version=1; COMMIT")
                .map_err(db_error)
        })();
        if migration.is_err() {
            let _ = connection.execute_batch("ROLLBACK");
        }
        migration?;
        let settings: Settings = connection
            .query_row(
                "SELECT value FROM preferences WHERE key='settings'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(db_error)?
            .map(|s| {
                serde_json::from_str(&s).map_err(|_| "本地设置无法解析；数据未被覆盖。".to_string())
            })
            .transpose()?
            .unwrap_or_default();
        settings.validate()?;
        onboarding::initialize(&connection)?;
        reliability::sync_configuration(&connection, &settings, &settings)?;
        // No requests from the previous process survive startup. Old tombstones
        // can now be discarded, but unfinished work must never restart itself.
        connection.execute_batch("DELETE FROM tombstones; UPDATE records SET status='interrupted', cache_key=NULL WHERE status IN ('capturing','recognizing','queued','translating');").map_err(db_error)?;
        operations::recover_interrupted(&connection, clock())?;
        let mut db = Self {
            root,
            connection,
            settings,
            clock,
        };
        db.cleanup()?;
        db.remove_orphans()?;
        Ok(db)
    }

    fn image_path(&self, id: &str) -> Result<PathBuf, String> {
        checked_id(id)?;
        Ok(self.root.join("images").join(format!("{id}.png")))
    }
    fn tombstoned(&self, id: &str) -> Result<bool, String> {
        self.connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tombstones WHERE id=?1)",
                [id],
                |r| r.get(0),
            )
            .map_err(db_error)
    }

    fn save_image(&mut self, id: &str, bytes: &[u8]) -> Result<bool, String> {
        checked_id(id)?;
        if self.settings.retention == Retention::Disabled || self.tombstoned(id)? {
            return Ok(false);
        }
        if bytes.len() > 64 * 1024 * 1024 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err("截图数据无效或过大。".into());
        }
        let path = self.image_path(id)?;
        write_atomic(&path, bytes)?;
        Ok(true)
    }

    fn save_task(&mut self, mut task: Task) -> Result<SaveOutcome, String> {
        checked_id(&task.id)?;
        if self.settings.retention == Retention::Disabled
            || task.save_state == SaveState::MemoryOnly
        {
            return Ok(SaveOutcome::MemoryOnly);
        }
        if self.tombstoned(&task.id)? {
            return Ok(SaveOutcome::Dropped);
        }
        task.expires_at = self.settings.retention.expires_at(task.created_at);
        if task.expired((self.clock)()) {
            self.remove(&[task.id])?;
            return Ok(SaveOutcome::Dropped);
        }
        if task.original_text.is_empty() && !task.has_image {
            return Ok(SaveOutcome::Dropped);
        }
        let image_failed = task.image_error.is_some();
        let missing_asset = task
            .source_segments
            .iter()
            .filter_map(|s| s.asset_id.as_deref())
            .any(|asset| {
                source::asset_name(&task.id, asset)
                    .is_ok_and(|name| !self.root.join("images").join(name).is_file())
            });
        let waiting_image = task.image_pending
            || missing_asset
            || (task.has_image
                && task.source_segments.is_empty()
                && !self.image_path(&task.id)?.is_file()
                && !image_failed);
        task.save_state = if image_failed {
            SaveState::Failed
        } else if waiting_image {
            SaveState::Pending
        } else {
            SaveState::Saved
        };
        task.save_error = task.image_error.clone();
        let key = if cacheable_result(&task) {
            cache_key(&task)
        } else {
            None
        };
        let data = serde_json::to_string(&task).map_err(db_error)?;
        let status = status_name(task.status);
        let tx = self.connection.transaction().map_err(db_error)?;
        let changed = tx.execute("INSERT INTO records(id,created_at,expires_at,revision,status,cache_key,data) VALUES(?1,?2,?3,?4,?5,?6,?7)
            ON CONFLICT(id) DO UPDATE SET expires_at=excluded.expires_at,revision=excluded.revision,status=excluded.status,cache_key=excluded.cache_key,data=excluded.data
            WHERE excluded.revision >= records.revision", params![task.id,task.created_at,task.expires_at,task.revision,status,key,data]).map_err(db_error)?;
        if changed > 0 {
            source::save(&tx, &task)?;
        }
        tx.commit().map_err(db_error)?;
        source::clean_files(self);
        if changed == 0 {
            Ok(SaveOutcome::Dropped)
        } else if image_failed {
            Ok(SaveOutcome::PartialImageFailure)
        } else if waiting_image {
            Ok(SaveOutcome::WaitingForImage)
        } else {
            Ok(SaveOutcome::Saved)
        }
    }

    fn task(&self, id: &str) -> Result<Option<Task>, String> {
        checked_id(id)?;
        let now = (self.clock)();
        self.connection.query_row("SELECT data,expires_at,status FROM records WHERE id=?1 AND (expires_at IS NULL OR expires_at>?2)", params![id,now], read_row).optional().map_err(db_error)?.map(decode_row).transpose()
    }

    fn history(&self, limit: usize, offset: usize) -> Result<Vec<Task>, String> {
        let mut query = self.connection.prepare("SELECT data,expires_at,status FROM records WHERE expires_at IS NULL OR expires_at>?1 ORDER BY created_at DESC,id DESC LIMIT ?2 OFFSET ?3").map_err(db_error)?;
        let rows = query
            .query_map(params![(self.clock)(), limit, offset], read_row)
            .map_err(db_error)?;
        rows.map(|r| decode_row(r.map_err(db_error)?)).collect()
    }

    fn cached(&self, key: &str) -> Result<Option<Task>, String> {
        if self.settings.retention == Retention::Disabled {
            return Ok(None);
        }
        let mut query = self.connection.prepare("SELECT data,expires_at,status FROM records WHERE cache_key=?1 AND status='completed' AND (expires_at IS NULL OR expires_at>?2) ORDER BY created_at DESC").map_err(db_error)?;
        let rows = query
            .query_map(params![key, (self.clock)()], read_row)
            .map_err(db_error)?;
        for row in rows {
            let task = decode_row(row.map_err(db_error)?)?;
            // Revalidate old rows too: earlier versions could save a retained
            // translation under an edited draft's key. Skip it without hiding
            // an older, valid result for the same request.
            if cacheable_result(&task) && cache_key(&task).as_deref() == Some(key) {
                return Ok(Some(task));
            }
        }
        Ok(None)
    }

    fn image(&self, id: &str) -> Result<Option<Vec<u8>>, String> {
        if self.task(id)?.is_none() {
            return Ok(None);
        }
        let path = self.image_path(id)?;
        let meta = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(db_error(e)),
        };
        if meta.len() > 64 * 1024 * 1024 {
            return Err("截图文件过大。".into());
        }
        fs::read(path).map(Some).map_err(db_error)
    }

    fn remove(&mut self, ids: &[String]) -> Result<(), String> {
        for id in ids {
            checked_id(id)?;
        }
        let tx = self.connection.transaction().map_err(db_error)?;
        for id in ids {
            tx.execute("INSERT OR IGNORE INTO tombstones VALUES(?1)", [id])
                .map_err(db_error)?;
            // Queue even an absent row: its PNG may have arrived before metadata.
            tx.execute("INSERT OR IGNORE INTO cleanup_jobs VALUES(?1)", [id])
                .map_err(db_error)?;
            source::queue_removal(&tx, id)?;
            tx.execute("DELETE FROM records WHERE id=?1", [id])
                .map_err(db_error)?;
        }
        tx.commit().map_err(db_error)?;
        self.clean_files();
        Ok(())
    }

    fn all_ids(&self) -> Result<Vec<String>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM records")
            .map_err(db_error)?;
        statement
            .query_map([], |r| r.get(0))
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)
    }

    fn clear(&mut self, mut extra_ids: Vec<String>) -> Result<(), String> {
        extra_ids.extend(self.all_ids()?);
        extra_ids.sort();
        extra_ids.dedup();
        self.remove(&extra_ids)
    }

    fn save_settings(&mut self, settings: Settings, known_ids: Vec<String>) -> Result<(), String> {
        settings.validate()?;
        for id in &known_ids {
            checked_id(id)?;
        }
        let json = serde_json::to_string(&settings).map_err(db_error)?;
        let tx = self.connection.transaction().map_err(db_error)?;
        reliability::sync_configuration(&tx, &self.settings, &settings)?;
        tx.execute("INSERT INTO preferences(key,value) VALUES('settings',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [json]).map_err(db_error)?;
        match settings.retention {
            Retention::Disabled => {
                tx.execute_batch("INSERT OR IGNORE INTO source_asset_cleanup SELECT path FROM source_assets; INSERT OR IGNORE INTO tombstones SELECT id FROM records; INSERT OR IGNORE INTO cleanup_jobs SELECT id FROM records; DELETE FROM records;").map_err(db_error)?;
                for id in known_ids {
                    tx.execute("INSERT OR IGNORE INTO tombstones VALUES(?1)", [&id])
                        .map_err(db_error)?;
                    tx.execute("INSERT OR IGNORE INTO cleanup_jobs VALUES(?1)", [&id])
                        .map_err(db_error)?;
                }
            }
            Retention::Forever => {
                tx.execute("UPDATE records SET expires_at=NULL", [])
                    .map_err(db_error)?;
            }
            Retention::Days(days) => {
                let delta = i64::from(days) * 86_400_000;
                tx.execute("UPDATE records SET expires_at=CASE WHEN created_at > ?1 THEN ?2 ELSE created_at+?3 END", params![i64::MAX-delta,i64::MAX,delta]).map_err(db_error)?;
            }
        }
        tx.commit().map_err(db_error)?;
        self.settings = settings;
        // The metadata transaction is committed; filesystem retries are separate.
        self.clean_files();
        Ok(())
    }

    fn cleanup(&mut self) -> Result<Vec<String>, String> {
        let ids = {
            let mut statement = self
                .connection
                .prepare("SELECT id FROM records WHERE expires_at IS NOT NULL AND expires_at<=?1")
                .map_err(db_error)?;
            statement
                .query_map([(self.clock)()], |r| r.get(0))
                .map_err(db_error)?
                .collect::<rusqlite::Result<Vec<String>>>()
                .map_err(db_error)?
        };
        self.remove(&ids)?;
        Ok(ids)
    }

    fn clean_files(&mut self) {
        source::clean_files(self);
        let ids = (|| -> rusqlite::Result<Vec<String>> {
            let mut query = self
                .connection
                .prepare("SELECT id FROM cleanup_jobs LIMIT 128")?;
            query.query_map([], |r| r.get(0))?.collect()
        })()
        .unwrap_or_default();
        for id in ids {
            let Ok(path) = self.image_path(&id) else {
                continue;
            };
            let removed = match fs::remove_file(path) {
                Ok(()) => true,
                Err(e) => e.kind() == std::io::ErrorKind::NotFound,
            };
            if removed {
                let _ = self
                    .connection
                    .execute("DELETE FROM cleanup_jobs WHERE id=?1", [id]);
            }
        }
        let _ = self
            .connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }

    fn remove_orphans(&mut self) -> Result<(), String> {
        source::remove_orphans(self)?;
        for entry in fs::read_dir(self.root.join("images")).map_err(db_error)? {
            let entry = entry.map_err(db_error)?;
            let path = entry.path();
            if !entry.file_type().map_err(db_error)?.is_file() {
                continue;
            }
            if path.extension().is_some_and(|s| s == "tmp") {
                let _ = fs::remove_file(path);
                continue;
            }
            if path.extension().is_some_and(|s| s == "png") {
                let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if checked_id(id).is_ok() && self.task(id)?.is_none() {
                    let _ = fs::remove_file(path);
                }
            }
        }
        Ok(())
    }
}

fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<(String, Option<i64>, String)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
}
fn decode_row((data, expiry, status): (String, Option<i64>, String)) -> Result<Task, String> {
    let mut task: Task =
        serde_json::from_str(&data).map_err(|_| "部分历史记录无法解析，数据未被覆盖。")?;
    if task.vocabulary_state == crate::tasks::VocabularyState::Extracting {
        task.vocabulary_state = crate::tasks::VocabularyState::Cancelled;
    }
    task.ensure_source_segments();
    task.expires_at = expiry;
    task.status = serde_json::from_value(serde_json::Value::String(status)).map_err(db_error)?;
    if task.status == Status::Interrupted && task.error.is_none() {
        task.error = Some("上次退出时任务尚未完成，可手动重试。".into());
    }
    Ok(task)
}
fn status_name(status: Status) -> &'static str {
    match status {
        Status::Capturing => "capturing",
        Status::Recognizing => "recognizing",
        Status::Queued => "queued",
        Status::Translating => "translating",
        Status::Completed => "completed",
        Status::OriginalOnly => "original_only",
        Status::Ready => "ready",
        Status::Failed => "failed",
        Status::Stopped => "stopped",
        Status::Interrupted => "interrupted",
    }
}
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(db_error)?;
        file.write_all(bytes).map_err(db_error)?;
        file.sync_all().map_err(db_error)?;
        fs::rename(&temp, path).map_err(db_error)?;
        #[cfg(unix)]
        fs::File::open(path.parent().ok_or(STORAGE_ERROR)?)
            .and_then(|f| f.sync_all())
            .map_err(db_error)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Profile,
        tasks::{Source, TaskBook},
    };
    use std::sync::atomic::{AtomicI64, Ordering};

    fn record() -> Task {
        let book = TaskBook::new(Retention::Days(7), 12);
        let task = book
            .create(
                Source::Selection,
                Profile {
                    endpoint: "https://example.test/v1".into(),
                    model: "fixture".into(),
                    ..Profile::default()
                },
                "中文".into(),
                0,
            )
            .unwrap();
        book.recognized(&task.id, "original".into(), 1, 1).unwrap();
        book.start(&task.id, 1, 0, 2);
        book.append(&task.id, 1, "译文", 4, 3).unwrap();
        book.finish(&task.id, 1, 5, 4).unwrap()
    }

    #[tokio::test]
    async fn source_and_cache_expire_together_and_late_writes_cannot_restore_them() {
        let dir = tempfile::tempdir().unwrap();
        let time = Arc::new(AtomicI64::new(5));
        let clock: Clock = {
            let t = time.clone();
            Arc::new(move || t.load(Ordering::SeqCst))
        };
        let db = Storage::open(dir.path().to_owned(), clock).await.unwrap();
        let task = record();
        let key = cache_key(&task).unwrap();
        db.save_task(task.clone()).await.unwrap();
        assert!(db.cached(key.clone()).await.unwrap().is_some());
        time.store(8 * 86_400_000, Ordering::SeqCst);
        assert!(db.task(task.id.clone()).await.unwrap().is_none());
        assert!(db.cached(key).await.unwrap().is_none());
        db.cleanup().await.unwrap();
        assert_eq!(db.save_task(task).await.unwrap(), SaveOutcome::Dropped);
    }

    #[tokio::test]
    async fn clearing_unknown_inflight_ids_also_prevents_image_resurrection() {
        let dir = tempfile::tempdir().unwrap();
        let db = Storage::open(dir.path().to_owned(), Arc::new(|| 5))
            .await
            .unwrap();
        let task = record();
        db.clear(vec![task.id.clone()]).await.unwrap();
        assert!(
            !db.save_image(
                task.id.clone(),
                Arc::new(b"\x89PNG\r\n\x1a\nfixture".to_vec())
            )
            .await
            .unwrap()
        );
        assert_eq!(db.save_task(task).await.unwrap(), SaveOutcome::Dropped);
    }

    #[tokio::test]
    async fn retention_changes_apply_to_existing_records_and_disabled_never_writes_text() {
        let dir = tempfile::tempdir().unwrap();
        let db = Storage::open(dir.path().to_owned(), Arc::new(|| 2 * 86_400_000))
            .await
            .unwrap();
        let task = record();
        db.save_task(task.clone()).await.unwrap();
        let mut settings = Settings {
            retention: Retention::Days(1),
            ..Settings::default()
        };
        db.save_settings(settings.clone(), vec![]).await.unwrap();
        assert!(db.history(100, 0).await.unwrap().is_empty());
        settings.retention = Retention::Disabled;
        db.save_settings(settings, vec![task.id.clone()])
            .await
            .unwrap();
        assert_eq!(db.save_task(task).await.unwrap(), SaveOutcome::MemoryOnly);
        assert_eq!(db.stats().await.unwrap().records, 0);
    }

    #[tokio::test]
    async fn older_snapshots_cannot_overwrite_newer_results() {
        let dir = tempfile::tempdir().unwrap();
        let db = Storage::open(dir.path().to_owned(), Arc::new(|| 5))
            .await
            .unwrap();
        let old = record();
        let mut new = old.clone();
        new.revision += 10;
        new.translation = "新的结果".into();
        db.save_task(new.clone()).await.unwrap();
        assert_eq!(db.save_task(old).await.unwrap(), SaveOutcome::Dropped);
        assert_eq!(
            db.task(new.id).await.unwrap().unwrap().translation,
            "新的结果"
        );
    }

    #[tokio::test]
    async fn image_deletion_and_path_validation() {
        let dir = tempfile::tempdir().unwrap();
        let db = Storage::open(dir.path().to_owned(), Arc::new(|| 5))
            .await
            .unwrap();
        let mut task = record();
        task.has_image = true;
        let data = Arc::new(b"\x89PNG\r\n\x1a\nfixture".to_vec());
        db.save_image(task.id.clone(), data.clone()).await.unwrap();
        db.save_task(task.clone()).await.unwrap();
        assert_eq!(db.image(task.id.clone()).await.unwrap().unwrap(), *data);
        db.remove(vec![task.id.clone()]).await.unwrap();
        assert!(
            !dir.path()
                .join("images")
                .join(format!("{}.png", task.id))
                .exists()
        );
        assert!(db.save_image("../../outside".into(), data).await.is_err());
    }

    #[test]
    fn cache_changes_with_target_model_revision_and_prompt_inputs() {
        let t = record();
        let key = cache_key(&t);
        let mut next = t.clone();
        next.profile.revision += 1;
        assert_ne!(key, cache_key(&next));
        next = t.clone();
        next.target_language = "日本語".into();
        assert_ne!(key, cache_key(&next));
        next = t;
        next.edited_text = Some("changed".into());
        assert_ne!(key, cache_key(&next));
        let edited_key = cache_key(&next);
        next.prompt_version = 3;
        assert_ne!(edited_key, cache_key(&next));
    }
    #[tokio::test]
    async fn resaving_legacy_history_does_not_promote_its_old_prompt_cache() {
        let dir = tempfile::tempdir().unwrap();
        let db = Storage::open(dir.path().into(), Arc::new(|| 5))
            .await
            .unwrap();
        let mut value = serde_json::to_value(record()).unwrap();
        value.as_object_mut().unwrap().remove("prompt_version");
        let mut old: Task = serde_json::from_value(value).unwrap();
        assert_eq!(old.prompt_version, 1);
        let mut current = old.clone();
        current.prompt_version = crate::provider::PROMPT_VERSION;
        let new_key = cache_key(&current).unwrap();
        assert_ne!(cache_key(&old).unwrap(), new_key);
        db.save_task(old.clone()).await.unwrap();
        assert!(db.cached(new_key.clone()).await.unwrap().is_none());
        old.revision += 1;
        db.save_task(old.clone()).await.unwrap();
        assert!(db.cached(new_key).await.unwrap().is_none());
        assert!(db.task(old.id).await.unwrap().is_some());
    }
    #[test]
    fn preserve_structure_isolates_translation_cache() {
        let mut task = record();
        let plain = cache_key(&task);
        assert!(plain.is_some());
        task.preserve_structure = true;
        let structured = cache_key(&task);
        assert!(structured.is_some());
        assert_ne!(plain, structured);
        task.preserve_structure = false;
        assert_eq!(cache_key(&task), plain);
    }
}
