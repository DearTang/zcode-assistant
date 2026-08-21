//! 从其他 AI 工具的配置导入 provider 到 zcode
//! 支持：opencode (opencode.json)、Claude Code (settings.json)、Codex (config.toml)
//! qwen-code 用 OAuth 无 apiKey 可导入；ccswitch 本机未装也暂不支持。
//!
//! 「重新获取上下文」（refetch_context）开启时：目录/内置表命中的模型覆盖为真实
//! 上下文；未命中的由前端弹窗让用户逐个确认（context_overrides 携带确认值，
//! 缺失时 200k 兜底）。开关关闭时不写任何上下文（保留已有配置）。
use crate::commands::models_cmd::{add_provider, apply_models, matched_spec, update_provider, ModelSpec};
use crate::state::AppState;
use crate::zcode::config_file;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::State;

/// 未匹配目录且用户未给确认值时的兜底上下文（与弹窗默认值一致）
const IMPORT_FALLBACK_CONTEXT: i64 = 200_000;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub models: Vec<String>,
    pub source: String,
    /// "success" | "updated" | "duplicate" | "failed"
    pub status: String,
    /// success: 新 provider key；duplicate: 命中的已有 key；failed: 空
    pub provider_key: String,
    pub message: String,
}

struct ParsedProvider {
    id: String,
    name: String,
    kind: String,
    base_url: String,
    api_key: String,
    models: Vec<String>,
}

/// 预览：解析出的待导入 provider（不写任何文件），供前端弹窗勾选
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreview {
    /// 解析出的条目 id（同一次解析内唯一，作为勾选 / 导入过滤的标识）
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub models: Vec<String>,
    pub has_api_key: bool,
    /// 命中的已有 provider key（导入时将覆盖更新该条）
    pub duplicate_of: Option<String>,
}

/// 把任意字符串转成合法 slug（小写字母数字，分隔符 -；空则 "provider"）
fn slugify(s: &str) -> String {
    let s: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = s.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "provider".to_string()
    } else {
        trimmed
    }
}

