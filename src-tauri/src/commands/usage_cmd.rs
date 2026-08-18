//! 用量查询命令：解析 zcode rollout → 聚合统计
use std::collections::HashMap;

use crate::state::AppState;
use crate::types::{UsageAggRow, UsageFilters, UsageOverview, UsageRecord, UsageSyncResult};
use crate::usage;
use tauri::State;

/// 同步：从 zcode 的 model_usage 表增量导入 usage_records。
/// `full=true` 时清空并全量重导；默认增量（按 rowid 续传，资源最省）。
/// async：全量重导要读外部库数万行，同步执行会冻结主线程。
#[tauri::command]
pub async fn usage_sync(state: State<'_, AppState>, full: Option<bool>) -> Result<UsageSyncResult, String> {
    usage::sync_usage(&state.db, full.unwrap_or(false)).map_err(|e| e.to_string())
}

/// 供应商别名映射（provider_id -> 可读名，解析自 transcript）。前端据此把 UUID 显示成真名。
#[tauri::command]
pub async fn usage_provider_labels(state: State<'_, AppState>) -> Result<HashMap<String, String>, String> {
    state.db.provider_alias_map().map_err(|e| e.to_string())
}

/// 筛选项（去重的供应商/模型/角色 + 日期范围 + 总条数）
#[tauri::command]
pub async fn usage_filters(state: State<'_, AppState>) -> Result<UsageFilters, String> {
    state.db.usage_filters().map_err(|e| e.to_string())
}

/// 整体汇总（随筛选条件）
#[tauri::command]
pub async fn usage_overview(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    role: Option<String>,
) -> Result<UsageOverview, String> {
    state
        .db
        .usage_overview(
            from.as_deref(),
            to.as_deref(),
            provider.as_deref(),
            model.as_deref(),
            role.as_deref(),
        )
        .map_err(|e| e.to_string())
}

/// 分组聚合（按供应商 / 模型 / 日期）
#[tauri::command]
pub async fn usage_aggregate(
    state: State<'_, AppState>,
    group_by: String,
    from: Option<String>,
    to: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    role: Option<String>,
) -> Result<Vec<UsageAggRow>, String> {
    state
        .db
        .usage_aggregate(
            &group_by,
            from.as_deref(),
            to.as_deref(),
            provider.as_deref(),
            model.as_deref(),
            role.as_deref(),
        )
        .map_err(|e| e.to_string())
}

/// 明细记录（分页，按时间倒序）
#[tauri::command]
pub async fn usage_records(
    state: State<'_, AppState>,
    from: Option<String>,
    to: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    role: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<UsageRecord>, String> {
    state
        .db
        .usage_records_list(
            from.as_deref(),
            to.as_deref(),
            provider.as_deref(),
            model.as_deref(),
            role.as_deref(),
            limit.unwrap_or(200),
            offset.unwrap_or(0),
        )
        .map_err(|e| e.to_string())
}
