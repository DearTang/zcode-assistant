//! Coding Plan 配额查询：走 bigmodel.cn 的 usage/quota/limit（用 provider apiKey，绕开 zcode.z.ai 的 WAF）
//! + 通用配额模板查询
use crate::db::Database;
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
    pub account_label: Option<String>,
    pub plan_name: Option<String>,
    pub buckets: Vec<QuotaBucket>,
    pub fetched_at: String,
    pub error: Option<String>,
}

/// 从 config.json 取用于配额查询的 apiKey（同时返回显示名/标识）
///
/// 配额接口（bigmodel.cn/api/monitor/usage/quota/limit）是 Coding Plan 订阅配额，
/// 必须用 Coding Plan 订阅 key 才有效。Coding Plan 判定优先用 db 显式标记
/// （覆盖自定义 provider；内置 builtin:bigmodel-coding-plan 由标识符兜底识别）。
/// 按优先级选取：
///   1) db 标记为 Coding Plan 的 enabled provider
///   2) db 标记为 Coding Plan 的任意 provider
///   3) builtin 标识符含 coding-plan（兜底识别内置订阅 provider）
///   4) 第一个 enabled 且有 apiKey 的 provider（回退，兼容旧行为）
///   5) 任意有 apiKey 的 provider（兜底）
/// 否则会用 qwen/MiniMax 等非 bigmodel 的 key 查 bigmodel 配额，得到 401。
fn current_provider_creds(db: &Database) -> Result<(String, String), String> {
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

    // db 显式标记为 Coding Plan 的 provider key（不依赖 config.json，可靠）
    let marked = db.list_coding_plan_provider_keys().unwrap_or_default();
    let is_marked = |key: &str| marked.iter().any(|m| m == key);
    // 内置兜底：标识符含 coding-plan / codingplan
    let is_builtin_cp = |key: &str| key.contains("coding-plan") || key.contains("codingplan");

    // 1) db 标记的 enabled Coding Plan provider
    for (key, p) in providers.iter() {
        if is_marked(key) && enabled_of(p) {
            if let Some(api_key) = api_key_of(p) {
                return Ok((api_key, name_of(p, key)));
            }
        }
    }
    // 2) db 标记的任意 Coding Plan provider
    for (key, p) in providers.iter() {
        if is_marked(key) {
            if let Some(api_key) = api_key_of(p) {
                return Ok((api_key, name_of(p, key)));
            }
        }
    }
    // 3) builtin coding-plan provider（标识符兜底）
    for (key, p) in providers.iter() {
        if is_builtin_cp(key) {
            if let Some(api_key) = api_key_of(p) {
                return Ok((api_key, name_of(p, key)));
            }
        }
    }
    // 4) 回退：第一个 enabled 且有 apiKey 的 provider
    for (key, p) in providers.iter() {
        if enabled_of(p) {
            if let Some(api_key) = api_key_of(p) {
                return Ok((api_key, name_of(p, key)));
            }
        }
    }
    // 5) 兜底：任意有 apiKey 的 provider
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

/// 用指定 apiKey 查询 bigmodel Coding Plan 配额（5H/每周 + 重置时间）
/// 由 fetch_coding_plan（全局选 key）和 get_provider_quota（按 provider）复用。
async fn fetch_bigmodel_quota(
    state: &AppState,
    api_key: &str,
    provider_name: &str,
) -> Result<QuotaOverview, String> {
    let client = state.client();

    // 1) 配额
    let quota_resp: Value = client
        .get("https://bigmodel.cn/api/monitor/usage/quota/limit")
        .header("authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| format!("quota 请求失败: {e}"))?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    // 2) 订阅（拿套餐名）-- 失败则回退到 quota level
    let plan_name = match client
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
        account_label: Some(account_label),
        plan_name,
        buckets,
        fetched_at: chrono::Utc::now().to_rfc3339(),
        error: None,
    })
}

/// 查询 Coding Plan 配额（全局：自动选 db 标记的 Coding Plan provider）
pub async fn fetch_coding_plan(state: &AppState) -> Result<QuotaOverview, String> {
    let (api_key, provider_name) = current_provider_creds(&state.db)?;
    fetch_bigmodel_quota(state, &api_key, &provider_name).await
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

    let url = tmpl
        .url
        .unwrap_or_default()
        .replace("{{apiKey}}", &api_key)
        .replace("{{baseURL}}", &base);
    let method = tmpl.method.unwrap_or_else(|| "GET".to_string());
    let client = state.client();

    let mut req = if method.eq_ignore_ascii_case("POST") {
        client
            .post(&url)
            .body(tmpl.body.clone().unwrap_or_default())
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
                            .replace("{{baseURL}}", &base),
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

    let total = tmpl
        .total_path
        .as_deref()
        .and_then(|p| extract_by_path(&resp, p));
    let used = tmpl
        .used_path
        .as_deref()
        .and_then(|p| extract_by_path(&resp, p));
    let remaining = tmpl
        .remaining_path
        .as_deref()
        .and_then(|p| extract_by_path(&resp, p));

    let total = total.unwrap_or(0.0);
    let used = used.unwrap_or(0.0);
    let remaining = remaining.unwrap_or_else(|| if total > used { total - used } else { 0.0 });

    Ok(QuotaOverview {
        source: "template".into(),
        account_label: None,
        plan_name: tmpl.name.clone(),
        buckets: vec![QuotaBucket {
            name: tmpl.name.unwrap_or_else(|| "配额".into()),
            total,
            used,
            remaining,
            unit: None,
            period_end: None,
        }],
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

/// 按供应商查询配额（统一入口）：
/// 智谱 BigModel 系列 → 内置 bigmodel 配额接口；其余 → 用量查询模板（需用户配置）
#[tauri::command]
pub async fn get_provider_quota(
    state: State<'_, AppState>,
    provider_key: String,
) -> Result<QuotaOverview, String> {
    let config = config_file::read_config().map_err(|e| e.to_string())?;
    let base = config_file::provider_base_url(&config, &provider_key).unwrap_or_default();
    let name = config
        .get("provider")
        .and_then(|p| p.get(&provider_key))
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or(&provider_key)
        .to_string();
    // 智谱 BigModel：走内置配额接口
    if base.to_ascii_lowercase().contains("bigmodel") {
        let api_key = config_file::provider_api_key(&config, &provider_key)
            .ok_or_else(|| "该供应商无 apiKey".to_string())?;
        return fetch_bigmodel_quota(state.inner(), &api_key, &name).await;
    }
    // 其余：用量查询模板（未配置则报错，前端显示「未配置用量查询」）
    run_template_quota(state.inner(), &provider_key).await
}
