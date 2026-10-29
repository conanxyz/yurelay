use crate::state::{self, AppState, ShortcutReport};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::atomic::Ordering,
    time::Duration,
};
use tauri::{AppHandle, Manager, ipc::Channel};
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use tokio_util::sync::CancellationToken;
use yurelay_core::{
    config::{Profile, Settings},
    engine::Configuration,
    provider::Metrics,
    storage::Stats,
    tasks::{Event, Snapshot, Source, Task},
};
use yurelay_platform::{NativeEvent, Permissions};

#[derive(Serialize)]
pub struct Bootstrap {
    configuration: Configuration,
    permissions: Permissions,
    shortcuts: ShortcutReport,
    snapshot: Snapshot,
    selected: Option<String>,
    notice: Option<String>,
    pinned: bool,
    expanded: bool,
    window_sizing: crate::window_sizing::WindowSizing,
}

#[tauri::command]
pub async fn bootstrap(app: AppHandle) -> Result<Bootstrap, String> {
    let engine = state::engine(&app).await?;
    engine.cleanup().await?;
    let configuration = engine.configuration().await;
    let shortcuts = app
        .state::<AppState>()
        .shortcut_report
        .lock()
        .unwrap()
        .clone();
    let s = app.state::<AppState>();
    let view = state::window_snapshot(&s, || {
        app.get_webview_window("main").is_some_and(|w| {
            w.is_maximized().unwrap_or(false) || w.is_fullscreen().unwrap_or(false)
        })
    });
    let window_sizing = {
        let mut sizing = s.sizing.lock().unwrap();
        sizing.observe(view.expanded, false);
        *sizing
    };
    Ok(Bootstrap {
        configuration,
        permissions: yurelay_platform::permissions(),
        shortcuts,
        snapshot: engine.book.snapshot(),
        selected: view.selected,
        notice: state::current_notice(&app),
        pinned: view.pinned,
        expanded: view.expanded,
        window_sizing,
    })
}

#[tauri::command]
pub fn permissions(app: AppHandle) -> Permissions {
    state::current_notice(&app);
    app.state::<AppState>().resync.notify_one();
    yurelay_platform::permissions()
}
#[tauri::command]
pub fn request_permission(kind: String) -> Result<(), String> {
    yurelay_platform::request_permission(&kind)
}
#[tauri::command]
pub fn hide_window(app: AppHandle) {
    state::hide(&app)
}

#[tauri::command]
pub fn settings_guard_ready(app: AppHandle) {
    app.state::<AppState>().settings_guard.enable();
}

#[tauri::command]
pub fn show_settings(app: AppHandle) {
    state::show_settings(&app);
}

#[tauri::command]
pub fn cancel_settings_close(app: AppHandle, request_id: u64) {
    app.state::<AppState>().settings_guard.cancel(request_id);
}

#[tauri::command]
pub fn finish_settings_close(app: AppHandle, request_id: u64) -> Result<(), String> {
    match app.state::<AppState>().settings_guard.confirm(request_id)? {
        crate::settings_guard::Intent::Close => {
            if let Some(window) = app.get_webview_window("settings") {
                window
                    .hide()
                    .map_err(|_| "无法关闭设置窗口。".to_string())?;
            }
        }
        crate::settings_guard::Intent::Quit => app.exit(0),
    }
    Ok(())
}
#[tauri::command]
pub fn set_window_expanded(app: AppHandle, expanded: bool) -> Result<(), String> {
    crate::window_sizing::set_expanded(&app, expanded)
}

#[tauri::command]
pub fn dismiss_notice(app: AppHandle) {
    state::clear_notice(&app);
}
#[tauri::command]
pub async fn pin_window(app: AppHandle, pinned: bool) -> Result<bool, String> {
    state::set_pinned(&app, pinned).await
}
#[tauri::command]
pub async fn copy_text(text: String) -> Result<(), String> {
    if text.len() > 2 * 1024 * 1024 {
        return Err("文字过长。".into());
    }
    tokio::task::spawn_blocking(move || yurelay_platform::copy_text(&text))
        .await
        .map_err(|_| "复制失败。")?
}

pub fn start_capture(app: AppHandle, screenshot: bool) {
    start_capture_target(app, screenshot, None);
}

