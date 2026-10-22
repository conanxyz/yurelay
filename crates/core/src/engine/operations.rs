use super::*;
use crate::operations::{
    BackTranslationState, OperationAttempt, OperationRecord, OperationRequest, OperationStatus,
    PreparedOperation, ReplyParser,
};

impl Engine {
    async fn operation_permit(
        &self,
        weight: u32,
        deadline: tokio::time::Instant,
        cancel: &CancellationToken,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, provider::Error> {
        loop {
            let permit = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(provider::Error::Cancelled),
                result = tokio::time::timeout_at(deadline, self.permits.clone().acquire_many_owned(weight)) => {
                    result.map_err(|_|provider::Error::Timeout)?
                        .map_err(|_|provider::Error::Cancelled)?
                }
            };
            // An assistant queued earlier may receive the semaphore first. Give
            // it back while a main translation is waiting; never interrupt work
            // whose visible output is already streaming.
            if !self
                .book
                .snapshot()
                .tasks
                .iter()
                .any(|task| task.status == Status::Queued)
            {
                return Ok(permit);
            }
            drop(permit);
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(provider::Error::Cancelled),
                _ = tokio::time::sleep_until(deadline) => return Err(provider::Error::Timeout),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
        }
    }

    pub async fn submit_operation(
        self: &Arc<Self>,
        request: OperationRequest,
    ) -> Result<OperationRecord, String> {
        self.operations.expire(self.now());
        if let Some(existing) = self.operations.existing(&request)? {
            return Ok(existing);
        }
        if self.closing.load(Ordering::Relaxed) {
            return Err("应用正在退出。".into());
        }
        request.validate()?;
        let sensitive = request.sensitive_kind();
        let guarded = self.sensitive_guard.load(Ordering::Relaxed) && sensitive.is_some();
        let mut parent = if let Some(id) = &request.task_id {
            Some(self.load_into_book(id).await?)
        } else {
            None
        };
        if let Some(task) = &parent {
            if request
                .source_revision
                .is_some_and(|r| r != task.source_revision)
            {
                return Err("原文已变化，请重新选择。".into());
            }
            if task.sensitive.is_some() {
                return Err("请先处理原文的敏感内容。".into());
            }
            if task.query_text() != request.source_text && !task.status.is_terminal() {
                return Err("正文正在生成，请先停止或等待完成后再修改原文。".into());
            }
        }
        let fallback_language = parent
            .as_ref()
            .filter(|task| task.query_text() == request.source_text)
            .and_then(|task| {
                task.source_language
                    .as_deref()
                    .filter(|language| crate::operations::explicit_language(language))
                    .or_else(|| {
                        task.detected_language
                            .as_deref()
                            .filter(|language| crate::operations::explicit_language(language))
                    })
            });
        // Reject invalid selections/intents before committing any edited source.
        request.prepare_for_profile(&Profile::default(), fallback_language)?;
        if !guarded
            && let Some(task) = &parent
            && task.query_text() != request.source_text
        {
            parent = Some(
                self.source_update(crate::source::UpdateRequest {
                    source: crate::source::SourceRequest {
                        task_id: task.id.clone(),
                        expected_revision: task.source_revision,
                    },
                    text: request.source_text.clone(),
                    source_language: None,
                    target_language: None,
                })
                .await?,
            );
        }
        let admission_guard = self.mutations.lock().await;
        if let Some(parent) = &parent {
            let current = self
                .book
                .get(&parent.id)
                .filter(|task| !task.expired(self.now()))
                .ok_or("记录已删除或到期。")?;
            if current.source_revision != parent.source_revision {
                return Err("原文已变化，请重新选择。".into());
            }
        }
        let live = self.live.read().await;
        let profile_id = request
            .profile_id
            .as_deref()
            .or_else(|| parent.as_ref().map(|task| task.profile.id.as_str()));
        let profile = live
            .settings
            .resolved_profile(query_profile(&live.settings, profile_id)?);
        if !crate::reliability::supports_generation(&profile) {
            return Err("语境解析和回复需要模型服务，请先选择已配置的模型。".into());
        }
        profile.validate()?;
        let fallback_language = parent
            .as_ref()
            .filter(|task| task.query_text() == request.source_text)
            .and_then(|task| {
                task.source_language
                    .as_deref()
                    .filter(|language| crate::operations::explicit_language(language))
                    .or_else(|| {
                        task.detected_language
                            .as_deref()
                            .filter(|language| crate::operations::explicit_language(language))
                    })
            });
        let prepared = request.prepare_for_profile(&profile, fallback_language)?;
        let now = self.now();
        let record = OperationRecord {
            id: uuid::Uuid::new_v4().to_string(),
            request_id: request.request_id.clone(),
            task_id: request.task_id.clone(),
            source_revision: parent
                .as_ref()
                .map_or(request.source_revision.unwrap_or(0), |p| p.source_revision),
            kind: request.kind,
            status: if guarded {
                OperationStatus::Blocked
            } else {
                OperationStatus::Queued
            },
            primary_text: String::new(),
            back_translation: String::new(),
            back_translation_state: BackTranslationState::None,
            profile,
            attempt: 1,
            attempts: vec![],
            error: None,
            failure: None,
            sensitive: if guarded { sensitive } else { None },
            output_language: prepared.output_language.clone(),
            reference_language: prepared.reference_language.clone(),
            created_at: now,
            updated_at: now,
            expires_at: parent
                .as_ref()
                .map_or_else(|| live.settings.retention.expires_at(now), |p| p.expires_at),
            revision: 1,
            memory_only: sensitive.is_some()
                || live.settings.retention == Retention::Disabled
                || parent
                    .as_ref()
                    .is_some_and(|p| p.save_state == SaveState::MemoryOnly),
            save_error: None,
        };
        drop(live);
        let admission = self.operations.admit(request, record)?;
        drop(admission_guard);
        if admission.created && !guarded {
            if let Some(parent) = &parent {
                self.persist(&parent.id).await;
            }
            self.persist_operation(&admission.record.id).await;
            self.schedule_operation(admission.record.clone(), prepared, sensitive.is_some())
                .await;
        }
        Ok(self
            .operations
            .get(&admission.record.id)
            .unwrap_or(admission.record))
    }

