use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut};
use tokio::sync::{Notify, OnceCell};
use tokio_util::sync::CancellationToken;
use yurelay_core::{
    engine::{Credentials, Engine},
    storage,
    tasks::Status,
};

pub struct PlatformDictionary;
impl yurelay_core::engine::LocalDictionary for PlatformDictionary {
    fn lookup(&self, text: &str) -> Result<String, String> {
        yurelay_platform::lookup_dictionary(text)
    }
}
pub struct PlatformCredentials;
impl Credentials for PlatformCredentials {
    fn exists(&self, id: &str) -> Result<bool, String> {
        yurelay_platform::credential_exists(id)
    }
    fn read(&self, id: &str) -> Result<Option<String>, String> {
        yurelay_platform::read_credential(id, false)
    }
    fn read_for_update(&self, id: &str) -> Result<Option<String>, String> {
        yurelay_platform::read_credential(id, true)
    }
    fn write(&self, id: &str, value: Option<&str>) -> Result<(), String> {
        yurelay_platform::write_credential(id, value)
    }
}

#[derive(Clone, Default, Serialize)]
pub struct ShortcutReport {
    pub registered: bool,
    pub error: Option<String>,
}

pub struct AppState {
    pub root: PathBuf,
    pub settings_route: Mutex<Option<crate::experience::SettingsRoute>>,
    pub sizing: Mutex<crate::window_sizing::WindowSizing>,
    pub locale: Mutex<String>,
    pub engine: OnceCell<Arc<Engine>>,
    pub bindings: Mutex<Option<ShortcutBindings>>,
    pub shortcut_recording: AtomicBool,
    pub captures: crate::capture::CaptureGate,
    pub pressed_shortcuts: Mutex<HashMap<u32, crate::capture::ShortcutPress>>,
    pub settings_guard: crate::settings_guard::SettingsGuard,
    pub recording_epoch: AtomicU64,
    pub shortcut_report: Mutex<ShortcutReport>,
    pub window_lifecycle: Arc<Mutex<crate::window_lifecycle::WindowLifecycle>>,
    pub selected: Mutex<Option<String>>,
    pub notice: Mutex<Option<String>>,
    pub subscriber: Mutex<Option<(String, CancellationToken)>>,
    pub resync: Arc<Notify>,
    pub unread: Mutex<HashSet<String>>,
    pub exit_requested: AtomicBool,
    pub exit_ready: AtomicBool,
    pub temporary_top: AtomicBool,
    pub last_position: Mutex<Option<[i32; 2]>>,
    pub position_ready: AtomicBool,
    pub position_epoch: AtomicU64,
    pub size_epoch: AtomicU64,
    pub vocabulary_sidebar_width: AtomicU64,
}

impl AppState {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            settings_route: Mutex::new(None),
            sizing: Mutex::new(Default::default()),
            locale: Mutex::new("zh".into()),
            engine: OnceCell::new(),
            bindings: Mutex::new(None),
            shortcut_recording: AtomicBool::new(false),
            captures: crate::capture::CaptureGate::default(),
            pressed_shortcuts: Mutex::new(HashMap::new()),
            settings_guard: crate::settings_guard::SettingsGuard::default(),
            recording_epoch: AtomicU64::new(0),
            shortcut_report: Mutex::new(ShortcutReport::default()),
            window_lifecycle: Arc::new(Mutex::new(
                crate::window_lifecycle::WindowLifecycle::default(),
            )),
            selected: Mutex::new(None),
            notice: Mutex::new(None),
            subscriber: Mutex::new(None),
            resync: Arc::new(Notify::new()),
            unread: Mutex::new(HashSet::new()),
            exit_requested: AtomicBool::new(false),
            exit_ready: AtomicBool::new(false),
            temporary_top: AtomicBool::new(false),
            last_position: Mutex::new(None),
            position_ready: AtomicBool::new(false),
            position_epoch: AtomicU64::new(0),
            size_epoch: AtomicU64::new(0),
            vocabulary_sidebar_width: AtomicU64::new(0),
        }
    }
}

