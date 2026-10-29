#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod commands;
mod experience;
mod placement;
mod reliability_commands;
mod settings_guard;
mod shortcuts;
mod source_commands;
mod state;
mod tray;
mod window_lifecycle;
mod window_sizing;

use state::AppState;
use std::sync::atomic::Ordering;
use tauri::{
    Manager,
    menu::{Menu, MenuItem, MenuItemKind},
    tray::TrayIconBuilder,
};
use tauri_plugin_global_shortcut::ShortcutState;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            state::show(app, true)
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    let state = app.state::<AppState>();
                    if state.shortcut_recording.load(Ordering::SeqCst) {
                        return;
                    }
                    if event.state() == ShortcutState::Pressed {
                        // Freeze the source at key-down. Key release and queued
                        // AppKit work must not choose a different foreground app.
                        // Keep busy presses ignored even if the first capture
                        // finishes before this key is released or auto-repeated.
                        let target = yurelay_platform::selection_target();
                        state
                            .pressed_shortcuts
                            .lock()
                            .unwrap()
                            .entry(shortcut.id())
                            .or_insert_with(|| state.captures.shortcut_press(target));
                        return;
                    }
                    let Some(capture::ShortcutPress::Capture(target)) = state
                        .pressed_shortcuts
                        .lock()
                        .unwrap()
                        .remove(&shortcut.id())
                    else {
                        return;
                    };
                    let bindings = *app.state::<AppState>().bindings.lock().unwrap();
                    if let Some((screenshot, append)) =
                        bindings.and_then(|keys| keys.action(*shortcut))
                    {
                        if append {
                            source_commands::start_capture_target(
                                app.clone(),
                                screenshot,
                                target,
                                source_commands::append_target(app),
                            );
                        } else {
                            commands::start_capture_target(app.clone(), screenshot, target);
                        }
                    }
                })
                .build(),
        )
        .register_asynchronous_uri_scheme_protocol(
            "yurelay-image",
            |context, request, responder| {
                let app = context.app_handle().clone();
                let allowed = matches!(context.webview_label(), "main" | "settings");
                let id = request.uri().path().trim_start_matches('/').to_string();
                tauri::async_runtime::spawn(async move {
                    let data = if allowed && uuid::Uuid::parse_str(&id).is_ok() {
                        match state::engine(&app).await {
                            Ok(engine) => engine.image(&id).await.ok().flatten(),
                            Err(_) => None,
                        }
                    } else {
                        None
                    };
                    let response = match data {
                        Some(data) => tauri::http::Response::builder()
                            .status(200)
                            .header("Content-Type", "image/png")
                            .header("Cache-Control", "no-store")
                            .header("X-Content-Type-Options", "nosniff")
                            .body(data.as_ref().clone()),
                        None => tauri::http::Response::builder()
                            .status(404)
                            .header("Cache-Control", "no-store")
                            .body(Vec::new()),
                    };
                    if let Ok(response) = response {
                        responder.respond(response);
                    }
                });
            },
        )
        .setup(|app| {
            let root = app.path().app_data_dir()?;
            app.manage(AppState::new(root));
            #[cfg(target_os = "macos")]
            if let Some(window) = app.get_webview_window("main") {
                // SAFETY: setup runs on the main thread and the NSWindow is live.
                unsafe {
                    yurelay_platform::configure_window(window.ns_window()?);
                }
            }
            state::initialize_result_position(app.handle());
            state::show(app.handle(), false);
            state::register_shortcuts(app.handle(), "Alt+D", "Alt+S", None, None);
            let open = MenuItem::with_id(app, "show", "打开翻译窗口", true, None::<&str>)?;
            let capture = MenuItem::with_id(app, "capture", "截屏翻译", true, None::<&str>)?;
            let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 Yurelay", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &capture, &settings, &quit])?;
            let app_menu = Menu::default(app.handle())?;
            if let Some(MenuItemKind::Submenu(submenu)) = app_menu.items()?.first() {
                submenu.insert(
                    &MenuItem::with_id(app, "app-settings", "设置…", true, Some("CmdOrCtrl+,"))?,
                    2,
                )?;
            }
            app.set_menu(app_menu)?;
            app.on_menu_event(|app, event| {
                if event.id.as_ref() == "app-settings" {
                    state::show_settings(app);
                }
            });
            let mut tray = TrayIconBuilder::with_id("yurelay")
                .tooltip("Yurelay · 翻译窗口")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => state::show(app, true),
                    "capture" => commands::start_capture(app.clone(), true),
                    "settings" => {
                        state::show_settings(app);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match state::engine(&handle).await {
                    Ok(engine) => {
                        let config = engine.configuration().await;
                        state::localize_menus(&handle, &config.settings);
                        state::register_shortcuts(
                            &handle,
                            &config.settings.selection_shortcut,
                            &config.settings.screenshot_shortcut,
                            config.settings.append_selection_shortcut.as_deref(),
                            config.settings.append_screenshot_shortcut.as_deref(),
                        );
                    }
                    Err(error) => state::notice(&handle, error),
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            let app = window.app_handle();
            if window.label() == "settings" {
                if matches!(event, tauri::WindowEvent::Focused(true)) {
                    use tauri::Emitter;
                    let _ = app.emit_to("settings", "settings-focused", ());
                }
                if matches!(
                    event,
                    tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Focused(false)
                ) {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        commands::end_recording(&app, None).await;
                    });
                }
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    if !state::request_settings_close(app, settings_guard::Intent::Close) {
                        let _ = window.hide();
                    }
                }
                return;
            }
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    state::hide(app);
                }
                tauri::WindowEvent::Moved(position) => {
                    state::remember_result_position(app, [position.x, position.y]);
                }
                tauri::WindowEvent::Resized(_) => {
                    use tauri::Emitter;
                    let expanded = window.is_maximized().unwrap_or(false)
                        || window.is_fullscreen().unwrap_or(false);
                    let mut live_resize = false;
                    #[cfg(target_os = "macos")]
                    if let Ok(ptr) = window.ns_window() {
                        // SAFETY: window events run on the main thread with a live NSWindow.
                        live_resize = unsafe { yurelay_platform::window_live_resize(ptr) };
                    }
                    if live_resize && !expanded {
                        state::remember_result_size(app);
                    }
                    let state = app.state::<AppState>();
                    let mut sizing = state.sizing.lock().unwrap();
                    if sizing.observe(expanded, live_resize) {
                        let _ = window.emit("window-mode-changed", *sizing);
                    }
                }
                tauri::WindowEvent::Focused(focused) => {
                    let blur = app
                        .state::<AppState>()
                        .window_lifecycle
                        .lock()
                        .unwrap()
                        .focus_changed(*focused);
                    if *focused {
                        state::current_notice(app);
                        app.state::<AppState>().resync.notify_one();
                        let selected = app.state::<AppState>().selected.lock().unwrap().clone();
                        if let Some(id) = selected {
                            app.state::<AppState>().unread.lock().unwrap().remove(&id);
                            state::update_tray(app);
                        }
                    } else {
                        state::release_temporary_top(app);
                        if window.is_minimized().unwrap_or(false) {
                            // Native minimize is also an explicit dismissal;
                            // return activation just as Esc/the hide button do.
                            state::hide(app);
                        } else if let Some(ticket) = blur {
                            let app = app.clone();
                            tauri::async_runtime::spawn(async move {
                                if let Ok(engine) = state::engine(&app).await
                                    && engine.configuration().await.settings.auto_hide
                                {
                                    state::hide_after_blur(&app, ticket);
                                }
                            });
                        }
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::credential_authorization_required,
            commands::authorize_saved_credential,
            commands::submit_operation,
            commands::operation_list,
            commands::cancel_operation,
            commands::resolve_operation_sensitive,
            reliability_commands::service_verifications,
            reliability_commands::default_reply_prompt,
            source_commands::source_update,
            source_commands::source_clean_preview,
            source_commands::source_clean_apply,
            source_commands::source_clean_text,
            source_commands::source_undo,
            source_commands::source_undo_cleanup,
            source_commands::source_restore,
            source_commands::source_remove_last,
            source_commands::source_layout_apply,
            source_commands::source_encounters,
            source_commands::source_assets,
            source_commands::source_asset,
            source_commands::source_pending,
            source_commands::resolve_source_pending,
            source_commands::set_append_mode,
            source_commands::capture_append,
            source_commands::capture_with_mode,
            commands::bootstrap,
            experience::experience_status,
            experience::reading_layout,
            experience::save_reading_height,
            experience::update_experience,
            experience::open_settings_at,
            experience::take_settings_route,
            experience::show_translation_window,
            commands::default_translation_prompt,
            commands::resolve_sensitive,
            commands::vocabulary_list,
            commands::vocabulary_lookup,
            commands::vocabulary_collect,
            commands::vocabulary_manual,
            commands::vocabulary_preview,
            commands::vocabulary_meaning,
            commands::vocabulary_enrich_existing,
            commands::speech_voices,
            commands::vocabulary_update,
            commands::vocabulary_remove,
            commands::vocabulary_export,
            commands::speak_word,
            commands::speak_text,
            commands::speech_status,
            commands::stop_speech,
            commands::permissions,
            commands::request_permission,
            commands::open_dictionary,
            commands::open_translation_link,
            commands::hide_window,
            commands::set_window_expanded,
            commands::dismiss_notice,
            commands::pin_window,
            commands::copy_text,
            commands::capture,
            commands::translate_text,
            commands::stop_task,
            commands::retry_task,
            commands::detect_text_language,
            commands::set_vocabulary_sidebar,
            commands::retry_save,
            commands::select_task,
            commands::history,
            commands::search_history,
            commands::history_detail,
            commands::show_task,
            commands::remove_task,
            commands::clear_history,
            commands::storage_stats,
            commands::fetch_models,
            commands::test_connection,
            commands::save_configuration,
            commands::save_preferences,
            commands::configuration_settings,
            commands::dictionary_guidance,
            commands::drag_window,
            commands::retry_shortcuts,
            commands::begin_shortcut_recording,
            commands::end_shortcut_recording,
            commands::subscribe,
            commands::unsubscribe,
            commands::settings_guard_ready,
            commands::finish_settings_close,
            commands::cancel_settings_close,
            commands::show_settings,
        ])
        .build(tauri::generate_context!())
        .expect("Yurelay could not start")
        .run(|app, event| match event {
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => state::show(app, true),
            tauri::RunEvent::Exit => yurelay_platform::speech::SPEECH.stop(None),
            tauri::RunEvent::ExitRequested { api, .. } => {
                if state::request_settings_close(app, settings_guard::Intent::Quit) {
                    api.prevent_exit();
                    return;
                }
                let state = app.state::<AppState>();
                if !state.exit_ready.load(Ordering::Relaxed) {
                    api.prevent_exit();
                    if !state.exit_requested.swap(true, Ordering::Relaxed) {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Some(engine) = app.state::<AppState>().engine.get() {
                                let _ = tokio::time::timeout(
                                    std::time::Duration::from_secs(4),
                                    engine.shutdown(),
                                )
                                .await;
                            }
                            app.state::<AppState>()
                                .exit_ready
                                .store(true, Ordering::Relaxed);
                            app.exit(0);
                        });
                    }
                }
            }
            _ => {}
        });
}