    pub async fn resolve_operation_sensitive(
        self: &Arc<Self>,
        id: &str,
        allow: bool,
    ) -> Result<OperationRecord, String> {
        let admission_guard = self.mutations.lock().await;
        let record = self.operations.get(id).ok_or("操作已清空或过期。")?;
        if record.status != OperationStatus::Blocked {
            return Err("此操作没有等待放行的内容。".into());
        }
        if !allow {
            return self
                .operations
                .cancel(id, self.now())
                .ok_or("操作已清空。".into());
        }
        let request = self.operations.request(id).ok_or("待确认内容已清空。")?;
        if let Some(task_id) = &request.task_id {
            let task = self.load_into_book(task_id).await?;
            if task.source_revision != record.source_revision {
                return Err("原文已变化，请重新生成并确认。".into());
            }
        }
        let mut prepared =
            request.prepare_for_profile(&record.profile, Some(&record.output_language))?;
        prepared.explicitly_allowed = true;
        let changed = self
            .operations
            .release_blocked(id, self.now())
            .ok_or("操作已过期。")?;
        drop(admission_guard);
        self.schedule_operation(changed.clone(), prepared, true)
            .await;
        Ok(changed)
    }

    async fn schedule_operation(
        self: &Arc<Self>,
        record: OperationRecord,
        prepared: PreparedOperation,
        sensitive: bool,
    ) {
        let profiles = self
            .dispatch_profiles(&record.profile, sensitive, true)
            .await;
        let current = self.live.read().await;
        if !current
            .settings
            .profiles
            .iter()
            .any(|p| current.settings.resolved_profile(p) == record.profile)
        {
            self.operations
                .change(&record.id, record.attempt, self.now(), |r| {
                    if r.status.active() {
                        r.status = OperationStatus::Failed;
                        r.error = Some("服务配置已变化，请重新生成。".into());
                    }
                });
            return;
        }
        let weight = if current.settings.max_concurrency == 1 {
            2
        } else {
            1
        };
        drop(current);
        let Some(cancel) = self.operations.token(&record.id) else {
            return;
        };
        let engine = self.clone();
        tokio::spawn(async move {
            engine
                .run_operation(record, prepared, profiles, sensitive, weight, cancel)
                .await;
        });
    }

