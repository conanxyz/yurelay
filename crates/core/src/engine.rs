use crate::{
    config::{Profile, Retention, Settings},
    provider::{self, TranslationClient},
    storage::{self, Clock, SaveOutcome, Storage},
    tasks::{SaveState, Source, Status, Task, TaskBook, VocabularyState},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub trait Credentials: Send + Sync + 'static {
    fn read(&self, id: &str) -> Result<Option<String>, String>;
    fn write(&self, id: &str, value: Option<&str>) -> Result<(), String>;
}
pub trait LocalDictionary: Send + Sync + 'static {
    fn lookup(&self, text: &str) -> Result<String, String>;
}
type Secret = Arc<Zeroizing<String>>;

struct LiveConfig {
    settings: Settings,
    keys: BTreeMap<String, Secret>,
    errors: BTreeMap<String, String>,
}

#[derive(Clone, Serialize)]
pub struct Configuration {
    pub settings: Settings,
    pub has_keys: Vec<String>,
    pub credential_errors: BTreeMap<String, String>,
}

pub struct CaptureHandle {
    pub id: String,
    key: Secret,
    credential_error: Option<String>,
    weight: u32,
}

#[derive(Default)]
struct ImagePool {
    data: HashMap<String, Arc<Vec<u8>>>,
    order: VecDeque<String>,
    bytes: usize,
}
impl ImagePool {
    fn remove(&mut self, id: &str) {
        if let Some(data) = self.data.remove(id) {
            self.bytes = self.bytes.saturating_sub(data.len());
        }
        self.order.retain(|key| key != id);
    }
    fn insert(
        &mut self,
        id: String,
        data: Arc<Vec<u8>>,
        active: &HashSet<String>,
    ) -> Result<Vec<String>, String> {
        const LIMIT: usize = 32 * 1024 * 1024;
        if data.len() > LIMIT {
            return Err("截图超过会话图片缓存上限；已尝试单独保存到本地。".into());
        }
        self.remove(&id);
        let mut removed = vec![];
        while self.bytes + data.len() > LIMIT {
            let candidate = self
                .order
                .iter()
                .find(|key| !active.contains(*key))
                .cloned();
            let Some(key) = candidate else {
                return Err("进行中的截图已占满会话缓存，请稍后重试。".into());
            };
            self.remove(&key);
            removed.push(key);
        }
        self.bytes += data.len();
        self.order.push_back(id.clone());
        self.data.insert(id, data);
        Ok(removed)
    }
}

