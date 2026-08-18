//! Token Plan / Coding Plan 额度查询（对齐 cc-switch src-tauri/src/services/coding_plan.rs）
//!
//! 按供应商 baseURL 自动识别 6 家 Token Plan 供应商，自动使用该供应商的
//! API Key（+ Base URL）查询额度，无需手配模板：
//!   Kimi For Coding / Zhipu GLM（个人·团队）/ MiniMax / ZenMux / 火山方舟（Volcengine）
//! 其中智谱团队版需组织/项目 ID、火山方舟需账号级 AK/SK，
//! 由用户在「模型管理 → 用量查询模板」填写，存于模板 extra_json。
use crate::commands::quota_cmd::{QuotaBucket, QuotaOverview};
use crate::state::AppState;
use crate::zcode::config_file;
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodingPlanProvider {
    Kimi,
    ZhipuCn,
    ZhipuEn,
    MiniMaxCn,
    MiniMaxEn,
    ZenMux,
    Volcengine,
}

/// 按 baseURL 识别 Token Plan 供应商（子串匹配口径与 cc-switch detect_provider 一致）
pub fn detect(base_url: &str) -> Option<CodingPlanProvider> {
    let url = base_url.to_lowercase();
    if url.contains("api.kimi.com/coding") {
        Some(CodingPlanProvider::Kimi)
    } else if url.contains("bigmodel.cn") {
        Some(CodingPlanProvider::ZhipuCn)
    } else if url.contains("api.z.ai") {
        Some(CodingPlanProvider::ZhipuEn)
    } else if url.contains("api.minimaxi.com") {
        Some(CodingPlanProvider::MiniMaxCn)
    } else if url.contains("api.minimax.io") {
        Some(CodingPlanProvider::MiniMaxEn)
    } else if url.contains("zenmux") {
        Some(CodingPlanProvider::ZenMux)
    } else if url.contains("volces.com/api/coding") {
        Some(CodingPlanProvider::Volcengine)
    } else {
        None
    }
}

/// 读取模板 extra_json 附加凭据（对象形式）
fn template_extra(state: &AppState, provider_key: &str) -> Option<serde_json::Map<String, Value>> {
    let t = state.db.get_template(provider_key).ok()??;
    let j = t.extra_json?;
    serde_json::from_str::<Value>(&j).ok()?.as_object().cloned()
}

fn extra_str(m: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    m.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(String::from)
}

/// 统一入口：按识别结果分发查询（自动使用供应商 apiKey）
pub async fn fetch_quota(
    state: &AppState,
    provider_key: &str,
    cp: CodingPlanProvider,
) -> Result<QuotaOverview, String> {
    let config = config_file::read_config().map_err(|e| e.to_string())?;
    let api_key = config_file::provider_api_key(&config, provider_key)
        .filter(|k| !k.is_empty() && k != "<REDACTED>")
        .ok_or_else(|| "该供应商未配置 apiKey".to_string())?;
    let base = config_file::provider_base_url(&config, provider_key).unwrap_or_default();
    let name = config
        .get("provider")
        .and_then(|p| p.get(provider_key))
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or(provider_key)
        .to_string();

    match cp {
        CodingPlanProvider::Kimi => query_kimi(state, &api_key, &name).await,
        CodingPlanProvider::ZhipuCn | CodingPlanProvider::ZhipuEn => {
            // 智谱团队版：模板填了组织/项目 ID 即按团队接口（?type=2 + 专属头）查询
            let extra = template_extra(state, provider_key);
            let team = extra.as_ref().and_then(|m| {
                let org = extra_str(m, "organizationId")?;
                let project = extra_str(m, "projectId")?;
                Some((org, project))
            });
            let root = if cp == CodingPlanProvider::ZhipuEn {
                "https://api.z.ai"
            } else {
                "https://bigmodel.cn"
            };
            let team = team.as_ref().map(|(o, p)| (o.as_str(), p.as_str()));
            crate::commands::quota_cmd::fetch_zhipu_quota(state, &api_key, &name, root, team).await
        }
        CodingPlanProvider::MiniMaxCn | CodingPlanProvider::MiniMaxEn => {
            query_minimax(state, cp == CodingPlanProvider::MiniMaxEn, &api_key, &name).await
        }
        CodingPlanProvider::ZenMux => query_zenmux(state, &base, &api_key, &name).await,
        CodingPlanProvider::Volcengine => {
            let extra = template_extra(state, provider_key);
            let (ak, sk) = extra
                .as_ref()
                .map(|m| (extra_str(m, "accessKeyId"), extra_str(m, "secretAccessKey")))
                .unwrap_or((None, None));
            match (ak, sk) {
                (Some(ak), Some(sk)) => query_volcengine(state, &base, &ak, &sk, &name).await,
                _ => Err(
                    "火山方舟额度查询需账号级 AccessKey（非模型 API Key）：请在 模型管理 → 该供应商 → 用量查询模板 填写 AccessKeyId / SecretAccessKey 并保存"
                        .into(),
                ),
            }
        }
    }
}

