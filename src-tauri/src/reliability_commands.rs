use crate::state;
use tauri::AppHandle;
use yurelay_core::reliability::VerificationStatus;

#[tauri::command]
pub async fn service_verifications(app: AppHandle) -> Result<Vec<VerificationStatus>, String> {
    state::engine(&app).await?.service_verifications().await
}

#[tauri::command]
pub fn default_reply_prompt() -> &'static str {
    yurelay_core::operations::DEFAULT_REPLY_PROMPT
}