pub fn start_capture_target(app: AppHandle, screenshot: bool, selection_target: Option<i32>) {
    let append_target = app.state::<AppState>().captures.append_target();
    if let Some(target) = append_target {
        crate::source_commands::start_capture_target(
            app,
            screenshot,
            selection_target,
            Some(target),
        );
        return;
    }
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
        state::clear_capture_view(&app);
        let permissions = yurelay_platform::permissions();
        if (screenshot && !permissions.screen) || (!screenshot && !permissions.accessibility) {
            state::notice(
                &app,
                if screenshot {
                    "请先在设置中开启 Yurelay 的屏幕录制权限。"
                } else {
                    "请先在设置中开启 Yurelay 的辅助功能权限。"
                }
                .into(),
            );
            state::present_capture(&app, epoch);
            return;
        }
        // Native acquisition starts before any database/keychain await can change focus.
        let mut native = yurelay_platform::start_for_target(screenshot, selection_target);
        tauri::async_runtime::spawn(async move {
            let _capture = permit;
            let engine = match state::engine(&app).await {
                Ok(e) => e,
                Err(e) => {
                    state::notice(&app, e);
                    state::present_capture(&app, epoch);
                    while native.recv().await.is_some() {}
                    return;
                }
            };
            let handle = match engine
                .begin(if screenshot {
                    Source::Screenshot
                } else {
                    Source::Selection
                })
                .await
            {
                Ok(h) => h,
                Err(e) => {
                    state::notice(&app, e);
                    state::present_capture(&app, epoch);
                    while native.recv().await.is_some() {}
                    return;
                }
            };
            state::select(&app, handle.id.clone());
            let mut source_started = std::time::Instant::now();
            while let Some(event) = native.recv().await {
                match event {
                    NativeEvent::Captured => {
                        source_started = std::time::Instant::now();
                        engine.book.recognizing(&handle.id, engine.now());
                        state::present_capture(&app, epoch);
                    }
                    NativeEvent::Text(text) => {
                        let (text, detected) = tokio::task::spawn_blocking(move || {
                            let language = yurelay_platform::detect_language(&text);
                            (text, language)
                        })
                        .await
                        .map_err(|_| ())
                        .unwrap_or_default();
                        engine.accept_detected_text(
                            &handle,
                            text,
                            source_started.elapsed().as_millis() as u64,
                            detected,
                        );
                        if !screenshot {
                            state::present_capture(&app, epoch);
                        }
                    }
                    NativeEvent::Image(bytes) => engine.accept_image(&handle.id, bytes),
                    NativeEvent::Layout(layout) => {
                        crate::source_commands::accept_ocr_layout(&engine, &handle.id, layout)
                    }
                    NativeEvent::OcrError(error) => engine.fail_capture(&handle.id, 1, error),
                    NativeEvent::ImageError(error) => engine.fail_image(&handle.id, error),
                    NativeEvent::Error(error) => {
                        state::notice(&app, error);
                        let _ = engine.remove(&handle.id).await;
                        *app.state::<AppState>().selected.lock().unwrap() = None;
                        state::present_capture(&app, epoch);
                    }
                    NativeEvent::Cancelled => {
                        if engine
                            .book
                            .get(&handle.id)
                            .is_some_and(|t| t.original_text.is_empty() && !t.has_image)
                        {
                            let _ = engine.remove(&handle.id).await;
                        }
                    }
                }
            }
        });
    });
}
#[tauri::command]
pub fn capture(app: AppHandle) {
    start_capture(app, true)
}

#[tauri::command]
pub async fn credential_authorization_required(
    app: AppHandle,
    profile_id: Option<String>,
) -> Result<bool, String> {
    if app.state::<AppState>().captures.is_active() {
        return Err("原文仍在读取，请等待完成后再按 D 或 S。".into());
    }
    state::engine(&app)
        .await?
        .credential_authorization_required(profile_id.as_deref())
        .await
}

#[tauri::command]
pub async fn authorize_saved_credential(app: AppHandle, profile_id: String) -> Result<(), String> {
    let _dialog = crate::window_lifecycle::SystemDialogGuard::new(
        app.state::<AppState>().window_lifecycle.clone(),
    );
    state::engine(&app)
        .await?
        .authorize_saved_credential(&profile_id)
        .await
}

