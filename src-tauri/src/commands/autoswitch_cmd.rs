//! 自动切换规则 CRUD
use crate::autoswitch::{log_attempt, norm_dir, project_session_stats, switch_provider};
use crate::state::AppState;
use crate::types::{AutoSwitchLog, AutoSwitchRule, ProjectOption};
use crate::zcode::config_file;
use tauri::{AppHandle, State};
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
    r.name = r.name.trim().to_string();
    if r.name.is_empty() {
        return Err("规则名称不能为空".to_string());
    }
    if r.id.is_empty() {
        r.id = Uuid::new_v4().to_string();
    }
    // 规则名称不允许重复（排除自身，支持改名保存）
    let dup = state
        .db
        .list_rules()
        .map_err(|e| e.to_string())?
        .into_iter()
        .any(|x| x.id != r.id && x.name == r.name);
    if dup {
        return Err(format!("规则名称「{}」已存在", r.name));
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

/// 批量重排规则优先级：按 ids 顺序写 priority=0,1,2...（前端拖拽后调用）
#[tauri::command]
pub fn reorder_rules(state: State<'_, AppState>, ids: Vec<String>) -> Result<(), String> {
    state.db.reorder_rules(&ids).map_err(|e| e.to_string())
}

/// 自动切换可选项目：当前打开（setting.json lastWorkspaceSession，purpose=project）
/// 且有具体对话（zcode cli db 的 session 表）的项目，按最近活跃倒序。
#[tauri::command]
pub fn autoswitch_projects() -> Result<Vec<ProjectOption>, String> {
    let setting = config_file::read_setting().map_err(|e| e.to_string())?;
    let open_dirs: Vec<String> = setting
        .get("lastWorkspaceSession")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|e| {
                    e.get("workspacePurpose").and_then(|p| p.as_str()) == Some("project")
                })
                .filter_map(|e| {
                    e.get("workspacePath")
                        .and_then(|p| p.as_str())
                        .map(String::from)
                })
                .collect()
        })
        .unwrap_or_default();
    if open_dirs.is_empty() {
        return Ok(vec![]);
    }
    let stats = project_session_stats();
    let mut list: Vec<ProjectOption> = open_dirs
        .into_iter()
        .filter_map(|dir| {
            // 只保留有对话的项目；路径大小写/分隔符差异经规范化对齐
            let (sessions, last_active_ms) = stats.get(&norm_dir(&dir)).cloned()?;
            let name = std::path::Path::new(&dir)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| dir.clone());
            Some(ProjectOption {
                dir,
                name,
                sessions,
                last_active_ms,
            })
        })
        .collect();
    list.sort_by(|a, b| b.last_active_ms.cmp(&a.last_active_ms));
    Ok(list)
}

/// 手动测试规则：跳过触发条件（时间/配额/项目/源），立即执行切换并记执行日志。
/// 切换 = 直写全部目标会话的模型选择 + 直改 setting.json；偏好「切换后重启
/// ZCode」开（默认）时自动重启 ZCode 立即生效，关闭时各对话恢复 / 新开时生效。
/// 返回说明文案给前端 toast。
/// async：内部可能 kill/重启 ZCode（数秒），同步执行会冻结主线程。
#[tauri::command]
pub async fn test_rule(
    state: State<'_, AppState>,
    app: AppHandle,
    id: String,
) -> Result<String, String> {
    let rule = state
        .db
        .list_rules()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or("规则不存在")?;
    if rule.to_provider.is_empty() {
        let msg = "规则未配置目标供应商";
        log_attempt(&state.db, &rule, "manual", false, Some(msg));
        return Err(msg.to_string());
    }
    let setting = config_file::read_setting().map_err(|e| e.to_string())?;
    let family = setting
        .get("providerFamilyDomain")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or("未设置 providerFamilyDomain，无法定位切换维度")?;
    let current = config_file::current_selected(&setting, &family)
        .as_deref()
        .map(config_file::selected_to_provider);
    let restart_mode =
        crate::commands::prefs_cmd::current_prefs(&state.db).switch_restart_zcode;
    let has_model = rule
        .to_model
        .as_deref()
        .map(|m| !m.is_empty())
        .unwrap_or(false);
    // 已是目标供应商且无模型目标 → 无事可做；
    // 有模型目标 → 交给 switch_provider（直写全部目标会话的模型选择）
    if current.as_deref() == Some(rule.to_provider.as_str()) && !has_model {
        let msg = "当前已是目标供应商，无需切换";
        log_attempt(&state.db, &rule, "manual", true, Some(msg));
        return Ok(msg.to_string());
    }
    let already_provider = current.as_deref() == Some(rule.to_provider.as_str());
    match switch_provider(&app, &family, &rule) {
        Ok(()) => {
            log_attempt(&state.db, &rule, "manual", true, None);
            Ok(if restart_mode {
                "已写入配置与会话模型选择，并重启 ZCode，全部对话使用目标供应商 / 模型".to_string()
            } else if already_provider {
                "已是目标供应商，已写入各会话的模型选择（对话恢复 / 新开时生效）".to_string()
            } else {
                "已写入配置与会话模型选择（ZCode 未重启，各对话在恢复 / 新开时生效）".to_string()
            })
        }
        Err(e) => {
            log_attempt(&state.db, &rule, "manual", false, Some(&e));
            Err(e)
        }
    }
}

/// 自动切换执行日志（最近 200 条，时间倒序）
#[tauri::command]
pub fn autoswitch_logs(state: State<'_, AppState>) -> Result<Vec<AutoSwitchLog>, String> {
    state.db.list_switch_logs(200).map_err(|e| e.to_string())
}
