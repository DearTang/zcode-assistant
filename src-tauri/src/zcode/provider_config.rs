//! ZCode 3.14+ 新版 provider 配置（v2/provider_config.json）读写适配层。
//!
//! 3.14 起 ZCode 把用户自定义 provider 从 v2/config.json 迁到 v2/provider_config.json，
//! 结构完全不同：
//!   { schemaVersion: 1,
//!     config: { providerOrder: [id...],
//!               providerConfigRules: { providerRules: [{providerId, providerName, enabled, config}] },
//!               modelConfigRules:    { providerModelRules: [{providerId, modelId, config}],
//!                                      manualProviderModelRules: [...] } } }
//! 旧结构（config.json 的 provider.<id>.{name,kind,options,models}）已不再被 ZCode 读取。
//!
//! 本模块做双向适配，让上层沿用旧的 {"provider": {...}} 形状：
//!   read_as_legacy —— 新结构投影为旧形状
//!   apply_legacy   —— 旧形状的改动合并回新结构并原子写盘（只动用户自定义 provider）

use crate::zcode::paths;
use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub fn personal_path() -> Option<PathBuf> {
    paths::zcode_v2_dir().map(|d| d.join("provider_config.json"))
}

/// 新版配置文件是否存在（存在即走新结构读写，否则回退旧 config.json）
pub fn is_active() -> bool {
    personal_path().map(|p| p.is_file()).unwrap_or(false)
}

fn read_json_file(p: &Path) -> Result<Value> {
    let txt = std::fs::read_to_string(p).with_context(|| format!("读取失败: {}", p.display()))?;
    serde_json::from_str(&txt).context("JSON 解析失败")
}

fn write_json_file(p: &Path, v: &Value) -> Result<()> {
    let txt = serde_json::to_string_pretty(v)?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, format!("{txt}\n"))
        .with_context(|| format!("写入临时文件失败: {}", tmp.display()))?;
    std::fs::rename(&tmp, p).with_context(|| format!("替换文件失败: {}", p.display()))?;
    Ok(())
}

/// ZCode api.type → 旧 kind
fn api_type_to_kind(t: &str) -> &'static str {
    match t {
        "openai-responses" => "openai",
        "openai-chat-completions" => "openai-compatible",
        _ => "anthropic",
    }
}

/// 旧 kind → ZCode api.type
fn kind_to_api_type(kind: &str) -> &'static str {
    match kind {
        "openai" => "openai-responses",
        "openai-compatible" => "openai-chat-completions",
        _ => "anthropic-messages",
    }
}

/// builtin:/account: 前缀归 ZCode 内置配置所有，本适配层只读不写。
/// 注意：投影后订阅 provider 以旧 `builtin:*` id 出现（见 account_id_to_legacy），
/// 因此 `builtin:*` 一律视为不可写。
fn is_managed(provider_id: &str) -> bool {
    !provider_id.starts_with("builtin:") && !provider_id.starts_with("account:")
}

/// inputFormat/outputFormat 布尔表 → 旧 modalities
fn modalities_from_formats(properties: &Value) -> Option<Value> {
    let f = properties.get("inputFormat")?;
    let mut inputs: Vec<&str> = Vec::new();
    for (flag, name) in [
        ("supportsText", "text"),
        ("supportsImage", "image"),
        ("supportsVideo", "video"),
        ("supportsAudio", "audio"),
        ("supportsPdf", "pdf"),
    ] {
        if f.get(flag).and_then(|v| v.as_bool()).unwrap_or(false) {
            inputs.push(name);
        }
    }
    if inputs.is_empty() {
        return None;
    }
    let outputs: Vec<&str> = match properties
        .get("outputFormat")
        .and_then(|o| o.get("supportsText"))
        .and_then(|v| v.as_bool())
    {
        Some(false) => Vec::new(),
        _ => vec!["text"],
    };
    Some(json!({ "input": inputs, "output": outputs }))
}

/// 单条模型规则（新结构）→ 旧 model 对象
fn project_model_rule(rule: &Value) -> Value {
    let empty = json!({});
    let cfg = rule.get("config").unwrap_or(&empty);
    let mut out = Map::new();
    if let Some(b) = cfg.get("enabled").and_then(|v| v.as_bool()) {
        out.insert("enabled".into(), json!(b));
    }
    let props = cfg.get("properties");
    let mut limit = Map::new();
    if let Some(c) = props
        .and_then(|p| p.get("contextWindow"))
        .and_then(|v| v.as_i64())
    {
        limit.insert("context".into(), json!(c));
    }
    if let Some(o) = cfg
        .get("optionSpecs")
        .and_then(|s| s.get("maxOutputTokens"))
        .and_then(|m| m.get("max"))
        .and_then(|v| v.as_i64())
    {
        limit.insert("output".into(), json!(o));
    }
    if !limit.is_empty() {
        out.insert("limit".into(), Value::Object(limit));
    }
    if let Some(p) = props {
        if let Some(m) = modalities_from_formats(p) {
            out.insert("modalities".into(), m);
        }
    }
    Value::Object(out)
}

/// provider 的模型 id 顺序：modelOrder 优先，其后补 personalModelIds / builtinModelIds 中未列出的
fn model_ids(cfg: &Value) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for key in ["modelOrder", "personalModelIds", "builtinModelIds"] {
        if let Some(arr) = cfg.get(key).and_then(|v| v.as_array()) {
            for v in arr {
                if let Some(s) = v.as_str() {
                    if !s.is_empty() && !ids.iter().any(|x| x == s) {
                        ids.push(s.to_string());
                    }
                }
            }
        }
    }
    ids
}

