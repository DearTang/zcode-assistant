//! 配额查询：
//!   - Token Plan 供应商（Kimi/智谱/MiniMax/ZenMux/火山）→ coding_plan 模块按 baseURL 自动识别，
//!     自动使用供应商 API Key + Base URL（对齐 cc-switch）
//!   - 智谱Coding Plan 账号 → fetch_zhipu_quota（bigmodel.cn usage/quota/limit）
//!   - 其余供应商 → 通用配额模板查询
use crate::state::AppState;
use crate::zcode::config_file;
use base64::{engine::general_purpose, Engine};
use serde::Serialize;
use serde_json::Value;
use tauri::State;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QuotaBucket {
    pub name: String,
    pub total: f64,
    pub used: f64,
    pub remaining: f64,
    pub unit: Option<String>,
    pub period_end: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QuotaOverview {
    pub source: String,
    /// 配额所属供应商显示名（总览 / 悬浮窗 / 托盘标注数据来源）
    pub provider_name: Option<String>,
    pub account_label: Option<String>,
    pub plan_name: Option<String>,
    pub buckets: Vec<QuotaBucket>,
    pub fetched_at: String,
    pub error: Option<String>,
}

/// 从 config.json 取用于配额查询的 apiKey（同时返回显示名/标识）
///
/// 配额接口（bigmodel.cn/api/monitor/usage/quota/limit）是 Coding Plan 订阅配额，
/// 必须用 Coding Plan 订阅 key 才有效。Coding Plan 供应商由账号自动确认，
/// 无需手动标记。按优先级选取：
///   1) builtin 标识符含 coding-plan（账号登录态托管的内置订阅 provider）
///   2) 第一个 enabled 且有 apiKey 的 provider（回退）
///   3) 任意有 apiKey 的 provider（兜底）
/// 否则会用 qwen/MiniMax 等非 bigmodel 的 key 查 bigmodel 配额，得到 401。
fn current_provider_creds() -> Result<(String, String), String> {
    let cfg = config_file::read_config().map_err(|e| e.to_string())?;
    let providers = cfg
        .get("provider")
        .and_then(|p| p.as_object())
        .ok_or_else(|| "config.json 无 provider".to_string())?;

    // 取 provider 的明文 apiKey（跳过空 / 脱敏占位）
    let api_key_of = |p: &Value| -> Option<String> {
        let k = p
            .get("options")
            .and_then(|o| o.get("apiKey"))
            .and_then(|v| v.as_str())?;
        if k.is_empty() || k == "<REDACTED>" {
            None
        } else {
            Some(k.to_string())
        }
    };
    let name_of = |p: &Value, key: &str| -> String {
        p.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(key)
            .to_string()
    };
    let enabled_of = |p: &Value| p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    // 内置订阅 provider：标识符含 coding-plan / codingplan（由账号登录态自动确认）
    let is_builtin_cp = |key: &str| key.contains("coding-plan") || key.contains("codingplan");

    // 1) builtin coding-plan provider（账号自动确认）
    for (key, p) in providers.iter() {
        if is_builtin_cp(key) {
            if let Some(api_key) = api_key_of(p) {
                return Ok((api_key, name_of(p, key)));
            }
        }
    }
    // 2) 回退：第一个 enabled 且有 apiKey 的 provider
    for (key, p) in providers.iter() {
        if enabled_of(p) {
            if let Some(api_key) = api_key_of(p) {
                return Ok((api_key, name_of(p, key)));
            }
        }
    }
    // 3) 兜底：任意有 apiKey 的 provider
    for (key, p) in providers.iter() {
        if let Some(api_key) = api_key_of(p) {
            return Ok((api_key, name_of(p, key)));
        }
    }
    Err("无可用 apiKey".into())
}

/// 从 JWT apiKey 的 payload 解码字段
fn jwt_field(token: &str, field: &str) -> Option<String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    let payload = general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .or_else(|_| general_purpose::URL_SAFE.decode(parts[1]))
        .ok()?;
    let v: Value = serde_json::from_slice(&payload).ok()?;
    v.get(field)?
        .as_str()
        .map(String::from)
        .or_else(|| v.get(field).and_then(|x| x.as_i64()).map(|n| n.to_string()))
}

/// 从 subscription list 提取套餐显示名
fn parse_subscription_plan(data: &Value) -> Option<String> {
    let arr = data.as_array()?;
    arr.iter()
        .find(|p| p.get("status").and_then(|s| s.as_str()) == Some("VALID"))
        .and_then(|p| p.get("productName").and_then(|n| n.as_str()).map(String::from))
        .or_else(|| {
            arr.first()
                .and_then(|p| p.get("productName").and_then(|n| n.as_str()).map(String::from))
        })
}