#[tauri::command]
pub async fn translate_text(
    app: AppHandle,
    text: String,
    source_language: Option<String>,
    target_language: Option<String>,
    profile_id: Option<String>,
) -> Result<String, String> {
    if app.state::<AppState>().captures.is_active() {
        return Err("原文仍在读取，请等待完成后再按 D 或 S。".into());
    }
    let engine = state::engine(&app).await?;
    let detected = if source_language.is_none() {
        detect_text_language(text.clone()).await?
    } else {
        None
    };
    let id = engine
        .manual_languages_with_profile(
            text,
            target_language.map(|target| (source_language, target)),
            detected,
            profile_id.as_deref(),
        )
        .await?;
    state::select(&app, id.clone());
    Ok(id)
}
#[tauri::command]
pub async fn stop_task(app: AppHandle, id: String) -> Result<(), String> {
    state::engine(&app).await?.stop(&id);
    Ok(())
}
#[tauri::command]
pub async fn retry_task(
    app: AppHandle,
    id: String,
    edited_text: Option<String>,
    source_language: Option<String>,
    target_language: Option<String>,
    profile_id: Option<String>,
) -> Result<(), String> {
    if app.state::<AppState>().captures.is_active() {
        return Err("原文仍在读取，请等待完成后再按 D 或 S。".into());
    }
    let engine = state::engine(&app).await?;
    let task = engine.load_into_book(&id).await?;
    let text = edited_text
        .as_deref()
        .unwrap_or_else(|| task.query_text())
        .to_owned();
    let detected = if source_language.is_none() {
        detect_text_language(text).await?
    } else {
        None
    };
    engine
        .retry_languages_with_profile(
            &id,
            edited_text,
            target_language.map(|target| (source_language, target)),
            detected,
            profile_id.as_deref(),
        )
        .await
}
#[tauri::command]
pub async fn retry_save(app: AppHandle, id: String) -> Result<(), String> {
    state::engine(&app).await?.retry_save(&id).await
}
#[tauri::command]
pub async fn select_task(app: AppHandle, id: String) -> Result<Task, String> {
    let task = state::engine(&app).await?.load_into_book(&id).await?;
    state::select(&app, id);
    Ok(task)
}
#[tauri::command]
pub async fn history(app: AppHandle, offset: usize) -> Result<Vec<Task>, String> {
    state::engine(&app).await?.storage.history(30, offset).await
}
#[tauri::command]
pub async fn remove_task(app: AppHandle, id: String) -> Result<(), String> {
    state::engine(&app).await?.remove(&id).await
}
#[tauri::command]
pub async fn clear_history(app: AppHandle) -> Result<(), String> {
    state::engine(&app).await?.clear().await
}
#[tauri::command]
pub async fn storage_stats(app: AppHandle) -> Result<Stats, String> {
    state::engine(&app).await?.storage.stats().await
}
#[tauri::command]
pub async fn fetch_models(
    app: AppHandle,
    profile: Profile,
    key_override: Option<String>,
) -> Result<Vec<yurelay_core::provider::ModelInfo>, String> {
    state::engine(&app)
        .await?
        .models(profile, key_override)
        .await
}
#[tauri::command]
pub async fn test_connection(
    app: AppHandle,
    profile: Profile,
    key_override: Option<String>,
) -> Result<Metrics, String> {
    state::engine(&app)
        .await?
        .test_connection(profile, key_override)
        .await
}

