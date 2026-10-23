use crate::state::{self, AppState};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use yurelay_core::onboarding::{Action, Experience, Readiness};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Translation,
    Services,
    Help,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct SettingsRoute {
    pub section: Section,
    pub profile_id: Option<String>,
    #[serde(default)]
    pub dictionary: bool,
    #[serde(default)]
    pub translation: bool,
}
#[derive(Serialize)]
pub struct ExperienceStatus {
    pub progress: Experience,
    pub services: Vec<Readiness>,
    pub shortcuts: state::ShortcutReport,
}
#[tauri::command]
pub async fn experience_status(app: AppHandle) -> Result<ExperienceStatus, String> {
    let engine = state::engine(&app).await?;
    Ok(ExperienceStatus {
        progress: engine.storage.experience().await?,
        services: yurelay_core::onboarding::readiness(&engine.configuration().await),
        shortcuts: app
            .state::<AppState>()
            .shortcut_report
            .lock()
            .unwrap()
            .clone(),
    })
}
#[tauri::command]
pub async fn update_experience(app: AppHandle, action: Action) -> Result<Experience, String> {
    let close_guide = matches!(action, Action::Skip | Action::Finish);
    let progress = state::engine(&app)
        .await?
        .storage
        .update_experience(action)
        .await?;
    let _ = app.emit(
        "experience-changed",
        serde_json::json!({ "progress": &progress, "close_guide": close_guide }),
    );
    Ok(progress)
}
#[tauri::command]
pub fn take_settings_route(app: AppHandle) -> Option<SettingsRoute> {
    app.state::<AppState>()
        .settings_route
        .lock()
        .unwrap()
        .take()
}
#[tauri::command]
pub fn open_settings_at(app: AppHandle, route: SettingsRoute) -> Result<(), String> {
    if route.profile_id.as_ref().is_some_and(|id| id.len() > 64) {
        return Err("服务标识无效。".into());
    }
    *app.state::<AppState>().settings_route.lock().unwrap() = Some(route);
    state::show_settings(&app);
    let _ = app.emit_to("settings", "settings-navigate", ());
    Ok(())
}

#[tauri::command]
pub fn show_translation_window(app: AppHandle) {
    state::show(&app, true);
}
