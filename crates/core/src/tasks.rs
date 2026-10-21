//! Application-owned task state. A view subscription never owns a job.
//!
//! Mutation, snapshot creation, and subscription registration share one short
//! critical section. This prevents lost updates when a window is recreated.

use crate::config::{Profile, Retention};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Capturing,
    Recognizing,
    Queued,
    Translating,
    Completed,
    OriginalOnly,
    Ready,
    Failed,
    Stopped,
    Interrupted,
}

impl Status {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::OriginalOnly
                | Self::Ready
                | Self::Failed
                | Self::Stopped
                | Self::Interrupted
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Selection,
    Screenshot,
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SaveState {
    Pending,
    Saved,
    MemoryOnly,
    Failed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Timings {
    pub source_ms: Option<u64>,
    pub queue_ms: Option<u64>,
    pub first_content_ms: Option<u64>,
    pub total_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PastAttempt {
    pub number: u32,
    pub profile: Profile,
    pub status: Status,
    pub translation: String,
    pub error: Option<String>,
    pub timings: Timings,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VocabularyState {
    #[default]
    None,
    Disabled,
    Extracting,
    Complete,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    #[serde(default)]
    pub extract_vocabulary: bool,
    #[serde(default)]
    pub vocabulary_state: VocabularyState,
    #[serde(default)]
    pub sensitive: Option<crate::sensitive::SensitiveKind>,
    #[serde(default)]
    pub vocabulary: Vec<crate::vocabulary::Candidate>,
    #[serde(default)]
    pub vocabulary_notice: Option<String>,
    #[serde(default = "legacy_prompt_version")]
    pub prompt_version: u32,
    pub id: String,
    pub revision: u64,
    pub attempt: u32,
    pub source: Source,
    pub status: Status,
    pub original_text: String,
    pub edited_text: Option<String>,
    pub translation: String,
    pub profile: Profile,
    pub target_language: String,
    #[serde(default)]
    pub source_language: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub expires_at: Option<i64>,
    pub error: Option<String>,
    pub save_error: Option<String>,
    pub save_state: SaveState,
    pub has_image: bool,
    #[serde(default)]
    pub image_pending: bool,
    #[serde(default)]
    pub image_error: Option<String>,
    pub cache_hit: bool,
    #[serde(default)]
    pub detected_language: Option<String>,
    pub timings: Timings,
    pub past_attempts: Vec<PastAttempt>,
}

fn legacy_prompt_version() -> u32 {
    1
}

impl Task {
    pub fn sentence_translation(&self, attempt: u32, source: &str, sentence: &str) -> String {
        if self.attempt != attempt
            || self.status != Status::Completed
            || self.query_text() != source
            || self.profile.provider == crate::config::Provider::SystemDictionary
        {
            return String::new();
        }
        crate::vocabulary::context::single_sentence_translation(source, sentence, &self.translation)
    }
    pub fn content_bytes(&self) -> usize {
        self.original_text.len()
            + self.edited_text.as_ref().map_or(0, String::len)
            + self.translation.len()
            + self
                .past_attempts
                .iter()
                .map(|a| a.translation.len())
                .sum::<usize>()
    }
    pub fn query_text(&self) -> &str {
        self.edited_text.as_deref().unwrap_or(&self.original_text)
    }
    pub fn expired(&self, now: i64) -> bool {
        self.expires_at.is_some_and(|expiry| expiry <= now)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub revision: u64,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Changed {
        task: Box<Task>,
    },
    Delta {
        id: String,
        attempt: u32,
        revision: u64,
        text: String,
    },
    Removed {
        id: String,
        revision: u64,
    },
}

struct Inner {
    tasks: BTreeMap<String, Task>,
    revision: u64,
    retention: Retention,
    max_pending: usize,
}

#[derive(Clone)]
pub struct TaskBook {
    inner: Arc<Mutex<Inner>>,
    events: broadcast::Sender<Event>,
}

impl TaskBook {
    pub fn new(retention: Retention, max_pending: usize) -> Self {
        let (events, _) = broadcast::channel(128);
        Self {
            inner: Arc::new(Mutex::new(Inner {
                tasks: BTreeMap::new(),
                revision: 0,
                retention,
                max_pending,
            })),
            events,
        }
    }

    /// Register first, then take a snapshot under the same lock used by writes.
    pub fn subscribe(&self) -> (Snapshot, broadcast::Receiver<Event>) {
        let inner = self.inner.lock().unwrap();
        let rx = self.events.subscribe();
        (Self::snapshot_inner(&inner), rx)
    }

    pub fn snapshot(&self) -> Snapshot {
        Self::snapshot_inner(&self.inner.lock().unwrap())
    }

    fn snapshot_inner(inner: &Inner) -> Snapshot {
        let mut tasks: Vec<_> = inner.tasks.values().cloned().collect();
        tasks.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        Snapshot {
            revision: inner.revision,
            tasks,
        }
    }

    pub fn get(&self, id: &str) -> Option<Task> {
        self.inner.lock().unwrap().tasks.get(id).cloned()
    }

    pub fn create(
        &self,
        source: Source,
        profile: Profile,
        target_language: String,
        now: i64,
    ) -> Result<Task, String> {
        self.create_with_language(source, profile, target_language, None, now)
    }

    pub fn create_with_language(
        &self,
        source: Source,
        profile: Profile,
        target_language: String,
        source_language: Option<String>,
        now: i64,
    ) -> Result<Task, String> {
        let mut inner = self.inner.lock().unwrap();
        if inner
            .tasks
            .values()
            .filter(|t| !t.status.is_terminal())
            .count()
            >= inner.max_pending
        {
            return Err("等待中的任务已满。请等待一个任务完成，或主动停止不再需要的任务。".into());
        }
        if inner
            .tasks
            .values()
            .filter(|t| {
                t.status.is_terminal()
                    && matches!(t.save_state, SaveState::Pending | SaveState::Failed)
            })
            .count()
            >= 20
        {
            return Err("未保存的结果较多，请先重试保存或在设置中关闭本地保存。".into());
        }
        inner.revision += 1;
        let task = Task {
            extract_vocabulary: true,
            vocabulary_state: VocabularyState::None,
            sensitive: None,
            vocabulary: vec![],
            vocabulary_notice: None,
            prompt_version: crate::provider::PROMPT_VERSION,
            id: Uuid::new_v4().to_string(),
            revision: inner.revision,
            attempt: 1,
            source,
            status: Status::Capturing,
            original_text: String::new(),
            edited_text: None,
            translation: String::new(),
            profile,
            target_language,
            source_language,
            created_at: now,
            updated_at: now,
            expires_at: inner.retention.expires_at(now),
            error: None,
            save_error: None,
            save_state: if inner.retention == Retention::Disabled {
                SaveState::MemoryOnly
            } else {
                SaveState::Pending
            },
            has_image: false,
            image_pending: source == Source::Screenshot,
            image_error: None,
            cache_hit: false,
            detected_language: None,
            timings: Timings::default(),
            past_attempts: vec![],
        };
        inner.tasks.insert(task.id.clone(), task.clone());
        let _ = self.events.send(Event::Changed {
            task: Box::new(task.clone()),
        });
        Ok(task)
    }

    fn update(
        &self,
        id: &str,
        attempt: Option<u32>,
        now: i64,
        mutate: impl FnOnce(&mut Task) -> bool,
    ) -> Option<Task> {
        let mut inner = self.inner.lock().unwrap();
        self.update_locked(&mut inner, id, attempt, now, mutate)
    }

    fn update_locked(
        &self,
        inner: &mut Inner,
        id: &str,
        attempt: Option<u32>,
        now: i64,
        mutate: impl FnOnce(&mut Task) -> bool,
    ) -> Option<Task> {
        let task = inner.tasks.get_mut(id)?;
        if task.expired(now) || attempt.is_some_and(|a| a != task.attempt) {
            return None;
        }
        // A saved acknowledgement belongs to one revision, not future changes.
        // Reset before mutation so set_save_state can explicitly acknowledge the
        // current revision without having its value overwritten afterward.
        let previous_save_state = task.save_state;
        if task.save_state == SaveState::Saved {
            task.save_state = SaveState::Pending;
        }
        if !mutate(task) {
            task.save_state = previous_save_state;
            return None;
        }
        inner.revision += 1;
        let revision = inner.revision;
        let task = inner.tasks.get_mut(id).unwrap();
        task.revision = revision;
        task.updated_at = now;
        let changed = task.clone();
        let _ = self.events.send(Event::Changed {
            task: Box::new(changed.clone()),
        });
        Some(changed)
    }

    pub fn mask_sensitive(&self, id: &str, kind: crate::sensitive::SensitiveKind, now: i64) {
        self.update(id, None, now, |t| {
            t.original_text.clear();
            t.edited_text = None;
            t.translation.clear();
            t.vocabulary.clear();
            t.vocabulary_notice = None;
            t.vocabulary_state = VocabularyState::None;
            t.past_attempts.clear();
            t.vocabulary.clear();
            t.vocabulary_notice = None;
            t.vocabulary_state = VocabularyState::None;
            t.sensitive = Some(kind);
            t.status = Status::Ready;
            t.has_image = false;
            t.image_pending = false;
            t.save_state = SaveState::MemoryOnly;
            t.error = None;
            true
        });
    }
    pub fn release_sensitive(&self, id: &str, text: String, now: i64) -> Option<Task> {
        self.update(id, None, now, |t| {
            if t.sensitive.is_none() {
                return false;
            }
            t.sensitive = None;
            t.original_text = text;
            t.status = Status::Ready;
            true
        })
    }
    pub fn set_extraction(&self, id: &str, enabled: bool, now: i64) -> Option<Task> {
        self.update(id, None, now, |t| {
            if !matches!(t.status, Status::Capturing | Status::Queued | Status::Ready) {
                return false;
            }
            let enabled = enabled
                && !matches!(
                    t.profile.provider,
                    crate::config::Provider::SystemDictionary | crate::config::Provider::Youdao
                );
            t.extract_vocabulary = enabled;
            t.vocabulary_state = if enabled {
                VocabularyState::None
            } else {
                VocabularyState::Disabled
            };
            true
        })
    }
    pub fn translation_done(
        &self,
        id: &str,
        attempt: u32,
        text: &str,
        elapsed_ms: u64,
        now: i64,
    ) -> Result<bool, String> {
        let mut inner = self.inner.lock().unwrap();
        if inner.tasks.values().map(Task::content_bytes).sum::<usize>() + text.len()
            > 32 * 1024 * 1024
        {
            return Err("当前会话内容已达到内存上限。".into());
        }
        if inner
            .tasks
            .get(id)
            .is_some_and(|t| t.translation.len() + text.len() > 1024 * 1024)
        {
            return Err("译文超过大小限制。".into());
        }
        if inner
            .tasks
            .get(id)
            .is_some_and(|t| t.translation.trim().is_empty() && text.trim().is_empty())
        {
            return Err("模型没有返回译文。".into());
        }
        Ok(self
            .update_locked(&mut inner, id, Some(attempt), now, |t| {
                if t.status != Status::Translating {
                    return false;
                }
                t.translation.push_str(text);
                if t.translation.trim().is_empty() {
                    return false;
                }
                t.timings.first_content_ms.get_or_insert(elapsed_ms);
                t.timings.total_ms = Some(elapsed_ms);
                t.status = Status::Completed;
                t.vocabulary_state = if t.extract_vocabulary {
                    VocabularyState::Extracting
                } else {
                    VocabularyState::Disabled
                };
                if t.save_state != SaveState::MemoryOnly {
                    t.save_state = SaveState::Pending;
                }
                true
            })
            .is_some())
    }
    pub fn finish_vocabulary(
        &self,
        id: &str,
        attempt: u32,
        state: VocabularyState,
        notice: Option<String>,
        now: i64,
    ) -> Option<Task> {
        self.update(id, Some(attempt), now, |t| {
            if t.vocabulary_state != VocabularyState::Extracting {
                return false;
            }
            t.vocabulary_state = state;
            t.vocabulary_notice = notice;
            true
        })
    }
    pub fn set_vocabulary(
        &self,
        id: &str,
        attempt: u32,
        mut items: Vec<crate::vocabulary::Candidate>,
        notice: Option<String>,
        now: i64,
    ) {
        // Cached metadata is untrusted too; revalidate it under current rules.
        items.retain_mut(|candidate| candidate.validate().is_ok());
        self.update(id, Some(attempt), now, |t| {
            if !matches!(
                t.status,
                Status::Queued | Status::Translating | Status::Completed
            ) || t.vocabulary_state == VocabularyState::Cancelled
            {
                return false;
            }
            t.vocabulary_state = if notice.is_some() && items.is_empty() {
                VocabularyState::Failed
            } else {
                VocabularyState::Complete
            };
            t.vocabulary = items;
            t.vocabulary_notice = notice;
            true
        });
    }

    pub fn capture_languages(
        &self,
        id: &str,
        source: Option<String>,
        target: String,
        detected: Option<String>,
        now: i64,
    ) {
        self.update(id, None, now, |t| {
            if t.status != Status::Capturing {
                return false;
            }
            t.source_language = source;
            t.target_language = target;
            t.detected_language = detected;
            true
        });
    }
    pub fn recognized(
        &self,
        id: &str,
        text: String,
        elapsed_ms: u64,
        now: i64,
    ) -> Result<Option<Task>, String> {
        if text.trim().is_empty() {
            return Err("没有可翻译的文字。".into());
        }
        if text.chars().count() > crate::MAX_INPUT_CHARACTERS {
            return Err("原文过长，请分段翻译。".into());
        }
        Ok(self.update(id, Some(1), now, |t| {
            if !matches!(t.status, Status::Capturing | Status::Recognizing) {
                return false;
            }
            t.original_text = text;
            t.status = Status::Queued;
            t.timings.source_ms = Some(elapsed_ms);
            true
        }))
    }

    pub fn ready(&self, id: &str, detected: Option<String>, now: i64) -> Option<Task> {
        self.update(id, Some(1), now, |task| {
            if task.status != Status::Queued {
                return false;
            }
            task.status = Status::Ready;
            task.detected_language = detected;
            true
        })
    }

    pub fn submit_ready(
        &self,
        id: &str,
        edited: Option<String>,
        now: i64,
    ) -> Result<Option<Task>, String> {
        if edited.as_ref().is_some_and(|text| {
            text.trim().is_empty() || text.chars().count() > crate::MAX_INPUT_CHARACTERS
        }) {
            return Err("更正后的原文为空或过长。".into());
        }
        let mut inner = self.inner.lock().unwrap();
        if inner
            .tasks
            .values()
            .filter(|t| !t.status.is_terminal())
            .count()
            >= inner.max_pending
        {
            return Err("等待中的任务已满，请稍后重试。".into());
        }
        Ok(self.update_locked(&mut inner, id, None, now, |task| {
            if task.status != Status::Ready {
                return false;
            }
            task.edited_text = edited.filter(|text| text != &task.original_text);
            task.status = Status::Queued;
            task.prompt_version = crate::provider::PROMPT_VERSION;
            true
        }))
    }

    pub fn original_only(&self, id: &str, language: String, now: i64) -> Option<Task> {
        self.update(id, Some(1), now, |task| {
            if task.status != Status::Queued {
                return false;
            }
            task.status = Status::OriginalOnly;
            task.detected_language = Some(language);
            true
        })
    }

    pub fn retarget_queued(
        &self,
        id: &str,
        profile: Profile,
        target: String,
        now: i64,
    ) -> Option<Task> {
        self.update(id, None, now, |task| {
            if task.status != Status::Queued {
                return false;
            }
            task.profile = profile;
            task.target_language = target;
            true
        })
    }

    pub fn languages_queued(
        &self,
        id: &str,
        source: Option<String>,
        target: String,
        detected: Option<String>,
        now: i64,
    ) -> Option<Task> {
        self.update(id, None, now, |task| {
            if task.status != Status::Queued {
                return false;
            }
            task.source_language = source;
            task.target_language = target;
            task.detected_language = detected;
            true
        })
    }

    pub fn image_captured(&self, id: &str, now: i64) -> Option<Task> {
        self.update(id, None, now, |t| {
            t.has_image = true;
            t.image_pending = false;
            if t.status == Status::Capturing {
                t.status = Status::Recognizing;
            }
            true
        })
    }

    pub fn recognizing(&self, id: &str, now: i64) -> Option<Task> {
        self.update(id, Some(1), now, |t| {
            if t.status != Status::Capturing {
                return false;
            }
            t.status = Status::Recognizing;
            true
        })
    }

    pub fn image_error(&self, id: &str, error: Option<String>, now: i64) -> Option<Task> {
        self.update(id, None, now, |t| {
            t.image_pending = false;
            t.image_error = error;
            true
        })
    }

    pub fn start(&self, id: &str, attempt: u32, queue_ms: u64, now: i64) -> Option<Task> {
        self.update(id, Some(attempt), now, |t| {
            if t.status != Status::Queued {
                return false;
            }
            t.status = Status::Translating;
            t.timings.queue_ms = Some(queue_ms);
            true
        })
    }

    pub fn append(
        &self,
        id: &str,
        attempt: u32,
        text: &str,
        elapsed_ms: u64,
        now: i64,
    ) -> Result<bool, String> {
        if text.is_empty() {
            return Ok(false);
        }
        let mut inner = self.inner.lock().unwrap();
        if inner.tasks.values().map(Task::content_bytes).sum::<usize>() + text.len()
            > 32 * 1024 * 1024
        {
            return Err("当前会话内容已达到内存上限，请先处理未保存的结果。".into());
        }
        let Some(task) = inner.tasks.get_mut(id) else {
            return Ok(false);
        };
        if task.expired(now) || task.attempt != attempt || task.status != Status::Translating {
            return Ok(false);
        }
        if task.translation.len() + text.len() > 1024 * 1024 {
            return Err("译文超过大小限制，已停止接收。".into());
        }
        task.translation.push_str(text);
        if task.save_state == SaveState::Saved {
            task.save_state = SaveState::Pending;
        }
        task.timings.first_content_ms.get_or_insert(elapsed_ms);
        inner.revision += 1;
        let revision = inner.revision;
        let task = inner.tasks.get_mut(id).unwrap();
        task.revision = revision;
        task.updated_at = now;
        let _ = self.events.send(Event::Delta {
            id: id.into(),
            attempt,
            revision,
            text: text.into(),
        });
        Ok(true)
    }

    pub fn finish(&self, id: &str, attempt: u32, elapsed_ms: u64, now: i64) -> Option<Task> {
        self.update(id, Some(attempt), now, |t| {
            if t.status != Status::Translating {
                return false;
            }
            t.timings.total_ms = Some(elapsed_ms);
            if t.translation.trim().is_empty() {
                t.status = Status::Failed;
                t.error = Some("模型没有返回译文。".into());
            } else {
                t.status = Status::Completed;
            }
            true
        })
    }

    pub fn fail(&self, id: &str, attempt: u32, error: String, now: i64) -> Option<Task> {
        self.update(id, Some(attempt), now, |t| {
            if t.status.is_terminal() {
                return false;
            }
            t.status = Status::Failed;
            t.error = Some(error);
            true
        })
    }

    pub fn stop(&self, id: &str, now: i64) -> Option<Task> {
        self.update(id, None, now, |t| {
            if t.status.is_terminal() {
                return false;
            }
            t.status = Status::Stopped;
            true
        })
    }

    pub fn retry(
        &self,
        id: &str,
        edited_text: Option<String>,
        now: i64,
    ) -> Result<Option<Task>, String> {
        if edited_text
            .as_ref()
            .is_some_and(|s| s.trim().is_empty() || s.chars().count() > crate::MAX_INPUT_CHARACTERS)
        {
            return Err("更正后的原文为空或过长。".into());
        }
        let mut inner = self.inner.lock().unwrap();
        if inner
            .tasks
            .values()
            .filter(|t| !t.status.is_terminal())
            .count()
            >= inner.max_pending
        {
            return Err("等待中的任务已满，请稍后重试。".into());
        }
        Ok(self.update_locked(&mut inner, id, None, now, |t| {
            if !t.status.is_terminal()
                || (t.query_text().trim().is_empty() && edited_text.is_none())
            {
                return false;
            }
            // Bounded per-record attempts, while preserving the immediately prior result.
            if t.past_attempts.len() >= 5 {
                t.past_attempts.remove(0);
            }
            t.past_attempts.push(PastAttempt {
                number: t.attempt,
                profile: t.profile.clone(),
                status: t.status,
                translation: std::mem::take(&mut t.translation),
                error: t.error.take(),
                timings: t.timings.clone(),
            });
            t.vocabulary.clear();
            t.vocabulary_notice = None;
            t.vocabulary_state = VocabularyState::None;
            t.attempt += 1;
            t.prompt_version = crate::provider::PROMPT_VERSION;
            t.status = Status::Queued;
            t.timings = Timings::default();
            t.cache_hit = false;
            if let Some(text) = edited_text {
                t.edited_text = Some(text);
            }
            true
        }))
    }

    pub fn set_save_state(
        &self,
        id: &str,
        expected_revision: u64,
        state: SaveState,
        error: Option<String>,
        now: i64,
    ) -> Option<Task> {
        self.update(id, None, now, |t| {
            if t.revision != expected_revision {
                return false;
            }
            t.save_state = state;
            t.save_error = error;
            true
        })
    }

    pub fn cached_result(
        &self,
        id: &str,
        attempt: u32,
        translation: String,
        now: i64,
    ) -> Option<Task> {
        self.complete_queued(id, attempt, translation, true, now)
    }

    pub fn local_result(&self, id: &str, attempt: u32, now: i64) -> Option<Task> {
        let task = self.get(id)?;
        self.set_extraction(id, false, now);
        self.complete_queued(id, attempt, task.query_text().into(), false, now)
    }

    fn complete_queued(
        &self,
        id: &str,
        attempt: u32,
        translation: String,
        cache_hit: bool,
        now: i64,
    ) -> Option<Task> {
        self.update(id, Some(attempt), now, |t| {
            if t.status != Status::Queued {
                return false;
            }
            t.translation = translation;
            t.status = Status::Completed;
            t.cache_hit = cache_hit;
            true
        })
    }

    pub fn restore(&self, tasks: Vec<Task>, now: i64) {
        let mut inner = self.inner.lock().unwrap();
        for mut task in tasks {
            if task.expired(now) || inner.tasks.contains_key(&task.id) {
                continue;
            }
            task.vocabulary
                .retain_mut(|candidate| candidate.validate().is_ok());
            if task.vocabulary_state == VocabularyState::Extracting {
                task.vocabulary_state = VocabularyState::Cancelled;
            }
            inner.revision = inner.revision.max(task.revision) + 1;
            task.revision = inner.revision;
            inner.tasks.insert(task.id.clone(), task.clone());
            let _ = self.events.send(Event::Changed {
                task: Box::new(task),
            });
        }
    }

    pub fn interrupt(&self, id: &str, now: i64) -> Option<Task> {
        self.update(id, None, now, |t| {
            if t.status.is_terminal() {
                return false;
            }
            t.status = Status::Interrupted;
            t.error = Some("退出时任务尚未完成，可手动重试。".into());
            true
        })
    }

    pub fn reconfigure_retention(&self, retention: Retention, now: i64) -> Vec<Task> {
        let mut inner = self.inner.lock().unwrap();
        inner.retention = retention;
        let mut updated = vec![];
        let ids: Vec<_> = inner.tasks.keys().cloned().collect();
        for id in ids {
            inner.revision += 1;
            let revision = inner.revision;
            let task = inner.tasks.get_mut(&id).unwrap();
            task.expires_at = retention.expires_at(task.created_at);
            task.revision = revision;
            if retention == Retention::Disabled {
                task.save_state = SaveState::MemoryOnly;
                task.save_error = None;
            }
            let _ = self.events.send(Event::Changed {
                task: Box::new(task.clone()),
            });
            updated.push(task.clone());
        }
        drop(inner);
        self.expire(now);
        updated
    }

    pub fn remove(&self, id: &str) -> Option<Task> {
        let mut inner = self.inner.lock().unwrap();
        let task = inner.tasks.remove(id)?;
        inner.revision += 1;
        let _ = self.events.send(Event::Removed {
            id: id.into(),
            revision: inner.revision,
        });
        Some(task)
    }

    /// Evict only durable or explicitly session-only terminal results. Saved
    /// records remain available through paginated history; active jobs stay put.
    pub fn trim(&self) -> Vec<Task> {
        let mut inner = self.inner.lock().unwrap();
        let mut candidates: Vec<_> = inner
            .tasks
            .values()
            .filter(|t| {
                t.status.is_terminal()
                    && t.vocabulary_state != VocabularyState::Extracting
                    && matches!(t.save_state, SaveState::Saved | SaveState::MemoryOnly)
            })
            .map(|t| (t.created_at, t.id.clone()))
            .collect();
        candidates.sort();
        let mut total = inner.tasks.values().map(Task::content_bytes).sum::<usize>();
        let mut terminals = inner
            .tasks
            .values()
            .filter(|t| t.status.is_terminal())
            .count();
        let mut removed = vec![];
        for (_, id) in candidates {
            if terminals <= 20 && total <= 24 * 1024 * 1024 {
                break;
            }
            if let Some(t) = inner.tasks.remove(&id) {
                total = total.saturating_sub(t.content_bytes());
                terminals -= 1;
                inner.revision += 1;
                let _ = self.events.send(Event::Removed {
                    id,
                    revision: inner.revision,
                });
                removed.push(t);
            }
        }
        removed
    }

    pub fn expire(&self, now: i64) -> Vec<Task> {
        let mut inner = self.inner.lock().unwrap();
        let ids: Vec<_> = inner
            .tasks
            .values()
            .filter(|t| t.expired(now))
            .map(|t| t.id.clone())
            .collect();
        ids.into_iter()
            .filter_map(|id| {
                let task = inner.tasks.remove(&id)?;
                inner.revision += 1;
                let _ = self.events.send(Event::Removed {
                    id,
                    revision: inner.revision,
                });
                Some(task)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queued(book: &TaskBook, now: i64) -> Task {
        let t = book
            .create(
                Source::Selection,
                Profile::default(),
                "简体中文".into(),
                now,
            )
            .unwrap();
        book.recognized(&t.id, "Hello".into(), 2, now + 1)
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn cached_and_restored_metadata_reject_translated_headwords() {
        let book = TaskBook::new(Retention::Days(7), 12);
        let task = queued(&book, 1);
        let invalid = crate::vocabulary::Candidate {
            word: "逐渐侵蚀".into(),
            surface: "Chipping away".into(),
            ..Default::default()
        };
        book.set_vocabulary(&task.id, 1, vec![invalid.clone()], None, 2);
        assert!(book.get(&task.id).unwrap().vocabulary.is_empty());
        let mut legacy = task;
        legacy.vocabulary = vec![invalid];
        let restored = TaskBook::new(Retention::Days(7), 12);
        restored.restore(vec![legacy.clone()], 3);
        assert!(restored.get(&legacy.id).unwrap().vocabulary.is_empty());
        assert_eq!(
            restored.get(&legacy.id).unwrap().original_text,
            legacy.original_text
        );
    }

    #[tokio::test]
    async fn dropping_a_window_subscription_does_not_stop_the_task() {
        let b = TaskBook::new(Retention::Days(7), 12);
        let (_, view) = b.subscribe();
        let t = queued(&b, 0);
        b.start(&t.id, 1, 0, 2).unwrap();
        drop(view);
        b.append(&t.id, 1, "你好", 4, 3).unwrap();
        b.finish(&t.id, 1, 6, 4).unwrap();
        let (restored, _) = b.subscribe();
        assert_eq!(restored.tasks[0].translation, "你好");
        assert_eq!(restored.tasks[0].status, Status::Completed);
    }

    #[test]
    fn saved_acknowledgement_does_not_cover_later_translation_or_metadata() {
        let book = TaskBook::new(Retention::Forever, 12);
        let task = queued(&book, 0);
        book.set_save_state(&task.id, task.revision, SaveState::Saved, None, 2);
        let started = book.start(&task.id, 1, 0, 3).unwrap();
        assert_eq!(started.save_state, SaveState::Pending);
        book.set_save_state(&task.id, started.revision, SaveState::Saved, None, 4);
        book.append(&task.id, 1, "translated", 1, 5).unwrap();
        let partial = book.get(&task.id).unwrap();
        assert_eq!(partial.save_state, SaveState::Pending);
        book.set_save_state(&task.id, partial.revision, SaveState::Saved, None, 6);
        let completed = book.finish(&task.id, 1, 2, 7).unwrap();
        assert_eq!(completed.status, Status::Completed);
        assert_eq!(completed.save_state, SaveState::Pending);
        book.set_save_state(&task.id, completed.revision, SaveState::Saved, None, 8);
        book.set_vocabulary(&task.id, 1, vec![], None, 9);
        assert_eq!(book.get(&task.id).unwrap().save_state, SaveState::Pending);
        assert!(
            book.set_save_state(&task.id, completed.revision, SaveState::Saved, None, 10)
                .is_none()
        );
        assert_eq!(book.get(&task.id).unwrap().save_state, SaveState::Pending);
        let latest = book.get(&task.id).unwrap();
        book.set_save_state(&task.id, latest.revision, SaveState::Saved, None, 11);
        assert_eq!(book.get(&task.id).unwrap().save_state, SaveState::Saved);
    }

    #[test]
    fn new_task_and_stale_attempt_cannot_overwrite_each_other() {
        let b = TaskBook::new(Retention::Forever, 12);
        let a = queued(&b, 0);
        let z = queued(&b, 2);
        b.start(&a.id, 1, 0, 4);
        b.start(&z.id, 1, 0, 4);
        b.append(&a.id, 1, "原来的部分", 1, 5).unwrap();
        b.stop(&a.id, 6);
        assert!(!b.append(&a.id, 1, "迟到", 2, 7).unwrap());
        let retried = b.retry(&a.id, None, 8).unwrap().unwrap();
        assert_eq!(retried.attempt, 2);
        assert_eq!(retried.past_attempts[0].translation, "原来的部分");
        b.start(&a.id, 2, 0, 9);
        assert!(!b.append(&a.id, 1, "迟到", 4, 10).unwrap());
        b.append(&a.id, 2, "重试结果", 4, 10).unwrap();
        assert!(b.get(&z.id).unwrap().translation.is_empty());
    }

    #[test]
    fn deletion_and_expiry_cannot_be_reversed_by_late_callbacks() {
        let b = TaskBook::new(Retention::Days(1), 12);
        let a = queued(&b, 0);
        let z = queued(&b, 0);
        b.start(&a.id, 1, 0, 3);
        b.start(&z.id, 1, 0, 3);
        b.remove(&a.id).unwrap();
        assert!(!b.append(&a.id, 1, "迟到", 1, 4).unwrap());
        assert!(b.image_captured(&a.id, 4).is_none());
        assert!(!b.append(&z.id, 1, "已到期", 2, 86_400_000).unwrap());
        assert_eq!(b.expire(86_400_000).len(), 1);
        assert!(b.finish(&z.id, 1, 5, 86_400_001).is_none());
        assert!(b.snapshot().tasks.is_empty());
    }

    #[test]
    fn snapshots_and_live_events_have_a_consistent_boundary() {
        let b = TaskBook::new(Retention::Disabled, 12);
        let t = queued(&b, 0);
        b.start(&t.id, 1, 0, 2);
        b.append(&t.id, 1, "甲", 1, 3).unwrap();
        let (snapshot, mut events) = b.subscribe();
        b.append(&t.id, 1, "乙", 2, 4).unwrap();
        assert_eq!(snapshot.tasks[0].translation, "甲");
        match events.try_recv().unwrap() {
            Event::Delta { text, revision, .. } => {
                assert_eq!(text, "乙");
                assert!(revision > snapshot.revision);
            }
            _ => panic!("Expected an incremental update"),
        }
        assert_eq!(snapshot.tasks[0].save_state, SaveState::MemoryOnly);
    }

    #[test]
    fn a_full_queue_does_not_discard_earlier_tasks() {
        let b = TaskBook::new(Retention::Forever, 1);
        let t = queued(&b, 0);
        assert!(
            b.create(Source::Manual, Profile::default(), "中文".into(), 3)
                .is_err()
        );
        assert_eq!(b.get(&t.id).unwrap().status, Status::Queued);
    }

    #[test]
    fn concurrent_retries_share_the_same_admission_limit() {
        let b = TaskBook::new(Retention::Forever, 1);
        let mut ids = vec![];
        for _ in 0..16 {
            let task = queued(&b, 0);
            b.stop(&task.id, 2);
            ids.push(task.id);
        }
        let barrier = Arc::new(std::sync::Barrier::new(ids.len()));
        let jobs: Vec<_> = ids
            .into_iter()
            .map(|id| {
                let book = b.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    book.retry(&id, None, 3).ok().flatten().is_some()
                })
            })
            .collect();
        assert_eq!(
            jobs.into_iter()
                .map(|j| usize::from(j.join().unwrap()))
                .sum::<usize>(),
            1
        );
        assert_eq!(
            b.snapshot()
                .tasks
                .iter()
                .filter(|t| !t.status.is_terminal())
                .count(),
            1
        );
    }
    #[test]
    fn manual_context_translation_requires_matching_completed_attempt_and_whole_source() {
        let book = TaskBook::new(Retention::Disabled, 10);
        let task = book
            .create(Source::Manual, Profile::default(), "zh-Hans".into(), 1)
            .unwrap();
        book.recognized(&task.id, "The resilient team prevailed.".into(), 0, 2)
            .unwrap();
        book.start(&task.id, 1, 0, 3);
        book.append(&task.id, 1, "坚韧的团队获胜了。", 1, 4)
            .unwrap();
        book.finish(&task.id, 1, 2, 5);
        let task = book.get(&task.id).unwrap();
        assert_eq!(
            task.sentence_translation(1, task.query_text(), task.query_text()),
            "坚韧的团队获胜了。"
        );
        assert!(
            task.sentence_translation(2, task.query_text(), task.query_text())
                .is_empty()
        );
        assert!(
            task.sentence_translation(1, "Edited source.", "Edited source.")
                .is_empty()
        );
        assert!(
            task.sentence_translation(1, task.query_text(), "The resilient team")
                .is_empty()
        );
        let mut multi = task.clone();
        multi.original_text = "The team won. They stayed resilient.".into();
        assert!(
            multi
                .sentence_translation(1, multi.query_text(), "They stayed resilient.")
                .is_empty()
        );
        let mut dictionary = task;
        dictionary.profile.provider = crate::config::Provider::SystemDictionary;
        assert!(
            dictionary
                .sentence_translation(1, dictionary.query_text(), dictionary.query_text())
                .is_empty()
        );
    }
}
