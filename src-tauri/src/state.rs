use glossa_core::{
    engine::{Credentials, Engine},
    storage,
    tasks::Status,
};
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

pub struct PlatformDictionary;
impl glossa_core::engine::LocalDictionary for PlatformDictionary {
    fn lookup(&self, text: &str) -> Result<String, String> {
        glossa_platform::lookup_dictionary(text)
    }
}
pub struct PlatformCredentials;
impl Credentials for PlatformCredentials {
    fn read(&self, id: &str) -> Result<Option<String>, String> {
        glossa_platform::read_credential(id)
    }
    fn write(&self, id: &str, value: Option<&str>) -> Result<(), String> {
        glossa_platform::write_credential(id, value)
    }
}

#[derive(Clone, Default, Serialize)]
pub struct ShortcutReport {
    pub registered: bool,
    pub error: Option<String>,
}

pub struct AppState {
    pub root: PathBuf,
    pub locale: Mutex<String>,
    pub engine: OnceCell<Arc<Engine>>,
    pub bindings: Mutex<Option<(Shortcut, Shortcut)>>,
    pub shortcut_recording: AtomicBool,
    pub pressed_shortcuts: Mutex<HashMap<u32, Option<i32>>>,
    pub settings_guard: crate::settings_guard::SettingsGuard,
    pub recording_epoch: AtomicU64,
    pub shortcut_report: Mutex<ShortcutReport>,
    pub visible: AtomicBool,
    pub focused: AtomicBool,
    pub hidden_epoch: AtomicU64,
    pub selected: Mutex<Option<String>>,
    pub notice: Mutex<Option<String>>,
    pub subscriber: Mutex<Option<(String, CancellationToken)>>,
    pub resync: Arc<Notify>,
    pub unread: Mutex<HashSet<String>>,
    pub exit_requested: AtomicBool,
    pub exit_ready: AtomicBool,
    pub pinned: AtomicBool,
    pub temporary_top: AtomicBool,
    pub last_position: Mutex<Option<[i32; 2]>>,
    pub position_ready: AtomicBool,
    pub position_epoch: AtomicU64,
    pub vocabulary_sidebar_width: AtomicU64,
}

