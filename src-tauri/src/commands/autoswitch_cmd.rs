//! 自动切换规则 CRUD
use crate::state::AppState;
use crate::types::AutoSwitchRule;
use tauri::State;
use uuid::Uuid;

#[tauri::command]
pub fn list_rules(state: State<'_, AppState>) -> Result<Vec<AutoSwitchRule>, String> {
    state.db.list_rules().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn upsert_rule(
    state: State<'_, AppState>,
    rule: AutoSwitchRule,
) -> Result<String, String> {
    let mut r = rule;
    if r.id.is_empty() {
        r.id = Uuid::new_v4().to_string();
    }
    if r.created_at.is_empty() {
        r.created_at = chrono::Utc::now().to_rfc3339();
    }
    let id = r.id.clone();
    state
        .db
        .upsert_rule(&r)
        .map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
pub fn delete_rule(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.db.delete_rule(&id).map_err(|e| e.to_string())
}