fn limit_name(ltype: &str, unit: i64) -> String {
    match (ltype, unit) {
        ("TIME_LIMIT", 5) => "MCP每月额度".to_string(),
        ("TIME_LIMIT", _) => "工具调用限额".to_string(),
        ("TOKENS_LIMIT", 3) => "每5小时使用额度".to_string(),
        ("TOKENS_LIMIT", 6) => "每周使用额度".to_string(),
        ("TOKENS_LIMIT", _) => "Token 额度".to_string(),
        _ => format!("{ltype}·{unit}"),
    }
}

/// 用指定 apiKey 查询智谱（bigmodel.cn / api.z.ai）Coding Plan 配额（5H/每周 + 重置时间）。
/// 由 fetch_coding_plan（全局选 key）、coding_plan::fetch_quota（按 provider）复用；
/// team = Some((组织 ID, 项目 ID)) 时按团队接口（?type=2 + bigmodel-organization/project 头）查询。
pub(crate) async fn fetch_zhipu_quota(
    state: &AppState,
    api_key: &str,
    provider_name: &str,
    root: &str,
    team: Option<(&str, &str)>,
) -> Result<QuotaOverview, String> {
    let client = state.client();

    // 1) 配额（团队版 ?type=2）
    let quota_url = format!(
        "{root}/api/monitor/usage/quota/limit{}",
        if team.is_some() { "?type=2" } else { "" }
    );
    // bigmodel.cn 沿用本应用已验证的 Bearer；api.z.ai 按 cc-switch 实测用裸 Authorization
    let auth_value = if root.contains("bigmodel") {
        format!("Bearer {api_key}")
    } else {
        api_key.to_string()
    };
    let mut req = client
        .get(&quota_url)
        .header("Authorization", &auth_value)
        .header("Content-Type", "application/json")
        .header("Accept-Language", "en-US,en");
    if let Some((org, project)) = team {
        req = req
            .header("bigmodel-organization", org)
            .header("bigmodel-project", project);
    }
    let quota_resp: Value = req
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| format!("quota 请求失败: {e}"))?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    // 2) 订阅（拿套餐名；z.ai 无此接口跳过）-- 失败则回退到 quota level
    let plan_name = if root.contains("bigmodel") {
        match client
            .get("https://bigmodel.cn/api/biz/subscription/list")
            .header("authorization", format!("Bearer {api_key}"))
            .send()
            .await
        {
            Ok(r) => match r.json::<Value>().await {
                Ok(j) => j
                    .get("data")
                    .and_then(parse_subscription_plan)
                    .or_else(|| {
                        quota_resp
                            .get("data")
                            .and_then(|d| d.get("level"))
                            .and_then(|v| v.as_str())
                            .map(String::from)
                    }),
                Err(_) => None,
            },
            Err(_) => None,
        }
    } else {
        quota_resp
            .get("data")
            .and_then(|d| d.get("level"))
            .and_then(|v| v.as_str())
            .map(String::from)
    };

    // 账号：优先从 apiKey JWT 解码
    let account_label = jwt_field(api_key, "email")
        .or_else(|| jwt_field(api_key, "user_id"))
        .or_else(|| jwt_field(api_key, "sub"))
        .unwrap_or_else(|| provider_name.to_string());

    let data = quota_resp.get("data").ok_or_else(|| {
        format!(
            "quota 响应无 data（code={:?} msg={:?}，使用 provider: {}）",
            quota_resp.get("code").and_then(|v| v.as_i64()),
            quota_resp.get("msg").and_then(|v| v.as_str()),
            provider_name,
        )
    })?;
    let limits = data
        .get("limits")
        .and_then(|l| l.as_array())
        .ok_or_else(|| "响应无 limits 数组".to_string())?;

    let buckets: Vec<QuotaBucket> = limits
        .iter()
        .filter_map(|l| {
            let ltype = l.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let unit = l.get("unit").and_then(|v| v.as_i64()).unwrap_or(0);
            let name = limit_name(ltype, unit);
            let nrt = l.get("nextResetTime");
            let next_reset = nrt
                .and_then(|v| {
                    let ms = v
                        .as_i64()
                        .or_else(|| v.as_u64().map(|n| n as i64))
                        .or_else(|| v.as_f64().map(|n| n as i64))
                        .or_else(|| v.as_str().and_then(|s| s.parse::<i64>().ok()));
                    ms
                })
                .and_then(|ms| {
                    if ms <= 0 {
                        None
                    } else {
                        chrono::DateTime::from_timestamp_millis(ms).map(|d| d.to_rfc3339())
                    }
                });

            if ltype == "TIME_LIMIT" {
                // 次数型：usage(总额) / currentValue(已用) / remaining
                let total = l.get("usage").and_then(|v| v.as_f64())?;
                let used = l.get("currentValue").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let remaining = l
                    .get("remaining")
                    .and_then(|v| v.as_f64())
                    .unwrap_or_else(|| (total - used).max(0.0));
                Some(QuotaBucket {
                    name,
                    total,
                    used,
                    remaining,
                    unit: Some("次".into()),
                    period_end: next_reset,
                })
            } else {
                // TOKENS_LIMIT：仅有 percentage（已用%）
                let pct = l.get("percentage").and_then(|v| v.as_f64()).unwrap_or(0.0);
                Some(QuotaBucket {
                    name,
                    total: 100.0,
                    used: pct,
                    remaining: (100.0 - pct).max(0.0),
                    unit: Some("%".into()),
                    period_end: next_reset,
                })
            }
        })
        .collect();

    Ok(QuotaOverview {
        source: "bigmodel-usage".into(),
        provider_name: Some(provider_name.to_string()),
        account_label: Some(account_label),
        plan_name,
        buckets,
        fetched_at: chrono::Utc::now().to_rfc3339(),
        error: None,
    })
}

