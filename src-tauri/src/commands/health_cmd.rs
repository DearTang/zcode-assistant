//! 当前模型可用性检测：GET /models 免费探测（0 token，不发起对话请求）+ 失败指数退避冷却
//!
//! 调度模型：前端 App 每 30s 调一次 check_current_health(false)，命令内部做冷却判断——
//! 冷却期内直接返回缓存、不发网络请求；手动「立即检测」传 force=true 绕过冷却。
//! 失败退避：60s → 120s → 300s → 900s → 1800s 封顶；成功后回到常规 30s。
//! 切换供应商后（缓存 provider 与当前不一致）视同 force，立即重新探测。
use crate::commands::models_cmd::models_endpoint;
use crate::state::AppState;
use crate::zcode::config_file;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

/// 常规检测间隔（秒）：可用状态下的下次可检测时间
const CHECK_INTERVAL_SECS: i64 = 30;
/// 连续失败退避表（秒）：第 n 次失败后等待 BACKOFF_SECS[min(n-1, len-1)]
const BACKOFF_SECS: [i64; 5] = [60, 120, 300, 900, 1800];
/// 单次探测超时（共享客户端默认 30s，探测要求快速反馈）
const PROBE_TIMEOUT_SECS: u64 = 10;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub provider_key: String,
    pub provider_name: String,
    pub ok: bool,
    pub message: String,
    /// 上次真实检测时间（unix 秒）
    pub checked_at: i64,
    /// 下次允许自动检测的时间（unix 秒）：失败后的冷却截止
    pub next_check_at: i64,
    /// 连续失败次数（成功后清零，决定退避档位）
    pub fail_count: u32,
    /// true=本次调用未发起网络请求（冷却中 / 缺凭据），返回的是缓存或跳过态
    pub skipped: bool,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// GET /models 探测：Ok(Some(n))=成功且解析到 n 个模型；Ok(None)=成功但无列表；Err=失败原因
async fn probe_models(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
) -> Result<Option<usize>, String> {
    let resp = client
        .get(url)
        .bearer_auth(api_key)
        .timeout(Duration::from_secs(PROBE_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| format!("连接失败：{e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(120).collect();
        return Err(format!("HTTP {}: {}", status.as_u16(), snippet));
    }
    // 模型数仅用于提示，解析失败不影响「连接可用」的判定
    let v: Value = resp.json().await.unwrap_or(Value::Null);
    Ok(v.get("data")
        .and_then(|d| d.as_array())
        .or_else(|| v.as_array())
        .map(|a| a.len()))
}

/// 检测当前选中供应商的可用性。
/// force=true 绕过冷却立即探测（手动「立即检测」）；否则冷却期内直接返回缓存。
#[tauri::command]
pub async fn check_current_health(
    app: AppHandle,
    state: State<'_, AppState>,
    force: bool,
) -> Result<HealthReport, String> {
    let key = config_file::current_provider_key()
        .ok_or_else(|| "无法确定当前选中的供应商".to_string())?;
    run_health_check(&app, state.inner(), &key, force).await
}

/// 手动检测指定供应商（模型卡片 ⚡ 立即检测）：绕过冷却直接探测，
/// 结果由前端 toast 通知。非当前供应商不写缓存（缓存语义 = 当前供应商，
/// 供 App 30s 轮询冷却复用，避免污染退避计数）。
#[tauri::command]
pub async fn check_provider_health(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_key: String,
) -> Result<HealthReport, String> {
    run_health_check(&app, state.inner(), &provider_key, true).await
}

/// 检测核心：按 key 取 baseURL/apiKey 探测 GET /models，
/// 仅当前供应商读写冷却缓存；其余供应商只广播事件不占缓存。
async fn run_health_check(
    app: &AppHandle,
    state: &AppState,
    key: &str,
    force: bool,
) -> Result<HealthReport, String> {
    let is_current = config_file::current_provider_key().as_deref() == Some(key);
    let config = config_file::read_config().map_err(|e| e.to_string())?;
    let base = config_file::provider_base_url(&config, key)
        .filter(|b| !b.trim().is_empty())
        .ok_or_else(|| format!("供应商「{key}」未配置 baseURL"))?;
    let provider_name = config_file::provider_name(&config, key);

    let now = now_secs();
    let cached = state.health.lock().ok().and_then(|g| g.clone());
    // 退避计数只对同一 provider 连续；切换供应商后从头算
    let prev_fail_count = cached
        .as_ref()
        .filter(|c| c.provider_key == key)
        .map(|c| c.fail_count)
        .unwrap_or(0);

    // 冷却：当前供应商、未到期、非 force → 直接返回缓存（不发请求）
    if let Some(rep) = cached {
        if is_current && rep.provider_key == key && !force && now < rep.next_check_at {
            let mut rep = rep;
            rep.skipped = true;
            return Ok(rep);
        }
    }

    // 缺 apiKey（如 codex OAuth 导入的条目）：无法探测，跳过态不发请求、不占冷却
    let Some(api_key) = config_file::provider_api_key(&config, key) else {
        let rep = HealthReport {
            provider_key: key.to_string(),
            provider_name,
            ok: false,
            message: "未配置 apiKey，无法检测".into(),
            checked_at: now,
            next_check_at: now,
            fail_count: 0,
            skipped: true,
        };
        emit_report(app, &rep);
        return Ok(rep);
    };

    let url = models_endpoint(&base);
    let client = state.client();
    let (ok, message, fail_count) = match probe_models(&client, &url, &api_key).await {
        Ok(Some(n)) => (true, format!("连接成功，发现 {n} 个模型"), 0),
        Ok(None) => (true, "连接成功".to_string(), 0),
        Err(e) => (false, e, prev_fail_count + 1),
    };
    let next_check_at = if ok {
        now + CHECK_INTERVAL_SECS
    } else {
        now + BACKOFF_SECS[((fail_count - 1) as usize).min(BACKOFF_SECS.len() - 1)]
    };
    let rep = HealthReport {
        provider_key: key.to_string(),
        provider_name,
        ok,
        message,
        checked_at: now,
        next_check_at,
        fail_count,
        skipped: false,
    };
    if is_current {
        // 当前供应商：写缓存（冷却 / 退避复用）+ 广播
        if let Ok(mut g) = state.health.lock() {
            *g = Some(rep.clone());
        }
    }
    emit_report(app, &rep);
    Ok(rep)
}

fn emit_report(app: &AppHandle, rep: &HealthReport) {
    let _ = app.emit("health://updated", rep.clone());
}