pub async fn engine(app: &AppHandle) -> Result<Arc<Engine>, String> {
    let state = app.state::<AppState>();
    state
        .engine
        .get_or_try_init(|| async {
            let engine = Engine::open_with_dictionary(
                state.root.clone(),
                Arc::new(PlatformCredentials),
                storage::system_clock(),
                Some(Arc::new(PlatformDictionary)),
            )
            .await?;
            let operation_app = app.clone();
            let operation_engine = engine.clone();
            let mut operation_events = engine.operations.subscribe();
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                loop {
                    match operation_events.recv().await {
                        Ok(record) => {
                            let _ = operation_app.emit("operation-changed", record);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            for record in operation_engine
                                .operations
                                .records(None, operation_engine.now())
                            {
                                let _ = operation_app.emit("operation-changed", record);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
            let observer_app = app.clone();
            let observing = engine.clone();
            tauri::async_runtime::spawn(async move {
                let (_, mut events) = observing.book.subscribe();
                let mut session = HashSet::new();
                let mut finished = HashSet::new();
                loop {
                    match events.recv().await {
                        Ok(yurelay_core::tasks::Event::Changed { task }) => {
                            if !task.status.is_terminal() {
                                session.insert(task.id.clone());
                            }
                            if session.contains(&task.id)
                                && matches!(
                                    task.status,
                                    Status::Completed
                                        | Status::OriginalOnly
                                        | Status::Ready
                                        | Status::Failed
                                )
                                && finished.insert((task.id.clone(), task.attempt))
                            {
                                let state = observer_app.state::<AppState>();
                                let reading = state.window_lifecycle.lock().unwrap().reading();
                                let reading = reading
                                    && state.selected.lock().unwrap().as_deref()
                                        == Some(task.id.as_str());
                                if !reading {
                                    state.unread.lock().unwrap().insert(task.id.clone());
                                    update_tray(&observer_app);
                                }
                            }
                        }
                        Ok(yurelay_core::tasks::Event::Removed { id, .. }) => {
                            session.remove(&id);
                            finished.retain(|(key, _)| key != &id);
                            observer_app
                                .state::<AppState>()
                                .unread
                                .lock()
                                .unwrap()
                                .remove(&id);
                            update_tray(&observer_app);
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(_) => break,
                    }
                }
            });
            Ok::<_, String>(engine)
        })
        .await
        .cloned()
}

#[derive(Debug, Clone, Copy)]
pub struct ShortcutBindings {
    pub selection: Shortcut,
    pub screenshot: Shortcut,
    pub append_selection: Option<Shortcut>,
    pub append_screenshot: Option<Shortcut>,
}
impl ShortcutBindings {
    pub fn keys(self) -> Vec<Shortcut> {
        [
            Some(self.selection),
            Some(self.screenshot),
            self.append_selection,
            self.append_screenshot,
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    pub fn action(self, key: Shortcut) -> Option<(bool, bool)> {
        if key == self.selection {
            Some((false, false))
        } else if key == self.screenshot {
            Some((true, false))
        } else if Some(key) == self.append_selection {
            Some((false, true))
        } else if Some(key) == self.append_screenshot {
            Some((true, true))
        } else {
            None
        }
    }
}
pub fn parse_shortcuts(
    selection: &str,
    screenshot: &str,
    append_selection: Option<&str>,
    append_screenshot: Option<&str>,
) -> Result<ShortcutBindings, String> {
    let parse = |value: &str| {
        value
            .parse::<Shortcut>()
            .map_err(|_| "快捷键格式无效，例如 Alt+D。".to_string())
    };
    let bindings = ShortcutBindings {
        selection: parse(selection)?,
        screenshot: parse(screenshot)?,
        append_selection: append_selection.map(parse).transpose()?,
        append_screenshot: append_screenshot.map(parse).transpose()?,
    };
    let keys = bindings.keys();
    for (index, shortcut) in keys.iter().enumerate() {
        if keys[..index].contains(shortcut) {
            return Err("捕获与追加快捷键不能重复。".into());
        }
        if !shortcut
            .mods
            .intersects(Modifiers::ALT | Modifiers::CONTROL | Modifiers::SUPER)
        {
            return Err("快捷键至少需要 Option/Alt、Control 或 Command 修饰键。".into());
        }
    }
    Ok(bindings)
}
pub fn register_shortcuts(
    app: &AppHandle,
    selection: &str,
    screenshot: &str,
    append_selection: Option<&str>,
    append_screenshot: Option<&str>,
) -> ShortcutReport {
    if app
        .state::<AppState>()
        .shortcut_recording
        .load(Ordering::SeqCst)
    {
        return ShortcutReport {
            registered: false,
            error: Some("请先结束快捷键录入。".into()),
        };
    }
    let result = (|| {
        let bindings = parse_shortcuts(selection, screenshot, append_selection, append_screenshot)?;
        let previous = *app.state::<AppState>().bindings.lock().unwrap();
        let old: Vec<_> = previous
            .into_iter()
            .flat_map(ShortcutBindings::keys)
            .filter(|key| app.global_shortcut().is_registered(*key))
            .collect();
        crate::shortcuts::replace(
            &old,
            &bindings.keys(),
            |key| {
                app.global_shortcut()
                    .register(key)
                    .map_err(|_| "快捷键无法注册，可能已被其他应用占用；原有键位保持生效。".into())
            },
            |key| {
                app.global_shortcut()
                    .unregister(key)
                    .map_err(|_| "无法更新快捷键，请重启应用。".into())
            },
        )?;
        *app.state::<AppState>().bindings.lock().unwrap() = Some(bindings);
        Ok::<_, String>(())
    })();
    let report = match result {
        Ok(()) => ShortcutReport {
            registered: true,
            error: None,
        },
        Err(error) => ShortcutReport {
            registered: false,
            error: Some(error),
        },
    };
    *app.state::<AppState>().shortcut_report.lock().unwrap() = report.clone();
    use tauri::Emitter;
    let _ = app.emit_to("settings", "shortcut-status-changed", &report);
    report
}

pub fn current_notice(app: &AppHandle) -> Option<String> {
    let permissions = yurelay_platform::permissions();
    let state = app.state::<AppState>();
    let mut notice = state.notice.lock().unwrap();
    if notice.as_deref().is_some_and(|message| {
        !crate::shortcuts::notice_is_relevant(
            message,
            permissions.accessibility,
            permissions.screen,
        )
    }) {
        *notice = None;
    }
    notice.clone()
}

pub fn clear_notice(app: &AppHandle) {
    *app.state::<AppState>().notice.lock().unwrap() = None;
    app.state::<AppState>().resync.notify_one();
}

pub fn clear_capture_view(app: &AppHandle) {
    *app.state::<AppState>().selected.lock().unwrap() = None;
    clear_notice(app);
}

pub fn request_settings_close(app: &AppHandle, intent: crate::settings_guard::Intent) -> bool {
    use tauri::Emitter;
    if let Some(request) = app.state::<AppState>().settings_guard.request(intent) {
        let _ = app.emit_to("settings", "settings-close-requested", request);
        true
    } else {
        false
    }
}

pub fn show(app: &AppHandle, focus: bool) {
    let state = app.state::<AppState>();
    state.window_lifecycle.lock().unwrap().show();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        if focus {
            let _ = window.set_focus();
        }
    }
    state.resync.notify_one();
}

pub fn begin_capture(app: &AppHandle) -> crate::window_lifecycle::CaptureEpoch {
    app.state::<AppState>()
        .window_lifecycle
        .lock()
        .unwrap()
        .capture_started()
}

pub fn hide(app: &AppHandle) {
    app.state::<AppState>()
        .window_lifecycle
        .lock()
        .unwrap()
        .hide();
    dismiss_result_window(app, None);
}

pub fn hide_after_blur(app: &AppHandle, ticket: crate::window_lifecycle::BlurTicket) {
    dismiss_result_window(app, Some(ticket));
}

fn dismiss_result_window(app: &AppHandle, blur: Option<crate::window_lifecycle::BlurTicket>) {
    if let Some(window) = app.get_webview_window("main") {
        let app = app.clone();
        let handle = window.clone();
        let _ = window.run_on_main_thread(move || {
            // A screenshot overlay or Settings can take key-window focus while
            // this app remains active. That is not switching to another app.
            // Query before taking the lifecycle lock: AppKit may reenter us.
            #[cfg(target_os = "macos")]
            let external_focus = blur.is_some()
                && yurelay_platform::selection_target()
                    .is_some_and(|pid| pid != std::process::id() as i32);
            #[cfg(not(target_os = "macos"))]
            let external_focus = true;
            let should_hide = {
                let state = app.state::<AppState>();
                let mut lifecycle = state.window_lifecycle.lock().unwrap();
                match blur {
                    // Check after the UI queue delay, not before dispatching it.
                    Some(ticket) => lifecycle.auto_hide(ticket, external_focus),
                    None => !lifecycle.visible(),
                }
            };
            if !should_hide {
                return;
            }
            yurelay_platform::speech::SPEECH.stop(None);
            release_temporary_top(&app);
            #[cfg(target_os = "macos")]
            if let Ok(pointer) = handle.ns_window() {
                // SAFETY: this closure runs on the main thread with a live window.
                unsafe {
                    yurelay_platform::dismiss_window(pointer);
                }
                return;
            }
            let _ = handle.hide();
        });
    }
}

pub fn select(app: &AppHandle, id: String) {
    *app.state::<AppState>().selected.lock().unwrap() = Some(id.clone());
    app.state::<AppState>().unread.lock().unwrap().remove(&id);
    update_tray(app);
    app.state::<AppState>().resync.notify_one();
}
pub fn notice(app: &AppHandle, message: String) {
    *app.state::<AppState>().notice.lock().unwrap() = Some(message);
    app.state::<AppState>().resync.notify_one();
}
pub fn update_tray(app: &AppHandle) {
    let count = app.state::<AppState>().unread.lock().unwrap().len();
    let en = app.state::<AppState>().locale.lock().unwrap().as_str() == "en";
    crate::tray::with_tray(app, move |tray| {
        let _ = tray.set_title(if count > 0 { Some("•") } else { None });
        let _ = tray.set_tooltip(Some(if count > 0 {
            if en {
                format!("Yurelay · {count} unread results")
            } else {
                format!("Yurelay · {count} 条未查看结果")
            }
        } else if en {
            "Yurelay · Latest result".into()
        } else {
            "Yurelay · 翻译窗口".into()
        }));
    });
}

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn localize_menus(app: &AppHandle, settings: &yurelay_core::config::Settings) {
    use tauri::menu::{Menu, MenuItem, MenuItemKind};
    let en = settings.ui_locale() == "en";
    yurelay_platform::set_locale(en);
    *app.state::<AppState>().locale.lock().unwrap() = settings.ui_locale().into();
    let labels = if en {
        [
            "Open translation window",
            "Screenshot translation",
            "Settings…",
            "Quit Yurelay",
        ]
    } else {
        ["打开翻译窗口", "截屏翻译", "设置…", "退出 Yurelay"]
    };
    let items: Result<Vec<_>, _> = ["show", "capture", "settings", "quit"]
        .into_iter()
        .zip(labels)
        .map(|(id, text)| MenuItem::with_id(app, id, text, true, None::<&str>))
        .collect();
    if let Ok(items) = items {
        let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
            items.iter().map(|item| item as _).collect();
        if let Ok(menu) = Menu::with_items(app, &refs) {
            crate::tray::with_tray(app, move |tray| {
                let _ = tray.set_menu(Some(menu));
            });
        }
    }
    if let Some(menu) = app.menu()
        && let Ok(items) = menu.items()
    {
        for item in items {
            if let MenuItemKind::Submenu(submenu) = item
                && let Ok(children) = submenu.items()
            {
                for child in children {
                    if let MenuItemKind::MenuItem(item) = child
                        && item.id().as_ref() == "app-settings"
                    {
                        let _ = item.set_text(labels[2]);
                    }
                }
            }
        }
    }
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_title(if en {
            "Yurelay Settings"
        } else {
            "Yurelay 设置"
        });
    }
    update_tray(app);
}

pub struct WindowSnapshot {
    pub selected: Option<String>,
    pub pinned: bool,
    pub expanded: bool,
}

pub fn window_snapshot(state: &AppState, query_expanded: impl FnOnce() -> bool) -> WindowSnapshot {
    // Drop Rust mutex guards before any synchronous dispatch to the UI thread.
    let selected = state.selected.lock().unwrap().clone();
    let pinned = state.window_lifecycle.lock().unwrap().pinned();
    let expanded = query_expanded();
    WindowSnapshot {
        selected,
        pinned,
        expanded,
    }
}

pub fn initialize_result_position(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let Ok(monitors) = window.available_monitors() else {
        return;
    };
    let saved = std::fs::read(app.state::<AppState>().root.join("window-position.json"))
        .ok()
        .and_then(|data| serde_json::from_slice::<[i32; 2]>(&data).ok());
    let cursor = window.cursor_position().ok();
    let monitor = saved
        .and_then(|position| {
            monitors.iter().find(|m| {
                let a = m.work_area();
                position[0] >= a.position.x
                    && position[0] < a.position.x + a.size.width as i32
                    && position[1] >= a.position.y
                    && position[1] < a.position.y + a.size.height as i32
            })
        })
        .or_else(|| {
            cursor.and_then(|p| {
                monitors.iter().find(|m| {
                    let a = m.work_area();
                    p.x >= a.position.x as f64
                        && p.x < (a.position.x + a.size.width as i32) as f64
                        && p.y >= a.position.y as f64
                        && p.y < (a.position.y + a.size.height as i32) as f64
                })
            })
        })
        .or_else(|| monitors.first());
    if let Some(monitor) = monitor {
        let area = monitor.work_area();
        let scale = monitor.scale_factor();
        let initial = crate::window_sizing::initial_size(
            &app.state::<AppState>().root.join("window-size.json"),
            [
                f64::from(area.size.width) / scale,
                f64::from(area.size.height) / scale,
            ],
        );
        let _ = window.set_size(tauri::LogicalSize::new(initial[0], initial[1]));
        let margin = (16.0 * scale).round() as i32;
        let size = window
            .outer_size()
            .ok()
            .map(|s| [s.width as i32, s.height as i32])
            .unwrap_or([(520.0 * scale) as i32, (560.0 * scale) as i32]);
        let area = [
            area.position.x,
            area.position.y,
            area.size.width as i32,
            area.size.height as i32,
        ];
        let point = crate::placement::clamp_position(
            saved.unwrap_or([area[0] + area[2] - size[0] - margin, area[1] + margin]),
            area,
            size,
            margin,
        );
        let _ = window.set_position(tauri::PhysicalPosition::new(point[0], point[1]));
        *app.state::<AppState>().last_position.lock().unwrap() = Some(point);
    }
    app.state::<AppState>()
        .position_ready
        .store(true, Ordering::Relaxed);
}

pub fn remember_result_position(app: &AppHandle, position: [i32; 2]) {
    let state = app.state::<AppState>();
    if !state.position_ready.load(Ordering::Relaxed) {
        return;
    }
    if let Some(window) = app.get_webview_window("main")
        && (window.is_maximized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false))
    {
        return;
    }
    *state.last_position.lock().unwrap() = Some(position);
    let epoch = state.position_epoch.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let state = app.state::<AppState>();
        if state.position_epoch.load(Ordering::Relaxed) == epoch {
            let path = state.root.join("window-position.json");
            let _ = tokio::fs::write(path, serde_json::to_vec(&position).unwrap_or_default()).await;
        }
    });
}

pub fn present_capture(app: &AppHandle, epoch: crate::window_lifecycle::CaptureEpoch) {
    let state = app.state::<AppState>();
    if !state
        .window_lifecycle
        .lock()
        .unwrap()
        .present_capture(epoch)
    {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        let app = app.clone();
        let handle = window.clone();
        let _ = window.run_on_main_thread(move || {
            let state = app.state::<AppState>();
            // An explicit hide after dispatch must also cancel this queued show.
            if !state
                .window_lifecycle
                .lock()
                .unwrap()
                .capture_is_current(epoch)
            {
                return;
            }
            state.temporary_top.store(true, Ordering::Relaxed);
            let _ = apply_top_level(&handle, true);
            let _ = handle.unminimize();
            let _ = handle.show();
            #[cfg(target_os = "macos")]
            if let Ok(pointer) = handle.ns_window() {
                // SAFETY: this closure executes on the main thread, window is live.
                unsafe {
                    yurelay_platform::present_window(pointer);
                }
            }
            let _ = handle.set_focus();
        });
    }
    state.resync.notify_one();
}

// All callers run on the UI thread, with no lifecycle lock held across AppKit.
fn apply_top_level(window: &tauri::WebviewWindow, pinned: bool) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        let pointer = window
            .ns_window()
            .map_err(|_| "无法改变窗口置顶状态。".to_string())?;
        // SAFETY: caller is on the main thread and owns the live window handle.
        Ok(unsafe { yurelay_platform::set_window_pinned(pointer, pinned) })
    }
    #[cfg(not(target_os = "macos"))]
    {
        window
            .set_always_on_top(pinned)
            .map_err(|_| "无法改变窗口置顶状态。".to_string())?;
        window
            .is_always_on_top()
            .map_err(|_| "无法改变窗口置顶状态。".to_string())
    }
}