impl AppState {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            locale: Mutex::new("zh".into()),
            engine: OnceCell::new(),
            bindings: Mutex::new(None),
            shortcut_recording: AtomicBool::new(false),
            pressed_shortcuts: Mutex::new(HashMap::new()),
            settings_guard: crate::settings_guard::SettingsGuard::default(),
            recording_epoch: AtomicU64::new(0),
            shortcut_report: Mutex::new(ShortcutReport::default()),
            visible: AtomicBool::new(true),
            focused: AtomicBool::new(true),
            hidden_epoch: AtomicU64::new(0),
            selected: Mutex::new(None),
            notice: Mutex::new(None),
            subscriber: Mutex::new(None),
            resync: Arc::new(Notify::new()),
            unread: Mutex::new(HashSet::new()),
            exit_requested: AtomicBool::new(false),
            exit_ready: AtomicBool::new(false),
            pinned: AtomicBool::new(false),
            temporary_top: AtomicBool::new(false),
            last_position: Mutex::new(None),
            position_ready: AtomicBool::new(false),
            position_epoch: AtomicU64::new(0),
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
            let observer_app = app.clone();
            let observing = engine.clone();
            tauri::async_runtime::spawn(async move {
                let (_, mut events) = observing.book.subscribe();
                let mut session = HashSet::new();
                let mut finished = HashSet::new();
                loop {
                    match events.recv().await {
                        Ok(glossa_core::tasks::Event::Changed { task }) => {
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
                                let reading = state.visible.load(Ordering::Relaxed)
                                    && state.focused.load(Ordering::Relaxed)
                                    && state.selected.lock().unwrap().as_deref()
                                        == Some(task.id.as_str());
                                if !reading {
                                    state.unread.lock().unwrap().insert(task.id.clone());
                                    update_tray(&observer_app);
                                }
                            }
                        }
                        Ok(glossa_core::tasks::Event::Removed { id, .. }) => {
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

pub fn parse_shortcuts(selection: &str, screenshot: &str) -> Result<(Shortcut, Shortcut), String> {
    let a: Shortcut = selection
        .parse()
        .map_err(|_| "划词快捷键格式无效，例如 Alt+D。")?;
    let b: Shortcut = screenshot
        .parse()
        .map_err(|_| "截图快捷键格式无效，例如 Alt+S。")?;
    if a == b {
        return Err("两个快捷键不能相同。".into());
    }
    for shortcut in [&a, &b] {
        if !shortcut
            .mods
            .intersects(Modifiers::ALT | Modifiers::CONTROL | Modifiers::SUPER)
        {
            return Err("快捷键至少需要 Option/Alt、Control 或 Command 修饰键。".into());
        }
    }
    Ok((a, b))
}

pub fn register_shortcuts(app: &AppHandle, selection: &str, screenshot: &str) -> ShortcutReport {
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
        let (a, b) = parse_shortcuts(selection, screenshot)?;
        let previous = *app.state::<AppState>().bindings.lock().unwrap();
        let old: Vec<_> = previous
            .into_iter()
            .flat_map(|(a, b)| [a, b])
            .filter(|key| app.global_shortcut().is_registered(*key))
            .collect();
        crate::shortcuts::replace(
            &old,
            &[a, b],
            |key| {
                app.global_shortcut().register(key).map_err(|_| {
                    format!(
                        "快捷键 {} 无法注册，可能已被其他应用占用。释放键位后可重新注册。",
                        if key == a { selection } else { screenshot }
                    )
                })
            },
            |key| {
                app.global_shortcut()
                    .unregister(key)
                    .map_err(|_| "无法更新快捷键，请重启应用。".into())
            },
        )?;
        *app.state::<AppState>().bindings.lock().unwrap() = Some((a, b));
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
    let permissions = glossa_platform::permissions();
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
    state.visible.store(true, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        if focus {
            let _ = window.set_focus();
        }
    }
    state.resync.notify_one();
}

pub fn hide(app: &AppHandle) {
    release_temporary_top(app);
    let state = app.state::<AppState>();
    state.visible.store(false, Ordering::Relaxed);
    state.hidden_epoch.fetch_add(1, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
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
    if let Some(tray) = app.tray_by_id("glossa") {
        let _ = tray.set_title(if count > 0 { Some("•") } else { None });
        let _ = tray.set_tooltip(Some(if count > 0 {
            if en {
                format!("Glossa · {count} unread results")
            } else {
                format!("Glossa · {count} 条未查看结果")
            }
        } else if en {
            "Glossa · Latest result".into()
        } else {
            "Glossa · 翻译窗口".into()
        }));
    }
}

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn localize_menus(app: &AppHandle, settings: &glossa_core::config::Settings) {
    use tauri::menu::{Menu, MenuItem, MenuItemKind};
    let en = settings.ui_locale() == "en";
    glossa_platform::set_locale(en);
    *app.state::<AppState>().locale.lock().unwrap() = settings.ui_locale().into();
    let labels = if en {
        [
            "Open translation window",
            "Screenshot translation",
            "Settings…",
            "Quit Glossa",
        ]
    } else {
        ["打开翻译窗口", "截屏翻译", "设置…", "退出 Glossa"]
    };
    let items: Result<Vec<_>, _> = ["show", "capture", "settings", "quit"]
        .into_iter()
        .zip(labels)
        .map(|(id, text)| MenuItem::with_id(app, id, text, true, None::<&str>))
        .collect();
    if let Ok(items) = items {
        let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
            items.iter().map(|item| item as _).collect();
        if let Ok(menu) = Menu::with_items(app, &refs)
            && let Some(tray) = app.tray_by_id("glossa")
        {
            let _ = tray.set_menu(Some(menu));
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
            "Glossa Settings"
        } else {
            "Glossa 设置"
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
    let pinned = state.pinned.load(Ordering::Relaxed);
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
        let margin = (16.0 * scale).round() as i32;
        let size = window
            .outer_size()
            .ok()
            .map(|s| [s.width as i32, s.height as i32])
            .unwrap_or([(460.0 * scale) as i32, (560.0 * scale) as i32]);
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

pub fn remember_result_position(app: &AppHandle, mut position: [i32; 2]) {
    let state = app.state::<AppState>();
    if !state.position_ready.load(Ordering::Relaxed) {
        return;
    }
    if let Some(window) = app.get_webview_window("main")
        && (window.is_maximized().unwrap_or(false) || window.is_fullscreen().unwrap_or(false))
    {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        let scale = window.scale_factor().unwrap_or(1.0);
        position[0] +=
            (state.vocabulary_sidebar_width.load(Ordering::Relaxed) as f64 * scale).round() as i32;
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

pub fn present_capture(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.temporary_top.store(true, Ordering::Relaxed);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_always_on_top(true);
        let _ = window.unminimize();
        let _ = window.show();
        let handle = window.clone();
        let _ = window.run_on_main_thread(move || {
            #[cfg(target_os = "macos")]
            if let Ok(pointer) = handle.ns_window() {
                // SAFETY: this closure executes on the main thread, window is live.
                unsafe {
                    glossa_platform::present_window(pointer);
                }
            }
            let _ = handle.set_focus();
        });
    }
    state.visible.store(true, Ordering::Relaxed);
    state.resync.notify_one();
}

pub fn release_temporary_top(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.temporary_top.swap(false, Ordering::Relaxed)
        && !state.pinned.load(Ordering::Relaxed)
        && let Some(window) = app.get_webview_window("main")
    {
        let _ = window.set_always_on_top(false);
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