#[derive(Serialize)]
pub struct SavedConfiguration {
    configuration: Configuration,
    shortcuts: ShortcutReport,
}
#[tauri::command]
pub async fn save_configuration(
    app: AppHandle,
    settings: Settings,
    api_key_updates: BTreeMap<String, Option<String>>,
) -> Result<SavedConfiguration, String> {
    let _dialog = crate::window_lifecycle::SystemDialogGuard::new(
        app.state::<AppState>().window_lifecycle.clone(),
    );
    settings.validate()?;
    state::parse_shortcuts(
        &settings.selection_shortcut,
        &settings.screenshot_shortcut,
        settings.append_selection_shortcut.as_deref(),
        settings.append_screenshot_shortcut.as_deref(),
    )?;
    let engine = state::engine(&app).await?;
    let previous = engine.configuration().await.settings;
    let shortcuts = state::register_shortcuts(
        &app,
        &settings.selection_shortcut,
        &settings.screenshot_shortcut,
        settings.append_selection_shortcut.as_deref(),
        settings.append_screenshot_shortcut.as_deref(),
    );
    if !shortcuts.registered {
        return Err(shortcuts.error.unwrap_or("快捷键未生效。".into()));
    }
    let configuration = match engine.save_configuration(settings, api_key_updates).await {
        Ok(configuration) => configuration,
        Err(error) => {
            state::register_shortcuts(
                &app,
                &previous.selection_shortcut,
                &previous.screenshot_shortcut,
                previous.append_selection_shortcut.as_deref(),
                previous.append_screenshot_shortcut.as_deref(),
            );
            return Err(error);
        }
    };
    use tauri::Emitter;
    let _ = app.emit("configuration-changed", &configuration.settings);
    state::localize_menus(&app, &configuration.settings);
    Ok(SavedConfiguration {
        configuration,
        shortcuts,
    })
}
#[tauri::command]
pub async fn retry_shortcuts(app: AppHandle) -> Result<ShortcutReport, String> {
    let config = state::engine(&app).await?.configuration().await;
    Ok(state::register_shortcuts(
        &app,
        &config.settings.selection_shortcut,
        &config.settings.screenshot_shortcut,
        config.settings.append_selection_shortcut.as_deref(),
        config.settings.append_screenshot_shortcut.as_deref(),
    ))
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Wire {
    Snapshot {
        snapshot: Snapshot,
        selected: Option<String>,
        notice: Option<String>,
    },
    Events {
        events: Vec<Event>,
    },
}
fn snapshot_message(app: &AppHandle, engine: &yurelay_core::engine::Engine) -> Wire {
    let state = app.state::<AppState>();
    let selected = state.selected.lock().unwrap().clone();
    let notice = state::current_notice(app);
    Wire::Snapshot {
        snapshot: engine.book.snapshot(),
        selected,
        notice,
    }
}

#[tauri::command]
pub async fn subscribe(app: AppHandle, on_event: Channel<Wire>) -> Result<String, String> {
    let engine = state::engine(&app).await?;
    let (_, mut events) = engine.book.subscribe();
    let id = uuid::Uuid::new_v4().to_string();
    let stop = CancellationToken::new();
    if let Some((_, previous)) = app
        .state::<AppState>()
        .subscriber
        .lock()
        .unwrap()
        .replace((id.clone(), stop.clone()))
    {
        previous.cancel();
    }
    let wake = app.state::<AppState>().resync.clone();
    on_event
        .send(snapshot_message(&app, &engine))
        .map_err(|_| "无法连接结果窗口。")?;
    tauri::async_runtime::spawn(async move {
        let mut pending: HashMap<String, Event> = HashMap::new();
        let mut seen: HashSet<(String, u32)> = engine
            .book
            .snapshot()
            .tasks
            .into_iter()
            .filter(|t| !t.translation.is_empty())
            .map(|t| (t.id, t.attempt))
            .collect();
        let mut deadline = tokio::time::Instant::now();
        loop {
            tokio::select! {
                biased;
                _=stop.cancelled()=>break,
                _=wake.notified()=>{pending.clear();if on_event.send(snapshot_message(&app,&engine)).is_err(){break;}},
                _=tokio::time::sleep_until(deadline),if !pending.is_empty()=>{if !flush(&on_event,&mut pending){break;}},
                event=events.recv()=>{
                    if !app.state::<AppState>().window_lifecycle.lock().unwrap().visible(){pending.clear();continue;}
                    match event{
                        Ok(Event::Delta{id,attempt,revision,text})=>{
                            if seen.insert((id.clone(),attempt)){
                                if on_event.send(Wire::Events{events:vec![Event::Delta{id,attempt,revision,text}]}).is_err(){break;}
                            }else{
                                if pending.is_empty(){deadline=tokio::time::Instant::now()+Duration::from_millis(35);}
                                match pending.get_mut(&id){Some(Event::Delta{attempt:a,revision:r,text:t,..})if *a==attempt=>{*r=revision;t.push_str(&text);},_=>{pending.insert(id.clone(),Event::Delta{id,attempt,revision,text});}}
                            }
                        },
                        Ok(event)=>{if !flush(&on_event,&mut pending)||on_event.send(Wire::Events{events:vec![event]}).is_err(){break;}},
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{pending.clear();if on_event.send(snapshot_message(&app,&engine)).is_err(){break;}},
                        Err(_)=>break,
                    }
                }
            }
        }
    });
    Ok(id)
}

fn flush(channel: &Channel<Wire>, pending: &mut HashMap<String, Event>) -> bool {
    if pending.is_empty() {
        return true;
    }
    let mut events: Vec<_> = pending.drain().map(|(_, v)| v).collect();
    events.sort_by_key(|e| match e {
        Event::Delta { revision, .. } => *revision,
        _ => 0,
    });
    channel.send(Wire::Events { events }).is_ok()
}
#[tauri::command]
pub fn unsubscribe(app: AppHandle, subscription_id: String) {
    let state = app.state::<AppState>();
    let mut current = state.subscriber.lock().unwrap();
    if current
        .as_ref()
        .is_some_and(|(id, _)| id == &subscription_id)
        && let Some((_, token)) = current.take()
    {
        token.cancel();
    }
}

#[tauri::command]
pub async fn search_history(
    app: AppHandle,
    query: String,
    source: String,
    offset: usize,
) -> Result<Vec<yurelay_core::storage::HistorySummary>, String> {
    let engine = state::engine(&app).await?;
    engine.cleanup().await?;
    engine.storage.search_history(query, source, offset).await
}
#[tauri::command]
pub async fn history_detail(app: AppHandle, id: String) -> Result<Task, String> {
    let engine = state::engine(&app).await?;
    if let Some(task) = engine
        .book
        .get(&id)
        .filter(|task| !task.expired(engine.now()))
    {
        return Ok(task);
    }
    engine
        .storage
        .task(id)
        .await?
        .ok_or("记录已删除或到期。".into())
}

#[tauri::command]
pub async fn show_task(app: AppHandle, id: String) -> Result<(), String> {
    select_task(app.clone(), id).await?;
    state::show(&app, true);
    Ok(())
}

#[tauri::command]
pub async fn begin_shortcut_recording(
    app: AppHandle,
    window: tauri::WebviewWindow,
) -> Result<u64, String> {
    if window.label() != "settings" {
        return Err("请在设置窗口录入快捷键。".into());
    }
    let state = app.state::<AppState>();
    let epoch = state.recording_epoch.fetch_add(1, Ordering::SeqCst) + 1;
    state.shortcut_recording.store(true, Ordering::SeqCst);
    state.pressed_shortcuts.lock().unwrap().clear();
    if app.global_shortcut().unregister_all().is_err() {
        state.shortcut_recording.store(false, Ordering::SeqCst);
        return Err("无法暂时释放快捷键，请重试。".into());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        end_recording(&handle, Some(epoch)).await;
    });
    Ok(epoch)
}

