//! 网络代理命令
use crate::state::AppState;
use crate::types::ProxyConfig;
use tauri::State;

const KV_PROXY: &str = "proxy";
const KEYRING_SVC: &str = "zcode-assistant";
const KEYRING_USER: &str = "proxy-password";

fn load_password() -> Option<String> {
    keyring::Entry::new(KEYRING_SVC, KEYRING_USER)
        .ok()?
        .get_password()
        .ok()
}

#[tauri::command]
pub fn get_proxy(state: State<'_, AppState>) -> Result<ProxyConfig, String> {
    let cfg = state
        .db
        .kv_get(KV_PROXY)
        .and_then(|s| serde_json::from_str::<ProxyConfig>(&s).ok())
        .unwrap_or_default();
    Ok(cfg)
}

#[tauri::command]
pub fn set_proxy(
    state: State<'_, AppState>,
    cfg: ProxyConfig,
    password: Option<String>,
) -> Result<(), String> {
    // 密码存 keyring
    if let Some(pw) = &password {
        if !pw.is_empty() {
            if let Ok(kr) = keyring::Entry::new(KEYRING_SVC, KEYRING_USER) {
                let _ = kr.set_password(pw);
            }
        } else {
            // 空密码 = 清除
            if let Ok(kr) = keyring::Entry::new(KEYRING_SVC, KEYRING_USER) {
                let _ = kr.delete_credential();
            }
        }
    }
    let txt = serde_json::to_string(&cfg).map_err(|e| e.to_string())?;
    state
        .db
        .kv_set(KV_PROXY, &txt)
        .map_err(|e| e.to_string())?;
    let pw = load_password();
    state.rebuild_http(Some(&cfg), pw.as_deref());
    Ok(())
}

#[tauri::command]
pub async fn test_proxy(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let client = state.client();
    let start = std::time::Instant::now();
    match client.get("https://zcode.z.ai").send().await {
        Ok(r) => Ok(serde_json::json!({
            "ok": r.status().as_u16() < 500,
            "status": r.status().as_u16(),
            "latencyMs": start.elapsed().as_millis() as u64
        })),
        Err(e) => Ok(serde_json::json!({ "ok": false, "error": e.to_string() })),
    }
}