/// 查询 Coding Plan 配额（全局：账号自动确认订阅 provider，无需手动标记）
pub async fn fetch_coding_plan(state: &AppState) -> Result<QuotaOverview, String> {
    let (api_key, provider_name) = current_provider_creds()?;
    fetch_zhipu_quota(state, &api_key, &provider_name, "https://bigmodel.cn", None).await
}

/// 构造空的 QuotaOverview（无主供应商且无智谱 Coding Plan 时返回，避免错误 toast）
fn empty_overview() -> QuotaOverview {
    QuotaOverview {
        source: "none".into(),
        provider_name: None,
        account_label: None,
        plan_name: None,
        buckets: vec![],
        fetched_at: chrono::Utc::now().to_rfc3339(),
        error: None,
    }
}

#[tauri::command]
pub async fn get_coding_plan_quota(
    state: State<'_, AppState>,
) -> Result<QuotaOverview, String> {
    fetch_coding_plan(state.inner()).await
}

/// 按 dot path 提取数字
fn extract_by_path(v: &Value, path: &str) -> Option<f64> {
    let mut cur = v;
    for seg in path.split('.') {
        cur = if let Ok(i) = seg.parse::<usize>() {
            cur.get(i)?
        } else {
            cur.get(seg)?
        };
    }
    cur.as_f64()
        .or_else(|| cur.as_str().and_then(|s| s.parse().ok()))
}

/// 按 dot path 提取重置时间（毫秒/秒时间戳自动判，ISO 字符串原样返回）
fn extract_reset_from_resp(resp: &Value, path: &str) -> Option<String> {
    let mut cur = resp;
    for seg in path.split('.') {
        cur = if let Ok(i) = seg.parse::<usize>() {
            cur.get(i)?
        } else {
            cur.get(seg)?
        };
    }
    if let Some(n) = cur.as_f64() {
        let n = n as i64;
        if n <= 0 {
            return None;
        }
        if n >= 1_000_000_000_000 {
            return chrono::DateTime::from_timestamp_millis(n).map(|d| d.to_rfc3339());
        }
        return chrono::DateTime::from_timestamp_millis(n * 1000).map(|d| d.to_rfc3339());
    }
    cur.as_str().filter(|s| !s.is_empty()).map(String::from)
}

/// 按一组路径（total/used/remaining + 重置时间）从响应构建一个配额桶。
/// 任一值路径配置且能提取到数值才返回；百分比模式下值 ≤ 1.0 自动 ×100、total 兜底 100。
#[allow(clippy::too_many_arguments)]
fn build_bucket(
    resp: &Value,
    name: &str,
    is_pct: bool,
    total_p: Option<&str>,
    used_p: Option<&str>,
    remaining_p: Option<&str>,
    reset_p: Option<&str>,
) -> Option<QuotaBucket> {
    let to_pct = |v: f64| if is_pct && v <= 1.0 { v * 100.0 } else { v };
    let total = total_p.and_then(|p| extract_by_path(resp, p));
    let used = used_p.and_then(|p| extract_by_path(resp, p));
    let remaining = remaining_p.and_then(|p| extract_by_path(resp, p));
    if total.is_none() && used.is_none() && remaining.is_none() {
        return None;
    }
    let total = total.map(to_pct).unwrap_or(if is_pct { 100.0 } else { 0.0 });
    let used = used.map(to_pct).unwrap_or(0.0);
    let remaining = remaining
        .map(to_pct)
        .unwrap_or_else(|| if total > used { total - used } else { 0.0 });
    let period_end = reset_p.and_then(|p| extract_reset_from_resp(resp, p));
    Some(QuotaBucket {
        name: name.to_string(),
        total,
        used,
        remaining,
        unit: if is_pct { Some("%".to_string()) } else { None },
        period_end,
    })
}

