use super::*;
use crate::source::SourceSnapshot;
struct SourceChange {
    remember: bool,
    memory_only: bool,
    cleanup_undo: Option<String>,
}
impl TaskBook {
    pub fn set_failure(
        &self,
        id: &str,
        attempt: u32,
        failure: crate::reliability::FailureSummary,
        now: i64,
    ) -> Option<Task> {
        self.update(id, Some(attempt), now, |task| {
            if !matches!(task.status, Status::Queued | Status::Translating) {
                return false;
            }
            task.failure = Some(failure);
            true
        })
    }
    pub fn source_detected(
        &self,
        id: &str,
        expected: u64,
        detected: Option<String>,
        now: i64,
    ) -> Option<Task> {
        self.update(id, None, now, |t| {
            if t.source_revision != expected || t.detected_language == detected {
                return false;
            }
            t.detected_language = detected;
            true
        })
    }
    pub fn source_touch(&self, id: &str, now: i64) -> Option<Task> {
        self.update(id, None, now, |_| true)
    }
    pub fn apply_source(
        &self,
        id: &str,
        expected: u64,
        snapshot: SourceSnapshot,
        remember: bool,
        memory_only: bool,
        now: i64,
    ) -> Option<Task> {
        self.apply_source_change(
            id,
            expected,
            snapshot,
            SourceChange {
                remember,
                memory_only,
                cleanup_undo: None,
            },
            now,
        )
    }
    pub fn apply_cleaned_source(
        &self,
        id: &str,
        expected: u64,
        snapshot: SourceSnapshot,
        unclean_text: String,
        memory_only: bool,
        now: i64,
    ) -> Option<Task> {
        self.apply_source_change(
            id,
            expected,
            snapshot,
            SourceChange {
                remember: true,
                memory_only,
                cleanup_undo: Some(unclean_text),
            },
            now,
        )
    }
    fn apply_source_change(
        &self,
        id: &str,
        expected: u64,
        snapshot: SourceSnapshot,
        change: SourceChange,
        now: i64,
    ) -> Option<Task> {
        let mut inner = self.inner.lock().unwrap();
        let mut candidate = inner.tasks.get(id)?.clone();
        if candidate.source_revision != expected
            || !candidate.status.is_terminal()
            || candidate.sensitive.is_some()
        {
            return None;
        }
        let previous_bytes = candidate.content_bytes();
        let source_bytes = candidate.source_snapshot().content_bytes();
        if change.remember {
            candidate.source_undo.push(candidate.source_snapshot());
            if candidate.source_undo.len() > 10 {
                candidate.source_undo.remove(0);
            }
        } else {
            candidate.source_undo.pop();
        }
        candidate.original_text = snapshot.original_text;
        candidate.edited_text = snapshot.edited_text;
        candidate.source_language = snapshot.source_language;
        candidate.target_language = snapshot.target_language;
        candidate.source_segments = snapshot.segments;
        candidate.cleanup_undo = change
            .cleanup_undo
            .map(|text| (candidate.source_revision.saturating_add(1), text));
        candidate.can_undo_cleanup = candidate.cleanup_undo.is_some();
        candidate.has_image = candidate
            .source_segments
            .iter()
            .any(|s| s.asset_id.is_some());
        candidate.image_pending = false;
        candidate.detected_language = None;
        if candidate.translation.is_empty() {
            candidate.status = Status::Ready;
        }
        if change.memory_only {
            candidate.save_state = SaveState::MemoryOnly;
        } else if candidate.save_state == SaveState::Saved {
            candidate.save_state = SaveState::Pending;
        }
        let bytes = inner
            .tasks
            .values()
            .map(Task::content_bytes)
            .sum::<usize>()
            .saturating_sub(previous_bytes)
            + candidate.content_bytes()
            + source_bytes;
        if bytes > 32 * 1024 * 1024 {
            return None;
        }
        self.update_locked(&mut inner, id, None, now, |task| {
            *task = candidate;
            true
        })
    }

    pub fn attach_layout(
        &self,
        id: &str,
        layout: crate::source::layout::OcrLayout,
        now: i64,
    ) -> Option<Task> {
        self.update(id, None, now, |task| {
            if task.sensitive.is_some() {
                return false;
            }
            let Some(segment) = task
                .source_segments
                .iter_mut()
                .find(|s| s.kind == Source::Screenshot)
            else {
                return false;
            };
            segment.ocr_layout = Some(layout);
            true
        })
    }
    pub fn failover(
        &self,
        id: &str,
        attempt: u32,
        profile: Profile,
        error: String,
        now: i64,
    ) -> Option<Task> {
        self.update(id, Some(attempt), now, |task| {
            if !matches!(task.status, Status::Queued | Status::Translating)
                || !task.translation.is_empty()
                || task.sensitive.is_some()
            {
                return false;
            }
            if task.past_attempts.len() >= 5 {
                task.past_attempts.remove(0);
            }
            task.past_attempts.push(PastAttempt {
                preserve_structure: task.preserve_structure,
                failure: task.failure.take(),
                reason: "failover".into(),
                number: task.attempt,
                profile: task.profile.clone(),
                status: Status::Failed,
                translation: String::new(),
                error: Some(error),
                timings: task.timings.clone(),
                source_revision: task.translation_source_revision,
                source_text: task.translation_source_text.clone(),
            });
            task.attempt += 1;
            task.profile = profile;
            task.status = Status::Queued;
            task.timings = Timings::default();
            task.error = None;
            task.cache_hit = false;
            task.vocabulary.clear();
            task.vocabulary_notice = None;
            task.vocabulary_state = VocabularyState::None;
            true
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_versions_ignore_stream_updates_and_undo_restores_edit() {
        let book = TaskBook::new(Retention::Disabled, 12);
        let task = book
            .create(Source::Selection, Profile::default(), "zh-Hans".into(), 0)
            .unwrap();
        book.recognized(&task.id, "Original paragraph".into(), 0, 1)
            .unwrap();
        let task = book.ready(&task.id, Some("en".into()), 2).unwrap();
        let revision = task.source_revision;
        let mut edit = task.source_snapshot();
        edit.edited_text = Some("Edited paragraph".into());
        let task = book
            .apply_source(&task.id, revision, edit, true, false, 3)
            .unwrap();
        assert_eq!(task.source_revision, revision + 1);
        let prior = task.source_undo.last().unwrap().clone();
        let task = book
            .apply_source(&task.id, task.source_revision, prior, false, false, 4)
            .unwrap();
        assert_eq!(task.query_text(), "Original paragraph");
        assert!(task.source_undo.is_empty());
        let submitted = book.submit_ready(&task.id, None, 5).unwrap().unwrap();
        book.start(&task.id, 1, 0, 6);
        book.append(&task.id, 1, "译文", 1, 7).unwrap();
        assert_eq!(
            book.get(&task.id).unwrap().source_revision,
            submitted.source_revision
        );
        assert!(
            book.apply_source(
                &task.id,
                task.source_revision,
                task.source_snapshot(),
                true,
                false,
                8
            )
            .is_none()
        );
    }
}
