//! Source commands validate IPC, then forward local business rules to core.
use crate::{
    capture::AppendTarget,
    state::{self, AppState},
};
use std::sync::{Arc, atomic::Ordering};
use tauri::{AppHandle, Emitter, Manager};
use yurelay_core::{
    engine::Engine,
    source::{
        CleanRequest, PendingAction, PendingSummary, SourceAsset, SourceRequest, UpdateRequest,
        cleaner::{CleanMode, CleanResult},
        encounters::Encounters,
        layout::{OcrBlock, OcrLayout},
    },
    tasks::{Source, Task},
};
use yurelay_platform::NativeEvent;

#[tauri::command]
pub async fn source_update(app: AppHandle, request: UpdateRequest) -> Result<Task, String> {
    state::engine(&app).await?.source_update(request).await
}
#[tauri::command]
pub async fn source_clean_preview(
    app: AppHandle,
    request: CleanRequest,
) -> Result<CleanResult, String> {
    state::engine(&app)
        .await?
        .source_clean_preview(request)
        .await
}
#[tauri::command]
pub async fn source_clean_apply(app: AppHandle, request: CleanRequest) -> Result<Task, String> {
    state::engine(&app).await?.source_clean_apply(request).await
}
#[tauri::command]
pub async fn source_clean_text(
    app: AppHandle,
    text: String,
    mode: CleanMode,
) -> Result<CleanResult, String> {
    state::engine(&app).await?.source_clean_text(&text, mode)
}
#[tauri::command]
pub async fn source_undo(app: AppHandle, request: SourceRequest) -> Result<Task, String> {
    state::engine(&app).await?.source_undo(request).await
}
#[tauri::command]
pub async fn source_undo_cleanup(app: AppHandle, request: SourceRequest) -> Result<Task, String> {
    state::engine(&app)
        .await?
        .source_undo_cleanup(request)
        .await
}
#[tauri::command]
pub async fn source_restore(app: AppHandle, request: SourceRequest) -> Result<Task, String> {
    state::engine(&app).await?.source_restore(request).await
}
#[tauri::command]
pub async fn source_remove_last(app: AppHandle, request: SourceRequest) -> Result<Task, String> {
    state::engine(&app).await?.source_remove_last(request).await
}
#[tauri::command]
pub async fn source_layout_apply(app: AppHandle, request: SourceRequest) -> Result<Task, String> {
    state::engine(&app)
        .await?
        .source_layout_apply(request)
        .await
}
#[tauri::command]
pub async fn source_encounters(
    app: AppHandle,
    request: SourceRequest,
) -> Result<Encounters, String> {
    let engine = state::engine(&app).await?;
    let text = engine.source_for_encounter(&request).await?;
    let (tokens, detected) = tokio::task::spawn_blocking(move || {
        (
            yurelay_platform::token_ranges(&text),
            yurelay_platform::detect_language(&text),
        )
    })
    .await
    .map_err(|_| "本地分词未完成。")?;
    let tokens = tokens?;
    engine.book.source_detected(
        &request.task_id,
        request.expected_revision,
        detected,
        engine.now(),
    );
    let tokens = tokens
        .into_iter()
        .map(|t| yurelay_core::source::encounters::TokenRange {
            start_utf16: t.start_utf16,
            end_utf16: t.end_utf16,
            surface: t.surface,
            lemma: t.lemma,
        })
        .collect();
    engine.source_encounters(request, tokens).await
}
#[tauri::command]
pub async fn source_assets(app: AppHandle, task_id: String) -> Result<Vec<SourceAsset>, String> {
    state::engine(&app).await?.source_assets(&task_id).await
}
#[tauri::command]
pub async fn source_asset(
    app: AppHandle,
    task_id: String,
    asset_id: String,
) -> Result<Vec<u8>, String> {
    state::engine(&app)
        .await?
        .source_asset(&task_id, &asset_id)
        .await?
        .map(|bytes| bytes.as_ref().clone())
        .ok_or("截图已不在本地。".into())
}
#[tauri::command]
pub async fn source_pending(
    app: AppHandle,
    task_id: Option<String>,
) -> Result<Vec<PendingSummary>, String> {
    Ok(state::engine(&app)
        .await?
        .source_pending(task_id.as_deref()))
}
#[tauri::command]
pub async fn resolve_source_pending(
    app: AppHandle,
    id: String,
    action: PendingAction,
    task_id: Option<String>,
    expected_revision: Option<u64>,
    allow_sensitive: Option<bool>,
) -> Result<Option<Task>, String> {
    let target = task_id
        .zip(expected_revision)
        .map(|(task_id, expected_revision)| SourceRequest {
            task_id,
            expected_revision,
        });
    let result = state::engine(&app)
        .await?
        .resolve_source_pending(&id, action, target, allow_sensitive.unwrap_or(false))
        .await;
    let _ = app.emit("source-pending-changed", ());
    let task = result?;
    if let Some(task) = &task {
        state::select(&app, task.id.clone());
    }
    Ok(task)
}
#[tauri::command]
pub fn set_append_mode(
    app: AppHandle,
    enabled: bool,
    task_id: Option<String>,
    expected_revision: Option<u64>,
) -> Result<(), String> {
    let target = if enabled {
        task_id
            .zip(expected_revision)
            .map(|(task_id, expected_revision)| AppendTarget {
                task_id,
                expected_revision,
            })
    } else {
        None
    };
    app.state::<AppState>().captures.set_append_target(target);
    Ok(())
}
pub fn append_target(app: &AppHandle) -> Option<AppendTarget> {
    let state = app.state::<AppState>();
    state.captures.append_target().or_else(|| {
        let id = state.selected.lock().unwrap().clone()?;
        let task = state.engine.get()?.book.get(&id)?;
        Some(AppendTarget {
            task_id: task.id,
            expected_revision: task.source_revision,
        })
    })
}
#[tauri::command]
pub fn capture_append(app: AppHandle, screenshot: bool) {
    let target = append_target(&app);
    start_capture_target(app, screenshot, None, target);
}
#[tauri::command]
pub fn capture_with_mode(
    app: AppHandle,
    screenshot: bool,
    task_id: Option<String>,
    expected_revision: Option<u64>,
) {
    let target = task_id
        .zip(expected_revision)
        .map(|(task_id, expected_revision)| AppendTarget {
            task_id,
            expected_revision,
        });
    start_capture_target(app, screenshot, None, target);
}
pub fn core_layout(layout: yurelay_platform::OcrLayout) -> OcrLayout {
    OcrLayout {
        width: layout.width,
        height: layout.height,
        orientation: layout.orientation,
        blocks: layout
            .blocks
            .into_iter()
            .map(|b| OcrBlock {
                text: b.text,
                confidence: b.confidence,
                x: b.x,
                y: b.y,
                width: b.width,
                height: b.height,
            })
            .collect(),
    }
}
pub fn accept_ocr_layout(engine: &Arc<Engine>, id: &str, layout: yurelay_platform::OcrLayout) {
    engine.accept_ocr_layout(id, core_layout(layout));
}

