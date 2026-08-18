//! 配额查询 Token 获取：弹内嵌登录窗口（加载平台登录页，用户登录含 2FA）→
//! 注入脚本按规则轮询提取 token（cookie / localStorage）→ 命中后导航到伪域名
//! 回传（on_navigation 拦截并阻止真实跳转）→ 写入系统凭证库 keyring（敏感
//! 凭证不落明文文件）→ 广播 quota://token-updated，前端即时刷新状态。
//!
//! 提取规则（模板 token_source 字段）：
//!   cookie:<名称>                     —— document.cookie 中的指定 cookie
//!   localstorage:<key>                —— localStorage 指定 key（整值）
//!   localstorage:<key>#<dot.path>     —— 值为 JSON 时按点路径取子字段
//! 限制：HttpOnly cookie 页面脚本读不到（浏览器安全模型），此类平台需用
//! localStorage 规则或改走响应拦截（暂不支持）。
use crate::state::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

/// 注入脚本回传 token 的伪域名（on_navigation 拦截后返回 false 阻止真实导航）
const TOKEN_SINK_HOST: &str = "quota-token.invalid";

fn keyring_entry(provider_key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("zcode-assistant", &format!("quota-token-{provider_key}"))
        .map_err(|e| e.to_string())
}

/// 登录密码单独存 keyring（不进 DB / 模板 JSON，避免明文落盘）
fn password_entry(provider_key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("zcode-assistant", &format!("quota-login-{provider_key}"))
        .map_err(|e| e.to_string())
}

fn read_login_password(provider_key: &str) -> Option<String> {
    password_entry(provider_key).ok()?.get_password().ok()
}

fn kv_time_key(provider_key: &str) -> String {
    format!("quota-token-time-{provider_key}")
}