pub async fn end_recording(app: &AppHandle, epoch: Option<u64>) -> ShortcutReport {
    let s = app.state::<AppState>();
    if epoch.is_some_and(|id| id != s.recording_epoch.load(Ordering::SeqCst))
        || !s.shortcut_recording.swap(false, Ordering::SeqCst)
    {
        return s.shortcut_report.lock().unwrap().clone();
    }
    let old_epoch = s.recording_epoch.fetch_add(1, Ordering::SeqCst);
    let report = match state::engine(app).await {
        Ok(engine) => {
            let settings = engine.configuration().await.settings;
            state::register_shortcuts(
                app,
                &settings.selection_shortcut,
                &settings.screenshot_shortcut,
                settings.append_selection_shortcut.as_deref(),
                settings.append_screenshot_shortcut.as_deref(),
            )
        }
        Err(error) => ShortcutReport {
            registered: false,
            error: Some(error),
        },
    };
    use tauri::Emitter;
    let _ = app.emit_to("settings", "shortcut-recording-ended", old_epoch);
    report
}
#[tauri::command]
pub async fn end_shortcut_recording(app: AppHandle, epoch: u64) -> ShortcutReport {
    end_recording(&app, Some(epoch)).await
}

#[tauri::command]
pub async fn save_preferences(
    app: AppHandle,
    preferences: yurelay_core::config::DisplayPreferences,
) -> Result<Configuration, String> {
    let configuration = state::engine(&app)
        .await?
        .save_preferences(preferences)
        .await?;
    use tauri::Emitter;
    let _ = app.emit("configuration-changed", &configuration.settings);
    state::localize_menus(&app, &configuration.settings);
    Ok(configuration)
}
#[tauri::command]
pub async fn configuration_settings(app: AppHandle) -> Result<Settings, String> {
    Ok(state::engine(&app).await?.configuration().await.settings)
}
#[tauri::command]
pub fn drag_window(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|_| "无法移动窗口。".into())
}