// ===== 通用小工具 =====

/// 数值或数字字符串 → f64（cc-switch parse_f64 口径）
fn num_or_str(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

fn ms_to_iso(ms: i64) -> Option<String> {
    if ms <= 0 {
        None
    } else {
        chrono::DateTime::from_timestamp_millis(ms).map(|d| d.to_rfc3339())
    }
}

/// 重置时间兼容解析：数值按秒/毫秒自动判断（>=1e12 视为毫秒），字符串原样返回
fn extract_reset_time(v: Option<&Value>) -> Option<String> {
    let v = v?;
    let n = v
        .as_i64()
        .or_else(|| v.as_f64().map(|f| f as i64))
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()));
    if let Some(n) = n {
        return if n <= 0 {
            None
        } else if n >= 1_000_000_000_000 {
            ms_to_iso(n)
        } else {
            ms_to_iso(n * 1000)
        };
    }
    v.as_str().filter(|s| !s.is_empty()).map(String::from)
}

/// 百分比桶（总量 100，已用 pct%）
fn pct_bucket(name: &str, pct: f64, period_end: Option<String>) -> QuotaBucket {
    QuotaBucket {
        name: name.to_string(),
        total: 100.0,
        used: pct,
        remaining: (100.0 - pct).max(0.0),
        unit: Some("%".into()),
        period_end,
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ===== Kimi For Coding =====

/// GET api.kimi.com/coding/v1/usages：limits[]（5 小时窗）+ usage（每周窗），绝对 token 数
async fn query_kimi(state: &AppState, api_key: &str, name: &str) -> Result<QuotaOverview, String> {
    let resp: Value = state
        .client()
        .get("https://api.kimi.com/coding/v1/usages")
        .bearer_auth(api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| format!("Kimi 用量请求失败: {e}"))?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let mut buckets: Vec<QuotaBucket> = Vec::new();
    let push = |label: &str, obj: &Value| {
        let limit = obj.get("limit").and_then(num_or_str)?;
        let remaining = obj.get("remaining").and_then(num_or_str).unwrap_or(0.0);
        if limit <= 0.0 {
            return None;
        }
        Some(QuotaBucket {
            name: label.to_string(),
            total: limit,
            used: (limit - remaining).max(0.0),
            remaining: remaining.max(0.0),
            unit: None,
            period_end: extract_reset_time(obj.get("resetTime")),
        })
    };
    if let Some(limits) = resp.get("limits").and_then(|v| v.as_array()) {
        for l in limits {
            if let Some(d) = l.get("detail") {
                if let Some(b) = push("每5小时使用额度", d) {
                    buckets.push(b);
                }
            }
        }
    }
    if let Some(u) = resp.get("usage") {
        if let Some(b) = push("每周使用额度", u) {
            buckets.push(b);
        }
    }
    if buckets.is_empty() {
        return Err("Kimi 用量响应无可解析字段".into());
    }
    Ok(QuotaOverview {
        source: "coding-plan".into(),
        provider_name: Some(name.to_string()),
        account_label: None,
        plan_name: Some("Kimi For Coding".into()),
        buckets,
        fetched_at: now_rfc3339(),
        error: None,
    })
}

// ===== MiniMax =====

/// GET /v1/api/openplatform/coding_plan/remains：model_remains.general 的剩余百分比（5 小时 + 每周）
async fn query_minimax(
    state: &AppState,
    en: bool,
    api_key: &str,
    name: &str,
) -> Result<QuotaOverview, String> {
    let root = if en {
        "https://api.minimax.io"
    } else {
        "https://api.minimaxi.com"
    };
    let resp: Value = state
        .client()
        .get(format!("{root}/v1/api/openplatform/coding_plan/remains"))
        .bearer_auth(api_key)
        .header("Content-Type", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| format!("MiniMax 用量请求失败: {e}"))?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    if let Some(code) = resp
        .get("base_resp")
        .and_then(|b| b.get("status_code"))
        .and_then(|v| v.as_i64())
    {
        if code != 0 {
            let msg = resp
                .get("base_resp")
                .and_then(|b| b.get("status_msg"))
                .and_then(|v| v.as_str())
                .unwrap_or("未知错误");
            return Err(format!("MiniMax 用量查询失败（{code}）: {msg}"));
        }
    }

    let general = resp
        .get("model_remains")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|m| m.get("model_name").and_then(|v| v.as_str()) == Some("general"))
        })
        .ok_or_else(|| "MiniMax 响应无 general 套餐用量".to_string())?;

    let mut buckets: Vec<QuotaBucket> = Vec::new();
    if let Some(remain) = general
        .get("current_interval_remaining_percent")
        .and_then(|v| v.as_f64())
    {
        buckets.push(pct_bucket(
            "每5小时使用额度",
            100.0 - remain,
            general
                .get("end_time")
                .and_then(|v| v.as_i64())
                .and_then(ms_to_iso),
        ));
    }
    // weekly_status==1 才有每周限额；2/3 表示无每周限制（剩余恒为 100），跳过
    if general.get("current_weekly_status").and_then(|v| v.as_i64()) == Some(1) {
        if let Some(remain) = general
            .get("current_weekly_remaining_percent")
            .and_then(|v| v.as_f64())
        {
            buckets.push(pct_bucket(
                "每周使用额度",
                100.0 - remain,
                general
                    .get("weekly_end_time")
                    .and_then(|v| v.as_i64())
                    .and_then(ms_to_iso),
            ));
        }
    }
    if buckets.is_empty() {
        return Err("MiniMax 响应无可解析额度字段".into());
    }
    Ok(QuotaOverview {
        source: "coding-plan".into(),
        provider_name: Some(name.to_string()),
        account_label: None,
        plan_name: Some("MiniMax Coding Plan".into()),
        buckets,
        fetched_at: now_rfc3339(),
        error: None,
    })
}

