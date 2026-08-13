//! 智谱账号管理命令
use crate::accounts;
use crate::state::AppState;
use crate::types::AccountMeta;
use tauri::State;

#[tauri::command]
pub fn list_accounts(state: State<'_, AppState>) -> Result<Vec<AccountMeta>, String> {
    state.db.list_accounts().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn capture_account(
    state: State<'_, AppState>,
    label: String,
) -> Result<AccountMeta, String> {
    accounts::capture(&state.db, &state.data_dir, &label).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn switch_account(
    state: State<'_, AppState>,
    id: String,
) -> Result<AccountMeta, String> {
    accounts::switch(&state.db, &state.data_dir, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_account(state: State<'_, AppState>, id: String) -> Result<(), String> {
    accounts::remove(&state.db, &state.data_dir, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_account(
    state: State<'_, AppState>,
    id: String,
    label: String,
) -> Result<(), String> {
    state
        .db
        .rename_account(&id, &label)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn current_account(state: State<'_, AppState>) -> Result<Option<AccountMeta>, String> {
    Ok(accounts::current(&state.db))
}