    async fn run_operation(
        self: Arc<Self>,
        mut record: OperationRecord,
        prepared: PreparedOperation,
        profiles: Vec<DispatchProfile>,
        sensitive: bool,
        weight: u32,
        cancel: CancellationToken,
    ) {
        let mut budget = crate::reliability::DispatchBudget::default();
        let initial_profile_id = record.profile.id.clone();
        let mut pending_failure: Option<provider::Error> = None;
        for dispatch in profiles {
            if cancel.is_cancelled() {
                break;
            }
            if dispatch.profile.id != initial_profile_id
                && !self
                    .dispatch_profile_eligible(&dispatch.profile, true)
                    .await
            {
                continue;
            }
            if !budget.begin(&dispatch.profile.id) {
                break;
            }
            let permit = self
                .operation_permit(weight, budget.deadline, &cancel)
                .await;
            if matches!(permit, Err(provider::Error::Cancelled)) {
                break;
            }
            let Ok(permit) = permit else {
                self.operations
                    .change(&record.id, record.attempt, self.now(), |r| {
                        if r.status.active() {
                            r.status = OperationStatus::Failed;
                            r.failure = Some(crate::reliability::FailureSummary::from_error(
                                &provider::Error::Timeout,
                            ));
                            r.error = Some("请求等待超时，请重试。".into());
                        }
                    });
                break;
            };
            let mut primary_permit = Some(permit);
            if sensitive
                && !prepared.explicitly_allowed
                && self.sensitive_guard.load(Ordering::Relaxed)
            {
                let kind = self
                    .operations
                    .request(&record.id)
                    .and_then(|input| input.sensitive_kind());
                self.operations
                    .change(&record.id, record.attempt, self.now(), |r| {
                        if r.status.active() {
                            r.status = OperationStatus::Blocked;
                            r.sensitive = kind;
                            r.memory_only = true;
                        }
                    });
                break;
            }
            if cancel.is_cancelled() {
                break;
            }
            let Some(changed) =
                self.operations
                    .change(&record.id, record.attempt, self.now(), |r| {
                        if !r.status.active() {
                            return;
                        }
                        if let Some(error) = pending_failure.take() {
                            r.attempts.push(OperationAttempt {
                                number: r.attempt,
                                profile: r.profile.clone(),
                                status: OperationStatus::Failed,
                                error: Some(error.to_string()),
                                failure: Some(crate::reliability::FailureSummary::from_error(
                                    &error,
                                )),
                                reason: "failover".into(),
                                partial_text: r.primary_text.clone(),
                                source_revision: r.source_revision,
                                back_translation: r.back_translation.clone(),
                            });
                            if r.attempts.len() > 5 {
                                r.attempts.remove(0);
                            }
                            r.attempt = r.attempt.saturating_add(1);
                            r.primary_text.clear();
                            r.back_translation.clear();
                        }
                        r.status = OperationStatus::Generating;
                        r.profile = dispatch.profile.clone();
                        r.error = None;
                        r.failure = None;
                    })
            else {
                break;
            };
            if !changed.status.active() || cancel.is_cancelled() {
                break;
            }
            record = changed;
            let mut parser = ReplyParser::default();
            let mut auxiliary_permit = None;
            let mut auxiliary_limited = false;
            let mut boundary = false;
            let append = |part: &str, _ms| {
                let (primary, reference, at_boundary) = if prepared.back_translation {
                    parser.push(part)
                } else {
                    (part.to_owned(), String::new(), false)
                };
                match self.operations.append(
                    &record.id,
                    record.attempt,
                    &primary,
                    &reference,
                    at_boundary,
                    self.now(),
                ) {
                    Ok(true) => {}
                    Ok(false) => return Err(provider::Error::Cancelled),
                    Err(error) => return Err(provider::Error::Protocol(error)),
                }
                if at_boundary {
                    boundary = true;
                    drop(primary_permit.take());
                    auxiliary_permit = self.metadata_permits.clone().try_acquire_owned().ok();
                    if auxiliary_permit.is_none() {
                        auxiliary_limited = true;
                        return Err(provider::Error::Cancelled);
                    }
                }
                Ok(())
            };
            let attempt_instruction = self
                .operations
                .request(&record.id)
                .ok_or(provider::Error::Cancelled)
                .and_then(|request| {
                    request
                        .prepare_for_profile(&dispatch.profile, Some(&prepared.output_language))
                        .map(|prepared| prepared.instruction)
                        .map_err(provider::Error::Configuration)
                });
            let result = if let Some(error) = dispatch.credential_error {
                Err(provider::Error::Configuration(error))
            } else if let Ok(attempt_instruction) = attempt_instruction {
                match tokio::time::timeout_at(
                    budget.deadline,
                    self.client.generate(
                        &dispatch.profile,
                        dispatch.key.as_str(),
                        provider::GenerationRequest {
                            input: &prepared.input,
                            instruction: &attempt_instruction,
                        },
                        cancel.clone(),
                        append,
                    ),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => Err(provider::Error::Timeout),
                }
            } else {
                Err(attempt_instruction
                    .err()
                    .unwrap_or(provider::Error::Cancelled))
            };
            drop(primary_permit);
            drop(auxiliary_permit);
            if cancel.is_cancelled() {
                self.operations.cancel(&record.id, self.now());
                break;
            }
            if prepared.back_translation {
                let tail = parser.finish();
                if !tail.is_empty() {
                    let _ = self.operations.append(
                        &record.id,
                        record.attempt,
                        &tail,
                        "",
                        false,
                        self.now(),
                    );
                }
            }
            let Some(current) = self.operations.get(&record.id) else {
                break;
            };
            if current.attempt != record.attempt {
                break;
            }
            let has_primary = !current.primary_text.trim().is_empty();
            match result {
                Ok(_) if has_primary => {
                    self.operations
                        .change(&record.id, record.attempt, self.now(), |r| {
                            if !r.status.active() {
                                return;
                            }
                            r.status = OperationStatus::Completed;
                            r.back_translation_state = if !prepared.back_translation {
                                BackTranslationState::None
                            } else if boundary && !r.back_translation.trim().is_empty() {
                                BackTranslationState::Complete
                            } else {
                                BackTranslationState::Failed
                            };
                            if prepared.back_translation
                                && r.back_translation_state == BackTranslationState::Failed
                            {
                                r.error = Some("回复已生成，未获得完整回译参考。".into());
                            }
                        });
                    break;
                }
                Err(provider::Error::Cancelled) if auxiliary_limited && has_primary => {
                    self.operations
                        .change(&record.id, record.attempt, self.now(), |r| {
                            if !r.status.active() {
                                return;
                            }
                            r.status = OperationStatus::Completed;
                            r.back_translation_state = BackTranslationState::Cancelled;
                            r.error = Some("回复已生成，回译参考因并发限制未继续。".into());
                        });
                    break;
                }
                outcome => {
                    let error = outcome.err().unwrap_or(provider::Error::Empty);
                    self.invalidate_service_failure(&dispatch.profile, &error)
                        .await;
                    let fallback = !boundary
                        && crate::reliability::permits_fallback(
                            &error,
                            has_primary,
                            cancel.is_cancelled(),
                            sensitive,
                        );
                    if fallback {
                        self.operations
                            .change(&record.id, record.attempt, self.now(), |r| {
                                if r.status.active() {
                                    r.error = Some(error.to_string());
                                    r.failure = Some(
                                        crate::reliability::FailureSummary::from_error(&error),
                                    );
                                }
                            });
                        pending_failure = Some(error);
                        continue;
                    }
                    self.operations
                        .change(&record.id, record.attempt, self.now(), |r| {
                            if !r.status.active() {
                                return;
                            }
                            r.failure =
                                Some(crate::reliability::FailureSummary::from_error(&error));
                            r.status = if boundary && has_primary {
                                OperationStatus::Completed
                            } else {
                                OperationStatus::Failed
                            };
                            r.error = Some(if boundary && has_primary {
                                "回复已生成，回译参考未完成。".into()
                            } else {
                                error.to_string()
                            });
                            if prepared.back_translation {
                                r.back_translation_state = BackTranslationState::Failed;
                            }
                        });
                    break;
                }
            }
        }
        if let Some(current) = self.operations.get(&record.id)
            && current.attempt == record.attempt
            && current.status.active()
        {
            self.operations
                .change(&record.id, current.attempt, self.now(), |r| {
                    if !r.status.active() {
                        return;
                    }
                    r.status = if cancel.is_cancelled() {
                        OperationStatus::Stopped
                    } else {
                        OperationStatus::Failed
                    };
                    if cancel.is_cancelled() {
                        r.error = Some("已停止。".into());
                    } else if let Some(error) = pending_failure {
                        r.error = Some(error.to_string());
                        r.failure = Some(crate::reliability::FailureSummary::from_error(&error));
                    } else {
                        r.error = Some("没有可用的备用服务，或请求已超时。".into());
                    }
                });
        }
        self.persist_operation(&record.id).await;
    }

