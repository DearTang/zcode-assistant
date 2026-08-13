//! 配额查询模板 CRUD
use crate::state::AppState;
use crate::types::QuotaTemplate;
use tauri::State;

#[tauri::command]
pub fn list_templates(state: State<'_, AppState>) -> Result<Vec<QuotaTemplate>, String> {
    state.db.list_templates().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_quota_template(
    state: State<'_, AppState>,
    provider_key: String,
) -> Result<Option<QuotaTemplate>, String> {
    state
        .db
        .get_template(&provider_key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn upsert_template(
    state: State<'_, AppState>,
    template: QuotaTemplate,
) -> Result<(), String> {
    state
        .db
        .upsert_template(&template)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_template(state: State<'_, AppState>, provider_key: String) -> Result<(), String> {
    state
        .db
        .delete_template(&provider_key)
        .map_err(|e| e.to_string())
}