fn resolve_path(source: &str, path: Option<&str>) -> Result<PathBuf, String> {
    if let Some(p) = path {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let home = dirs::home_dir().ok_or("无 HOME")?;
    Ok(match source {
        "opencode" => home.join(".config").join("opencode").join("opencode.json"),
        "claude" => home.join(".claude").join("settings.json"),
        "codex" => home.join(".codex").join("config.toml"),
        "zcode" => home.join(".zcode").join("v2").join("config.json"),
        _ => return Err(format!("不支持的来源或需手动指定路径: {source}")),
    })
}

// ===== opencode / zcode 通用：解析单个 provider 条目 =====
fn parse_provider_entry(id: &str, p: &Value) -> Option<ParsedProvider> {
    let name = p
        .get("name")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| id.to_string());
    let opts = p.get("options");
    let base = opts
        .and_then(|o| o.get("baseURL"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let key = opts
        .and_then(|o| o.get("apiKey"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let models: Vec<String> = p
        .get("models")
        .and_then(|m| m.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();
    if base.is_empty() {
        return None;
    }
    let kind = if base.contains("/anthropic") || base.contains("anthropic.") {
        "anthropic"
    } else {
        "openai-compatible"
    };
    Some(ParsedProvider {
        id: id.to_string(),
        name,
        kind: kind.to_string(),
        base_url: base,
        api_key: key,
        models,
    })
}

// ===== opencode =====
fn parse_opencode(path: &Path) -> Result<Vec<ParsedProvider>, String> {
    let txt = std::fs::read_to_string(path).map_err(|e| format!("读取失败: {e}"))?;
    let v: Value = serde_json::from_str(&txt).map_err(|e| format!("JSON 解析失败: {e}"))?;
    let providers = v
        .get("provider")
        .and_then(|p| p.as_object())
        .ok_or("无 provider 字段")?;
    let out = providers
        .iter()
        .filter_map(|(id, p)| parse_provider_entry(id, p))
        .collect();
    Ok(out)
}

// ===== zcode config.json（同构于 opencode，跳过 builtin: 智谱账号）=====
fn parse_zcode(path: &Path) -> Result<Vec<ParsedProvider>, String> {
    let txt = std::fs::read_to_string(path).map_err(|e| format!("读取失败: {e}"))?;
    let v: Value = serde_json::from_str(&txt).map_err(|e| format!("JSON 解析失败: {e}"))?;
    let providers = v
        .get("provider")
        .and_then(|p| p.as_object())
        .ok_or("无 provider 字段")?;
    let out = providers
        .iter()
        .filter(|(id, _)| !id.starts_with("builtin:"))
        .filter_map(|(id, p)| parse_provider_entry(id, p))
        .collect();
    Ok(out)
}

// ===== Claude Code =====
fn parse_claude(path: &Path) -> Result<Vec<ParsedProvider>, String> {
    let txt = std::fs::read_to_string(path).map_err(|e| format!("读取失败: {e}"))?;
    let v: Value = serde_json::from_str(&txt).map_err(|e| format!("JSON 解析失败: {e}"))?;
    let env = v
        .get("env")
        .and_then(|e| e.as_object())
        .ok_or("settings.json 无 env")?;
    let base = env
        .get("ANTHROPIC_BASE_URL")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let key = env
        .get("ANTHROPIC_AUTH_TOKEN")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    if base.is_empty() && key.is_empty() {
        return Err("未找到 ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN".into());
    }
    let mut models = Vec::new();
    for k in [
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    ] {
        if let Some(v) = env.get(k).and_then(|x| x.as_str()) {
            if !v.is_empty() && !models.contains(&v.to_string()) {
                models.push(v.to_string());
            }
        }
    }
    let host = if base.contains("://") {
        base.splitn(2, "://")
            .nth(1)
            .unwrap_or(&base)
            .split('/')
            .next()
            .unwrap_or("")
            .to_string()
    } else {
        base.clone()
    };
    let name = if !host.is_empty() {
        format!("Claude Code ({host})")
    } else {
        "Claude Code".into()
    };
    let id = if host.is_empty() {
        "claude-code".to_string()
    } else {
        format!("claude-{}", slugify(&host))
    };
    Ok(vec![ParsedProvider {
        id,
        name,
        kind: "anthropic".into(),
        base_url: base,
        api_key: key,
        models,
    }])
}

// ===== Codex（简单 TOML 解析）=====
fn parse_codex(path: &Path) -> Result<Vec<ParsedProvider>, String> {
    let txt = std::fs::read_to_string(path).map_err(|e| format!("读取失败: {e}"))?;
    let provider_name = toml_extract(&txt, "model_provider").ok_or("无 model_provider")?;
    let model = toml_extract(&txt, "model").unwrap_or_default();
    let section_marker = format!("[model_providers.{}]", provider_name);
    let after = txt.split(&section_marker).nth(1).ok_or("无 provider section")?;
    let body = after.split("\n[").next().unwrap_or(after);
    let name = toml_extract(body, "name").unwrap_or_else(|| provider_name.clone());
    let base = toml_extract(body, "base_url").ok_or("无 base_url")?;
    let kind = if base.contains("/anthropic") {
        "anthropic"
    } else {
        "openai-compatible"
    };
    let models: Vec<String> = if !model.is_empty() { vec![model] } else { vec![] };
    // Codex 的 apiKey 在 auth.json（OAuth），暂不导入
    let id = format!("codex-{}", slugify(&provider_name));
    Ok(vec![ParsedProvider {
        id,
        name: format!("Codex ({name})"),
        kind: kind.to_string(),
        base_url: base,
        api_key: String::new(),
        models,
    }])
}

fn toml_extract(body: &str, key: &str) -> Option<String> {
    for line in body.lines() {
        let t = line.trim();
        if t.starts_with('#') || t.is_empty() {
            continue;
        }
        if let Some(rest) = t.strip_prefix(key) {
            let rest = rest.trim().trim_start_matches('=').trim();
            let v = rest.trim_matches('"').trim_matches('\'');
            return Some(v.to_string());
        }
    }
    None
}

/// 在现有 config 中查找 baseURL + apiKey 完全一致的 provider。
/// 两者都非空且匹配才视为重复（空 key 不参与判定，避免误判）。
fn find_duplicate_provider(config: &Value, base_url: &str, api_key: &str) -> Option<String> {
    let b = base_url.trim();
    let k = api_key.trim();
    if b.is_empty() || k.is_empty() {
        return None;
    }
    let target_base = b.trim_end_matches('/').to_ascii_lowercase();
    let providers = config.get("provider")?.as_object()?;
    for (key, p) in providers {
        let opts = match p.get("options") {
            Some(o) => o,
            None => continue,
        };
        let pb = opts
            .get("baseURL")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim();
        let pk = opts
            .get("apiKey")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim();
        if pk.is_empty() {
            continue;
        }
        let pb_norm = pb.trim_end_matches('/').to_ascii_lowercase();
        if pb_norm == target_base && pk == k {
            return Some(key.clone());
        }
    }
    None
}

/// 弹出系统文件选择器，返回选中的文件绝对路径（用于导入配置）
/// `default_path` 为空或不存在时用各来源的默认目录。
#[tauri::command]
pub fn pick_config_file(source: String, default_path: Option<String>) -> Option<String> {
    let mut dialog = rfd::FileDialog::new()
        .add_filter("配置文件", &["json", "toml"])
        .add_filter("所有文件", &["*"]);
    if let Some(dir) = resolve_default_dir(&source, default_path.as_deref()) {
        if dir.exists() {
            dialog = dialog.set_directory(dir);
        }
    }
    dialog.pick_file().map(|p| p.to_string_lossy().to_string())
}

/// 计算文件选择器的初始目录：优先用传入路径，否则取各来源默认目录。
fn resolve_default_dir(source: &str, path: Option<&str>) -> Option<PathBuf> {
    if let Some(p) = path {
        let p = p.trim();
        if !p.is_empty() {
            let pb = PathBuf::from(p);
            return Some(if pb.is_dir() { pb } else { pb.parent()?.to_path_buf() });
        }
    }
    let home = dirs::home_dir()?;
    Some(match source {
        "opencode" => home.join(".config").join("opencode"),
        "claude" => home.join(".claude"),
        "codex" => home.join(".codex"),
        "zcode" => home.join(".zcode").join("v2"),
        _ => home,
    })
}

/// 按来源解析配置文件（只读不写）
fn parse_source(source: &str, path: &Path) -> Result<Vec<ParsedProvider>, String> {
    match source {
        "opencode" => parse_opencode(path),
        "claude" => parse_claude(path),
        "codex" => parse_codex(path),
        "zcode" => parse_zcode(path),
        _ => Err(format!("不支持的来源: {source}")),
    }
}

/// 解析来源路径（带存在性检查）
fn resolve_existing_path(source: &str, path: Option<&str>) -> Result<PathBuf, String> {
    let path = resolve_path(source, path)?;
    if !path.exists() {
        return Err(format!("配置文件不存在: {}", path.display()));
    }
    Ok(path)
}

/// 构建导入模型规格：
/// - refetch 关闭：不写上下文（apply_models 保留已有配置）
/// - refetch 开启：目录/内置表命中 → 覆盖为真实值（含输出上限）；
///   未命中 → 用户弹窗确认值（context_overrides），无确认值时 200k 兜底
/// 返回 (spec, 是否走了用户确认值/兜底)
fn import_spec(
    catalog: &Option<crate::openrouter::Catalog>,
    id: &str,
    refetch: bool,
    overrides: &HashMap<String, i64>,
) -> (ModelSpec, bool) {
    if !refetch {
        return (
            ModelSpec {
                id: id.to_string(),
                name: None,
                context_length: None,
                max_output: None,
            },
            false,
        );
    }
    if let Some((ctx, out)) = matched_spec(catalog, id) {
        return (
            ModelSpec {
                id: id.to_string(),
                name: None,
                context_length: Some(ctx),
                max_output: out,
            },
            false,
        );
    }
    let ctx = overrides
        .get(id)
        .copied()
        .filter(|c| *c > 0)
        .unwrap_or(IMPORT_FALLBACK_CONTEXT);
    (
        ModelSpec {
            id: id.to_string(),
            name: None,
            context_length: Some(ctx),
            // 未命中目录时输出按默认值兜底（与目录解析口径一致）
            max_output: Some(crate::openrouter::DEFAULT_OUTPUT_LENGTH),
        },
        true,
    )
}

/// 目录匹配的单个模型（真实上下文）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedModel {
    pub id: String,
    pub context: i64,
    /// 输出上限（目录 / 内置命中时为真实值，否则默认 131072）
    pub output: Option<i64>,
}

/// 上下文预解析结果：matched=目录/内置表命中（将按真实值写入），
/// unmatched=未命中（前端弹窗让用户逐个确认，默认 200k）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResolveContextsResult {
    pub matched: Vec<ResolvedModel>,
    pub unmatched: Vec<String>,
}

/// 预解析导入模型的上下文（「重新获取上下文」开启、导入确认前调用）：
/// 只做匹配不写任何文件，供前端弹窗展示未命中清单。
#[tauri::command]
pub fn resolve_import_contexts(
    state: State<'_, AppState>,
    source: String,
    path: Option<String>,
    selected: Option<Vec<String>>,
) -> Result<ResolveContextsResult, String> {
    let p = resolve_existing_path(&source, path.as_deref())?;
    let mut parsed = parse_source(&source, &p)?;
    if let Some(sel) = selected {
        parsed.retain(|prov| sel.iter().any(|id| id == &prov.id));
    }
    let or_catalog = crate::openrouter::load_catalog(&state.db);
    let mut seen = std::collections::HashSet::new();
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();
    for prov in &parsed {
        for m in &prov.models {
            if !seen.insert(m.clone()) {
                continue;
            }
            match matched_spec(&or_catalog, m) {
                Some((ctx, out)) => matched.push(ResolvedModel {
                    id: m.clone(),
                    context: ctx,
                    output: out,
                }),
                None => unmatched.push(m.clone()),
            }
        }
    }
    Ok(ResolveContextsResult { matched, unmatched })
}

/// 预览导入内容：解析来源配置并标记覆盖关系，不执行任何写入。
/// 前端弹窗展示全量条目供勾选，确认后再调 import_providers_from 传入选中 id。
#[tauri::command]
pub fn preview_providers_from(
    source: String,
    path: Option<String>,
) -> Result<Vec<ProviderPreview>, String> {
    let path = resolve_existing_path(&source, path.as_deref())?;
    let parsed = parse_source(&source, &path)?;
    if parsed.is_empty() {
        return Err("未从配置中解析到任何 provider".into());
    }
    // 读取失败不影响预览，只是无法标记覆盖关系
    let config = config_file::read_config().unwrap_or(Value::Null);
    Ok(parsed
        .into_iter()
        .map(|p| {
            let duplicate_of = find_duplicate_provider(&config, &p.base_url, &p.api_key);
            ProviderPreview {
                id: p.id,
                name: p.name,
                kind: p.kind,
                base_url: p.base_url,
                models: p.models,
                has_api_key: !p.api_key.trim().is_empty(),
                duplicate_of,
            }
        })
        .collect())
}

#[tauri::command]
pub fn import_providers_from(
    state: State<'_, AppState>,
    source: String,
    path: Option<String>,
    selected: Option<Vec<String>>,
    refetch_context: Option<bool>,
    context_overrides: Option<HashMap<String, i64>>,
) -> Result<Vec<ImportResult>, String> {
    let path = resolve_existing_path(&source, path.as_deref())?;
    let mut parsed = parse_source(&source, &path)?;
    if parsed.is_empty() {
        return Err("未从配置中解析到任何 provider".into());
    }
    // 只导入选中的条目（按 preview 返回的 id 过滤）；None = 全部（向后兼容）
    if let Some(sel) = selected {
        parsed.retain(|p| sel.iter().any(|id| id == &p.id));
    }
    // 「重新获取上下文」开启时按目录/内置表覆盖真实值，未命中的用弹窗确认值
    let refetch = refetch_context.unwrap_or(false);
    let overrides = context_overrides.unwrap_or_default();
    let or_catalog = crate::openrouter::load_catalog(&state.db);
    let mut results = Vec::new();
    for p in parsed {
        // 先读一份配置快照用于重复检测
        let snapshot = match config_file::read_config() {
            Ok(c) => c,
            Err(e) => {
                results.push(ImportResult {
                    name: p.name.clone(),
                    kind: p.kind.clone(),
                    base_url: p.base_url.clone(),
                    models: p.models.clone(),
                    source: source.clone(),
                    status: "failed".into(),
                    provider_key: String::new(),
                    message: format!("读取配置失败: {e}"),
                });
                continue;
            }
        };
        // 重复：baseURL + apiKey 与已有 provider 完全一致 → 覆盖更新
        // （用户可能在源端改过模型/参数，跳过会导致不同步）
        if let Some(existing) = find_duplicate_provider(&snapshot, &p.base_url, &p.api_key) {
            let existing_name = snapshot
                .get("provider")
                .and_then(|x| x.get(&existing))
                .and_then(|x| x.get("name"))
                .and_then(|x| x.as_str())
                .unwrap_or(&existing)
                .to_string();
            // 更新基础信息（apiKey 为空时 update_provider 内部保留原值）
            if let Err(e) = update_provider(
                existing.clone(),
                Some(p.name.clone()),
                Some(p.kind.clone()),
                Some(p.base_url.clone()),
                Some(p.api_key.clone()),
            ) {
                results.push(ImportResult {
                    name: p.name.clone(),
                    kind: p.kind.clone(),
                    base_url: p.base_url.clone(),
                    models: p.models.clone(),
                    source: source.clone(),
                    status: "failed".into(),
                    provider_key: String::new(),
                    message: format!("覆盖更新失败: {e}"),
                });
                continue;
            }
            // 合并写入源里的模型（保留已有限制，只加/更新）；
            // 「重新获取上下文」开启时：命中的覆盖为真实值，未命中的用确认值
            let mut resolved = 0usize;
            let mut confirmed = 0usize;
            let model_count = if !p.models.is_empty() {
                let mut specs = Vec::with_capacity(p.models.len());
                for m in &p.models {
                    let (s, user_set) = import_spec(&or_catalog, m, refetch, &overrides);
                    if user_set {
                        confirmed += 1;
                    } else if refetch {
                        resolved += 1;
                    }
                    specs.push(s);
                }
                apply_models(existing.clone(), specs).unwrap_or(0)
            } else {
                0
            };
            let ctx_note = if refetch {
                let confirmed_part = if confirmed > 0 {
                    format!("、{confirmed} 个按确认值设置")
                } else {
                    String::new()
                };
                format!("，上下文：{resolved} 个按目录更新{confirmed_part}")
            } else {
                String::new()
            };
            results.push(ImportResult {
                name: p.name.clone(),
                kind: p.kind.clone(),
                base_url: p.base_url.clone(),
                models: p.models.clone(),
                source: source.clone(),
                status: "updated".into(),
                provider_key: existing,
                message: format!("已覆盖「{existing_name}」，合并 {model_count} 个模型{ctx_note}"),
            });
            continue;
        }
        // 生成唯一标识：导入用源标识（slugify 规整）；与现有冲突则加 -2/-3 后缀
        let base_id = {
            let raw = if p.id.is_empty() {
                slugify(&p.name)
            } else {
                slugify(&p.id)
            };
            if raw.is_empty() {
                "provider".to_string()
            } else {
                raw
            }
        };
        let existing_keys: std::collections::HashSet<&str> = snapshot
            .get("provider")
            .and_then(|x| x.as_object())
            .map(|m| m.keys().map(|s| s.as_str()))
            .into_iter()
            .flatten()
            .collect();
        let mut new_id = base_id.clone();
        let mut suffix = 1;
        while existing_keys.contains(new_id.as_str()) {
            suffix += 1;
            new_id = format!("{}-{}", base_id, suffix);
        }
        // 写入
        match add_provider(
            p.name.clone(),
            p.kind.clone(),
            p.base_url.clone(),
            p.api_key.clone(),
            new_id.clone(),
        ) {
            Ok(key) => {
                // 「重新获取上下文」开启时：命中的写真实值，未命中的用确认值
                let mut resolved = 0usize;
                let mut confirmed = 0usize;
                if !p.models.is_empty() {
                    let mut specs = Vec::with_capacity(p.models.len());
                    for m in &p.models {
                        let (s, user_set) = import_spec(&or_catalog, m, refetch, &overrides);
                        if user_set {
                            confirmed += 1;
                        } else if refetch {
                            resolved += 1;
                        }
                        specs.push(s);
                    }
                    let _ = apply_models(key.clone(), specs);
                }
                let ctx_note = if refetch {
                    let confirmed_part = if confirmed > 0 {
                        format!("、{confirmed} 个按确认值设置")
                    } else {
                        String::new()
                    };
                    format!("，上下文：{resolved} 个按目录更新{confirmed_part}")
                } else {
                    String::new()
                };
                results.push(ImportResult {
                    name: p.name.clone(),
                    kind: p.kind.clone(),
                    base_url: p.base_url.clone(),
                    models: p.models.clone(),
                    source: source.clone(),
                    status: "success".into(),
                    provider_key: key,
                    message: format!("已写入 {} 个模型{ctx_note}", p.models.len()),
                });
            }
            Err(e) => {
                results.push(ImportResult {
                    name: p.name.clone(),
                    kind: p.kind.clone(),
                    base_url: p.base_url.clone(),
                    models: p.models.clone(),
                    source: source.clone(),
                    status: "failed".into(),
                    provider_key: String::new(),
                    message: format!("写入失败: {e}"),
                });
            }
        }
    }
    Ok(results)
}