/// 通用模板配额查询核心（按 provider 的配置模板发请求 + dot-path 提取）
async fn run_template_quota(
    state: &AppState,
    provider_key: &str,
) -> Result<QuotaOverview, String> {
    let tmpl = state
        .db
        .get_template(provider_key)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "该 provider 未配置配额查询模板".to_string())?;

    let config = config_file::read_config().map_err(|e| e.to_string())?;
    let api_key = config_file::provider_api_key(&config, provider_key).unwrap_or_default();
    let base = config_file::provider_base_url(&config, provider_key).unwrap_or_default();
    let provider_name = config
        .get("provider")
        .and_then(|p| p.get(provider_key))
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or(provider_key)
        .to_string();

    // {{token}}：登录获取的会话 token（keyring 存储）；模板引用了但未获取 → 明确报错
    let token = crate::commands::token_cmd::read_token(provider_key);
    let refs_token = tmpl.url.as_deref().map(|s| s.contains("{{token}}")).unwrap_or(false)
        || tmpl.body.as_deref().map(|s| s.contains("{{token}}")).unwrap_or(false)
        || tmpl
            .headers_json
            .as_deref()
            .map(|s| s.contains("{{token}}"))
            .unwrap_or(false);
    if refs_token && token.is_none() {
        return Err(
            "模板引用了 {{token}}，但尚未获取：请到 设置 → 配额查询模板 点「登录获取 Token」".into(),
        );
    }
    let token_s = token.unwrap_or_default();

    let url = tmpl
        .url
        .unwrap_or_default()
        .replace("{{apiKey}}", &api_key)
        .replace("{{baseURL}}", &base)
        .replace("{{token}}", &token_s);
    let method = tmpl.method.unwrap_or_else(|| "GET".to_string());
    let client = state.client();

    let mut req = if method.eq_ignore_ascii_case("POST") {
        client.post(&url).body(
            tmpl.body
                .clone()
                .unwrap_or_default()
                .replace("{{apiKey}}", &api_key)
                .replace("{{baseURL}}", &base)
                .replace("{{token}}", &token_s),
        )
    } else {
        client.get(&url)
    };
    if !api_key.is_empty() {
        req = req.bearer_auth(&api_key);
    }
    if let Some(h) = tmpl.headers_json {
        if let Ok(map) = serde_json::from_str::<serde_json::Map<String, Value>>(&h) {
            for (k, v) in map {
                if let Some(s) = v.as_str() {
                    req = req.header(
                        k,
                        s.replace("{{apiKey}}", &api_key)
                            .replace("{{baseURL}}", &base)
                            .replace("{{token}}", &token_s),
                    );
                }
            }
        }
    }

    let resp: Value = req
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| format!("请求失败: {e}"))?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let is_pct = tmpl.unit.as_deref() == Some("%");

    // 四类桶（cc-switch 口径）：主桶（余额/通用）+ 每5小时 + 每周 + 每月。
    // 各组路径可选；任一路径配置且能提取到数值才生成对应桶，全部为空则报错。
    let mut buckets = Vec::new();

    // 主桶（余额）：用模板名命名（如「DeepSeek 余额」）；只配 5h/weekly 的模板不会生成多余的 0 桶
    if let Some(b) = build_bucket(
        &resp,
        &tmpl.name.clone().unwrap_or_else(|| "配额".into()),
        is_pct,
        tmpl.total_path.as_deref(),
        tmpl.used_path.as_deref(),
        tmpl.remaining_path.as_deref(),
        tmpl.reset_time_path.as_deref(),
    ) {
        buckets.push(b);
    }

    if let Some(b) = build_bucket(
        &resp,
        "每5小时使用额度",
        is_pct,
        tmpl.five_hour_total_path.as_deref(),
        tmpl.five_hour_used_path.as_deref(),
        tmpl.five_hour_remaining_path.as_deref(),
        tmpl.five_hour_reset_time_path.as_deref(),
    ) {
        buckets.push(b);
    }

    if let Some(b) = build_bucket(
        &resp,
        "每周使用额度",
        is_pct,
        tmpl.weekly_total_path.as_deref(),
        tmpl.weekly_used_path.as_deref(),
        tmpl.weekly_remaining_path.as_deref(),
        tmpl.weekly_reset_time_path.as_deref(),
    ) {
        buckets.push(b);
    }

    if let Some(b) = build_bucket(
        &resp,
        "每月使用额度",
        is_pct,
        tmpl.monthly_total_path.as_deref(),
        tmpl.monthly_used_path.as_deref(),
        tmpl.monthly_remaining_path.as_deref(),
        tmpl.monthly_reset_time_path.as_deref(),
    ) {
        buckets.push(b);
    }

    if buckets.is_empty() {
        return Err("模板未配置提取路径，或响应中无可提取的数值".into());
    }

    Ok(QuotaOverview {
        source: "template".into(),
        provider_name: Some(provider_name),
        account_label: None,
        plan_name: tmpl.name.clone(),
        buckets,
        fetched_at: chrono::Utc::now().to_rfc3339(),
        error: None,
    })
}