// ===== ZenMux =====

/// GET 供应商 baseURL：data.quota_5_hour / quota_7_day（USD 用量 + 百分比）
async fn query_zenmux(
    state: &AppState,
    base: &str,
    api_key: &str,
    name: &str,
) -> Result<QuotaOverview, String> {
    if base.is_empty() {
        return Err("ZenMux 供应商未配置 Base URL".into());
    }
    let resp: Value = state
        .client()
        .get(base)
        .bearer_auth(api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| format!("ZenMux 用量请求失败: {e}"))?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    if resp.get("success").and_then(|v| v.as_bool()) != Some(true) {
        let msg = resp
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown error");
        return Err(format!("ZenMux 用量查询失败: {msg}"));
    }
    let data = resp
        .get("data")
        .ok_or_else(|| "ZenMux 响应无 data".to_string())?;

    let bucket = |label: &str, q: &Value| -> Option<QuotaBucket> {
        let resets = q.get("resets_at").and_then(|v| v.as_str()).map(String::from);
        // 优先 USD 绝对值，缺 max 时回退 usage_percentage（0-1 小数 → 百分比）
        if let Some(total) = q.get("max_value_usd").and_then(num_or_str).filter(|m| *m > 0.0) {
            let used = q.get("used_value_usd").and_then(num_or_str).unwrap_or(0.0);
            return Some(QuotaBucket {
                name: label.to_string(),
                total,
                used,
                remaining: (total - used).max(0.0),
                unit: Some("USD".into()),
                period_end: resets,
            });
        }
        let pct = q.get("usage_percentage").and_then(num_or_str).map(|p| p * 100.0)?;
        Some(pct_bucket(label, pct, resets))
    };
    let mut buckets: Vec<QuotaBucket> = Vec::new();
    if let Some(q) = data.get("quota_5_hour") {
        if let Some(b) = bucket("每5小时使用额度", q) {
            buckets.push(b);
        }
    }
    if let Some(q) = data.get("quota_7_day") {
        if let Some(b) = bucket("每周使用额度", q) {
            buckets.push(b);
        }
    }
    if buckets.is_empty() {
        return Err("ZenMux 响应无可解析额度字段".into());
    }

    let plan_name = match (
        data.get("plan").and_then(|p| p.get("tier")).and_then(|v| v.as_str()),
        data.get("account_status").and_then(|v| v.as_str()),
    ) {
        (Some(tier), Some(status)) => Some(format!("{tier} ({status})")),
        (Some(tier), None) => Some(tier.to_string()),
        _ => None,
    };
    Ok(QuotaOverview {
        source: "coding-plan".into(),
        provider_name: Some(name.to_string()),
        account_label: None,
        plan_name,
        buckets,
        fetched_at: now_rfc3339(),
        error: None,
    })
}