/// ZCode 自己的旧→新 id 映射（app.asar `Un` 表）：旧版订阅 provider 用 `builtin:*`，
/// 3.14+ 改叫 `account:*`（另有 zai/bigmodel 裸 `-api` 模板 id）。
/// 投影时把 `account:*` 还原为旧 `builtin:*`，让上层的订阅识别逻辑继续生效。
const ACCOUNT_TO_BUILTIN: &[(&str, &str)] = &[
    ("account:bigmodel-start-plan", "builtin:bigmodel-start-plan"),
    ("account:zai-start-plan", "builtin:zai-start-plan"),
    (
        "account:bigmodel-individual-coding-plan",
        "builtin:bigmodel-coding-plan",
    ),
    (
        "account:zai-individual-coding-plan",
        "builtin:zai-coding-plan",
    ),
    ("account:bigmodel-team-coding-plan", "builtin:bigmodel-team-plan"),
    ("account:zai-team-coding-plan", "builtin:zai-team-plan"),
    (
        "account:bigmodel-offpeak-idle-plan",
        "builtin:bigmodel-offpeak-plan",
    ),
    (
        "account:zai-offpeak-idle-plan",
        "builtin:zai-offpeak-plan",
    ),
];

/// 新 id → 旧 id（非订阅 id 原样返回）
pub fn account_id_to_legacy(pid: &str) -> String {
    ACCOUNT_TO_BUILTIN
        .iter()
        .find(|(new, _)| *new == pid)
        .map(|(_, old)| (*old).to_string())
        .unwrap_or_else(|| pid.to_string())
}

/// 读取新结构并投影为旧 {"provider": {...}}
pub fn read_as_legacy() -> Result<Value> {
    let p = personal_path().context("未定位 ~/.zcode/v2/provider_config.json")?;
    read_as_legacy_at(&p)
}