pub async fn set_pinned(app: &AppHandle, pinned: bool) -> Result<bool, String> {
    let window = app.get_webview_window("main").ok_or("窗口不可用。")?;
    let handle = window.clone();
    let app = app.clone();
    let (send, receive) = tokio::sync::oneshot::channel();
    window
        .run_on_main_thread(move || {
            let result = apply_top_level(&handle, pinned);
            if let Ok(actual) = &result {
                app.state::<AppState>()
                    .window_lifecycle
                    .lock()
                    .unwrap()
                    .set_pinned(*actual);
            }
            let _ = send.send(result);
        })
        .map_err(|_| "无法改变窗口置顶状态。".to_string())?;
    receive
        .await
        .map_err(|_| "无法改变窗口置顶状态。".to_string())?
}

pub fn release_temporary_top(app: &AppHandle) {
    let state = app.state::<AppState>();
    let pinned = state.window_lifecycle.lock().unwrap().pinned();
    if state.temporary_top.swap(false, Ordering::Relaxed)
        && !pinned
        && let Some(window) = app.get_webview_window("main")
    {
        let _ = apply_top_level(&window, false);
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;
    #[test]
    fn window_snapshot_releases_state_before_ui_query() {
        let state = AppState::new(PathBuf::new());
        let snapshot = window_snapshot(&state, || {
            // A synchronous UI query can run a focus callback which needs this
            // exact lock. Holding it until the return struct is complete hangs.
            assert!(
                state.selected.try_lock().is_ok(),
                "UI query called while selection mutex is held"
            );
            true
        });
        assert!(snapshot.expanded);
    }
}

#[cfg(test)]
mod append_shortcut_tests {
    use super::*;
    #[test]
    fn optional_keys_are_disabled_by_default_and_all_four_are_distinct() {
        let defaults = parse_shortcuts("Alt+D", "Alt+S", None, None).unwrap();
        assert_eq!(defaults.keys().len(), 2);
        let all =
            parse_shortcuts("Alt+D", "Alt+S", Some("Shift+Alt+D"), Some("Shift+Alt+S")).unwrap();
        assert_eq!(all.keys().len(), 4);
        assert_eq!(
            all.action(all.append_screenshot.unwrap()),
            Some((true, true))
        );
        assert!(parse_shortcuts("Alt+D", "Alt+S", Some("Alt+D"), None).is_err());
        assert!(parse_shortcuts("Alt+D", "Alt+S", Some("Shift+A"), None).is_err());
    }
}

/// Debounce native user drags; content and mode changes never save a new size.
pub fn remember_result_size(app: &AppHandle) {
    let epoch = app
        .state::<AppState>()
        .size_epoch
        .fetch_add(1, Ordering::Relaxed)
        + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let state = app.state::<AppState>();
        if state.size_epoch.load(Ordering::Relaxed) != epoch
            || !state.sizing.lock().unwrap().settled()
        {
            return;
        }
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        if window.is_maximized().unwrap_or(true) || window.is_fullscreen().unwrap_or(true) {
            return;
        }
        let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
            return;
        };
        let size = [
            f64::from(size.width) / scale,
            f64::from(size.height) / scale,
        ];
        let path = state.root.join("window-size.json");
        let temporary = state.root.join(format!("window-size-{epoch}.tmp"));
        if tokio::fs::write(&temporary, serde_json::to_vec(&size).unwrap_or_default())
            .await
            .is_ok()
        {
            if state.size_epoch.load(Ordering::Relaxed) == epoch {
                let _ = tokio::fs::rename(&temporary, path).await;
            } else {
                let _ = tokio::fs::remove_file(&temporary).await;
            }
        }
    });
}
