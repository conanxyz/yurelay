use super::*;
use crate::source::{
    self, CleanRequest, SourceRequest, SourceSegment, SourceSnapshot, UpdateRequest,
    cleaner::{self, CleanResult},
    encounters::{self, Encounters, TokenRange},
    layout::OcrLayout,
};
use crate::source::{PendingAction, PendingSummary, SourceAsset};

#[derive(Clone)]
pub(super) struct PendingSource {
    task_id: String,
    expected_revision: u64,
    text: Zeroizing<String>,
    kind: Source,
    image: Option<Zeroizing<Vec<u8>>>,
    layout: Option<OcrLayout>,
    created_at: i64,
    sensitive: Option<crate::sensitive::SensitiveKind>,
    digest: String,
}

impl Engine {
    pub(super) fn source_auto_clean_capture(&self, id: &str) {
        if !self.auto_clean.load(Ordering::Relaxed) {
            return;
        }
        let Some(task) = self.book.get(id) else {
            return;
        };
        let Ok(result) = self.source_clean_text(task.query_text(), cleaner::CleanMode::Pdf) else {
            return;
        };
        if result.text == task.query_text() {
            return;
        }
        let mut snapshot = task.source_snapshot();
        snapshot.edited_text = Some(result.text);
        if let Some(changed) = self.book.apply_cleaned_source(
            id,
            task.source_revision,
            snapshot,
            task.query_text().into(),
            false,
            self.now(),
        ) {
            self.book.source_detected(
                id,
                changed.source_revision,
                task.detected_language,
                self.now(),
            );
        }
    }
    pub fn source_forget(&self, id: &str) {
        self.source_pending
            .lock()
            .unwrap()
            .retain(|_, p| p.task_id != id);
        let prefix = format!("{id}--");
        let mut images = self.images.lock().unwrap();
        let ids: Vec<_> = images
            .data
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect();
        for key in ids {
            images.remove(&key);
        }
    }
    pub fn source_forget_all(&self) {
        self.source_pending.lock().unwrap().clear();
    }
    pub fn source_pending(&self, task_id: Option<&str>) -> Vec<PendingSummary> {
        let now = self.now();
        let mut pending = self.source_pending.lock().unwrap();
        pending.retain(|_, p| now - p.created_at < 600_000);
        pending
            .iter()
            .filter(|(_, p)| task_id.is_none_or(|id| p.task_id == id))
            .map(|(id, p)| PendingSummary {
                id: id.clone(),
                task_id: p.task_id.clone(),
                expected_revision: p.expected_revision,
                sensitive: p.sensitive,
                reason: if p.sensitive.is_some() {
                    "sensitive"
                } else {
                    "stale"
                }
                .into(),
            })
            .collect()
    }
    pub async fn source_for_encounter(&self, request: &SourceRequest) -> Result<String, String> {
        let task = self.checked_source(request).await?;
        self.guard_source(task.query_text())?;
        Ok(task.query_text().into())
    }
    async fn checked_source(&self, request: &SourceRequest) -> Result<Task, String> {
        let task = self.load_into_book(&request.task_id).await?;
        if task.source_revision != request.expected_revision {
            return Err("原文已变化，请重新操作。".into());
        }
        if !task.status.is_terminal() {
            return Err("正文正在生成，请先停止或等待完成后再修改原文。".into());
        }
        if task.sensitive.is_some() {
            return Err("请先处理敏感内容。".into());
        }
        Ok(task)
    }
    fn guard_source(&self, text: &str) -> Result<(), String> {
        source::validate_text(text)?;
        if self.sensitive_guard.load(Ordering::Relaxed) && crate::sensitive::detect(text).is_some()
        {
            return Err("检测到疑似敏感内容，原安全稿未改变。".into());
        }
        Ok(())
    }
    async fn apply_source_snapshot(
        self: &Arc<Self>,
        task: &Task,
        snapshot: SourceSnapshot,
        remember: bool,
        memory_only: bool,
    ) -> Result<Task, String> {
        self.apply_source_snapshot_with_cleanup(task, snapshot, remember, memory_only, None)
            .await
    }
    async fn apply_source_snapshot_with_cleanup(
        self: &Arc<Self>,
        task: &Task,
        snapshot: SourceSnapshot,
        remember: bool,
        memory_only: bool,
        cleanup_undo: Option<String>,
    ) -> Result<Task, String> {
        self.guard_source(&snapshot.original_text)
            .or_else(|error| if memory_only { Ok(()) } else { Err(error) })?;
        source::validate_text(
            snapshot
                .edited_text
                .as_deref()
                .unwrap_or(&snapshot.original_text),
        )?;
        if task.vocabulary_state == VocabularyState::Extracting {
            self.stop(&task.id);
        }
        let changed = if let Some(text) = cleanup_undo {
            self.book.apply_cleaned_source(
                &task.id,
                task.source_revision,
                snapshot,
                text,
                memory_only,
                self.now(),
            )
        } else {
            self.book.apply_source(
                &task.id,
                task.source_revision,
                snapshot,
                remember,
                memory_only,
                self.now(),
            )
        }
        .ok_or("原文已变化或任务已删除，请重新操作。")?;
        if memory_only {
            self.storage.remove(vec![task.id.clone()]).await?;
        } else {
            self.approved_sensitive.lock().unwrap().remove(&task.id);
            self.persist(&task.id).await;
        }
        Ok(self.book.get(&task.id).unwrap_or(changed))
    }
    pub async fn source_update(self: &Arc<Self>, request: UpdateRequest) -> Result<Task, String> {
        let _guard = self.mutations.lock().await;
        let task = self.checked_source(&request.source).await?;
        self.guard_source(&request.text)?;
        let mut snapshot = task.source_snapshot();
        snapshot.edited_text = (request.text != task.original_text).then_some(request.text);
        let valid = |value: &str| {
            !value.is_empty()
                && value.len() <= 32
                && value.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
        };
        if let Some(language) = request.source_language {
            if !language.is_empty() && !valid(&language) {
                return Err("无效的原文语言。".into());
            }
            snapshot.source_language = (!language.is_empty()).then_some(language);
        }
        if let Some(language) = request.target_language {
            if !valid(&language) {
                return Err("无效的译文语言。".into());
            }
            snapshot.target_language = language;
        }

        if snapshot == task.source_snapshot() {
            return Ok(task);
        }
        self.apply_source_snapshot(&task, snapshot, true, false)
            .await
    }
    pub fn source_clean_text(
        &self,
        text: &str,
        mode: cleaner::CleanMode,
    ) -> Result<CleanResult, String> {
        self.guard_source(text)?;
        let result = cleaner::clean(text, mode);
        self.guard_source(&result.text)?;
        Ok(result)
    }
    pub async fn source_clean_preview(&self, request: CleanRequest) -> Result<CleanResult, String> {
        let task = self.checked_source(&request.source).await?;
        self.source_clean_text(task.query_text(), request.mode)
    }
    pub async fn source_clean_apply(
        self: &Arc<Self>,
        request: CleanRequest,
    ) -> Result<Task, String> {
        let _guard = self.mutations.lock().await;
        let task = self.checked_source(&request.source).await?;
        let result = self.source_clean_text(task.query_text(), request.mode)?;
        let mut snapshot = task.source_snapshot();
        snapshot.edited_text = (result.text != task.original_text).then_some(result.text);
        self.apply_source_snapshot(&task, snapshot, true, false)
            .await
    }
    pub async fn source_undo(self: &Arc<Self>, request: SourceRequest) -> Result<Task, String> {
        let _guard = self.mutations.lock().await;
        let task = self.checked_source(&request).await?;
        let snapshot = task
            .source_undo
            .last()
            .ok_or("没有可撤销的原文操作。")?
            .clone();
        self.guard_source(
            snapshot
                .edited_text
                .as_deref()
                .unwrap_or(&snapshot.original_text),
        )?;
        self.apply_source_snapshot(&task, snapshot, false, false)
            .await
    }
    pub async fn source_undo_cleanup(
        self: &Arc<Self>,
        request: SourceRequest,
    ) -> Result<Task, String> {
        let _guard = self.mutations.lock().await;
        let task = self.checked_source(&request).await?;
        let (_, text) = task
            .cleanup_undo
            .as_ref()
            .filter(|(revision, _)| *revision == task.source_revision)
            .ok_or("没有可撤销的自动整理。")?;
        self.guard_source(text)?;
        let mut snapshot = task.source_snapshot();
        snapshot.edited_text = (text != &task.original_text).then(|| text.clone());
        self.apply_source_snapshot(&task, snapshot, true, false)
            .await
    }
    pub async fn source_restore(self: &Arc<Self>, request: SourceRequest) -> Result<Task, String> {
        let _guard = self.mutations.lock().await;
        let task = self.checked_source(&request).await?;
        self.guard_source(&task.original_text)?;
        let mut snapshot = task.source_snapshot();
        snapshot.edited_text = None;
        self.apply_source_snapshot(&task, snapshot, true, false)
            .await
    }
    pub async fn source_remove_last(
        self: &Arc<Self>,
        request: SourceRequest,
    ) -> Result<Task, String> {
        let _guard = self.mutations.lock().await;
        let task = self.checked_source(&request).await?;
        if task.source_segments.len() < 2 {
            return Err("至少保留一个原文片段。".into());
        }
        // Undo preserves the full pre-append editor state; otherwise a dirty edit
        // cannot be reliably attributed to a segment, so require restore first.
        let mut snapshot = task.source_snapshot();
        let removed = snapshot.segments.pop().unwrap();
        snapshot.original_text = snapshot
            .segments
            .iter()
            .map(|s| s.original_text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        if let Some(edited) = snapshot.edited_text.as_mut() {
            let suffix = format!("\n\n{}", removed.original_text);
            if !edited.ends_with(&suffix) {
                return Err("编辑稿已改变，请先撤销编辑或恢复捕获原文。".into());
            }
            edited.truncate(edited.len() - suffix.len());
        }
        self.guard_source(
            snapshot
                .edited_text
                .as_deref()
                .unwrap_or(&snapshot.original_text),
        )?;
        self.apply_source_snapshot(&task, snapshot, true, false)
            .await
    }
    pub async fn source_layout_apply(
        self: &Arc<Self>,
        request: SourceRequest,
    ) -> Result<Task, String> {
        let _guard = self.mutations.lock().await;
        let task = self.checked_source(&request).await?;
        let mut texts = Vec::new();
        let mut found = false;
        for segment in &task.source_segments {
            if let Some(table) = segment.ocr_layout.as_ref().and_then(source::layout::table) {
                texts.push(table);
                found = true;
            } else {
                texts.push(segment.original_text.clone());
            }
        }
        if !found {
            return Err("未识别到可靠的基础表格，请对照原图编辑。".into());
        }
        let text = texts.join("\n\n");
        self.guard_source(&text)?;
        let mut snapshot = task.source_snapshot();
        snapshot.edited_text = Some(text);
        self.apply_source_snapshot(&task, snapshot, true, false)
            .await
    }
    pub fn accept_ocr_layout(self: &Arc<Self>, id: &str, layout: OcrLayout) {
        let Some(task) = self.book.get(id) else {
            return;
        };
        if task.sensitive.is_some()
            || task.original_text.is_empty()
            || self.approved_sensitive.lock().unwrap().contains_key(id)
        {
            return;
        }
        let raw = layout
            .blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if raw != task.original_text || self.guard_source(&raw).is_err() {
            return;
        }
        self.book.attach_layout(id, layout, self.now());
        self.persist_later(id.into());
    }
    pub async fn source_encounters(
        &self,
        request: SourceRequest,
        tokens: Vec<TokenRange>,
    ) -> Result<Encounters, String> {
        let task = self.checked_source(&request).await?;
        self.guard_source(task.query_text())?;
        if !crate::vocabulary::english_source(
            task.source_language.as_deref(),
            task.detected_language.as_deref(),
        ) {
            return Ok(Encounters {
                task_id: task.id,
                source_revision: task.source_revision,
                matches: vec![],
            });
        }
        // Tokens originate from the native boundary but are still range checked.
        let text = task.query_text().to_owned();
        let utf16: Vec<_> = text.encode_utf16().collect();
        if tokens.len() > 50000
            || tokens.iter().any(|t| {
                t.start_utf16 >= t.end_utf16
                    || t.end_utf16 > utf16.len()
                    || String::from_utf16(&utf16[t.start_utf16..t.end_utf16])
                        .ok()
                        .as_deref()
                        != Some(&t.surface)
            })
        {
            return Err("本地分词范围无效。".into());
        }
        let keys = encounters::lookup_keys(&text, &tokens);
        let items = self.storage.vocabulary_lookup(keys).await?;
        let matches =
            tokio::task::spawn_blocking(move || encounters::matches(&text, &tokens, &items))
                .await
                .map_err(|_| "本地匹配未完成。")?;
        let current = self.book.get(&task.id).ok_or("记录已删除或到期。")?;
        if current.source_revision != task.source_revision {
            return Err("原文已变化，请重新匹配。".into());
        }
        Ok(Encounters {
            task_id: task.id,
            source_revision: task.source_revision,
            matches,
        })
    }
    pub(super) async fn source_retry_assets(&self, task: &Task) -> Result<(), String> {
        let assets: Vec<_> = task
            .source_segments
            .iter()
            .filter_map(|s| s.asset_id.as_ref())
            .collect();
        if assets.is_empty() {
            return if task.image_error.is_some() {
                Err("原始截图未成功编码，请重新截图。".into())
            } else {
                Ok(())
            };
        }
        self.storage.save_task(task.clone()).await?;
        for asset in assets {
            let key = if asset == &task.id {
                task.id.clone()
            } else {
                format!("{}--{asset}", task.id)
            };
            let data = self.images.lock().unwrap().data.get(&key).cloned();
            if let Some(data) = data {
                let saved = if asset == &task.id {
                    self.storage.save_image(task.id.clone(), data).await?
                } else {
                    self.storage
                        .save_source_asset(task.id.clone(), asset.clone(), data)
                        .await?
                };
                if !saved {
                    return Err("记录已删除或到期。".into());
                }
            } else if self
                .storage
                .source_asset(task.id.clone(), asset.clone())
                .await?
                .is_none()
            {
                return Err("原始截图已不在内存中，请重新截图。".into());
            }
        }
        Ok(())
    }
    pub async fn source_assets(&self, id: &str) -> Result<Vec<SourceAsset>, String> {
        let task = self.load_into_book(id).await?;
        if task.sensitive.is_some() {
            return Ok(vec![]);
        }
        Ok(task
            .source_segments
            .iter()
            .filter_map(|s| {
                s.asset_id.as_ref().map(|asset| SourceAsset {
                    id: asset.clone(),
                    segment_id: s.id.clone(),
                    legacy: asset == id,
                })
            })
            .collect())
    }
    pub async fn source_asset(
        &self,
        id: &str,
        asset: &str,
    ) -> Result<Option<Arc<Vec<u8>>>, String> {
        let task = self.load_into_book(id).await?;
        if task.sensitive.is_some()
            || !task
                .source_segments
                .iter()
                .any(|s| s.asset_id.as_deref() == Some(asset))
        {
            return Ok(None);
        }
        if id == asset {
            return self.image(id).await;
        }
        let key = format!("{id}--{asset}");
        if let Some(bytes) = self.images.lock().unwrap().data.get(&key).cloned() {
            return Ok(Some(bytes));
        }
        self.storage
            .source_asset(id.into(), asset.into())
            .await
            .map(|bytes| bytes.map(Arc::new))
    }
    pub async fn source_append(
        self: &Arc<Self>,
        request: SourceRequest,
        kind: Source,
        text: String,
        image: Option<Vec<u8>>,
        layout: Option<OcrLayout>,
    ) -> Result<Option<Task>, String> {
        self.source_append_inner(request, kind, text, image, layout, None)
            .await
    }
    async fn source_append_inner(
        self: &Arc<Self>,
        request: SourceRequest,
        kind: Source,
        text: String,
        image: Option<Vec<u8>>,
        layout: Option<OcrLayout>,
        approved: Option<String>,
    ) -> Result<Option<Task>, String> {
        source::validate_text(&text)?;
        if let Some(bytes) = &image {
            source::validate_png(bytes)?;
        }
        let _guard = self.mutations.lock().await;
        if let Some(bytes) = &image {
            let retained = self.images.lock().unwrap().bytes;
            let staged = self
                .pending_images
                .lock()
                .unwrap()
                .values()
                .map(Vec::len)
                .sum::<usize>();
            let pending = self
                .source_pending
                .lock()
                .unwrap()
                .values()
                .filter_map(|p| p.image.as_ref())
                .map(|b| b.len())
                .sum::<usize>();
            if retained + staged + pending + bytes.len() > 32 * 1024 * 1024 {
                return Err("会话图片已达到 32 MiB 上限，原稿未改变。".into());
            }
        }
        let task = self.load_into_book(&request.task_id).await.ok();
        if let Some(task) = &task
            && !task.status.is_terminal()
        {
            return Err("正文正在生成，请先停止或等待完成后再追加。".into());
        }
        if let Some(task) = &task
            && task.source_segments.len() >= source::MAX_SEGMENTS
        {
            return Err("每稿最多 10 段，原文未改变。".into());
        }
        let raw = task
            .as_ref()
            .filter(|t| !t.original_text.is_empty())
            .map(|t| format!("{}\n\n{text}", t.original_text))
            .unwrap_or_else(|| text.clone());
        let mut query = task
            .as_ref()
            .filter(|t| !t.query_text().is_empty())
            .map(|t| format!("{}\n\n{text}", t.query_text()))
            .unwrap_or_else(|| text.clone());
        let unclean_query = query.clone();
        if self.auto_clean.load(Ordering::Relaxed) {
            query = cleaner::clean(&query, cleaner::CleanMode::Pdf).text;
        }
        let cleanup_undo = (query != unclean_query).then_some(unclean_query);
        source::validate_text(&raw)?;
        source::validate_text(&query)?;
        let digest = fingerprint(&format!("{raw}\0{query}"));
        let sensitive = if self.sensitive_guard.load(Ordering::Relaxed) {
            crate::sensitive::detect(&text)
                .or_else(|| crate::sensitive::detect(&raw))
                .or_else(|| crate::sensitive::detect(&query))
        } else {
            None
        };
        let memory_only = sensitive.is_some() && approved.as_deref() == Some(&digest);
        if task
            .as_ref()
            .is_none_or(|t| t.source_revision != request.expected_revision)
            || sensitive.is_some() && !memory_only
        {
            let mut pending = self.source_pending.lock().unwrap();
            pending.retain(|_, p| self.now() - p.created_at < 600_000);
            if pending.len() >= 10
                || pending
                    .values()
                    .filter_map(|p| p.image.as_ref())
                    .map(|b| b.len())
                    .sum::<usize>()
                    + image.as_ref().map_or(0, Vec::len)
                    > 32 * 1024 * 1024
            {
                return Err("待接收内容已满，请先处理已有片段。".into());
            }
            pending.insert(
                uuid::Uuid::new_v4().to_string(),
                PendingSource {
                    task_id: request.task_id.clone(),
                    expected_revision: request.expected_revision,
                    text: Zeroizing::new(text),
                    kind,
                    image: image.map(Zeroizing::new),
                    layout: if sensitive.is_none() { layout } else { None },
                    created_at: self.now(),
                    sensitive,
                    digest,
                },
            );
            drop(pending);
            self.book.source_touch(&request.task_id, self.now());
            return Ok(None);
        }
        let task = task.unwrap();
        let asset_id = image.as_ref().map(|_| uuid::Uuid::new_v4().to_string());
        let mut snapshot = task.source_snapshot();
        snapshot.original_text = raw;
        snapshot.edited_text = (query != snapshot.original_text).then_some(query);
        snapshot.segments.push(SourceSegment {
            id: uuid::Uuid::new_v4().to_string(),
            kind,
            original_text: text,
            captured_at: self.now(),
            asset_id: asset_id.clone(),
            ocr_layout: layout,
        });
        let image_data = image.map(Arc::new);
        if let (Some(asset), Some(data)) = (&asset_id, &image_data) {
            let key = format!("{}--{asset}", task.id);
            let active: HashSet<_> = self
                .book
                .snapshot()
                .tasks
                .into_iter()
                .flat_map(|t| {
                    let mut keys = vec![t.id.clone()];
                    keys.extend(
                        t.source_segments
                            .iter()
                            .filter_map(|s| s.asset_id.as_ref())
                            .map(|a| format!("{}--{a}", t.id)),
                    );
                    keys
                })
                .collect();
            self.images
                .lock()
                .unwrap()
                .insert(key, data.clone(), &active)?;
        }
        let mut changed = match self
            .apply_source_snapshot_with_cleanup(&task, snapshot, true, memory_only, cleanup_undo)
            .await
        {
            Ok(task) => task,
            Err(error) => {
                if let Some(asset) = &asset_id {
                    self.images
                        .lock()
                        .unwrap()
                        .remove(&format!("{}--{asset}", task.id));
                }
                return Err(error);
            }
        };
        if memory_only {
            self.approved_sensitive
                .lock()
                .unwrap()
                .insert(task.id.clone(), fingerprint(changed.query_text()));
        }
        if let (Some(asset), Some(data)) = (asset_id, image_data) {
            if changed.save_state != SaveState::MemoryOnly {
                match self
                    .storage
                    .save_source_asset(task.id.clone(), asset, data)
                    .await
                {
                    Ok(true) => {
                        self.book.image_error(&task.id, None, self.now());
                        self.persist(&task.id).await;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        self.book.image_error(&task.id, Some(error), self.now());
                        self.persist(&task.id).await;
                    }
                }
            }
            changed = self.book.get(&task.id).unwrap_or(changed);
        }
        Ok(Some(changed))
    }
    pub async fn resolve_source_pending(
        self: &Arc<Self>,
        id: &str,
        action: PendingAction,
        target: Option<SourceRequest>,
        allow: bool,
    ) -> Result<Option<Task>, String> {
        let pending = self
            .source_pending
            .lock()
            .unwrap()
            .remove(id)
            .ok_or("待接收片段已清空或过期。")?;
        if self.now() - pending.created_at >= 600_000 {
            return Err("待接收片段已过期。".into());
        }
        if matches!(action, PendingAction::Discard) {
            return Ok(None);
        }
        let backup = pending.clone();
        let result = async {
            let request = if matches!(action, PendingAction::New) {
                let handle = self.begin(pending.kind).await?;
                let task = self.book.get(&handle.id).ok_or("无法创建原文草稿。")?;
                self.book.stop(&task.id, self.now());
                SourceRequest {
                    task_id: task.id,
                    expected_revision: task.source_revision,
                }
            } else {
                target.unwrap_or(SourceRequest {
                    task_id: pending.task_id,
                    expected_revision: pending.expected_revision,
                })
            };
            self.source_append_inner(
                request,
                pending.kind,
                pending.text.to_string(),
                pending.image.map(|b| b.to_vec()),
                pending.layout,
                allow.then_some(pending.digest),
            )
            .await
        }
        .await;
        if result.is_err() {
            self.source_pending
                .lock()
                .unwrap()
                .insert(id.into(), backup);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct FakeCredentials;
    impl Credentials for FakeCredentials {
        fn read(&self, _: &str) -> Result<Option<String>, String> {
            Ok(None)
        }
        fn write(&self, _: &str, _: Option<&str>) -> Result<(), String> {
            Ok(())
        }
    }
    async fn setup(text: &str) -> (tempfile::TempDir, Arc<Engine>, Task) {
        let root = tempfile::tempdir().unwrap();
        let engine = Engine::open(
            root.path().into(),
            Arc::new(FakeCredentials),
            Arc::new(|| 100),
        )
        .await
        .unwrap();
        let handle = engine.begin(Source::Selection).await.unwrap();
        engine.accept_text(&handle, text.into(), 1);
        let task = engine.book.get(&handle.id).unwrap();
        (root, engine, task)
    }
    fn request(task: &Task) -> SourceRequest {
        SourceRequest {
            task_id: task.id.clone(),
            expected_revision: task.source_revision,
        }
    }
    fn png() -> Vec<u8> {
        let mut value = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        value.extend(2u32.to_be_bytes());
        value.extend(2u32.to_be_bytes());
        value
    }
    #[tokio::test]
    async fn combined_guard_preserves_safe_draft_and_explicit_bypass_is_scoped() {
        let half = ["abandon"; 6].join(" ");
        let (_dir, engine, task) = setup(&half).await;
        assert!(
            engine
                .source_append(request(&task), Source::Selection, half.clone(), None, None)
                .await
                .unwrap()
                .is_none()
        );
        let preserved = engine.book.get(&task.id).unwrap();
        assert_eq!(preserved.original_text, half);
        assert_eq!(preserved.source_revision, task.source_revision);
        let pending = engine.source_pending(Some(&task.id));
        assert_eq!(pending.len(), 1);
        assert!(pending[0].sensitive.is_some());
        assert!(!serde_json::to_string(&pending).unwrap().contains("abandon"));
        let allowed = engine
            .resolve_source_pending(&pending[0].id, PendingAction::Append, None, true)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(allowed.save_state, SaveState::MemoryOnly);
        assert_eq!(allowed.source_segments.len(), 2);
        assert!(
            engine
                .storage
                .task(task.id.clone())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            engine
                .source_append(
                    request(&allowed),
                    Source::Selection,
                    "new text".into(),
                    None,
                    None
                )
                .await
                .unwrap()
                .is_none()
        );
    }
    #[tokio::test]
    async fn append_undo_retains_edited_text_and_previous_translation() {
        let (_dir, engine, task) = setup("Original paragraph").await;
        let task = engine
            .source_update(UpdateRequest {
                source_language: None,
                target_language: None,
                source: request(&task),
                text: "Edited paragraph".into(),
            })
            .await
            .unwrap();
        let appended = engine
            .source_append(
                request(&task),
                Source::Selection,
                "Second paragraph".into(),
                None,
                None,
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            appended.query_text(),
            "Edited paragraph\n\nSecond paragraph"
        );
        assert_eq!(
            appended.original_text,
            "Original paragraph\n\nSecond paragraph"
        );
        let undone = engine.source_undo(request(&appended)).await.unwrap();
        assert_eq!(undone.query_text(), "Edited paragraph");
        assert_eq!(undone.source_segments.len(), 1);
        assert!(
            engine
                .source_append(
                    request(&task),
                    Source::Selection,
                    "Late safe capture".into(),
                    None,
                    None
                )
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            engine.book.get(&task.id).unwrap().query_text(),
            "Edited paragraph"
        );
    }
    #[tokio::test]
    async fn multi_image_assets_survive_reopen_and_parent_deletion_cleans_all() {
        let (dir, engine, task) = setup("Initial selection").await;
        let first = engine
            .source_append(
                request(&task),
                Source::Screenshot,
                "First image text".into(),
                Some(png()),
                None,
            )
            .await
            .unwrap()
            .unwrap();
        let second = engine
            .source_append(
                request(&first),
                Source::Screenshot,
                "Second image text".into(),
                Some(png()),
                None,
            )
            .await
            .unwrap()
            .unwrap();
        engine.persist(&task.id).await;
        let assets = engine.source_assets(&task.id).await.unwrap();
        assert_eq!(assets.len(), 2);
        for asset in &assets {
            assert!(
                engine
                    .source_asset(&task.id, &asset.id)
                    .await
                    .unwrap()
                    .is_some()
            );
        }
        let restored = Storage::open(dir.path().into(), Arc::new(|| 100))
            .await
            .unwrap();
        let saved = restored.task(task.id.clone()).await.unwrap().unwrap();
        assert_eq!(saved.source_segments.len(), 3);
        assert_eq!(saved.source_revision, second.source_revision);
        for asset in &assets {
            assert!(
                restored
                    .source_asset(task.id.clone(), asset.id.clone())
                    .await
                    .unwrap()
                    .is_some()
            );
        }
        engine.remove(&task.id).await.unwrap();
        assert_eq!(
            std::fs::read_dir(dir.path().join("images"))
                .unwrap()
                .count(),
            0
        );
    }
    #[tokio::test]
    async fn limits_and_failed_images_leave_previous_draft_intact() {
        let (_dir, engine, task) = setup("Safe original").await;
        assert!(
            engine
                .source_append(
                    request(&task),
                    Source::Screenshot,
                    "Image text".into(),
                    Some(vec![0; 64]),
                    None
                )
                .await
                .is_err()
        );
        assert_eq!(
            engine.book.get(&task.id).unwrap().source_revision,
            task.source_revision
        );
        assert!(
            engine
                .source_update(UpdateRequest {
                    source_language: None,
                    target_language: None,
                    source: request(&task),
                    text: format!("sk-{}", "x".repeat(40))
                })
                .await
                .is_err()
        );
        assert_eq!(
            engine.book.get(&task.id).unwrap().query_text(),
            "Safe original"
        );
        let mut current = task;
        for i in 0..9 {
            current = engine
                .source_append(
                    request(&current),
                    Source::Selection,
                    format!("fragment {i}"),
                    None,
                    None,
                )
                .await
                .unwrap()
                .unwrap();
        }
        let before = current.source_revision;
        assert!(
            engine
                .source_append(
                    request(&current),
                    Source::Selection,
                    "too many".into(),
                    None,
                    None
                )
                .await
                .is_err()
        );
        assert_eq!(
            engine.book.get(&current.id).unwrap().source_revision,
            before
        );
    }
    #[tokio::test]
    async fn auto_clean_preserves_raw_and_rechecks_transformed_content() {
        let text = "This paragraph contains enough ordinary prose to be a wrapped\ncontinuation with enough ordinary words to be confidently joined.";
        let (_dir, engine, task) = setup(text).await;
        engine.auto_clean.store(true, Ordering::Relaxed);
        engine.source_auto_clean_capture(&task.id);
        let cleaned = engine.book.get(&task.id).unwrap();
        assert_eq!(cleaned.original_text, text);
        assert!(!cleaned.query_text().contains('\n'));
        assert_eq!(cleaned.source_undo.len(), 1);
        assert_eq!(cleaned.status, Status::Ready);
        let raw = format!("sk-\u{00ad}{}", "x".repeat(40));
        assert!(crate::sensitive::detect(&raw).is_none());
        let handle = engine.begin(Source::Selection).await.unwrap();
        engine.accept_text(&handle, raw.clone(), 1);
        let safe = engine.book.get(&handle.id).unwrap();
        assert_eq!(safe.query_text(), raw);
        assert!(safe.edited_text.is_none());
    }
    #[tokio::test]
    async fn undo_automatic_cleanup_preserves_appended_segment_and_is_one_shot() {
        let (_dir, engine, task) = setup("First original paragraph.").await;
        engine.auto_clean.store(true, Ordering::Relaxed);
        let wrapped = "This paragraph contains enough ordinary prose to be a wrapped\ncontinuation with enough ordinary words to be confidently joined.";
        let appended = engine
            .source_append(
                request(&task),
                Source::Selection,
                wrapped.into(),
                None,
                None,
            )
            .await
            .unwrap()
            .unwrap();
        assert!(appended.can_undo_cleanup);
        assert_eq!(appended.source_segments.len(), 2);
        assert!(!appended.query_text().contains("wrapped\ncontinuation"));
        let restored = engine
            .source_undo_cleanup(request(&appended))
            .await
            .unwrap();
        assert_eq!(restored.source_segments.len(), 2);
        assert!(restored.query_text().contains("wrapped\ncontinuation"));
        assert_eq!(restored.original_text, appended.original_text);
        assert!(!restored.can_undo_cleanup);
        assert!(
            engine
                .source_undo_cleanup(request(&restored))
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn cleanup_undo_is_invalidated_by_editing_and_not_advertised_after_restart() {
        let (_dir, engine, task) = setup("First original paragraph.").await;
        engine.auto_clean.store(true, Ordering::Relaxed);
        let wrapped = "This paragraph contains enough ordinary prose to be a wrapped\ncontinuation with enough ordinary words to be confidently joined.";
        let cleaned = engine
            .source_append(
                request(&task),
                Source::Selection,
                wrapped.into(),
                None,
                None,
            )
            .await
            .unwrap()
            .unwrap();
        assert!(cleaned.can_undo_cleanup);
        let decoded: Task =
            serde_json::from_str(&serde_json::to_string(&cleaned).unwrap()).unwrap();
        assert!(!decoded.can_undo_cleanup);
        assert!(decoded.cleanup_undo.is_none());
        let edited = engine
            .source_update(UpdateRequest {
                source: request(&cleaned),
                text: "My newer edit.".into(),
                source_language: None,
                target_language: None,
            })
            .await
            .unwrap();
        assert!(!edited.can_undo_cleanup);
        assert!(engine.source_undo_cleanup(request(&edited)).await.is_err());
        assert_eq!(
            engine.book.get(&task.id).unwrap().query_text(),
            "My newer edit."
        );
    }
    #[tokio::test]
    async fn explicit_language_changes_version_without_sending_translation() {
        let (_dir, engine, task) = setup("Running with friends.").await;
        let updated = engine
            .source_update(UpdateRequest {
                source: request(&task),
                text: task.original_text.clone(),
                source_language: Some("en".into()),
                target_language: Some("fr".into()),
            })
            .await
            .unwrap();
        assert_eq!(updated.source_revision, task.source_revision + 1);
        assert_eq!(updated.source_language.as_deref(), Some("en"));
        assert_eq!(updated.target_language, "fr");
        assert_eq!(updated.status, Status::Ready);
        assert!(updated.translation.is_empty());
    }
    #[tokio::test]
    async fn retry_save_rewrites_every_missing_source_asset_from_private_cache() {
        let (dir, engine, task) = setup("Initial selection").await;
        let first = engine
            .source_append(
                request(&task),
                Source::Screenshot,
                "Image one".into(),
                Some(png()),
                None,
            )
            .await
            .unwrap()
            .unwrap();
        let current = engine
            .source_append(
                request(&first),
                Source::Screenshot,
                "Image two".into(),
                Some(png()),
                None,
            )
            .await
            .unwrap()
            .unwrap();
        for asset in engine.source_assets(&task.id).await.unwrap() {
            std::fs::remove_file(
                dir.path()
                    .join("images")
                    .join(format!("{}--{}.png", task.id, asset.id)),
            )
            .unwrap();
        }
        engine.book.image_error(
            &task.id,
            Some("Synthetic temporary write failure".into()),
            engine.now(),
        );
        engine.retry_save(&task.id).await.unwrap();
        let restored = engine.book.get(&task.id).unwrap();
        assert!(restored.image_error.is_none());
        assert_eq!(restored.source_revision, current.source_revision);
        assert_eq!(restored.save_state, SaveState::Saved);
        for asset in engine.source_assets(&task.id).await.unwrap() {
            assert!(
                engine
                    .storage
                    .source_asset(task.id.clone(), asset.id)
                    .await
                    .unwrap()
                    .is_some()
            );
        }
    }
}