/// 通用模板配额查询（command）
#[tauri::command]
pub async fn get_template_quota(
    state: State<'_, AppState>,
    provider_key: String,
) -> Result<QuotaOverview, String> {
    run_template_quota(state.inner(), &provider_key).await
}

/// 按供应商查询配额核心（统一入口）：
///   Token Plan 供应商（Kimi/智谱/MiniMax/ZenMax/火山，按 baseURL 自动识别）
///     → coding_plan 专用查询（自动使用该供应商的 API Key + Base URL）
///   其余 → 用量查询模板（需用户配置）
async fn provider_quota(
    state: &AppState,
    provider_key: &str,
) -> Result<QuotaOverview, String> {
    let config = config_file::read_config().map_err(|e| e.to_string())?;
    let base = config_file::provider_base_url(&config, provider_key).unwrap_or_default();
    if let Some(cp) = crate::coding_plan::detect(&base) {
        return crate::coding_plan::fetch_quota(state, provider_key, cp).await;
    }
    // 其余：用量查询模板（未配置则报错，前端显示「未配置用量查询」）
    run_template_quota(state, provider_key).await
}

/// 按供应商查询配额（command）
#[tauri::command]
pub async fn get_provider_quota(
    state: State<'_, AppState>,
    provider_key: String,
) -> Result<QuotaOverview, String> {
    provider_quota(state.inner(), &provider_key).await
}

/// 总览配额（Dashboard / 悬浮球 / 悬浮面板 / 托盘共用的数据源）：
///   1) 设了主供应商 → 查主供应商（Token Plan 供应商自动查询，其余走其用量模板）
///      查询失败时回退到 2，不直接报错（避免 broken primary 卡住整个总览）
///   2) 智谱 Coding Plan（账号确认）
///   3) 都没有 / 都失败 → 返回空 QuotaOverview（error:None），不触发错误 toast
pub async fn fetch_overview_quota(state: &AppState) -> Result<QuotaOverview, String> {
    let cfg = config_file::read_config().ok();
    let has_provider = |key: &str| {
        cfg.as_ref()
            .and_then(|c| c.get("provider"))
            .and_then(|p| p.as_object())
            .is_some_and(|m| m.contains_key(key))
    };

    // 1) 主供应商（失败时静默回退，不 toast）
    if let Some(key) = state.db.primary_provider_key().ok().flatten() {
        if has_provider(&key) {
            if let Ok(q) = provider_quota(state, &key).await {
                return Ok(q);
            }
            // 主供应商配额查询失败 → 回退智谱 Coding Plan
        }
    }

    // 2) 智谱 Coding Plan（失败时返回空概览，不 toast）
    let has_zhipu_coding_plan = cfg
        .as_ref()
        .and_then(|c| c.get("provider"))
        .and_then(|p| p.as_object())
        .map(|m| m.keys().any(|k| k.contains("coding-plan")))
        .unwrap_or(false);
    if has_zhipu_coding_plan {
        return match fetch_coding_plan(state).await {
            Ok(q) => Ok(q),
            Err(_) => Ok(empty_overview()),
        };
    }

    // 3) 无可用源 → 空概览
    Ok(empty_overview())
}

/// 总览配额查询（command，主窗口 App 全局轮询调用）
#[tauri::command]
pub async fn get_overview_quota(
    state: State<'_, AppState>,
) -> Result<QuotaOverview, String> {
    fetch_overview_quota(state.inner()).await
}