pub(crate) fn read_as_legacy_at(p: &Path) -> Result<Value> {
    let doc = read_json_file(p)?;
    let empty = json!({});
    let cfg = doc.get("config").unwrap_or(&empty);

    let rules: Vec<Value> = cfg
        .get("providerConfigRules")
        .and_then(|r| r.get("providerRules"))
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    // (providerId, modelId) → 规则；manual 规则后写覆盖 provider 规则
    let mut model_rules: Map<String, Value> = Map::new();
    for list in ["providerModelRules", "manualProviderModelRules"] {
        if let Some(arr) = cfg
            .get("modelConfigRules")
            .and_then(|m| m.get(list))
            .and_then(|v| v.as_array())
        {
            for r in arr {
                let pid = r.get("providerId").and_then(|v| v.as_str()).unwrap_or("");
                let mid = r.get("modelId").and_then(|v| v.as_str()).unwrap_or("");
                if pid.is_empty() || mid.is_empty() {
                    continue;
                }
                model_rules.insert(format!("{pid}\u{1}{mid}"), r.clone());
            }
        }
    }

    let by_id: Map<String, Value> = rules
        .iter()
        .filter_map(|r| {
            r.get("providerId")
                .and_then(|v| v.as_str())
                .map(|k| (k.to_string(), r.clone()))
        })
        .collect();

    // providerOrder 优先，其余按文件顺序补后
    let mut seq: Vec<String> = Vec::new();
    if let Some(arr) = cfg.get("providerOrder").and_then(|v| v.as_array()) {
        for v in arr {
            if let Some(s) = v.as_str() {
                if by_id.contains_key(s) && !seq.iter().any(|x| x == s) {
                    seq.push(s.to_string());
                }
            }
        }
    }
    for k in by_id.keys() {
        if !seq.iter().any(|x| x == k) {
            seq.push(k.clone());
        }
    }

    let mut providers = Map::new();
    for pid in seq {
        let rule = &by_id[&pid];
        let pcfg = rule.get("config").unwrap_or(&empty);
        let mut prov = Map::new();
        prov.insert(
            "name".into(),
            json!(rule
                .get("providerName")
                .and_then(|v| v.as_str())
                .unwrap_or(&pid)),
        );
        if let Some(b) = rule.get("enabled").and_then(|v| v.as_bool()) {
            prov.insert("enabled".into(), json!(b));
        }
        let api_type = pcfg
            .get("api")
            .and_then(|a| a.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("anthropic-messages");
        prov.insert("kind".into(), json!(api_type_to_kind(api_type)));

        let mut opts = Map::new();
        if let Some(k) = pcfg
            .get("access")
            .and_then(|a| a.get("apiKey"))
            .and_then(|v| v.as_str())
        {
            opts.insert("apiKey".into(), json!(k));
        }
        if let Some(u) = pcfg
            .get("api")
            .and_then(|a| a.get("baseUrl"))
            .and_then(|v| v.as_str())
        {
            opts.insert("baseURL".into(), json!(u));
        }
        opts.insert("apiKeyRequired".into(), json!(true));
        prov.insert("options".into(), Value::Object(opts));
        prov.insert("source".into(), json!("custom"));

        let mut models = Map::new();
        for mid in model_ids(pcfg) {
            let key = format!("{pid}\u{1}{mid}");
            models.insert(
                mid,
                match model_rules.get(&key) {
                    Some(r) => project_model_rule(r),
                    None => json!({}),
                },
            );
        }
        prov.insert("models".into(), Value::Object(models));
        // 订阅 provider 还原为旧 builtin:* id，让上层既有的订阅识别逻辑继续生效
        providers.insert(account_id_to_legacy(&pid), Value::Object(prov));
    }

    Ok(json!({ "provider": Value::Object(providers) }))
}

/// 按 path 逐层写入嵌套对象（缺失的中间层自动创建）
fn set_nested(obj: &mut Map<String, Value>, path: &[&str], v: Value) {
    if path.len() == 1 {
        obj.insert(path[0].to_string(), v);
        return;
    }
    let child = obj
        .entry(path[0].to_string())
        .or_insert_with(|| json!({}));
    if !child.is_object() {
        *child = json!({});
    }
    if let Some(m) = child.as_object_mut() {
        set_nested(m, &path[1..], v);
    }
}

/// 把旧形状里单个 model 的管理字段合并进新结构规则（保留 optionSpecs 等 ZCode 自写字段）
fn merge_model_rule(rule: &mut Value, m: &Value) {
    let Some(obj) = rule.as_object_mut() else {
        return;
    };
    if let Some(b) = m.get("enabled").and_then(|v| v.as_bool()) {
        set_nested(obj, &["config", "enabled"], json!(b));
    }
    let limit = m.get("limit");
    if let Some(c) = limit
        .and_then(|l| l.get("context"))
        .and_then(|v| v.as_i64())
    {
        set_nested(obj, &["config", "properties", "contextWindow"], json!(c));
    }
    if let Some(o) = limit.and_then(|l| l.get("output")).and_then(|v| v.as_i64()) {
        set_nested(obj, &["config", "optionSpecs", "maxOutputTokens", "max"], json!(o));
    }
}

/// 旧形状 provider 的模型 id 顺序 = models 映射的键顺序
fn legacy_model_ids(prov: &Value) -> Vec<String> {
    prov.get("models")
        .and_then(|m| m.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

/// 账号切换用：把快照中「账号作用域」的规则合并进当前配置。
/// 账号作用域 = providerId 以 builtin:/account: 开头（订阅套餐的模型顺序/启用态）；
/// 用户自定义 provider 一律保留当前值，避免切账号冲掉手动添加的供应商。
/// 任一端非法时退化为保留当前配置。
pub fn merge_account_overlay(curr: &str, snap: &str) -> String {
    let mut curr_v: Value = match serde_json::from_str(curr) {
        Ok(v) => v,
        Err(_) => return curr.to_string(),
    };
    let snap_v: Value = match serde_json::from_str(snap) {
        Ok(v) => v,
        Err(_) => return curr.to_string(),
    };

    // 快照里账号作用域的 providerRules
    let snap_rules: Vec<Value> = snap_v
        .get("config")
        .and_then(|c| c.get("providerConfigRules"))
        .and_then(|r| r.get("providerRules"))
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();

    let Some(curr_rules) = curr_v
        .get_mut("config")
        .and_then(|c| c.get_mut("providerConfigRules"))
        .and_then(|r| r.get_mut("providerRules"))
        .and_then(|r| r.as_array_mut())
    else {
        return curr.to_string();
    };

    for sr in snap_rules {
        let Some(pid) = sr.get("providerId").and_then(|v| v.as_str()) else {
            continue;
        };
        if is_managed(pid) {
            continue;
        }
        match curr_rules
            .iter_mut()
            .find(|r| r.get("providerId").and_then(|v| v.as_str()) == Some(pid))
        {
            Some(slot) => *slot = sr,
            None => curr_rules.push(sr),
        }
    }

    // providerOrder 里的账号作用域项也按快照顺序对齐（其余保留）
    if let Some(order) = curr_v
        .get_mut("config")
        .and_then(|c| c.get_mut("providerOrder"))
        .and_then(|v| v.as_array_mut())
    {
        let snap_order: Vec<String> = snap_v
            .get("config")
            .and_then(|c| c.get("providerOrder"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        order.retain(|v| {
            v.as_str().map(is_managed).unwrap_or(false)
                || snap_order.iter().any(|s| Some(s.as_str()) == v.as_str())
        });
        for s in snap_order {
            if !is_managed(&s) && !order.iter().any(|v| v.as_str() == Some(s.as_str())) {
                order.push(json!(s));
            }
        }
    }

    serde_json::to_string_pretty(&curr_v).unwrap_or_else(|_| curr.to_string())
}

/// 把旧形状的改动合并回新结构并原子写盘。
/// 只处理用户自定义 provider；builtin:/account: 归内置配置，跳过。
/// legacy 中缺失的受管 provider 视为已删除，从新结构中移除。
pub fn apply_legacy(legacy: &Value) -> Result<()> {
    let p = personal_path().context("未定位 ~/.zcode/v2/provider_config.json")?;
    apply_legacy_at(&p, legacy)
}

pub(crate) fn apply_legacy_at(p: &Path, legacy: &Value) -> Result<()> {
    let mut doc = read_json_file(p)?;
    {
        let root = doc
            .as_object_mut()
            .context("provider_config.json 顶层非对象")?;
        if root.get("schemaVersion").is_none() {
            root.insert("schemaVersion".into(), json!(1));
        }
        if !root.get("config").map(|c| c.is_object()).unwrap_or(false) {
            root.insert("config".into(), json!({}));
        }
    }

    let legacy_providers: Map<String, Value> = legacy
        .get("provider")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    let managed: Vec<String> = legacy_providers
        .keys()
        .filter(|k| is_managed(k))
        .cloned()
        .collect();

    // 1) providerRules：先移除已删除的受管项，再逐个 upsert
    {
        let cfg = doc
            .get_mut("config")
            .and_then(|c| c.as_object_mut())
            .context("config 非对象")?;
        let rules = cfg
            .entry("providerConfigRules".to_string())
            .or_insert_with(|| json!({ "providerRules": [] }));
        if !rules.is_object() {
            *rules = json!({ "providerRules": [] });
        }
        let arr = rules
            .as_object_mut()
            .unwrap()
            .entry("providerRules".to_string())
            .or_insert_with(|| json!([]));
        if !arr.is_array() {
            *arr = json!([]);
        }
        let list = arr.as_array_mut().unwrap();

        list.retain(|r| {
            let pid = r.get("providerId").and_then(|v| v.as_str()).unwrap_or("");
            !is_managed(pid) || managed.iter().any(|k| k == pid)
        });

        for key in &managed {
            let prov = &legacy_providers[key];
            let pcfg = prov;
            let api_type = kind_to_api_type(
                pcfg.get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("anthropic"),
            );
            let base_url = pcfg
                .get("options")
                .and_then(|o| o.get("baseURL"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let api_key = pcfg
                .get("options")
                .and_then(|o| o.get("apiKey"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let name = pcfg
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(key);
            let ids = legacy_model_ids(pcfg);

            let idx = list
                .iter()
                .position(|r| r.get("providerId").and_then(|v| v.as_str()) == Some(key.as_str()));
            let rule = match idx {
                Some(i) => &mut list[i],
                None => {
                    list.push(json!({ "providerId": key, "config": {} }));
                    list.last_mut().unwrap()
                }
            };
            let Some(obj) = rule.as_object_mut() else {
                continue;
            };
            obj.insert("providerId".into(), json!(key));
            // 名称与 id 相同则省略，避免 ZCode 侧重名校验
            if name != key {
                obj.insert("providerName".into(), json!(name));
            }
            match pcfg.get("enabled").and_then(|v| v.as_bool()) {
                Some(b) => {
                    obj.insert("enabled".into(), json!(b));
                }
                None => {
                    obj.remove("enabled");
                }
            }
            // 保留既有 group（ZCode 侧写的 standard-personal 等）
            let group = obj
                .get("config")
                .and_then(|c| c.get("group"))
                .and_then(|v| v.as_str())
                .unwrap_or("standard-personal")
                .to_string();
            set_nested(obj, &["config", "group"], json!(group));
            if api_key.is_empty() {
                set_nested(obj, &["config", "access", "type"], json!("api-key"));
            } else {
                set_nested(obj, &["config", "access", "type"], json!("api-key"));
                set_nested(obj, &["config", "access", "apiKey"], json!(api_key));
            }
            set_nested(obj, &["config", "api", "type"], json!(api_type));
            set_nested(obj, &["config", "api", "baseUrl"], json!(base_url));
            set_nested(obj, &["config", "personalModelIds"], json!(ids));
            set_nested(obj, &["config", "modelOrder"], json!(ids));
        }
    }

    // 2) providerOrder：受管 provider 按旧形状顺序，其余保留在后
    {
        let cfg = doc
            .get_mut("config")
            .and_then(|c| c.as_object_mut())
            .context("config 非对象")?;
        let existing: Vec<String> = cfg
            .get("providerOrder")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let mut order = managed.clone();
        for k in existing {
            if !order.iter().any(|x| *x == k) {
                order.push(k);
            }
        }
        cfg.insert("providerOrder".into(), json!(order));
    }

    // 3) modelConfigRules：按 (providerId, modelId) upsert，保留既有字段
    {
        let cfg = doc
            .get_mut("config")
            .and_then(|c| c.as_object_mut())
            .context("config 非对象")?;
        let mcr = cfg
            .entry("modelConfigRules".to_string())
            .or_insert_with(|| json!({}));
        if !mcr.is_object() {
            *mcr = json!({});
        }
        let mcr_obj = mcr.as_object_mut().unwrap();
        for list_key in ["providerModelRules", "manualProviderModelRules"] {
            let arr = mcr_obj
                .entry(list_key.to_string())
                .or_insert_with(|| json!([]));
            if !arr.is_array() {
                *arr = json!([]);
            }
        }

        // 先删掉已删除 provider 的规则
        for list_key in ["providerModelRules", "manualProviderModelRules"] {
            let list = mcr_obj
                .get_mut(list_key)
                .and_then(|v| v.as_array_mut())
                .unwrap();
            list.retain(|r| {
                let pid = r.get("providerId").and_then(|v| v.as_str()).unwrap_or("");
                !is_managed(pid) || managed.iter().any(|k| k == pid)
            });
        }

        for key in &managed {
            let Some(models) = legacy_providers[key].get("models").and_then(|m| m.as_object())
            else {
                continue;
            };
            for (mid, m) in models {
                // manual 规则优先（同 provider/model 两表互斥，不能同时存在）。
                // 先定位再取可变引用，避免跨迭代持有 mcr_obj 的可变借用。
                let mut found: Option<(&str, usize)> = None;
                for list_key in ["manualProviderModelRules", "providerModelRules"] {
                    let Some(list) = mcr_obj.get(list_key).and_then(|v| v.as_array()) else {
                        continue;
                    };
                    if let Some(i) = list.iter().position(|r| {
                        r.get("providerId").and_then(|v| v.as_str()) == Some(key.as_str())
                            && r.get("modelId").and_then(|v| v.as_str()) == Some(mid.as_str())
                    }) {
                        found = Some((list_key, i));
                        break;
                    }
                }
                match found {
                    Some((list_key, i)) => {
                        if let Some(rule) = mcr_obj
                            .get_mut(list_key)
                            .and_then(|v| v.as_array_mut())
                            .and_then(|a| a.get_mut(i))
                        {
                            merge_model_rule(rule, m);
                        }
                    }
                    None => {
                        let mut rule = json!({
                            "providerId": key,
                            "modelId": mid,
                            "config": {}
                        });
                        merge_model_rule(&mut rule, m);
                        // 无任何管理字段时不必落盘空规则
                        if rule
                            .get("config")
                            .and_then(|c| c.as_object())
                            .map(|o| o.is_empty())
                            .unwrap_or(true)
                        {
                            continue;
                        }
                        mcr_obj
                            .get_mut("providerModelRules")
                            .and_then(|v| v.as_array_mut())
                            .unwrap()
                            .push(rule);
                    }
                }
            }
        }
    }

    write_json_file(&p, &doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 一份贴近真实的新结构样本（含内置/账号 provider，必须原样保留）
    fn sample_doc() -> Value {
        json!({
          "schemaVersion": 1,
          "config": {
            "providerOrder": ["new-provider", "account:bigmodel-individual-coding-plan"],
            "providerConfigRules": {
              "providerRules": [
                {
                  "providerId": "new-provider",
                  "providerName": "someway",
                  "config": {
                    "group": "standard-personal",
                    "access": { "type": "api-key", "apiKey": "sk-abc" },
                    "api": { "type": "openai-chat-completions", "baseUrl": "https://someway.timaic.cn/v1" },
                    "personalModelIds": ["minimax-m3", "hy3"],
                    "modelOrder": ["minimax-m3", "hy3"]
                  }
                },
                {
                  "providerId": "account:bigmodel-individual-coding-plan",
                  "config": {
                    "personalModelIds": [],
                    "modelOrder": ["GLM-5.3", "GLM-5.3-Flash"]
                  }
                }
              ]
            },
            "modelConfigRules": {
              "providerModelRules": [
                {
                  "providerId": "new-provider",
                  "modelId": "minimax-m3",
                  "config": {
                    "enabled": true,
                    "properties": { "contextWindow": 1000000 },
                    "optionSpecs": { "reasoningLevel": { "values": ["disabled", "enabled"] } }
                  }
                }
              ],
              "manualProviderModelRules": []
            }
          }
        })
    }

    fn tmp_file(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("zca-pc-test-{name}-{}.json", std::process::id()));
        p
    }

    #[test]
    fn projects_new_schema_to_legacy_shape() {
        let p = tmp_file("proj");
        fs::write(&p, serde_json::to_string_pretty(&sample_doc()).unwrap()).unwrap();

        let legacy = read_as_legacy_at(&p).expect("read");
        let providers = legacy.get("provider").unwrap().as_object().unwrap();

        // 自定义 provider 投影
        let np = &providers["new-provider"];
        assert_eq!(np["name"], "someway");
        assert_eq!(np["kind"], "openai-compatible");
        assert_eq!(np["options"]["baseURL"], "https://someway.timaic.cn/v1");
        assert_eq!(np["options"]["apiKey"], "sk-abc");
        assert_eq!(np["source"], "custom");

        // 模型顺序与 limit/modalities 投影
        let models = np["models"].as_object().unwrap();
        let keys: Vec<&String> = models.keys().collect();
        assert_eq!(keys, vec!["minimax-m3", "hy3"]);
        assert_eq!(models["minimax-m3"]["limit"]["context"], 1000000);
        assert_eq!(models["minimax-m3"]["enabled"], true);

        // 订阅 provider 还原为旧 builtin:* id（供上层订阅识别逻辑使用）
        assert!(providers.contains_key("builtin:bigmodel-coding-plan"));

        let _ = fs::remove_file(&p);
    }

    #[test]
    fn round_trip_preserves_builtin_and_untouched_fields() {
        let p = tmp_file("roundtrip");
        fs::write(&p, serde_json::to_string_pretty(&sample_doc()).unwrap()).unwrap();

        // 改上下文 + 输出上限 + 禁用模型，并新增一个 provider
        let mut legacy = read_as_legacy_at(&p).unwrap();
        let provs = legacy.get_mut("provider").unwrap().as_object_mut().unwrap();
        let np = provs["new-provider"].as_object_mut().unwrap();
        let models = np.get_mut("models").unwrap().as_object_mut().unwrap();
        models.insert(
            "minimax-m3".into(),
            json!({ "limit": { "context": 200000, "output": 32000 } }),
        );
        models.insert("hy3".into(), json!({ "enabled": false }));
        provs.insert(
            "brand-new".into(),
            json!({
                "name": "Brand New",
                "kind": "anthropic",
                "options": { "apiKey": "sk-new", "baseURL": "https://new.example/anthropic" },
                "source": "custom",
                "enabled": true,
                "models": { "m1": { "limit": { "context": 12345 } } }
            }),
        );

        apply_legacy_at(&p, &legacy).expect("write");

        let doc: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(doc["schemaVersion"], 1);
        let rules = doc["config"]["providerConfigRules"]["providerRules"]
            .as_array()
            .unwrap();

        // 账号 provider 必须原样保留
        let acct = rules
            .iter()
            .find(|r| r["providerId"] == "account:bigmodel-individual-coding-plan")
            .expect("account provider preserved");
        assert_eq!(acct["config"]["modelOrder"][0], "GLM-5.3");

        // 自定义 provider 的既有 optionSpecs 不能被抹掉
        let np = rules
            .iter()
            .find(|r| r["providerId"] == "new-provider")
            .unwrap();
        assert_eq!(np["config"]["access"]["apiKey"], "sk-abc");
        assert_eq!(np["config"]["group"], "standard-personal");

        let mrules = doc["config"]["modelConfigRules"]["providerModelRules"]
            .as_array()
            .unwrap();
        let m3 = mrules
            .iter()
            .find(|r| r["providerId"] == "new-provider" && r["modelId"] == "minimax-m3")
            .unwrap();
        assert_eq!(m3["config"]["properties"]["contextWindow"], 200000);
        assert_eq!(m3["config"]["optionSpecs"]["maxOutputTokens"]["max"], 32000);
        // ZCode 自己写的字段必须留着
        assert!(m3["config"]["optionSpecs"]["reasoningLevel"].is_object());

        let hy3 = mrules
            .iter()
            .find(|r| r["providerId"] == "new-provider" && r["modelId"] == "hy3")
            .unwrap();
        assert_eq!(hy3["config"]["enabled"], false);

        // 新 provider 已加入 providerOrder
        let order = doc["config"]["providerOrder"].as_array().unwrap();
        assert!(order.iter().any(|v| v == "brand-new"));

        let _ = fs::remove_file(&p);
    }

    #[test]
    fn removed_provider_is_dropped_from_new_schema() {
        let p = tmp_file("remove");
        fs::write(&p, serde_json::to_string_pretty(&sample_doc()).unwrap()).unwrap();

        let mut legacy = read_as_legacy_at(&p).unwrap();
        legacy
            .get_mut("provider")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove("new-provider");
        apply_legacy_at(&p, &legacy).unwrap();

        let doc: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        let rules = doc["config"]["providerConfigRules"]["providerRules"]
            .as_array()
            .unwrap();
        assert!(!rules.iter().any(|r| r["providerId"] == "new-provider"));
        // 账号 provider 不受影响
        assert!(rules
            .iter()
            .any(|r| r["providerId"] == "account:bigmodel-individual-coding-plan"));

        let _ = fs::remove_file(&p);
    }

    /// 只读核对真实配置：`cargo test --lib provider_config -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn inspect_real_config() {
        let Some(p) = personal_path() else {
            println!("no personal_path");
            return;
        };
        println!("file      = {}", p.display());
        println!("is_active = {}", is_active());
        if !p.is_file() {
            println!("(not found)");
            return;
        }
        let before = fs::read_to_string(&p).unwrap();
        let legacy = read_as_legacy_at(&p).expect("read_as_legacy_at");
        let providers = legacy.get("provider").unwrap().as_object().unwrap();
        println!("\n=== {} providers projected ===", providers.len());
        for (k, v) in providers {
            let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("");
            let kind = v.get("kind").and_then(|x| x.as_str()).unwrap_or("");
            let base = v
                .get("options")
                .and_then(|o| o.get("baseURL"))
                .and_then(|x| x.as_str())
                .unwrap_or("");
            let models = v.get("models").and_then(|m| m.as_object()).unwrap();
            println!(
                "  {:<44} kind={:<18} models={:<3} {:<22} {}",
                k,
                kind,
                models.len(),
                name,
                base
            );
        }
        for key in ["new-provider", "company-china", "minimax", "deepseek"] {
            match providers.get(key) {
                Some(v) => {
                    let models = v.get("models").and_then(|m| m.as_object()).unwrap();
                    println!(
                        "  OK      {key}: {} models {:?}",
                        models.len(),
                        models.keys().take(4).collect::<Vec<_>>()
                    );
                }
                None => println!("  MISSING {key}"),
            }
        }
        // 只读：确认没有写盘
        assert_eq!(before, fs::read_to_string(&p).unwrap());
        println!("\nfile untouched: true");
    }

    /// 无损性核对：对真实配置副本做「读→写回」，逐字段比对。
    /// `cargo test --lib provider_config::tests::write_is_lossless -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn write_is_lossless() {
        let Some(src) = personal_path() else { return };
        if !src.is_file() {
            println!("(no real config)");
            return;
        }
        let orig: Value = serde_json::from_str(&fs::read_to_string(&src).unwrap()).unwrap();

        let p = tmp_file("lossless");
        fs::copy(&src, &p).unwrap();

        // 读投影 → 原样写回（不做任何修改）
        let legacy = read_as_legacy_at(&p).expect("read");
        apply_legacy_at(&p, &legacy).expect("write");

        let after: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();

        // 1) schemaVersion / 内置与账号 provider 必须逐字节不变
        assert_eq!(orig["schemaVersion"], after["schemaVersion"]);
        let orig_rules = orig["config"]["providerConfigRules"]["providerRules"]
            .as_array()
            .unwrap();
        let after_rules = after["config"]["providerConfigRules"]["providerRules"]
            .as_array()
            .unwrap();
        for r in orig_rules {
            let pid = r["providerId"].as_str().unwrap();
            let a = after_rules
                .iter()
                .find(|x| x["providerId"] == pid)
                .unwrap_or_else(|| panic!("provider lost: {pid}"));
            if !is_managed(pid) {
                assert_eq!(r, a, "non-managed provider mutated: {pid}");
            }
        }
        // 2) 受管 provider 的 access/api 保持等价
        for r in orig_rules.iter().filter(|r| is_managed(r["providerId"].as_str().unwrap())) {
            let pid = r["providerId"].as_str().unwrap();
            let a = after_rules.iter().find(|x| x["providerId"] == pid).unwrap();
            assert_eq!(r["config"]["access"], a["config"]["access"], "{pid} access");
            assert_eq!(r["config"]["api"], a["config"]["api"], "{pid} api");
            assert_eq!(
                r["config"]["personalModelIds"], a["config"]["personalModelIds"],
                "{pid} models"
            );
        }
        // 3) 模型规则的 ZCode 自有字段（optionSpecs 等）保留
        for list in ["providerModelRules", "manualProviderModelRules"] {
            let o = orig["config"]["modelConfigRules"][list].as_array().unwrap();
            let a = after["config"]["modelConfigRules"][list].as_array().unwrap();
            for r in o {
                let pid = r["providerId"].as_str().unwrap_or("");
                let mid = r["modelId"].as_str().unwrap_or("");
                if !is_managed(pid) {
                    continue;
                }
                let hit = a
                    .iter()
                    .find(|x| x["providerId"] == pid && x["modelId"] == mid)
                    .unwrap_or_else(|| panic!("model rule lost: {pid}/{mid}"));
                assert_eq!(r, hit, "model rule mutated: {pid}/{mid}");
            }
        }
        // 4) 总条数不减少
        assert_eq!(orig_rules.len(), after_rules.len(), "providerRules count");
        println!("lossless OK: {} providers, rules preserved", orig_rules.len());

        let _ = fs::remove_file(&p);
    }

    /// 端到端写路径：模拟「改某模型 ctx / 禁用模型 / 删除 provider」，
    /// 确认落盘后 ZCode 能读到的正是这些改动。
    /// `cargo test --lib provider_config::tests::edits_land_in_storage -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn edits_land_in_storage() {
        let Some(src) = personal_path() else { return };
        if !src.is_file() {
            println!("(no real config)");
            return;
        }
        let p = tmp_file("edits");
        fs::copy(&src, &p).unwrap();

        let mut legacy = read_as_legacy_at(&p).unwrap();
        let provs = legacy.get_mut("provider").unwrap().as_object_mut().unwrap();

        // 找一个受管 provider 及其首个模型
        let key = provs
            .keys()
            .find(|k| is_managed(k))
            .expect("a managed provider")
            .clone();
        let prov = provs[&key].as_object_mut().unwrap();
        let models = prov.get_mut("models").unwrap().as_object_mut().unwrap();
        let mid = models.keys().next().expect("a model").clone();
        models.insert(
            mid.clone(),
            json!({ "limit": { "context": 424242, "output": 7777 }, "enabled": false }),
        );
        // 复制一份到临时变量，避免与 provs 的可变借用冲突
        let key2 = key.clone();
        let mid2 = mid.clone();

        apply_legacy_at(&p, &legacy).expect("write");

        // 重新读回：改动必须生效
        let back = read_as_legacy_at(&p).unwrap();
        let m = &back["provider"][&key2]["models"][&mid2];
        assert_eq!(m["limit"]["context"], 424242, "ctx not persisted");
        assert_eq!(m["limit"]["output"], 7777, "output not persisted");
        assert_eq!(m["enabled"], false, "enabled not persisted");

        // 落盘文件里必须是新结构，且规则写在 providerModelRules
        let raw: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(raw["schemaVersion"], 1);
        let rules = raw["config"]["modelConfigRules"]["providerModelRules"]
            .as_array()
            .unwrap();
        let rule = rules
            .iter()
            .find(|r| r["providerId"] == key2.as_str() && r["modelId"] == mid2.as_str())
            .unwrap_or_else(|| panic!("no rule for {key2}/{mid2}"));
        assert_eq!(rule["config"]["properties"]["contextWindow"], 424242);
        assert_eq!(rule["config"]["optionSpecs"]["maxOutputTokens"]["max"], 7777);
        assert_eq!(rule["config"]["enabled"], false);

        println!("edits landed: {key2}/{mid2} ctx=424242 out=7777 enabled=false");
        let _ = fs::remove_file(&p);
    }

    /// 前端列表逻辑核对：投影结果的 provider 分类必须与「模型管理」页筛选口径一致。
    /// `cargo test --lib provider_config::tests::frontend_partition -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn frontend_partition() {
        let Some(src) = personal_path() else { return };
        if !src.is_file() {
            return;
        }
        let legacy = read_as_legacy_at(&src).unwrap();
        let setting: Value = serde_json::from_str(
            &fs::read_to_string(src.parent().unwrap().join("setting.json")).unwrap_or_default(),
        )
        .unwrap_or(json!({}));

        let provs = legacy["provider"].as_object().unwrap();
        // 前端 builtinKey：优先取 setting 里 family 选中的 builtin，兜底取第一个启用 builtin
        let current = setting["modelProviderFamilySelectedKeys"]
            .as_object()
            .and_then(|m| {
                m.values()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.strip_prefix("coding-plan:").unwrap_or(s).to_string())
                    .find(|k| k.starts_with("builtin:") && provs.contains_key(k))
            });
        let builtin_key = current.or_else(|| {
            provs
                .keys()
                .find(|k| k.starts_with("builtin:") && provs[*k]["enabled"] != false)
                .cloned()
        });
        // 前端 providers：非 builtin、无 systemDisabledReason
        let listed: Vec<&String> = provs
            .iter()
            .filter(|(k, p)| !k.starts_with("builtin:") && p.get("systemDisabledReason").is_none())
            .map(|(k, _)| k)
            .collect();

        println!("builtinKey   = {builtin_key:?}");
        println!("listed({})   = {listed:?}", listed.len());
        // 订阅 provider 必须被归入 builtin 而不是自定义列表
        assert!(
            !listed.iter().any(|k| k.starts_with("account:")),
            "account provider leaked into custom list"
        );
        assert!(
            builtin_key.is_some(),
            "subscription provider not detected as builtin"
        );
        println!("partition OK");
    }

    /// schema 合规性：ZCode 侧用 zod `.strict()` 解析，多一个未知键就会整份配置被拒。
    /// 因此本适配层写入的键必须全部落在 ZCode 允许的集合内。
    #[test]
    fn writes_only_schema_allowed_keys() {
        fn keys(v: &Value) -> Vec<String> {
            v.as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default()
        }
        fn assert_subset(actual: &[String], allowed: &[&str], what: &str) {
            for k in actual {
                assert!(
                    allowed.contains(&k.as_str()),
                    "{what} 写入了非 schema 键: {k} (allowed={allowed:?})"
                );
            }
        }

        let p = tmp_file("schema");
        fs::write(&p, serde_json::to_string_pretty(&sample_doc()).unwrap()).unwrap();

        // 制造一次包含新增 provider + 新增模型 + 改限值的完整写入
        let mut legacy = read_as_legacy_at(&p).unwrap();
        let provs = legacy.get_mut("provider").unwrap().as_object_mut().unwrap();
        provs.insert(
            "fresh".into(),
            json!({
                "name": "Fresh",
                "kind": "openai",
                "options": { "apiKey": "sk-fresh", "baseURL": "https://fresh.example/v1" },
                "source": "custom",
                "enabled": true,
                "models": { "nm": { "limit": { "context": 1000, "output": 200 } } }
            }),
        );
        apply_legacy_at(&p, &legacy).unwrap();

        let doc: Value = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();

        // providerRules 允许键（ZCode `Ik`/`uY`，均为 strict）
        let rule_allowed = [
            "providerId",
            "providerName",
            "enabled",
            "config",
            "templateId",
        ];
        // 自定义 provider 的 config 允许键（`qo` 去掉 builtinModelIds）
        let cfg_allowed = [
            "group",
            "logo",
            "access",
            "api",
            "personalModelIds",
            "modelOrder",
            "visibility",
        ];
        let access_allowed = ["type", "apiKey", "apiKeyManagementUrl"];
        let api_allowed = ["type", "baseUrl", "headers"];

        for r in doc["config"]["providerConfigRules"]["providerRules"]
            .as_array()
            .unwrap()
        {
            let pid = r["providerId"].as_str().unwrap();
            assert_subset(&keys(r), &rule_allowed, &format!("providerRule({pid})"));
            if !is_managed(pid) {
                continue; // 内置/账号 provider 原样保留，非本层写入
            }
            assert_subset(
                &keys(&r["config"]),
                &cfg_allowed,
                &format!("providerRule({pid}).config"),
            );
            if let Some(a) = r["config"].get("access") {
                assert_subset(&keys(a), &access_allowed, &format!("{pid}.access"));
            }
            if let Some(a) = r["config"].get("api") {
                assert_subset(&keys(a), &api_allowed, &format!("{pid}.api"));
            }
        }

        // 模型规则允许键（ZCode `Xl` / `SK`，均为 strict；properties/optionSpecs 为 partial）
        let mrule_allowed = ["providerId", "modelId", "config"];
        let mcfg_allowed = ["enabled", "properties", "optionSpecs"];
        let props_allowed = [
            "requiresMfjsToolSchema",
            "contextWindow",
            "inputFormat",
            "outputFormat",
            "supportsToolCall",
            "supportsJsonSchemaOutput",
            "supportsNativeWebSearch",
            "supportsMidConversationSystem",
        ];
        let opts_allowed = ["reasoningLevel", "maxOutputTokens"];
        let maxout_allowed = ["max", "values", "map"];

        for list in ["providerModelRules", "manualProviderModelRules"] {
            for r in doc["config"]["modelConfigRules"][list].as_array().unwrap() {
                let pid = r["providerId"].as_str().unwrap_or("");
                assert_subset(&keys(r), &mrule_allowed, &format!("{list}[{pid}]"));
                if !is_managed(pid) {
                    continue;
                }
                let c = &r["config"];
                assert_subset(&keys(c), &mcfg_allowed, &format!("{list}[{pid}].config"));
                if let Some(x) = c.get("properties") {
                    assert_subset(&keys(x), &props_allowed, &format!("{pid}.properties"));
                }
                if let Some(x) = c.get("optionSpecs") {
                    assert_subset(&keys(x), &opts_allowed, &format!("{pid}.optionSpecs"));
                    if let Some(m) = x.get("maxOutputTokens") {
                        assert_subset(&keys(m), &maxout_allowed, &format!("{pid}.maxOutputTokens"));
                    }
                }
            }
        }

        println!("schema-conformance OK");
        let _ = fs::remove_file(&p);
    }
}