#[tauri::command]
pub async fn detect_text_language(text: String) -> Result<Option<String>, String> {
    if text.chars().count() > yurelay_core::MAX_INPUT_CHARACTERS {
        return Err("原文为空或过长。".into());
    }
    tokio::task::spawn_blocking(move || yurelay_platform::detect_language(&text))
        .await
        .map_err(|_| "语言识别失败。".into())
}

#[tauri::command]
pub fn open_dictionary(kind: String, text: String) -> Result<(), String> {
    yurelay_platform::open_dictionary(&kind, &text)
}

#[tauri::command]
pub fn default_translation_prompt() -> &'static str {
    yurelay_core::provider::DEFAULT_PROMPT
}
#[tauri::command]
pub async fn resolve_sensitive(app: AppHandle, id: String, allow: bool) -> Result<(), String> {
    let engine = state::engine(&app).await?;
    engine.resolve_sensitive(&id, allow).await?;
    if !allow {
        *app.state::<AppState>().selected.lock().unwrap() = None;
        app.state::<AppState>().resync.notify_one();
    }
    Ok(())
}
#[tauri::command]
pub async fn vocabulary_list(
    app: AppHandle,
) -> Result<Vec<yurelay_core::vocabulary::VocabularyItem>, String> {
    state::engine(&app).await?.storage.vocabulary().await
}
#[tauri::command]
pub async fn vocabulary_lookup(
    app: AppHandle,
    words: Vec<String>,
) -> Result<Vec<yurelay_core::vocabulary::VocabularyItem>, String> {
    state::engine(&app)
        .await?
        .storage
        .vocabulary_lookup(words)
        .await
}
#[tauri::command]
pub async fn vocabulary_collect(
    app: AppHandle,
    mut candidate: yurelay_core::vocabulary::Candidate,
    source_text: String,
) -> Result<yurelay_core::vocabulary::VocabularyItem, String> {
    candidate.validate_manual_source(&source_text)?;
    let item = state::engine(&app)
        .await?
        .storage
        .collect_word(candidate)
        .await?;
    use tauri::Emitter;
    let _ = app.emit("vocabulary-changed", ());
    Ok(item)
}
#[tauri::command]
pub async fn vocabulary_meaning(
    app: AppHandle,
    candidate: yurelay_core::vocabulary::Candidate,
    source_text: String,
    language: String,
    profile_id: Option<String>,
) -> Result<yurelay_core::vocabulary::Candidate, String> {
    let result = state::engine(&app)
        .await?
        .vocabulary_meaning(candidate, source_text, language, profile_id)
        .await?;
    use tauri::Emitter;
    let _ = app.emit("vocabulary-changed", ());
    Ok(result)
}
#[tauri::command]
pub async fn vocabulary_enrich_existing(
    app: AppHandle,
    mut candidate: yurelay_core::vocabulary::Candidate,
    source_text: String,
) -> Result<(), String> {
    candidate.validate_manual_source(&source_text)?;
    state::engine(&app)
        .await?
        .storage
        .enrich_collected_word(candidate)
        .await?;
    use tauri::Emitter;
    let _ = app.emit("vocabulary-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn vocabulary_preview(
    app: AppHandle,
    source_text: String,
    selection_start: usize,
    selection_end: usize,
    task_id: Option<String>,
    attempt: Option<u32>,
    source_language: Option<String>,
) -> Result<yurelay_core::vocabulary::selection::Preview, String> {
    use yurelay_core::vocabulary::selection::{DictionaryEntry, Selection};
    let selection = Selection::new(&source_text, selection_start, selection_end)?;
    let detected = detect_text_language(source_text.clone()).await?;
    yurelay_core::vocabulary::require_preview_source(
        source_language.as_deref(),
        detected.as_deref(),
    )?;
    let word = selection.surface.clone();
    let entry = tokio::task::spawn_blocking(move || {
        let lemma = yurelay_platform::lemma(&word);
        let details = yurelay_platform::lookup_dictionary_details(&lemma).unwrap_or(
            yurelay_platform::DictionaryDetails {
                definition: String::new(),
                phonetic: None,
            },
        );
        DictionaryEntry {
            lemma,
            definition: details.definition,
            phonetic: details.phonetic.unwrap_or_default(),
            pos: String::new(),
        }
    })
    .await
    .map_err(|_| "本地词形解析失败。")?;
    let engine = state::engine(&app).await?;
    let task = task_id.as_deref().and_then(|id| engine.book.get(id));
    let mut preview = selection.preview(entry, task.as_ref().zip(attempt))?;
    preview.candidate = engine.vocabulary_cached_meanings(preview.candidate).await?;
    Ok(preview)
}

#[tauri::command]
pub async fn vocabulary_manual(
    app: AppHandle,
    source_text: String,
    selection_start: usize,
    selection_end: usize,
    task_id: Option<String>,
    attempt: Option<u32>,
    source_language: Option<String>,
) -> Result<yurelay_core::vocabulary::VocabularyItem, String> {
    let preview = vocabulary_preview(
        app.clone(),
        source_text.clone(),
        selection_start,
        selection_end,
        task_id,
        attempt,
        source_language.clone(),
    )
    .await?;
    vocabulary_collect(app, preview.candidate, source_text).await
}

#[tauri::command]
pub async fn vocabulary_update(
    app: AppHandle,
    id: String,
    note: Option<String>,
    status: Option<String>,
    remove_context: Option<String>,
) -> Result<yurelay_core::vocabulary::VocabularyItem, String> {
    let item = state::engine(&app)
        .await?
        .storage
        .update_word(id, note, status, remove_context)
        .await?;
    use tauri::Emitter;
    let _ = app.emit("vocabulary-changed", ());
    Ok(item)
}
#[tauri::command]
pub async fn vocabulary_remove(app: AppHandle, id: String) -> Result<(), String> {
    state::engine(&app).await?.storage.remove_word(id).await?;
    use tauri::Emitter;
    let _ = app.emit("vocabulary-changed", ());
    Ok(())
}
#[tauri::command]
pub async fn vocabulary_export(
    app: AppHandle,
    format: String,
    language: Option<String>,
) -> Result<bool, String> {
    let items = state::engine(&app).await?.storage.vocabulary().await?;
    let language = language.unwrap_or(
        state::engine(&app)
            .await?
            .configuration()
            .await
            .settings
            .target_language,
    );
    let data = yurelay_core::storage::vocabulary::export_in_language(&items, &format, &language)?;
    let extension = match format.as_str() {
        "csv" => "csv",
        "markdown" => "md",
        "anki" => "txt",
        _ => return Err("不支持的导出格式。".into()),
    };
    let path = yurelay_platform::choose_export(&format!("Yurelay-vocabulary.{extension}")).await?;
    if let Some(path) = path {
        tokio::fs::write(path, data)
            .await
            .map_err(|_| "无法保存导出文件。")?;
        Ok(true)
    } else {
        Ok(false)
    }
}
#[tauri::command]
pub async fn speech_voices() -> Result<Vec<yurelay_platform::SpeechVoice>, String> {
    tokio::task::spawn_blocking(yurelay_platform::speech_voices)
        .await
        .map_err(|_| "无法读取系统朗读声音。".into())
}

#[tauri::command]
pub async fn speak_text(
    request_id: String,
    text: String,
    language: Option<String>,
) -> Result<(), String> {
    if request_id.len() > 128
        || request_id.is_empty()
        || language.as_ref().is_some_and(|l| {
            l.len() > 32
                || !l
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
    {
        return Err("朗读内容无效。".into());
    }
    let text = yurelay_core::speech::prepare(&text)?;
    yurelay_platform::speech::SPEECH.reserve(&request_id)?;
    tokio::task::spawn_blocking(move || {
        let result = yurelay_platform::speech::SPEECH.start(&request_id, text, language.as_deref());
        if result.is_err() {
            yurelay_platform::speech::SPEECH.stop(Some(&request_id));
        }
        result
    })
    .await
    .map_err(|_| "系统朗读失败。")?
}

#[tauri::command]
pub fn speech_status(request_id: String) -> Result<bool, String> {
    yurelay_platform::speech::SPEECH.is_playing(&request_id)
}

#[tauri::command]
pub fn stop_speech(request_id: String) {
    yurelay_platform::speech::SPEECH.stop(Some(&request_id));
}

#[tauri::command]
pub async fn speak_word(
    word: String,
    locale: Option<String>,
    slow: Option<bool>,
) -> Result<(), String> {
    if yurelay_core::sensitive::detect(&word).is_some() {
        return Err("朗读内容无效。".into());
    }
    tokio::task::spawn_blocking(move || {
        yurelay_platform::speak_word(&word, locale.as_deref(), slow.unwrap_or(false))
    })
    .await
    .map_err(|_| "系统朗读失败。")?
}

#[tauri::command]
pub async fn open_translation_link(url: String) -> Result<(), String> {
    let target = checked_translation_link(&url)?;
    tokio::task::spawn_blocking(move || {
        std::process::Command::new("/usr/bin/open")
            .arg(target)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|_| "无法打开链接。".to_string())
            .and_then(|s| {
                if s.success() {
                    Ok(())
                } else {
                    Err("无法打开链接。".into())
                }
            })
    })
    .await
    .map_err(|_| "无法打开链接。".to_string())?
}
fn checked_translation_link(value: &str) -> Result<String, String> {
    let url = tauri::Url::parse(value).map_err(|_| "链接无效。".to_string())?;
    if value.len() > 8192
        || !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("仅支持不含登录凭据的 HTTP/HTTPS 链接。".into());
    }
    Ok(url.to_string())
}
#[cfg(test)]
mod translation_link_tests {
    use super::*;
    #[test]
    fn only_explicit_web_links_are_opened() {
        for url in [
            "javascript:alert(1)",
            "file:///tmp/a",
            "data:text/html,x",
            "https://user:secret@example.test/",
            "relative/page",
        ] {
            assert!(checked_translation_link(url).is_err());
        }
        assert_eq!(
            checked_translation_link("https://example.test/path?q=1").unwrap(),
            "https://example.test/path?q=1"
        );
    }
}

#[tauri::command]
pub fn set_vocabulary_sidebar(app: AppHandle, width: u32) -> Result<u32, String> {
    if !matches!(width, 0 | 40 | 220) {
        return Err("无效的生词栏尺寸。".into());
    }
    // Sidebars change the internal layout, never the user's native bounds.
    app.state::<AppState>()
        .vocabulary_sidebar_width
        .store(u64::from(width), Ordering::Relaxed);
    Ok(width)
}

#[tauri::command]
pub async fn dictionary_guidance(
    app: AppHandle,
    text: String,
) -> Result<yurelay_core::dictionary::Guidance, String> {
    if text.chars().count() > yurelay_core::MAX_INPUT_CHARACTERS {
        return Err("原文为空或过长。".into());
    }
    Ok(state::engine(&app).await?.dictionary_guidance(&text).await)
}

#[tauri::command]
pub async fn submit_operation(
    app: AppHandle,
    request: yurelay_core::operations::OperationRequest,
) -> Result<yurelay_core::operations::OperationRecord, String> {
    state::engine(&app).await?.submit_operation(request).await
}
#[tauri::command]
pub async fn operation_list(
    app: AppHandle,
    task_id: Option<String>,
) -> Result<Vec<yurelay_core::operations::OperationRecord>, String> {
    state::engine(&app).await?.operation_list(task_id).await
}
#[tauri::command]
pub async fn cancel_operation(app: AppHandle, id: String) -> Result<(), String> {
    state::engine(&app).await?.cancel_operation(&id).await
}
#[tauri::command]
pub async fn resolve_operation_sensitive(
    app: AppHandle,
    id: String,
    allow: bool,
) -> Result<yurelay_core::operations::OperationRecord, String> {
    state::engine(&app)
        .await?
        .resolve_operation_sensitive(&id, allow)
        .await
}