    pub async fn cancel_operation(&self, id: &str) -> Result<(), String> {
        self.operations
            .cancel(id, self.now())
            .ok_or("操作已清空或过期。")?;
        self.persist_operation(id).await;
        Ok(())
    }
    pub async fn operation_list(
        &self,
        task_id: Option<String>,
    ) -> Result<Vec<OperationRecord>, String> {
        // Keep the disk snapshot and its session restoration on one side of
        // deletion/retention changes; a late read must not resurrect cleared rows.
        let _guard = self.mutations.lock().await;
        self.operations
            .restore(self.storage.operation_history(task_id.clone()).await?);
        self.operations.expire(self.now());
        Ok(self.operations.records(task_id.as_deref(), self.now()))
    }
    async fn persist_operation(&self, id: &str) {
        let Some(saved) = self.operations.saved(id) else {
            if self.operations.get(id).is_some_and(|r| r.memory_only) {
                let _ = self.storage.remove_operation(id.to_owned()).await;
            }
            return;
        };
        let attempt = saved.record.attempt;
        let result = self.storage.save_operation(saved).await;
        match result {
            Err(error) => {
                self.operations
                    .change(id, attempt, self.now(), |r| r.save_error = Some(error));
            }
            Ok(storage::SaveOutcome::MemoryOnly) => {
                let disabled = self.live.read().await.settings.retention == Retention::Disabled;
                self.operations.change(id, attempt, self.now(), |r| {
                    r.memory_only = true;
                    if !disabled {
                        r.save_error =
                            Some("操作历史已达到容量限制；结果仅保留在当前会话。".into());
                    }
                });
                let _ = self.storage.remove_operation(id.to_owned()).await;
            }
            Ok(storage::SaveOutcome::Saved) => {
                if self
                    .operations
                    .get(id)
                    .is_some_and(|r| r.save_error.is_some())
                {
                    self.operations
                        .change(id, attempt, self.now(), |r| r.save_error = None);
                }
            }
            Ok(
                storage::SaveOutcome::Dropped
                | storage::SaveOutcome::WaitingForImage
                | storage::SaveOutcome::PartialImageFailure,
            ) => {}
        }
    }
    pub(crate) async fn cleanup_operations(&self) -> Result<(), String> {
        let live = self.live.read().await;
        let expiry = self
            .book
            .snapshot()
            .tasks
            .into_iter()
            .map(|task| (task.id, task.expires_at))
            .collect();
        self.operations
            .refresh_expiry(live.settings.retention, &expiry);
        self.operations.expire(self.now());
        drop(live);
        self.storage.prune_operations().await
    }
}

#[cfg(test)]
mod tests;