pub fn start_capture_target(
    app: AppHandle,
    screenshot: bool,
    selection_target: Option<i32>,
    target: Option<AppendTarget>,
) {
    let Some(target) = target else {
        crate::commands::start_capture_target(app, screenshot, selection_target);
        return;
    };
    let selection_target = if screenshot {
        selection_target
    } else {
        selection_target.or_else(yurelay_platform::selection_target)
    };
    let captures = app.state::<AppState>().captures.clone();
    let recording = app
        .state::<AppState>()
        .shortcut_recording
        .load(Ordering::SeqCst);
    captures.start(recording, screenshot, selection_target, |permit| {
        let epoch = state::begin_capture(&app);
        let permissions = yurelay_platform::permissions();
        if screenshot && !permissions.screen || !screenshot && !permissions.accessibility {
            state::notice(
                &app,
                if screenshot {
                    "请先在设置中开启屏幕录制权限。"
                } else {
                    "请先在设置中开启辅助功能权限。"
                }
                .into(),
            );
            state::present_capture(&app, epoch);
            return;
        }
        let mut native = yurelay_platform::start_for_target(screenshot, selection_target);
        tauri::async_runtime::spawn(async move {
            let _permit = permit;
            let mut text = None;
            let mut image = None;
            let mut layout = None;
            let mut error = None;
            let mut image_error = None;
            let mut cancelled = false;
            while let Some(event) = native.recv().await {
                match event {
                    NativeEvent::Text(value) => text = Some(value),
                    NativeEvent::Image(bytes) => image = Some(bytes),
                    NativeEvent::Layout(value) => layout = Some(core_layout(value)),
                    NativeEvent::Error(value) | NativeEvent::OcrError(value) => error = Some(value),
                    NativeEvent::ImageError(value) => image_error = Some(value),
                    NativeEvent::Cancelled => cancelled = true,
                    NativeEvent::Captured => {}
                }
            }
            if cancelled {
                return;
            }
            if let Some(error) = error {
                state::notice(&app, error);
                state::present_capture(&app, epoch);
                return;
            }
            let Some(text) = text else {
                state::notice(&app, "没有识别到文字，原稿未改变。".into());
                state::present_capture(&app, epoch);
                return;
            };
            let engine = match state::engine(&app).await {
                Ok(engine) => engine,
                Err(error) => {
                    state::notice(&app, error);
                    state::present_capture(&app, epoch);
                    return;
                }
            };
            let result = engine
                .source_append(
                    SourceRequest {
                        task_id: target.task_id.clone(),
                        expected_revision: target.expected_revision,
                    },
                    if screenshot {
                        Source::Screenshot
                    } else {
                        Source::Selection
                    },
                    text,
                    image,
                    layout,
                )
                .await;
            let _ = app.emit("source-pending-changed", ());
            match result {
                Ok(Some(task)) => {
                    if let Some(error) = image_error {
                        engine.fail_image(&task.id, error);
                    }
                    let captures = &app.state::<AppState>().captures;
                    if captures.append_target().is_some_and(|bound| {
                        bound.task_id == target.task_id
                            && bound.expected_revision == target.expected_revision
                    }) {
                        captures.set_append_target(Some(AppendTarget {
                            task_id: task.id.clone(),
                            expected_revision: task.source_revision,
                        }));
                    }
                    state::select(&app, task.id);
                }
                Ok(None) => state::notice(&app, "新片段待确认，原安全稿保持不变。".into()),
                Err(error) => state::notice(&app, error),
            }
            state::present_capture(&app, epoch);
        });
    });
}