// ===== 火山方舟（Volcengine）=====
// 控制面 OpenAPI（open.volcengineapi.com），Volcengine 签名 V4（HMAC-SHA256，service=ark），
// 账号级 AK/SK 签名；先 GetAFPUsage（Agent Plan 绝对值），无结果再 GetCodingPlanUsage（百分比）。

const VOLC_HOST: &str = "open.volcengineapi.com";
const VOLC_SERVICE: &str = "ark";
const VOLC_VERSION: &str = "2024-01-01";
// 火山要求固定顺序（非字典序）
const VOLC_SIGNED_HEADERS: &str = "host;x-date;x-content-sha256;content-type";

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn hmac_sha256(key: &[u8], data: &str) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC-SHA256 支持任意长度密钥");
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// 从 baseURL host 段推导 region（cn-* / ap-*），默认 cn-beijing
fn volc_region(base: &str) -> String {
    let host = base.split("://").nth(1).unwrap_or(base);
    let host = host.split('/').next().unwrap_or("");
    host.split('.')
        .find(|s| s.starts_with("cn-") || s.starts_with("ap-"))
        .unwrap_or("cn-beijing")
        .to_string()
}

/// 签名并发起控制面 POST（空 body），返回 JSON（已检查错误包络）
async fn volc_signed_post(
    state: &AppState,
    ak: &str,
    sk: &str,
    region: &str,
    action: &str,
) -> Result<Value, String> {
    let now = chrono::Utc::now();
    let x_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let short_date = now.format("%Y%m%d").to_string();
    let payload_hash = to_hex(&Sha256::digest(b""));
    // Action < Region < Version 已按字典序
    let query = format!("Action={action}&Region={region}&Version={VOLC_VERSION}");
    let canonical_request = format!(
        "POST\n/\n{query}\nhost:{VOLC_HOST}\nx-date:{x_date}\nx-content-sha256:{payload_hash}\ncontent-type:application/json; charset=utf-8\n\n{VOLC_SIGNED_HEADERS}\n{payload_hash}"
    );
    let scope = format!("{short_date}/{region}/{VOLC_SERVICE}/request");
    let string_to_sign = format!(
        "HMAC-SHA256\n{x_date}\n{scope}\n{}",
        to_hex(&Sha256::digest(canonical_request.as_bytes()))
    );
    let k_date = hmac_sha256(sk.as_bytes(), &short_date);
    let k_region = hmac_sha256(&k_date, region);
    let k_service = hmac_sha256(&k_region, VOLC_SERVICE);
    let k_signing = hmac_sha256(&k_service, "request");
    let signature = to_hex(&hmac_sha256(&k_signing, &string_to_sign));
    let authorization = format!(
        "HMAC-SHA256 Credential={ak}/{scope}, SignedHeaders={VOLC_SIGNED_HEADERS}, Signature={signature}"
    );

    let resp: Value = state
        .client()
        .post(format!("https://{VOLC_HOST}/?{query}"))
        .header("X-Date", &x_date)
        .header("X-Content-Sha256", &payload_hash)
        .header("Content-Type", "application/json; charset=utf-8")
        .header("Authorization", authorization)
        .body(String::new())
        .send()
        .await
        .map_err(|e| format!("{action} 请求失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("{action} 请求失败: {e}"))?
        .json()
        .await
        .map_err(|e| format!("{action} 响应解析失败: {e}"))?;

    let err = resp
        .get("ResponseMetadata")
        .and_then(|m| m.get("Error"))
        .or_else(|| resp.get("Error"));
    if let Some(err) = err {
        let code = err.get("Code").and_then(|v| v.as_str()).unwrap_or("");
        let msg = err.get("Message").and_then(|v| v.as_str()).unwrap_or("未知错误");
        return Err(format!("{action} 失败（{code}）: {msg}"));
    }
    Ok(resp)
}

/// 火山额度桶级别名 → 显示名
fn volc_level_name(level: &str) -> String {
    let l = level.to_lowercase();
    if l.contains("session") || l.contains("five") || l.contains("5") {
        "每5小时使用额度".to_string()
    } else if l.contains("week") {
        "每周使用额度".to_string()
    } else if l.contains("month") {
        "每月使用额度".to_string()
    } else {
        level.to_string()
    }
}

async fn query_volcengine(
    state: &AppState,
    base: &str,
    ak: &str,
    sk: &str,
    name: &str,
) -> Result<QuotaOverview, String> {
    let region = volc_region(base);

    // 1) Agent Plan（绝对值：Quota/Used）
    let mut plan_name: Option<String> = None;
    let mut buckets: Vec<QuotaBucket> = Vec::new();
    if let Ok(afp) = volc_signed_post(state, ak, sk, &region, "GetAFPUsage").await {
        if let Some(result) = afp.get("Result") {
            if let Some(t) = result.get("PlanType").and_then(|v| v.as_str()) {
                plan_name = Some(format!("Agent Plan {t}"));
            }
            for (key, label) in [
                ("AFPFiveHour", "每5小时使用额度"),
                ("AFPWeekly", "每周使用额度"),
                ("AFPMonthly", "每月使用额度"),
            ] {
                let q = match result.get(key) {
                    Some(q) => q,
                    None => continue,
                };
                let quota = match q.get("Quota").and_then(num_or_str) {
                    Some(v) if v > 0.0 => v,
                    _ => continue,
                };
                let used = q.get("Used").and_then(num_or_str).unwrap_or(0.0);
                buckets.push(QuotaBucket {
                    name: label.to_string(),
                    total: quota,
                    used,
                    remaining: (quota - used).max(0.0),
                    unit: None,
                    period_end: extract_reset_time(q.get("ResetTime")),
                });
            }
        }
    }

    // 2) 无 Agent Plan 数据 → Coding Plan（百分比）
    if buckets.is_empty() {
        let cp = volc_signed_post(state, ak, sk, &region, "GetCodingPlanUsage").await?;
        let result = cp
            .get("Result")
            .ok_or_else(|| "GetCodingPlanUsage 响应无 Result".to_string())?;
        let arr = ["QuotaUsage", "Usages", "Details"]
            .iter()
            .find_map(|k| result.get(k).and_then(|v| v.as_array()))
            .ok_or_else(|| "火山响应无额度数组".to_string())?;
        for item in arr {
            let level = ["Level", "Type", "Period", "Label", "Window"]
                .iter()
                .find_map(|k| item.get(k).and_then(|v| v.as_str()));
            let pct = ["Percent", "UsedPercent", "UsagePercent"]
                .iter()
                .find_map(|k| item.get(k).and_then(num_or_str));
            let (Some(level), Some(pct)) = (level, pct) else {
                continue;
            };
            let reset = ["ResetTime", "ResetTimestamp"]
                .iter()
                .find_map(|k| item.get(k))
                .and_then(|v| extract_reset_time(Some(v)));
            buckets.push(pct_bucket(&volc_level_name(level), pct, reset));
        }
    }

    if buckets.is_empty() {
        return Err("火山方舟未返回可用额度数据（可能未订阅 Coding / Agent Plan）".into());
    }
    Ok(QuotaOverview {
        source: "coding-plan".into(),
        provider_name: Some(name.to_string()),
        account_label: None,
        plan_name,
        buckets,
        fetched_at: now_rfc3339(),
        error: None,
    })
}