struct PendingSensitive {
    text: Zeroizing<String>,
    languages: Option<(Option<String>, String)>,
    detected: Option<String>,
}
fn fingerprint(text: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub struct Engine {
    sensitive_guard: AtomicBool,
    auto_vocabulary: AtomicBool,
    pending_sensitive: Mutex<HashMap<String, PendingSensitive>>,
    approved_sensitive: Mutex<HashMap<String, String>>,
    pending_images: Mutex<HashMap<String, Vec<u8>>>,
    pub book: TaskBook,
    pub storage: Storage,
    live: RwLock<LiveConfig>,
    credentials: Arc<dyn Credentials>,
    dictionary: Option<Arc<dyn LocalDictionary>>,
    client: TranslationClient,
    clock: Clock,
    jobs: Mutex<HashMap<String, (u32, CancellationToken)>>,
    images: Mutex<ImagePool>,
    mutations: tokio::sync::Mutex<()>,
    permits: Arc<Semaphore>,
    wake: Arc<Notify>,
    closing: AtomicBool,
}

impl Engine {
    pub fn now(&self) -> i64 {
        (self.clock)()
    }
    pub async fn open(
        root: PathBuf,
        credentials: Arc<dyn Credentials>,
        clock: Clock,
    ) -> Result<Arc<Self>, String> {
        Self::open_with_dictionary(root, credentials, clock, None).await
    }

    pub async fn open_with_dictionary(
        root: PathBuf,
        credentials: Arc<dyn Credentials>,
        clock: Clock,
        dictionary: Option<Arc<dyn LocalDictionary>>,
    ) -> Result<Arc<Self>, String> {
        let storage = Storage::open(root, clock.clone()).await?;
        let settings = storage.settings().await?;
        let ids: Vec<_> = settings.profiles.iter().map(|p| p.id.clone()).collect();
        let vault = credentials.clone();
        let (keys, errors) = tokio::task::spawn_blocking(move || {
            let mut keys = BTreeMap::new();
            let mut errors = BTreeMap::new();
            for id in ids {
                match vault.read(&id) {
                    Ok(Some(value)) => {
                        keys.insert(id, Arc::new(Zeroizing::new(value)));
                    }
                    Ok(None) => {}
                    Err(error) => {
                        errors.insert(id, error);
                    }
                }
            }
            (keys, errors)
        })
        .await
        .map_err(|_| "读取系统凭据失败。")?;
        let book = TaskBook::new(settings.retention, 12);
        book.restore(storage.history(20, 0).await?, clock());
        let engine = Arc::new(Self {
            sensitive_guard: AtomicBool::new(settings.sensitive_guard),
            auto_vocabulary: AtomicBool::new(settings.auto_vocabulary),
            pending_sensitive: Mutex::new(HashMap::new()),
            approved_sensitive: Mutex::new(HashMap::new()),
            pending_images: Mutex::new(HashMap::new()),
            book,
            storage,
            live: RwLock::new(LiveConfig {
                settings,
                keys,
                errors,
            }),
            credentials,
            dictionary,
            client: TranslationClient::new(provider::Limits::default())
                .map_err(|e| e.to_string())?,
            clock,
            jobs: Mutex::new(HashMap::new()),
            images: Mutex::new(ImagePool::default()),
            mutations: tokio::sync::Mutex::new(()),
            permits: Arc::new(Semaphore::new(2)),
            wake: Arc::new(Notify::new()),
            closing: AtomicBool::new(false),
        });
        engine.trim();
        let weak = Arc::downgrade(&engine);
        let wake = engine.wake.clone();
        tokio::spawn(async move {
            loop {
                let Some(engine) = weak.upgrade() else {
                    break;
                };
                if engine.closing.load(Ordering::Relaxed) {
                    break;
                }
                let now = (engine.clock)();
                let next = engine
                    .book
                    .snapshot()
                    .tasks
                    .iter()
                    .filter_map(|t| t.expires_at)
                    .min()
                    .unwrap_or(now + 60_000);
                let delay = Duration::from_millis((next - now).clamp(1, 60_000) as u64);
                drop(engine);
                tokio::select! { _ = tokio::time::sleep(delay) => {}, _ = wake.notified() => {} }
                let Some(engine) = weak.upgrade() else {
                    break;
                };
                let _ = engine.cleanup().await;
            }
        });
        Ok(engine)
    }

    pub async fn configuration(&self) -> Configuration {
        let config = self.live.read().await;
        Configuration {
            settings: config.settings.clone(),
            has_keys: config
                .keys
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, _)| k.clone())
                .collect(),
            credential_errors: config.errors.clone(),
        }
    }

    pub async fn begin(self: &Arc<Self>, source: Source) -> Result<CaptureHandle, String> {
        self.cancel_vocabulary_jobs();
        let _guard = self.mutations.lock().await;
        if self.closing.load(Ordering::Relaxed) {
            return Err("应用正在退出。".into());
        }
        self.trim();
        let live = self.live.read().await;
        let profile = live.settings.active()?.clone();
        let key = live
            .keys
            .get(&profile.id)
            .cloned()
            .unwrap_or_else(|| Arc::new(Zeroizing::new(String::new())));
        let credential_error = live.errors.get(&profile.id).cloned();
        let weight = if live.settings.max_concurrency == 1 {
            2
        } else {
            1
        };
        let task = self.book.create_with_language(
            source,
            profile,
            live.settings.target_language.clone(),
            (!live.settings.source_language.is_empty())
                .then(|| live.settings.source_language.clone()),
            (self.clock)(),
        )?;
        self.wake.notify_one();
        Ok(CaptureHandle {
            id: task.id,
            key,
            credential_error,
            weight,
        })
    }

    pub fn accept_text(self: &Arc<Self>, handle: &CaptureHandle, text: String, elapsed_ms: u64) {
        self.accept_detected_text(handle, text, elapsed_ms, None);
    }

    pub fn accept_detected_text(
        self: &Arc<Self>,
        handle: &CaptureHandle,
        text: String,
        elapsed_ms: u64,
        detected: Option<String>,
    ) {
        if text.chars().count() > crate::MAX_INPUT_CHARACTERS {
            self.fail_capture(&handle.id, 1, "原文过长，请分段翻译。".into());
            return;
        }
        let languages = self
            .book
            .get(&handle.id)
            .map(|t| (t.source_language, t.target_language));
        if self.guard_text(&handle.id, &text, languages, detected.clone()) {
            return;
        }
        self.book.set_extraction(
            &handle.id,
            crate::vocabulary::extraction_enabled(
                self.auto_vocabulary.load(Ordering::Relaxed),
                &text,
            ),
            self.now(),
        );
        match self
            .book
            .recognized(&handle.id, text, elapsed_ms, (self.clock)())
        {
            Ok(Some(task)) => {
                let image = self.pending_images.lock().unwrap().remove(&task.id);
                if let Some(bytes) = image {
                    self.accept_image(&task.id, bytes)
                }
                if task.source != Source::Manual {
                    self.book.ready(&task.id, detected, (self.clock)());
                    self.persist_later(task.id);
                    return;
                }
                self.persist_later(task.id.clone());
                self.schedule(
                    task,
                    handle.key.clone(),
                    handle.credential_error.clone(),
                    handle.weight,
                );
            }
            Ok(None) => {}
            Err(error) => self.fail_capture(&handle.id, 1, error),
        }
    }

    fn guard_text(
        &self,
        id: &str,
        text: &str,
        languages: Option<(Option<String>, String)>,
        detected: Option<String>,
    ) -> bool {
        if !self.sensitive_guard.load(Ordering::Relaxed) {
            return false;
        }
        if self
            .approved_sensitive
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|digest| digest == &fingerprint(text))
        {
            return false;
        }
        let Some(kind) = crate::sensitive::detect(text) else {
            return false;
        };
        if let Some((_, token)) = self.jobs.lock().unwrap().get(id) {
            token.cancel()
        }
        self.pending_sensitive.lock().unwrap().insert(
            id.into(),
            PendingSensitive {
                text: Zeroizing::new(text.to_owned()),
                languages,
                detected,
            },
        );
        self.pending_images.lock().unwrap().remove(id);
        self.images.lock().unwrap().remove(id);
        self.book.mask_sensitive(id, kind, self.now());
        true
    }
    pub async fn resolve_sensitive(self: &Arc<Self>, id: &str, allow: bool) -> Result<(), String> {
        if !allow {
            return self.remove(id).await;
        }
        let pending = self
            .pending_sensitive
            .lock()
            .unwrap()
            .remove(id)
            .ok_or("敏感内容已清空或过期。")?;
        self.approved_sensitive
            .lock()
            .unwrap()
            .insert(id.into(), fingerprint(&pending.text));
        self.book
            .release_sensitive(id, pending.text.to_string(), self.now())
            .ok_or("记录已删除或到期。")?;
        self.retry_languages(id, None, pending.languages, pending.detected)
            .await
    }

    pub fn fail_capture(self: &Arc<Self>, id: &str, attempt: u32, error: String) {
        if self.book.get(id).is_some() {
            self.book.fail(id, attempt, error, (self.clock)());
            self.persist_later(id.into());
        }
    }

    pub fn fail_image(self: &Arc<Self>, id: &str, error: String) {
        self.book.image_error(id, Some(error), (self.clock)());
        self.persist_later(id.into());
    }

    pub fn accept_image(self: &Arc<Self>, id: &str, bytes: Vec<u8>) {
        let Some(task) = self.book.get(id) else {
            return;
        };
        if task.sensitive.is_some() || self.approved_sensitive.lock().unwrap().contains_key(id) {
            return;
        }
        if task.original_text.is_empty() {
            let mut pending = self.pending_images.lock().unwrap();
            if pending.values().map(Vec::len).sum::<usize>() + bytes.len() <= 32 * 1024 * 1024 {
                pending.insert(id.into(), bytes);
            } else {
                drop(pending);
                self.fail_image(id, "等待识别的截图缓存已满，请重新截取较小区域。".into());
            }
            return;
        }
        if self.book.image_captured(id, (self.clock)()).is_none() {
            return;
        }
        let data = Arc::new(bytes);
        let active: HashSet<_> = self
            .book
            .snapshot()
            .tasks
            .into_iter()
            .filter(|t| {
                !t.status.is_terminal()
                    || matches!(t.save_state, SaveState::Pending | SaveState::Failed)
            })
            .map(|t| t.id)
            .collect();
        let cache_result = self
            .images
            .lock()
            .unwrap()
            .insert(id.into(), data.clone(), &active);
        let task = self.book.get(id);
        match cache_result {
            Ok(evicted) => {
                for id in evicted {
                    if self
                        .book
                        .get(&id)
                        .is_some_and(|t| t.save_state == SaveState::MemoryOnly)
                    {
                        self.pending_sensitive.lock().unwrap().remove(&id);
                        self.approved_sensitive.lock().unwrap().remove(&id);
                        self.pending_images.lock().unwrap().remove(&id);
                        self.book.remove(&id);
                    }
                }
            }
            Err(error) if task.is_some_and(|t| t.save_state == SaveState::MemoryOnly) => {
                self.fail_image(id, error);
                return;
            }
            Err(_) => {}
        }
        let engine = self.clone();
        let id = id.to_string();
        tokio::spawn(async move {
            match engine.storage.save_image(id.clone(), data).await {
                Ok(true) => {
                    engine.book.image_error(&id, None, (engine.clock)());
                }
                Ok(false) => {}
                Err(error) => {
                    engine.book.image_error(&id, Some(error), (engine.clock)());
                }
            }
            engine.persist(&id).await;
        });
    }

    fn schedule(
        self: &Arc<Self>,
        task: Task,
        key: Secret,
        credential_error: Option<String>,
        weight: u32,
    ) {
        let cancel = CancellationToken::new();
        self.jobs
            .lock()
            .unwrap()
            .insert(task.id.clone(), (task.attempt, cancel.clone()));
        let engine = self.clone();
        tokio::spawn(async move {
            engine
                .run_job(&task, key, credential_error, weight, cancel)
                .await;
            let mut jobs = engine.jobs.lock().unwrap();
            if jobs
                .get(&task.id)
                .is_some_and(|(attempt, _)| *attempt == task.attempt)
            {
                jobs.remove(&task.id);
            }
        });
    }

    async fn run_job(
        self: &Arc<Self>,
        task: &Task,
        key: Secret,
        credential_error: Option<String>,
        weight: u32,
        cancel: CancellationToken,
    ) {
        if self.guard_text(
            &task.id,
            task.query_text(),
            Some((task.source_language.clone(), task.target_language.clone())),
            task.detected_language.clone(),
        ) {
            return;
        }
        let local_dictionary = task.profile.provider == crate::config::Provider::SystemDictionary;
        if !local_dictionary
            && (task.profile.provider == crate::config::Provider::Youdao
                || !crate::vocabulary::short_query(task.query_text()))
            && task
                .source_language
                .as_deref()
                .or(task.detected_language.as_deref())
                .is_some_and(|source| {
                    crate::config::same_translation_language(source, &task.target_language)
                })
        {
            self.book
                .local_result(&task.id, task.attempt, (self.clock)());
            self.persist(&task.id).await;
            self.trim();
            return;
        }
        if let Some(error) = credential_error.filter(|_| !local_dictionary) {
            self.book
                .fail(&task.id, task.attempt, error, (self.clock)());
            self.persist(&task.id).await;
            return;
        }
        if let Err(error) = task.profile.validate() {
            self.book.fail(
                &task.id,
                task.attempt,
                format!("请先配置翻译服务：{error}"),
                (self.clock)(),
            );
            self.persist(&task.id).await;
            return;
        }
        if !local_dictionary
            && task.save_state != SaveState::MemoryOnly
            && let Some(hash) = storage::cache_key(task)
            && let Ok(Ok(Some(cached))) =
                tokio::time::timeout(Duration::from_millis(15), self.storage.cached(hash)).await
        {
            if task.extract_vocabulary {
                self.book.set_vocabulary(
                    &task.id,
                    task.attempt,
                    cached.vocabulary,
                    cached.vocabulary_notice,
                    self.now(),
                );
            }
            self.book
                .cached_result(&task.id, task.attempt, cached.translation, (self.clock)());
            self.persist(&task.id).await;
            return;
        }
        let queue_start = tokio::time::Instant::now();
        let permit = tokio::select! { biased; _ = cancel.cancelled() => return, permit = self.permits.clone().acquire_many_owned(weight) => permit };
        let Ok(acquired) = permit else {
            return;
        };
        if self.guard_text(
            &task.id,
            task.query_text(),
            Some((task.source_language.clone(), task.target_language.clone())),
            task.detected_language.clone(),
        ) {
            return;
        }
        if self
            .book
            .start(
                &task.id,
                task.attempt,
                queue_start.elapsed().as_millis() as u64,
                (self.clock)(),
            )
            .is_none()
        {
            return;
        }
        let mut permit = Some(acquired);
        let append = |text: &str, ms| match self.book.append(
            &task.id,
            task.attempt,
            text,
            ms,
            (self.clock)(),
        ) {
            Ok(true) => Ok(()),
            Ok(false) => Err(provider::Error::Cancelled),
            Err(error) => Err(provider::Error::Protocol(error)),
        };
        let mut parser = crate::vocabulary::StreamParser::default();
        let parse_output = task.extract_vocabulary
            && !local_dictionary
            && task.profile.provider != crate::config::Provider::Youdao;
        let on_text = |text: &str, ms| {
            let before = parser.has_metadata();
            let visible = if parse_output {
                parser.push(text)
            } else {
                text.to_string()
            };
            if !before && parser.has_metadata() {
                match self
                    .book
                    .translation_done(&task.id, task.attempt, &visible, ms, self.now())
                {
                    Ok(true) => {
                        self.persist_later(task.id.clone());
                        drop(permit.take());
                    }
                    Ok(false) => return Err(provider::Error::Cancelled),
                    Err(error) => return Err(provider::Error::Protocol(error)),
                }
                if !task.extract_vocabulary {
                    return Err(provider::Error::Cancelled);
                }
                Ok(())
            } else if visible.is_empty() {
                Ok(())
            } else {
                append(&visible, ms)
            }
        };
        let result = if local_dictionary {
            self.lookup_dictionary(task.query_text(), cancel, on_text)
                .await
        } else {
            self.client
                .translate_policy(
                    &task.profile,
                    key.as_str(),
                    task.query_text(),
                    provider::TranslationOptions {
                        target: &task.target_language,
                        source: task.source_language.as_deref(),
                        extract_vocabulary: task.extract_vocabulary,
                    },
                    cancel,
                    on_text,
                )
                .await
        };
        match result {
            Ok(metrics) => {
                if parse_output {
                    let (tail, items, notice) = parser.finish(
                        task.query_text(),
                        &self
                            .book
                            .get(&task.id)
                            .map(|t| t.translation)
                            .unwrap_or_default(),
                    );
                    if !tail.is_empty() {
                        let _ = append(&tail, metrics.total_ms);
                    }
                    if task.extract_vocabulary {
                        self.book
                            .set_vocabulary(&task.id, task.attempt, items, notice, self.now());
                    }
                }
                if self
                    .book
                    .get(&task.id)
                    .is_some_and(|t| t.translation.trim().is_empty())
                {
                    self.book.fail(
                        &task.id,
                        task.attempt,
                        provider::Error::Empty.to_string(),
                        self.now(),
                    );
                } else {
                    self.book
                        .finish(&task.id, task.attempt, metrics.total_ms, (self.clock)());
                }
            }
            Err(provider::Error::Cancelled) => {
                self.book.finish_vocabulary(
                    &task.id,
                    task.attempt,
                    VocabularyState::Cancelled,
                    None,
                    self.now(),
                );
            }
            Err(error) => {
                if self
                    .book
                    .get(&task.id)
                    .is_some_and(|t| t.attempt == task.attempt && t.status == Status::Completed)
                {
                    self.book.finish_vocabulary(
                        &task.id,
                        task.attempt,
                        VocabularyState::Failed,
                        Some("生词提炼未完成，译文已保存。".into()),
                        self.now(),
                    );
                } else {
                    self.book
                        .fail(&task.id, task.attempt, error.to_string(), self.now());
                }
            }
        }
        drop(permit);
        self.persist(&task.id).await;
        self.trim();
    }

    pub async fn manual(self: &Arc<Self>, text: String) -> Result<String, String> {
        self.manual_languages(text, None, None).await
    }
    pub async fn manual_languages(
        self: &Arc<Self>,
        text: String,
        languages: Option<(Option<String>, String)>,
        detected: Option<String>,
    ) -> Result<String, String> {
        if text.trim().is_empty() || text.chars().count() > crate::MAX_INPUT_CHARACTERS {
            return Err("原文为空或过长。".into());
        }
        if let Some((source, target)) = &languages {
            let valid = |v: &str| {
                !v.is_empty()
                    && v.len() <= 32
                    && v.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
            };
            if !valid(target) || source.as_ref().is_some_and(|s| !valid(s)) {
                return Err("无效的语言代码。".into());
            }
        }
        let handle = self.begin(Source::Manual).await?;
        if let Some((source, target)) = languages {
            self.book
                .capture_languages(&handle.id, source, target, detected.clone(), self.now());
        }
        let id = handle.id.clone();
        self.accept_detected_text(&handle, text, 0, detected);
        Ok(id)
    }

    pub async fn retry(self: &Arc<Self>, id: &str, edited: Option<String>) -> Result<(), String> {
        self.retry_languages(id, edited, None, None).await
    }

    pub async fn retry_languages(
        self: &Arc<Self>,
        id: &str,
        edited: Option<String>,
        languages: Option<(Option<String>, String)>,
        detected: Option<String>,
    ) -> Result<(), String> {
        if let Some((source, target)) = &languages {
            let valid = |v: &str| {
                !v.is_empty()
                    && v.len() <= 32
                    && v.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
            };
            if !valid(target) || source.as_ref().is_some_and(|v| !valid(v)) {
                return Err("无效的语言代码。".into());
            }
        }
        self.cancel_vocabulary_jobs();
        let _guard = self.mutations.lock().await;
        self.load_into_book(id).await?;
        let old = self.book.get(id).ok_or("记录已删除或到期。")?;
        if old.sensitive.is_some() {
            return Err("请先确认或清空敏感内容。".into());
        }
        if self.guard_text(
            id,
            edited.as_deref().unwrap_or_else(|| old.query_text()),
            languages.clone(),
            detected.clone(),
        ) {
            return Ok(());
        }
        let live = self.live.read().await;
        // Detection belongs to the exact query. Editing the source invalidates
        // the capture's old language, including when fresh detection is unknown.
        let detected = detected.or_else(|| {
            if edited
                .as_deref()
                .is_none_or(|text| text == old.query_text())
            {
                old.detected_language.clone()
            } else {
                None
            }
        });
        let current = live.settings.active()?;
        let key = live
            .keys
            .get(&current.id)
            .cloned()
            .unwrap_or_else(|| Arc::new(Zeroizing::new(String::new())));
        let error = live.errors.get(&current.id).cloned();
        let weight = if live.settings.max_concurrency == 1 {
            2
        } else {
            1
        };
        let _queued = if old.status == Status::Ready {
            self.book.submit_ready(id, edited, (self.clock)())?
        } else {
            self.book.retry(id, edited, (self.clock)())?
        }
        .ok_or("此任务正在执行，或缺少可翻译原文。")?;
        let task = {
            self.book
                .retarget_queued(
                    id,
                    current.clone(),
                    live.settings.target_language.clone(),
                    (self.clock)(),
                )
                .ok_or("此任务已不在等待队列。")?
        };
        let (source, target) = languages.unwrap_or((task.source_language, task.target_language));
        let task = self
            .book
            .languages_queued(id, source, target, detected, (self.clock)())
            .ok_or("此任务已不在等待队列。")?;
        let enabled =
            crate::vocabulary::extraction_enabled(live.settings.auto_vocabulary, task.query_text());
        let task = self
            .book
            .set_extraction(id, enabled, self.now())
            .ok_or("此任务已不在等待队列。")?;
        self.persist_later(id.into());
        self.schedule(task, key, error, weight);
        Ok(())
    }

    fn cancel_vocabulary_jobs(self: &Arc<Self>) {
        for task in self.book.snapshot().tasks {
            if task.vocabulary_state == VocabularyState::Extracting {
                self.stop(&task.id);
            }
        }
    }

    pub fn stop(self: &Arc<Self>, id: &str) {
        if let Some(task) = self.book.get(id) {
            self.book.finish_vocabulary(
                id,
                task.attempt,
                VocabularyState::Cancelled,
                None,
                self.now(),
            );
        }
        self.book.stop(id, (self.clock)());
        if let Some((_, token)) = self.jobs.lock().unwrap().get(id) {
            token.cancel();
        }
        self.persist_later(id.into());
    }

    pub async fn remove(&self, id: &str) -> Result<(), String> {
        let _guard = self.mutations.lock().await;
        if let Some((_, token)) = self.jobs.lock().unwrap().remove(id) {
            token.cancel();
        }
        self.pending_sensitive.lock().unwrap().remove(id);
        self.approved_sensitive.lock().unwrap().remove(id);
        self.pending_images.lock().unwrap().remove(id);
        self.book.remove(id);
        self.images.lock().unwrap().remove(id);
        self.storage.remove(vec![id.into()]).await
    }

    pub async fn clear(&self) -> Result<(), String> {
        let _guard = self.mutations.lock().await;
        let ids: Vec<_> = self
            .book
            .snapshot()
            .tasks
            .into_iter()
            .map(|t| t.id)
            .collect();
        for id in &ids {
            if let Some((_, token)) = self.jobs.lock().unwrap().remove(id) {
                token.cancel();
            }
            self.book.remove(id);
        }
        *self.images.lock().unwrap() = ImagePool::default();
        self.pending_sensitive.lock().unwrap().clear();
        self.approved_sensitive.lock().unwrap().clear();
        self.pending_images.lock().unwrap().clear();
        self.storage.clear(ids).await
    }

    pub async fn cleanup(&self) -> Result<(), String> {
        let _guard = self.mutations.lock().await;
        let mut ids: Vec<_> = self
            .book
            .expire((self.clock)())
            .into_iter()
            .map(|t| t.id)
            .collect();
        ids.extend(self.storage.cleanup().await?);
        for id in ids {
            self.pending_sensitive.lock().unwrap().remove(&id);
            self.approved_sensitive.lock().unwrap().remove(&id);
            self.pending_images.lock().unwrap().remove(&id);
            self.book.remove(&id);
            self.images.lock().unwrap().remove(&id);
            if let Some((_, token)) = self.jobs.lock().unwrap().remove(&id) {
                token.cancel();
            }
        }
        Ok(())
    }

    pub async fn image(&self, id: &str) -> Result<Option<Arc<Vec<u8>>>, String> {
        let task = self.book.get(id);
        if let Some(task) = &task
            && (task.expired((self.clock)())
                || task.sensitive.is_some()
                || self.approved_sensitive.lock().unwrap().contains_key(id))
        {
            return Ok(None);
        }
        if task.is_some()
            && let Some(data) = self.images.lock().unwrap().data.get(id)
        {
            return Ok(Some(data.clone()));
        }
        self.storage
            .image(id.into())
            .await
            .map(|data| data.map(Arc::new))
    }

    pub async fn load_into_book(&self, id: &str) -> Result<Task, String> {
        if let Some(task) = self.book.get(id).filter(|t| !t.expired((self.clock)())) {
            return Ok(task);
        }
        let task = self
            .storage
            .task(id.into())
            .await?
            .ok_or("记录已删除或到期。")?;
        self.book.restore(vec![task.clone()], (self.clock)());
        Ok(self.book.get(id).unwrap_or(task))
    }

    fn persist_later(self: &Arc<Self>, id: String) {
        let engine = self.clone();
        tokio::spawn(async move {
            engine.persist(&id).await;
        });
    }

    async fn persist(&self, id: &str) {
        let Some(task) = self.book.get(id).filter(|t| !t.expired((self.clock)())) else {
            return;
        };
        if task.save_state == SaveState::MemoryOnly {
            return;
        }
        let revision = task.revision;
        let image_error = task.image_error.clone();
        let outcome = tokio::time::timeout(Duration::from_secs(3), self.storage.save_task(task))
            .await
            .unwrap_or_else(|_| Err("本地保存尚未确认，请稍后重试保存。".into()));
        match outcome {
            Ok(SaveOutcome::Saved) => {
                self.book
                    .set_save_state(id, revision, SaveState::Saved, None, (self.clock)());
            }
            Ok(SaveOutcome::PartialImageFailure) => {
                self.book.set_save_state(
                    id,
                    revision,
                    SaveState::Failed,
                    image_error,
                    (self.clock)(),
                );
            }
            Ok(SaveOutcome::MemoryOnly) => {
                self.book
                    .set_save_state(id, revision, SaveState::MemoryOnly, None, (self.clock)());
            }
            Ok(_) => {}
            Err(error) => {
                self.book.set_save_state(
                    id,
                    revision,
                    SaveState::Failed,
                    Some(error),
                    (self.clock)(),
                );
            }
        }
    }

    pub async fn retry_save(&self, id: &str) -> Result<(), String> {
        let task = self.book.get(id).ok_or("记录已删除或到期。")?;
        if task.save_state == SaveState::MemoryOnly {
            return Err("这条结果仅保留在本次会话。".into());
        }
        let data = self.images.lock().unwrap().data.get(id).cloned();
        if task.image_error.is_some() && data.is_none() {
            return Err("原始截图已不在内存中，请重新截图。".into());
        }
        if let Some(data) = data {
            self.storage.save_image(id.into(), data).await?;
            self.book.image_error(id, None, (self.clock)());
        }
        self.persist(id).await;
        Ok(())
    }

    fn trim(&self) {
        for task in self.book.trim() {
            self.images.lock().unwrap().remove(&task.id);
            self.pending_sensitive.lock().unwrap().remove(&task.id);
            self.approved_sensitive.lock().unwrap().remove(&task.id);
            self.pending_images.lock().unwrap().remove(&task.id);
        }
    }

    pub async fn models(
        &self,
        profile: Profile,
        override_key: Option<String>,
    ) -> Result<Vec<provider::ModelInfo>, String> {
        let live = self.live.read().await;
        let key = override_key
            .map(|v| Arc::new(Zeroizing::new(v)))
            .or_else(|| live.keys.get(&profile.id).cloned())
            .unwrap_or_else(|| Arc::new(Zeroizing::new(String::new())));
        drop(live);
        self.client
            .models(&profile, key.as_str())
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn test_connection(
        &self,
        profile: Profile,
        override_key: Option<String>,
    ) -> Result<provider::Metrics, String> {
        if profile.provider == crate::config::Provider::SystemDictionary {
            return self
                .lookup_dictionary("hello", CancellationToken::new(), |_, _| Ok(()))
                .await
                .map_err(|e| e.to_string());
        }
        let live = self.live.read().await;
        let key = override_key
            .map(|v| Arc::new(Zeroizing::new(v)))
            .or_else(|| live.keys.get(&profile.id).cloned())
            .unwrap_or_else(|| Arc::new(Zeroizing::new(String::new())));
        drop(live);
        self.client
            .translate_policy(
                &profile,
                key.as_str(),
                "This is a connection test.",
                provider::TranslationOptions {
                    target: "zh-Hans",
                    source: None,
                    extract_vocabulary: false,
                },
                CancellationToken::new(),
                |_, _| Ok(()),
            )
            .await
            .map_err(|e| e.to_string())
    }

    async fn lookup_dictionary(
        &self,
        text: &str,
        cancel: CancellationToken,
        mut on_text: impl FnMut(&str, u64) -> Result<(), provider::Error>,
    ) -> Result<provider::Metrics, provider::Error> {
        let dictionary = self
            .dictionary
            .clone()
            .ok_or_else(|| provider::Error::Configuration("此平台暂不支持系统词典。".into()))?;
        let text = text.trim().to_owned();
        if text.is_empty() || text.chars().count() > 200 {
            return Err(provider::Error::Configuration(
                "系统词典用于查询单词或短语，请缩短原文后重试。".into(),
            ));
        }
        let started = tokio::time::Instant::now();
        let result = tokio::select! {biased; _=cancel.cancelled()=>return Err(provider::Error::Cancelled), result=tokio::task::spawn_blocking(move||dictionary.lookup(&text))=>result.map_err(|_|provider::Error::Protocol("系统词典查询失败。".into()))?.map_err(provider::Error::Protocol)?};
        if result.trim().is_empty() {
            return Err(provider::Error::Empty);
        }
        let ms = started.elapsed().as_millis() as u64;
        on_text(&result, ms)?;
        Ok(provider::Metrics {
            headers_ms: 0,
            first_content_ms: ms,
            total_ms: ms,
            characters: result.chars().count(),
        })
    }

    pub async fn save_configuration(
        &self,
        settings: Settings,
        updates: BTreeMap<String, Option<String>>,
    ) -> Result<Configuration, String> {
        let _guard = self.mutations.lock().await;
        self.save_configuration_locked(settings, updates).await
    }

    pub async fn save_preferences(
        &self,
        preferences: crate::config::DisplayPreferences,
    ) -> Result<Configuration, String> {
        let _guard = self.mutations.lock().await;
        let mut settings = self.live.read().await.settings.clone();
        preferences.apply(&mut settings);
        self.save_configuration_locked(settings, BTreeMap::new())
            .await
    }

    async fn save_configuration_locked(
        &self,
        mut settings: Settings,
        updates: BTreeMap<String, Option<String>>,
    ) -> Result<Configuration, String> {
        settings.validate()?;
        for (id, value) in &updates {
            if !settings.profiles.iter().any(|p| &p.id == id) {
                return Err("密钥对应的服务不存在。".into());
            }
            if value
                .as_ref()
                .is_some_and(|v| v.len() > 8192 || v.contains(['\r', '\n']))
            {
                return Err("API Key 过长或包含换行。".into());
            }
        }
        let mut live = self.live.write().await;
        let mut changed = updates;
        for old in &live.settings.profiles {
            if !settings.profiles.iter().any(|p| p.id == old.id) {
                changed.insert(old.id.clone(), None);
            }
        }
        for profile in &mut settings.profiles {
            if let Some(old) = live.settings.profiles.iter().find(|p| p.id == profile.id) {
                profile.revision = old.revision;
                if profile != old || changed.contains_key(&profile.id) {
                    profile.revision = old.revision.saturating_add(1);
                }
            } else {
                profile.revision = 1;
            }
        }
        let mut rollback = BTreeMap::new();
        for id in changed.keys() {
            if live.errors.contains_key(id) {
                return Err(
                    "原密钥无法读取，暂不能安全替换。请先解锁系统钥匙串并重启应用。".into(),
                );
            }
            rollback.insert(id.clone(), live.keys.get(id).cloned());
        }
        let vault = self.credentials.clone();
        let changes = changed.clone();
        let backup = rollback.clone();
        tokio::task::spawn_blocking(move || {
            let mut written: Vec<String> = vec![];
            for (id, value) in &changes {
                if let Err(error) = vault.write(id, value.as_deref()) {
                    let mut restored = true;
                    for prior in written {
                        let old = backup
                            .get(&prior)
                            .and_then(|v| v.as_ref())
                            .map(|v| v.as_str());
                        restored &= vault.write(&prior, old).is_ok();
                    }
                    return Err(if restored {
                        error
                    } else {
                        "系统凭据更新和回滚均未完成，请检查钥匙串后重新配置。".into()
                    });
                }
                written.push(id.clone());
            }
            Ok(())
        })
        .await
        .map_err(|_| "系统凭据操作失败。")??;
        if settings.retention == Retention::Disabled
            && live.settings.retention != Retention::Disabled
        {
            // Preserve currently loaded screenshots in the bounded session pool
            // before removing their disk copies. Older entries may be evicted.
            let tasks = self.book.snapshot().tasks;
            let protected: HashSet<_> = tasks
                .iter()
                .filter(|t| !t.status.is_terminal())
                .map(|t| t.id.clone())
                .collect();
            for task in tasks.iter().rev().filter(|t| t.has_image) {
                let present = self.images.lock().unwrap().data.contains_key(&task.id);
                if !present && let Ok(Some(data)) = self.storage.image(task.id.clone()).await {
                    let _ = self.images.lock().unwrap().insert(
                        task.id.clone(),
                        Arc::new(data),
                        &protected,
                    );
                }
            }
        }
        let ids: Vec<_> = self
            .book
            .snapshot()
            .tasks
            .iter()
            .map(|t| t.id.clone())
            .collect();
        if let Err(error) = self.storage.save_settings(settings.clone(), ids).await {
            let vault = self.credentials.clone();
            let restored = tokio::task::spawn_blocking(move || {
                let mut restored = true;
                for (id, value) in rollback {
                    restored &= vault.write(&id, value.as_ref().map(|v| v.as_str())).is_ok();
                }
                restored
            })
            .await
            .unwrap_or(false);
            return Err(if restored {
                error
            } else {
                "设置保存失败，密钥回滚也失败，请重新设置密钥。".into()
            });
        }
        for (id, value) in changed {
            if let Some(value) = value {
                live.keys
                    .insert(id.clone(), Arc::new(Zeroizing::new(value)));
            } else {
                live.keys.remove(&id);
            }
            live.errors.remove(&id);
        }
        let retention_changed = live.settings.retention != settings.retention;
        self.sensitive_guard
            .store(settings.sensitive_guard, Ordering::Relaxed);
        self.auto_vocabulary
            .store(settings.auto_vocabulary, Ordering::Relaxed);
        live.settings = settings;
        if retention_changed {
            let affected = self
                .book
                .reconfigure_retention(live.settings.retention, (self.clock)());
            for task in affected {
                if task.expired((self.clock)()) {
                    self.images.lock().unwrap().remove(&task.id);
                    self.pending_sensitive.lock().unwrap().remove(&task.id);
                    self.approved_sensitive.lock().unwrap().remove(&task.id);
                    self.pending_images.lock().unwrap().remove(&task.id);
                    if let Some((_, token)) = self.jobs.lock().unwrap().remove(&task.id) {
                        token.cancel();
                    }
                } else if live.settings.retention == Retention::Disabled
                    && task.has_image
                    && task.status.is_terminal()
                    && !self.images.lock().unwrap().data.contains_key(&task.id)
                {
                    self.book.remove(&task.id);
                }
            }
        }
        drop(live);
        self.wake.notify_one();
        self.trim();
        Ok(self.configuration().await)
    }

    pub async fn shutdown(&self) {
        self.closing.store(true, Ordering::Relaxed);
        self.pending_sensitive.lock().unwrap().clear();
        self.approved_sensitive.lock().unwrap().clear();
        self.pending_images.lock().unwrap().clear();
        self.wake.notify_one();
        let tasks = self.book.snapshot().tasks;
        for task in &tasks {
            self.book.finish_vocabulary(
                &task.id,
                task.attempt,
                VocabularyState::Cancelled,
                None,
                self.now(),
            );
            if !task.status.is_terminal() {
                self.book.interrupt(&task.id, (self.clock)());
            }
        }
        for (_, token) in self.jobs.lock().unwrap().values() {
            token.cancel();
        }
        for task in tasks {
            self.persist(&task.id).await;
        }
        let _ = self.storage.flush().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::Status;
    use std::sync::atomic::AtomicUsize;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[derive(Default)]
    struct MemoryCredentials(Mutex<BTreeMap<String, String>>);
    impl Credentials for MemoryCredentials {
        fn read(&self, id: &str) -> Result<Option<String>, String> {
            Ok(self.0.lock().unwrap().get(id).cloned())
        }
        fn write(&self, id: &str, value: Option<&str>) -> Result<(), String> {
            let mut m = self.0.lock().unwrap();
            if let Some(v) = value {
                m.insert(id.into(), v.into());
            } else {
                m.remove(id);
            }
            Ok(())
        }
    }

    async fn server() -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let observed = count.clone();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let count = observed.clone();
                tokio::spawn(async move {
                    let mut input = vec![0; 8192];
                    let _ = socket.read(&mut input).await;
                    count.fetch_add(1, Ordering::SeqCst);
                    if socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").await.is_err(){return;}
                    for (content, finish) in [("后台", None), ("译文", Some("stop"))] {
                        tokio::time::sleep(Duration::from_millis(25)).await;
                        let line = format!(
                            "data: {}\n\n",
                            serde_json::json!({"choices":[{"delta":{"content":content},"finish_reason":finish}]})
                        );
                        if socket.write_all(line.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });
        (format!("http://{addr}/v1"), count)
    }

    async fn configured(root: PathBuf, retention: Retention) -> (Arc<Engine>, Arc<AtomicUsize>) {
        let (endpoint, count) = server().await;
        let engine = Engine::open(
            root,
            Arc::new(MemoryCredentials::default()),
            storage::system_clock(),
        )
        .await
        .unwrap();
        let mut settings = engine.configuration().await.settings;
        settings.profiles[0].endpoint = endpoint;
        settings.profiles[0].model = "fixture".into();
        settings.retention = retention;
        engine
            .save_configuration(settings, BTreeMap::new())
            .await
            .unwrap();
        (engine, count)
    }

    async fn until(engine: &Engine, id: &str, predicate: impl Fn(&Task) -> bool) -> Task {
        tokio::time::timeout(Duration::from_secs(3), async {
            let (_, mut events) = engine.book.subscribe();
            loop {
                if let Some(t) = engine.book.get(id)
                    && predicate(&t)
                {
                    return t;
                }
                let _ = events.recv().await;
            }
        })
        .await
        .expect("task did not reach its expected state")
    }

    #[tokio::test]
    async fn system_dictionary_needs_no_model_key_or_language_change() {
        struct Dictionary(Arc<AtomicUsize>);
        impl LocalDictionary for Dictionary {
            fn lookup(&self, text: &str) -> Result<String, String> {
                assert_eq!(text, "hello");
                self.0.fetch_add(1, Ordering::SeqCst);
                Ok("a greeting".into())
            }
        }
        let root = tempfile::tempdir().unwrap();
        let lookups = Arc::new(AtomicUsize::new(0));
        let engine = Engine::open_with_dictionary(
            root.path().into(),
            Arc::new(MemoryCredentials::default()),
            storage::system_clock(),
            Some(Arc::new(Dictionary(lookups.clone()))),
        )
        .await
        .unwrap();
        let mut settings = engine.configuration().await.settings;
        settings.profiles[0].provider = crate::config::Provider::SystemDictionary;
        settings.source_language = "en".into();
        settings.target_language = "en".into();
        settings.retention = Retention::Disabled;
        engine
            .save_configuration(settings, BTreeMap::new())
            .await
            .unwrap();
        let id = engine.manual("hello".into()).await.unwrap();
        let task = until(&engine, &id, |t| t.status == Status::Completed).await;
        assert_eq!(task.translation, "a greeting");
        assert_eq!(lookups.load(Ordering::SeqCst), 1);
        assert!(!task.cache_hit);
        engine.shutdown().await;
    }

    #[tokio::test]
    async fn detached_window_completes_and_repeat_query_uses_the_persisted_cache() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, count) = configured(dir.path().into(), Retention::Days(7)).await;
        let (_, view) = engine.book.subscribe();
        let id = engine.manual("hello".into()).await.unwrap();
        drop(view);
        let first = until(&engine, &id, |t| {
            t.status == Status::Completed && t.save_state == SaveState::Saved
        })
        .await;
        assert_eq!(first.translation, "后台译文");
        let repeated = engine.manual("hello".into()).await.unwrap();
        let second = until(&engine, &repeated, |t| {
            t.status == Status::Completed && t.save_state == SaveState::Saved
        })
        .await;
        assert!(second.cache_hit);
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(engine.storage.history(20, 0).await.unwrap().len(), 2);
        engine.shutdown().await;
    }

    #[tokio::test]
    async fn deleting_a_streaming_task_removes_source_and_cannot_be_undone_by_completion() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = configured(dir.path().into(), Retention::Days(7)).await;
        let id = engine.manual("remove me".into()).await.unwrap();
        until(&engine, &id, |t| !t.translation.is_empty()).await;
        engine.remove(&id).await.unwrap();
        tokio::time::sleep(Duration::from_millis(70)).await;
        assert!(engine.book.get(&id).is_none());
        assert!(engine.storage.task(id).await.unwrap().is_none());
        engine.shutdown().await;
    }

    #[tokio::test]
    async fn failed_disk_writes_do_not_prevent_translation() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = configured(dir.path().into(), Retention::Days(7)).await;
        engine.storage.reject_writes_for_test().await;
        let id = engine.manual("keep translating".into()).await.unwrap();
        let task = until(&engine, &id, |t| {
            t.status == Status::Completed && t.save_state == SaveState::Failed
        })
        .await;
        assert_eq!(task.translation, "后台译文");
        assert!(task.save_error.is_some());
        engine.shutdown().await;
    }

    #[tokio::test]
    async fn session_only_results_survive_window_detach_without_disk_history() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, count) = configured(dir.path().into(), Retention::Disabled).await;
        let id = engine.manual("session only".into()).await.unwrap();
        let task = until(&engine, &id, |t| t.status == Status::Completed).await;
        assert_eq!(task.save_state, SaveState::MemoryOnly);
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(engine.storage.history(20, 0).await.unwrap().is_empty());
        let (restored, _) = engine.book.subscribe();
        assert_eq!(restored.tasks[0].translation, "后台译文");
        engine.shutdown().await;
    }

    #[tokio::test]
    async fn stopping_and_retrying_preserves_original_and_ignores_late_capture_errors() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = configured(dir.path().into(), Retention::Days(7)).await;
        let id = engine.manual("original".into()).await.unwrap();
        until(&engine, &id, |t| !t.translation.is_empty()).await;
        engine.stop(&id);
        engine.retry(&id, Some("corrected".into())).await.unwrap();
        engine.fail_capture(&id, 1, "late OCR error".into());
        let task = until(&engine, &id, |t| {
            t.status == Status::Completed && t.attempt == 2
        })
        .await;
        assert_eq!(task.original_text, "original");
        assert_eq!(task.query_text(), "corrected");
        assert_eq!(task.translation, "后台译文");
        engine.shutdown().await;
    }
    #[tokio::test]
    async fn same_language_expansion_completes_locally_and_persists_the_source() {
        let root = tempfile::tempdir().unwrap();
        let (engine, requests) = configured(root.path().into(), Retention::Days(7)).await;
        let handle = engine.begin(Source::Screenshot).await.unwrap();
        engine.accept_image(&handle.id, b"\x89PNG\r\n\x1a\nfixture".to_vec());
        engine.accept_detected_text(
            &handle,
            "这是已经可以阅读的中文原文。job 的 status 是 needs_reconciliation，任务归属于 muse03。".into(),
            12,
            Some("zh-Hans".into()),
        );
        let task = until(&engine, &handle.id, |t| {
            t.status == Status::Ready && t.save_state == SaveState::Saved
        })
        .await;
        assert!(task.has_image);
        assert!(task.translation.is_empty());
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        assert_eq!(
            engine
                .storage
                .task(task.id.clone())
                .await
                .unwrap()
                .unwrap()
                .status,
            Status::Ready
        );
        assert_eq!(
            engine
                .storage
                .search_history("中文原文".into(), "screenshot".into(), 0)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            engine
                .storage
                .search_history("中文原文".into(), "selection".into(), 0)
                .await
                .unwrap()
                .is_empty()
        );
        // Setting up or changing a service after a source-only capture must not
        // make the explicit Translate action unusable.
        let mut settings = engine.configuration().await.settings;
        settings.profiles[0].name = "Updated service".into();
        engine
            .save_configuration(settings, BTreeMap::new())
            .await
            .unwrap();
        engine.retry(&task.id, None).await.unwrap();
        let translated = until(&engine, &task.id, |t| t.status == Status::Completed).await;
        assert_eq!(translated.original_text, task.original_text);
        assert!(translated.has_image);
        assert_eq!(translated.translation, task.original_text);
        assert!(!translated.cache_hit);
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        let saved = until(&engine, &task.id, |t| t.save_state == SaveState::Saved).await;
        assert_eq!(saved.translation, task.original_text);
        engine.shutdown().await;
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn native_detection_of_technical_chinese_completes_without_http_requests() {
        let root = tempfile::tempdir().unwrap();
        let (engine, requests) = configured(root.path().into(), Retention::Disabled).await;
        let source = "查到了：两个题目其实都有job，归属都是muse03，目前处于 needs_reconciliation（待核对恢复），租约分别在今晚22:27:47和22:28:28过期。\n所以不是单纯刷新版本号能解决的问题：7/8是原稿版本，和认领租约是两套状态。按题目查询、不限制status时能看到job，原先列表可能过滤掉了这种状态。我再确认恢复路径，不能直接当作无任务清锁。";
        let detected = glossa_platform::detect_language(source);
        assert_eq!(detected.as_deref(), Some("zh-Hans"));
        let capture = engine.begin(Source::Selection).await.unwrap();
        engine.accept_detected_text(&capture, source.into(), 0, detected.clone());
        until(&engine, &capture.id, |task| task.status == Status::Ready).await;
        engine
            .retry_languages(
                &capture.id,
                Some(source.into()),
                Some((None, "zh-Hans".into())),
                detected,
            )
            .await
            .unwrap();
        let task = until(&engine, &capture.id, |task| {
            task.status == Status::Completed
        })
        .await;
        assert_eq!(task.translation, source);
        assert_eq!(task.timings.first_content_ms, None);
        assert!(!task.cache_hit);
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        engine.shutdown().await;
    }

    #[tokio::test]
    async fn default_source_language_is_captured_and_isolated_from_later_settings() {
        let root = tempfile::tempdir().unwrap();
        let (engine, requests) = configured(root.path().into(), Retention::Disabled).await;
        let mut settings = engine.configuration().await.settings;
        settings.source_language = "en".into();
        settings.target_language = "en".into();
        engine
            .save_configuration(settings, BTreeMap::new())
            .await
            .unwrap();
        let capture = engine.begin(Source::Selection).await.unwrap();
        assert_eq!(
            engine
                .book
                .get(&capture.id)
                .unwrap()
                .source_language
                .as_deref(),
            Some("en")
        );
        engine.accept_detected_text(
            &capture,
            "Keep the selected source.".into(),
            1,
            Some("fr".into()),
        );
        engine.retry(&capture.id, None).await.unwrap();
        let task = until(&engine, &capture.id, |t| t.status == Status::Completed).await;
        assert_eq!(task.translation, "Keep the selected source.");
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        let mut settings = engine.configuration().await.settings;
        settings.source_language.clear();
        engine
            .save_configuration(settings, BTreeMap::new())
            .await
            .unwrap();
        let next = engine.begin(Source::Selection).await.unwrap();
        assert!(engine.book.get(&next.id).unwrap().source_language.is_none());
        assert_eq!(
            engine
                .book
                .get(&capture.id)
                .unwrap()
                .source_language
                .as_deref(),
            Some("en")
        );
        engine.shutdown().await;
    }

    #[tokio::test]
    async fn immediate_preferences_preserve_service_credentials_and_retention() {
        let root = tempfile::tempdir().unwrap();
        let (engine, requests) = configured(root.path().into(), Retention::Days(7)).await;
        let mut settings = engine.configuration().await.settings;
        settings.profiles[0].name = "Saved service".into();
        let mut keys = BTreeMap::new();
        keys.insert(settings.profiles[0].id.clone(), Some("fixture-key".into()));
        engine
            .save_configuration(settings.clone(), keys)
            .await
            .unwrap();
        let result = engine
            .save_preferences(crate::config::DisplayPreferences {
                target_language: "en".into(),
                ui_language: "follow_primary".into(),
                theme: "dark".into(),
                ui_font_size: 15,
                font_size: 18,
                auto_hide: false,
                max_concurrency: 2,
            })
            .await
            .unwrap();
        assert_eq!(result.settings.target_language, "en");
        assert_eq!(result.settings.profiles[0].name, "Saved service");
        assert_eq!(
            result.settings.profiles[0].endpoint,
            settings.profiles[0].endpoint
        );
        assert_eq!(
            result.settings.selection_shortcut,
            settings.selection_shortcut
        );
        assert_eq!(result.settings.retention, settings.retention);
        assert!(result.has_keys.contains(&settings.profiles[0].id));
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        engine.shutdown().await;
    }

    #[tokio::test]
    async fn explicit_same_language_works_without_a_configured_model() {
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::open(
            root.path().into(),
            Arc::new(MemoryCredentials::default()),
            storage::system_clock(),
        )
        .await
        .unwrap();
        let capture = engine.begin(Source::Selection).await.unwrap();
        engine.accept_detected_text(
            &capture,
            "Keep this text exactly.\nIncluding its newlines.".into(),
            1,
            Some("fr".into()),
        );
        engine
            .retry_languages(
                &capture.id,
                None,
                Some((Some("en".into()), "en".into())),
                None,
            )
            .await
            .unwrap();
        let task = until(&engine, &capture.id, |t| t.status == Status::Completed).await;
        assert_eq!(task.translation, task.original_text);
        assert!(task.error.is_none());
        assert!(!task.cache_hit);
        assert!(task.timings.first_content_ms.is_none());
        engine.shutdown().await;
    }

    #[tokio::test]
    async fn edited_source_invalidates_old_detection_and_new_detection_can_skip_network() {
        let root = tempfile::tempdir().unwrap();
        let (engine, requests) = configured(root.path().into(), Retention::Disabled).await;
        let capture = engine.begin(Source::Selection).await.unwrap();
        engine.accept_detected_text(&capture, "中文原文".into(), 1, Some("zh-Hans".into()));
        engine
            .retry_languages(
                &capture.id,
                Some("Edited English".into()),
                Some((None, "zh-Hans".into())),
                None,
            )
            .await
            .unwrap();
        let task = until(&engine, &capture.id, |t| t.status == Status::Completed).await;
        assert_eq!(task.detected_language, None);
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        engine
            .retry_languages(
                &capture.id,
                Some("新的中文原文".into()),
                Some((None, "zh-Hans".into())),
                Some("zh-Hans".into()),
            )
            .await
            .unwrap();
        let task = until(&engine, &capture.id, |t| {
            t.status == Status::Completed && t.attempt == 2
        })
        .await;
        assert_eq!(task.translation, "新的中文原文");
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        engine
            .retry_languages(
                &capture.id,
                Some("中文原文".into()),
                Some((None, "zh-Hans".into())),
                Some("zh-Hans".into()),
            )
            .await
            .unwrap();
        let task = until(&engine, &capture.id, |t| {
            t.status == Status::Completed && t.attempt == 3
        })
        .await;
        assert_eq!(task.translation, "中文原文");
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        engine.shutdown().await;
    }

    #[tokio::test]
    async fn chinese_script_conversion_and_unknown_source_still_use_the_model() {
        let root = tempfile::tempdir().unwrap();
        let (engine, requests) = configured(root.path().into(), Retention::Disabled).await;
        for detected in [Some("zh-Hant".into()), None] {
            let capture = engine.begin(Source::Selection).await.unwrap();
            engine.accept_detected_text(&capture, "測試文字".into(), 1, detected);
            engine.retry(&capture.id, None).await.unwrap();
            until(&engine, &capture.id, |t| t.status == Status::Completed).await;
        }
        assert_eq!(requests.load(Ordering::SeqCst), 2);
        engine.shutdown().await;
    }

    #[tokio::test]
    async fn per_task_languages_do_not_change_primary_language() {
        let root = tempfile::tempdir().unwrap();
        let (engine, requests) = configured(root.path().into(), Retention::Days(7)).await;
        let capture = engine.begin(Source::Selection).await.unwrap();
        engine.accept_detected_text(&capture, "Hello there".into(), 1, Some("en".into()));
        engine
            .retry_languages(
                &capture.id,
                None,
                Some((Some("en".into()), "fr".into())),
                None,
            )
            .await
            .unwrap();
        let task = until(&engine, &capture.id, |t| t.status == Status::Completed).await;
        assert_eq!(task.target_language, "fr");
        assert_eq!(task.source_language.as_deref(), Some("en"));
        assert_eq!(
            engine.configuration().await.settings.target_language,
            "zh-Hans"
        );
        let hash = storage::cache_key(&task);
        let mut alternate = task.clone();
        alternate.source_language = Some("de".into());
        assert_ne!(hash, storage::cache_key(&alternate));
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        engine.shutdown().await;
    }

    #[tokio::test]
    async fn every_capture_waits_for_confirmation_and_duplicate_submit_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let (engine, requests) = configured(root.path().into(), Retention::Disabled).await;
        for detected in [None, Some("en".into())] {
            let handle = engine.begin(Source::Selection).await.unwrap();
            engine.accept_detected_text(
                &handle,
                "This text must still be translated.".into(),
                1,
                detected.clone(),
            );
            assert_eq!(engine.book.get(&handle.id).unwrap().status, Status::Ready);
            assert_eq!(
                requests.load(Ordering::SeqCst),
                if detected.is_none() { 0 } else { 1 }
            );
            engine.retry(&handle.id, None).await.unwrap();
            assert!(engine.retry(&handle.id, None).await.is_err());
            until(&engine, &handle.id, |t| t.status == Status::Completed).await;
        }
        assert_eq!(requests.load(Ordering::SeqCst), 2);
    }
    #[tokio::test]
    async fn sensitive_capture_is_masked_and_never_persisted_or_sent_until_explicit_consent() {
        let root = tempfile::tempdir().unwrap();
        let (engine, requests) = configured(root.path().into(), Retention::Forever).await;
        let secret = format!("sk-{}", "synthetic".repeat(5));
        let handle = engine.begin(Source::Screenshot).await.unwrap();
        engine.accept_image(&handle.id, vec![7; 100]);
        assert!(!engine.book.get(&handle.id).unwrap().has_image);
        engine.accept_text(&handle, secret.clone(), 0);
        let task = engine.book.get(&handle.id).unwrap();
        assert!(task.sensitive.is_some());
        assert!(task.original_text.is_empty());
        assert!(task.translation.is_empty());
        assert!(engine.image(&handle.id).await.unwrap().is_none());
        assert!(engine.storage.history(100, 0).await.unwrap().is_empty());
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        assert!(engine.retry(&handle.id, None).await.is_err());
        engine.resolve_sensitive(&handle.id, true).await.unwrap();
        until(&engine, &handle.id, |t| t.status == Status::Completed).await;
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(
            engine
                .storage
                .task(handle.id.clone())
                .await
                .unwrap()
                .is_none()
        );
        engine
            .retry(&handle.id, Some(format!("sk-{}", "different".repeat(5))))
            .await
            .unwrap();
        assert!(engine.book.get(&handle.id).unwrap().sensitive.is_some());
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        engine.resolve_sensitive(&handle.id, false).await.unwrap();
        assert!(engine.book.get(&handle.id).is_none());
        assert!(engine.pending_sensitive.lock().unwrap().is_empty());
        engine.shutdown().await;
    }
    #[tokio::test]
    async fn manual_input_and_disabled_guard_obey_the_same_boundary() {
        let root = tempfile::tempdir().unwrap();
        let (engine, requests) = configured(root.path().into(), Retention::Disabled).await;
        let secret = "abandon ".repeat(12);
        let id = engine.manual(secret.clone()).await.unwrap();
        assert!(engine.book.get(&id).unwrap().sensitive.is_some());
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        engine.resolve_sensitive(&id, false).await.unwrap();
        let mut settings = engine.configuration().await.settings;
        settings.sensitive_guard = false;
        engine
            .save_configuration(settings, BTreeMap::new())
            .await
            .unwrap();
        let id = engine.manual(secret).await.unwrap();
        until(&engine, &id, |t| t.status == Status::Completed).await;
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        engine.shutdown().await;
    }
    #[tokio::test]
    async fn completed_task_retries_with_the_new_default_service() {
        let root = tempfile::tempdir().unwrap();
        let (engine, first) = configured(root.path().into(), Retention::Disabled).await;
        let id = engine.manual("A useful sentence.".into()).await.unwrap();
        until(&engine, &id, |t| t.status == Status::Completed).await;
        let (endpoint, second) = server().await;
        let mut settings = engine.configuration().await.settings;
        let mut p = settings.profiles[0].clone();
        p.id = "new-default".into();
        p.endpoint = endpoint;
        settings.profiles.push(p);
        settings.active_profile = "new-default".into();
        engine
            .save_configuration(settings, BTreeMap::new())
            .await
            .unwrap();
        engine.retry(&id, None).await.unwrap();
        let task = until(&engine, &id, |t| t.status == Status::Completed).await;
        assert_eq!(task.profile.id, "new-default");
        assert_eq!(first.load(Ordering::SeqCst), 1);
        assert_eq!(second.load(Ordering::SeqCst), 1);
        assert_eq!(task.past_attempts[0].profile.id, "default");
        engine.shutdown().await;
    }
    #[tokio::test]
    async fn two_part_stream_keeps_json_out_of_task_and_restores_vocabulary_from_cache() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 16384];
            let _ = socket.read(&mut request).await.unwrap();
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").await.unwrap();
            for content in [
                "坚韧的团队。\n---VO",
                "CAB_JSON---",
                r#"[{"word":"resilient","surface":"resilient","phonetic":"/rɪˈzɪliənt/","pos":"adj.","meaning":"坚韧的","level":"B2"}]"#,
            ] {
                let event = format!(
                    "data: {}\n\n",
                    serde_json::json!({"choices":[{"delta":{"content":content},"finish_reason":null}]})
                );
                socket.write_all(event.as_bytes()).await.unwrap();
            }
            socket.write_all(b"data: [DONE]\n\n").await.unwrap();
        });
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::open(
            root.path().into(),
            Arc::new(MemoryCredentials::default()),
            storage::system_clock(),
        )
        .await
        .unwrap();
        let mut settings = engine.configuration().await.settings;
        settings.profiles[0].endpoint = format!("http://{address}/v1");
        settings.profiles[0].model = "fixture".into();
        engine
            .save_configuration(settings, BTreeMap::new())
            .await
            .unwrap();
        let id = engine.manual("A resilient team.".into()).await.unwrap();
        let first = until(&engine, &id, |t| {
            t.status == Status::Completed
                && t.save_state == SaveState::Saved
                && t.vocabulary_state == VocabularyState::Complete
        })
        .await;
        assert_eq!(first.translation, "坚韧的团队。\n");
        assert_eq!(first.vocabulary.len(), 1);
        assert_eq!(first.vocabulary[0].context_sentence, "A resilient team.");
        assert_eq!(first.vocabulary[0].context_meaning, "坚韧的");
        assert_eq!(first.vocabulary[0].sentence_trans, "坚韧的团队。");
        let collected = engine
            .storage
            .collect_word(first.vocabulary[0].clone())
            .await
            .unwrap();
        assert_eq!(collected.contexts[0].context_sentence, "A resilient team.");
        assert_eq!(collected.contexts[0].context_meaning, "坚韧的");
        assert!(!first.translation.contains("VOCAB"));
        server.await.unwrap();
        let next = engine.manual("A resilient team.".into()).await.unwrap();
        let second = until(&engine, &next, |t| t.status == Status::Completed).await;
        assert!(second.cache_hit);
        assert_eq!(second.vocabulary, first.vocabulary);
        engine.shutdown().await;
    }
    #[tokio::test]
    async fn short_same_language_word_still_gets_dictionary_analysis() {
        let root = tempfile::tempdir().unwrap();
        let (engine, requests) = configured(root.path().into(), Retention::Disabled).await;
        let id = engine
            .manual_languages(
                "resilient".into(),
                Some((Some("en".into()), "en".into())),
                None,
            )
            .await
            .unwrap();
        until(&engine, &id, |t| t.status == Status::Completed).await;
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        engine.shutdown().await;
    }
    #[tokio::test]
    async fn delimiter_finishes_text_before_tail_and_stopping_keeps_durable_translation() {
        for action in ["stop", "capture", "shutdown"] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let (sent, received) = tokio::sync::oneshot::channel();
            let (release, tail) = tokio::sync::oneshot::channel::<()>();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buffer = [0u8; 16384];
                let _ = socket.read(&mut buffer).await;
                let _=socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").await;
                let event = format!(
                    "data: {}\n\n",
                    serde_json::json!({"choices":[{"delta":{"content":"完整译文。---VOCAB_JSON---["},"finish_reason":null}]})
                );
                let _ = socket.write_all(event.as_bytes()).await;
                let _ = sent.send(());
                let _ = tail.await;
            });
            let root = tempfile::tempdir().unwrap();
            let engine = Engine::open(
                root.path().into(),
                Arc::new(MemoryCredentials::default()),
                storage::system_clock(),
            )
            .await
            .unwrap();
            let mut settings = engine.configuration().await.settings;
            settings.profiles[0].endpoint = format!("http://{addr}/v1");
            settings.profiles[0].model = "fixture".into();
            engine
                .save_configuration(settings, BTreeMap::new())
                .await
                .unwrap();
            let id = engine
                .manual("This is a complete sentence.".into())
                .await
                .unwrap();
            received.await.unwrap();
            let task = until(&engine, &id, |t| t.status == Status::Completed).await;
            assert_eq!(task.translation, "完整译文。");
            assert_eq!(engine.permits.available_permits(), 2);
            match action {
                "capture" => {
                    engine.begin(Source::Selection).await.unwrap();
                }
                "shutdown" => engine.shutdown().await,
                _ => engine.stop(&id),
            }
            let saved = until(&engine, &id, |t| {
                t.status == Status::Completed && t.save_state == SaveState::Saved
            })
            .await;
            assert_eq!(
                engine.storage.task(id).await.unwrap().unwrap().translation,
                saved.translation
            );
            assert!(saved.vocabulary.is_empty());
            let _ = release.send(());
            server.await.unwrap();
            engine.shutdown().await;
        }
    }
    #[tokio::test]
    async fn broken_metadata_tail_does_not_fail_completed_text() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut b = [0u8; 8192];
            let _ = socket.read(&mut b).await;
            let _=socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").await;
            let event = format!(
                "data: {}\n\n",
                serde_json::json!({"choices":[{"delta":{"content":"已完成译文。---VOCAB_JSON---["},"finish_reason":null}]})
            );
            let _ = socket.write_all(event.as_bytes()).await;
        });
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::open(
            root.path().into(),
            Arc::new(MemoryCredentials::default()),
            storage::system_clock(),
        )
        .await
        .unwrap();
        let mut settings = engine.configuration().await.settings;
        settings.profiles[0].endpoint = format!("http://{addr}/v1");
        settings.profiles[0].model = "fixture".into();
        engine
            .save_configuration(settings, BTreeMap::new())
            .await
            .unwrap();
        let id = engine
            .manual("A complete source sentence.".into())
            .await
            .unwrap();
        let task = until(&engine, &id, |t| {
            t.vocabulary_state == VocabularyState::Failed && t.save_state == SaveState::Saved
        })
        .await;
        assert_eq!(task.status, Status::Completed);
        assert_eq!(task.translation, "已完成译文。");
        assert!(task.error.is_none());
        assert!(storage::cache_key(&task).is_none());
        engine.shutdown().await;
    }
    #[tokio::test]
    async fn extraction_policy_is_snapshotted_and_partitions_translation_cache() {
        let root = tempfile::tempdir().unwrap();
        let (engine, requests) = configured(root.path().into(), Retention::Days(7)).await;
        let long = engine.manual(vec!["xo"; 151].join(" ")).await.unwrap();
        let task = until(&engine, &long, |t| t.status == Status::Completed).await;
        assert!(!task.extract_vocabulary);
        assert_eq!(task.vocabulary_state, VocabularyState::Disabled);
        let mut settings = engine.configuration().await.settings;
        settings.auto_vocabulary = false;
        engine
            .save_configuration(settings.clone(), BTreeMap::new())
            .await
            .unwrap();
        let first = engine.manual("A resilient team.".into()).await.unwrap();
        let first = until(&engine, &first, |t| {
            t.status == Status::Completed && t.save_state == SaveState::Saved
        })
        .await;
        assert!(!first.extract_vocabulary);
        settings.auto_vocabulary = true;
        engine
            .save_configuration(settings, BTreeMap::new())
            .await
            .unwrap();
        let second = engine.manual("A resilient team.".into()).await.unwrap();
        let second = until(&engine, &second, |t| t.status == Status::Completed).await;
        assert!(second.extract_vocabulary);
        assert!(!second.cache_hit);
        assert_ne!(storage::cache_key(&first), storage::cache_key(&second));
        assert_eq!(requests.load(Ordering::SeqCst), 3);
        engine.shutdown().await;
    }
    #[tokio::test]
    async fn pure_translation_does_not_interpret_a_literal_separator_in_source_output() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut b = [0u8; 8192];
            let _ = socket.read(&mut b).await;
            let _=socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").await;
            let event = format!(
                "data: {}\n\n",
                serde_json::json!({"choices":[{"delta":{"content":"文档中的 ---VOCAB_JSON--- 是示例。"},"finish_reason":"stop"}]})
            );
            let _ = socket.write_all(event.as_bytes()).await;
        });
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::open(
            root.path().into(),
            Arc::new(MemoryCredentials::default()),
            storage::system_clock(),
        )
        .await
        .unwrap();
        let mut settings = engine.configuration().await.settings;
        settings.auto_vocabulary = false;
        settings.profiles[0].endpoint = format!("http://{addr}/v1");
        settings.profiles[0].model = "fixture".into();
        engine
            .save_configuration(settings, BTreeMap::new())
            .await
            .unwrap();
        let id = engine
            .manual("The marker in the document is a literal example.".into())
            .await
            .unwrap();
        let task = until(&engine, &id, |t| t.status == Status::Completed).await;
        assert_eq!(task.translation, "文档中的 ---VOCAB_JSON--- 是示例。");
        assert_eq!(task.vocabulary_state, VocabularyState::Disabled);
        assert!(task.vocabulary.is_empty());
        server.await.unwrap();
        engine.shutdown().await;
    }
}