/// 读取已存的 token（供配额查询渲染 {{token}}）
pub fn read_token(provider_key: &str) -> Option<String> {
    keyring_entry(provider_key).ok()?.get_password().ok()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenStatus {
    pub has_token: bool,
    pub fetched_at: Option<String>,
    /// 登录密码是否已保存（自动填充用；只报有无，不回显明文）
    pub has_password: bool,
}

#[tauri::command]
pub fn quota_token_status(
    state: State<'_, AppState>,
    provider_key: String,
) -> TokenStatus {
    let fetched_at = state
        .db
        .kv_get(&kv_time_key(&provider_key))
        .filter(|s| !s.is_empty());
    TokenStatus {
        has_token: read_token(&provider_key).is_some(),
        fetched_at,
        has_password: read_login_password(&provider_key).is_some(),
    }
}

/// 保存登录密码（自动填充用；keyring 存储）。空串 = 删除已存密码。
#[tauri::command]
pub fn set_quota_login_password(provider_key: String, password: String) -> Result<(), String> {
    let entry = password_entry(&provider_key)?;
    if password.is_empty() {
        let _ = entry.delete_credential();
    } else {
        entry
            .set_password(&password)
            .map_err(|e| format!("保存密码失败: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub fn clear_quota_token(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_key: String,
) -> Result<(), String> {
    if let Ok(e) = keyring_entry(&provider_key) {
        let _ = e.delete_credential();
    }
    let _ = state.db.kv_set(&kv_time_key(&provider_key), "");
    let _ = app.emit(
        "quota://token-updated",
        serde_json::json!({ "providerKey": provider_key }),
    );
    Ok(())
}

/// token_source 解析结果（注入脚本用）
struct ExtractConf {
    mode: &'static str, // "cookie" | "localstorage"
    key: String,
    path: Vec<String>, // localStorage JSON 值的点路径（可为空）
}

fn parse_source(s: &str) -> Result<ExtractConf, String> {
    let s = s.trim();
    if let Some(key) = s.strip_prefix("cookie:") {
        if key.is_empty() {
            return Err("cookie: 后需要写 cookie 名称".into());
        }
        return Ok(ExtractConf {
            mode: "cookie",
            key: key.to_string(),
            path: vec![],
        });
    }
    if let Some(rest) = s.strip_prefix("localstorage:") {
        let (key, path) = match rest.split_once('#') {
            Some((k, p)) => (
                k,
                p.split('.')
                    .filter(|x| !x.is_empty())
                    .map(String::from)
                    .collect(),
            ),
            None => (rest, vec![]),
        };
        if key.is_empty() {
            return Err("localstorage: 后需要写 key".into());
        }
        return Ok(ExtractConf {
            mode: "localstorage",
            key: key.to_string(),
            path,
        });
    }
    Err("提取方式需以 cookie: 或 localstorage: 开头".into())
}

/// 注入到登录页的提取脚本：CONF 由 Rust 侧 JSON 序列化嵌入（免转义问题）。
/// - autofill：自动填写账号/密码（React/Vue 受控组件需原生 setter + 事件触发）
/// - extract：命中 token 后 location.href 到伪域名——被 on_navigation 拦截（阻止跳转）
fn build_script(conf: &ExtractConf, user: &str, pass: &str) -> Result<String, String> {
    let c = serde_json::json!({
        "mode": conf.mode,
        "key": conf.key,
        "path": conf.path,
        "sink": format!("https://{TOKEN_SINK_HOST}/#"),
        "user": user,
        "pass": pass,
    });
    Ok(format!(
        r#"(function () {{
  if (window.__ZQ_HOOK__) return; window.__ZQ_HOOK__ = 1;
  var CONF = {conf};
  // —— 自动填充（React 兼容：原生 value setter + input/change 事件）——
  function setValue(el, v) {{
    var proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    var d = Object.getOwnPropertyDescriptor(proto, 'value');
    if (d && d.set) d.set.call(el, v); else el.value = v;
    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
  }}
  function findUserInput() {{
    var all = document.querySelectorAll('input:not([type=password]):not([type=hidden]):not([type=checkbox]):not([type=radio]):not([type=submit]):not([type=button])');
    var kw = /(user|email|phone|mobile|account|login|账号|手机|邮箱|用户)/i;
    var fallback = null;
    for (var i = 0; i < all.length; i++) {{
      var el = all[i];
      if (el.disabled || el.readOnly) continue;
      var meta = (el.name || '') + ' ' + (el.id || '') + ' ' + (el.placeholder || '') + ' ' + (el.type || '');
      if (kw.test(meta)) return el;
      if (fallback === null && el.offsetParent !== null) fallback = el;
    }}
    return fallback;
  }}
  function autofill() {{
    if (!CONF.user || !CONF.pass || window.__ZQ_FILLED__) return;
    var pws = document.querySelectorAll('input[type=password]');
    if (!pws.length) return; // 尚未出现密码框（可能还没到登录页）
    var pw = pws[0];
    if (pw.disabled || pw.readOnly) return;
    var u = findUserInput();
    if (u && !u.value) setValue(u, CONF.user);
    if (!pw.value) setValue(pw, CONF.pass);
    window.__ZQ_FILLED__ = 1;
  }}
  // —— token 提取 ——
  function pick(v) {{
    if (!CONF.path || !CONF.path.length) return typeof v === 'string' ? v : null;
    try {{
      var c = typeof v === 'string' ? JSON.parse(v) : v;
      for (var i = 0; i < CONF.path.length; i++) c = c[CONF.path[i]];
      return typeof c === 'string' ? c : (c == null ? null : JSON.stringify(c));
    }} catch (e) {{ return null; }}
  }}
  function extract() {{
    try {{
      if (CONF.mode === 'cookie') {{
        var esc = CONF.key.replace(/[.*+?^${{}}()|[\]\\]/g, '\\$&');
        var m = document.cookie.match(new RegExp('(?:^|;\\s)' + esc + '=([^;]*)'));
        return m ? decodeURIComponent(m[1]) : null;
      }}
      var raw = localStorage.getItem(CONF.key);
      return raw == null ? null : pick(raw);
    }} catch (e) {{ return null; }}
  }}
  var t0 = Date.now();
  var timer = setInterval(function () {{
    autofill();
    var t = extract();
    if (t && t.length >= 8) {{
      clearInterval(timer);
      try {{ location.href = CONF.sink + encodeURIComponent(t); }} catch (e) {{}}
    }}
    if (Date.now() - t0 > 600000) clearInterval(timer); // 10 分钟超时自动停
  }}, 600);
}})();"#,
        conf = c
    ))
}

/// 启动登录获取 Token：弹独立 webview 窗口加载模板 login_url。
/// async：建窗需 dispatch 到主线程（同 window.rs 约定，同步命令会死锁）。
#[tauri::command]
pub async fn start_quota_token_login(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_key: String,
) -> Result<(), String> {
    let tmpl = state
        .db
        .get_template(&provider_key)
        .map_err(|e| e.to_string())?
        .ok_or("该 provider 未配置配额查询模板")?;
    let login_url = tmpl
        .login_url
        .clone()
        .filter(|s| !s.is_empty())
        .ok_or("请先在模板中填写「登录页 URL」")?;
    let source = tmpl
        .token_source
        .clone()
        .filter(|s| !s.is_empty())
        .ok_or("请先在模板中填写「Token 提取方式」（如 cookie:xxx）")?;
    let conf = parse_source(&source)?;
    // 登录凭据（自动填充用）：账号在模板里，密码在 keyring（未存则跳过填充）
    let user = tmpl.login_username.clone().unwrap_or_default();
    let pass = read_login_password(&provider_key).unwrap_or_default();
    let script = build_script(&conf, &user, &pass)?;
    let url: tauri::Url = login_url
        .trim()
        .parse()
        .map_err(|e| format!("登录页 URL 无效: {e}"))?;

    // 复用窗口：initialization_script 只能在建窗时注入 → 先关旧窗再重建
    if let Some(w) = app.get_webview_window("quota-login") {
        let _ = w.close();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }

    let sink_app = app.clone();
    let sink_key = provider_key.clone();
    WebviewWindowBuilder::new(&app, "quota-login", WebviewUrl::External(url))
        .title("登录获取 Token")
        .inner_size(500.0, 680.0)
        .resizable(true)
        .initialization_script(&script)
        .on_navigation(move |nav| {
            // 伪域名 = token 回传通道；返回 false 阻止真实导航
            if nav.host_str() != Some(TOKEN_SINK_HOST) {
                return true;
            }
            let token = nav
                .fragment()
                .map(percent_decode)
                .filter(|t| t.len() >= 8);
            if let Some(token) = token {
                if let Ok(e) = keyring_entry(&sink_key) {
                    if e.set_password(&token).is_ok() {
                        if let Some(s) = sink_app.try_state::<AppState>() {
                            let _ = s
                                .db
                                .kv_set(&kv_time_key(&sink_key), &chrono::Local::now().to_rfc3339());
                        }
                        let _ = sink_app.emit(
                            "quota://token-updated",
                            serde_json::json!({ "providerKey": sink_key }),
                        );
                    }
                }
            }
            if let Some(w) = sink_app.get_webview_window("quota-login") {
                let _ = w.close();
            }
            false
        })
        .build()
        .map_err(|e| format!("打开登录窗口失败: {e}"))?;
    Ok(())
}

/// encodeURIComponent 的逆运算（%XX 解码；该编码不产生 '+'，无需处理）
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
