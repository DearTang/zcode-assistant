//! ZCode 模型调用重试配置：读写用户环境变量 `ZCODE_MODEL_RETRY_*`（ZCode 官方
//! 支持的全局重试调参入口，zcode.cjs 启动时读取）。写入后广播
//! WM_SETTINGCHANGE 让资源管理器刷新环境；ZCode 正在运行时发全局重启确认弹窗。
//!
//! ZCode 侧语义（resources/glm/zcode.cjs 的 resolveAiSdkModelRetryOptions）：
//!   - 实际尝试次数 = MAX_RETRIES + 1（1 次初始 + N 次重试，默认 10 次重试）
//!   - 第 n 次重试延迟 = BASE_DELAY_MS × BACKOFF_FACTOR^n，封顶 MAX_DELAY_MS
//!     （默认 2000ms / ×2 / 60000ms）
//!   - 全局生效，所有模型共用；429/5xx/网络错误/流超时可重试，其余直接失败
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

const V_MAX_RETRIES: &str = "ZCODE_MODEL_RETRY_MAX_RETRIES";
const V_BASE_DELAY: &str = "ZCODE_MODEL_RETRY_BASE_DELAY_MS";
const V_BACKOFF: &str = "ZCODE_MODEL_RETRY_BACKOFF_FACTOR";
const V_MAX_DELAY: &str = "ZCODE_MODEL_RETRY_MAX_DELAY_MS";

/// 字段 None = 未设置该环境变量（跟随 ZCode 内置默认值）。
#[derive(Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRetryConfig {
    /// 最大重试次数（默认 10；0 = 失败不重试）
    pub max_retries: Option<i64>,
    /// 重试起步延迟 ms（默认 2000）
    pub base_delay_ms: Option<i64>,
    /// 指数退避倍数（默认 2）
    pub backoff_factor: Option<f64>,
    /// 单次重试延迟上限 ms（默认 60000）
    pub max_delay_ms: Option<i64>,
}

fn validate(cfg: &ModelRetryConfig) -> Result<(), String> {
    if let Some(v) = cfg.max_retries {
        if !(0..=100).contains(&v) {
            return Err(format!("最大重试次数需在 0–100 之间（当前 {v}）"));
        }
    }
    if let Some(v) = cfg.base_delay_ms {
        if !(0..=600_000).contains(&v) {
            return Err(format!("起步延迟需在 0–600000ms 之间（当前 {v}）"));
        }
    }
    if let Some(v) = cfg.backoff_factor {
        if !(1.0..=10.0).contains(&v) {
            return Err(format!("退避倍数需在 1–10 之间（当前 {v}）"));
        }
    }
    if let Some(v) = cfg.max_delay_ms {
        if !(0..=600_000).contains(&v) {
            return Err(format!("延迟上限需在 0–600000ms 之间（当前 {v}）"));
        }
    }
    Ok(())
}

/// reg 子进程（无窗口）。返回 (stdout+stderr, 是否成功)；reg delete 值不存在
/// 时返回非 0，调用方按需忽略。
#[cfg(target_os = "windows")]
fn reg(args: &[&str]) -> (String, bool) {
    use crate::zcode::process::no_window;
    let mut c = std::process::Command::new("reg");
    no_window(&mut c);
    match c.args(args).output() {
        Ok(out) => {
            let mut s = String::from_utf8_lossy(&out.stdout).to_string();
            s.push_str(&String::from_utf8_lossy(&out.stderr));
            (s, out.status.success())
        }
        Err(e) => (format!("启动 reg 失败: {e}"), false),
    }
}

/// 读取当前配置。async：spawn reg 子进程，避免阻塞主线程。
#[tauri::command]
#[cfg(target_os = "windows")]
pub async fn get_model_retry_config() -> Result<ModelRetryConfig, String> {
    let (out, ok) = reg(&["query", r"HKCU\Environment"]);
    if !ok {
        return Err(format!("读取用户环境变量失败: {out}"));
    }
    let val = |name: &str| -> Option<String> {
        // 行格式：`    变量名    REG_SZ    值`（reg add 只写 REG_SZ，宽松接受其他类型）
        out.lines().find_map(|l| {
            let mut it = l.split_whitespace();
            match (it.next(), it.next(), it.next()) {
                (Some(k), Some(_), Some(v)) if k == name => Some(v.to_string()),
                _ => None,
            }
        })
    };
    let num = |name: &str| val(name).and_then(|s| s.trim().parse::<f64>().ok());
    Ok(ModelRetryConfig {
        max_retries: num(V_MAX_RETRIES).map(|f| f as i64),
        base_delay_ms: num(V_BASE_DELAY).map(|f| f as i64),
        backoff_factor: num(V_BACKOFF),
        max_delay_ms: num(V_MAX_DELAY).map(|f| f as i64),
    })
}

/// 非 Windows：zcode-assistant 主要面向 Windows（ZCode 桌面版），重试环境变量
/// 读写仅在 Windows 实现。
#[tauri::command]
#[cfg(not(target_os = "windows"))]
pub async fn get_model_retry_config() -> Result<ModelRetryConfig, String> {
    Ok(ModelRetryConfig::default())
}

/// 写入配置：Some → reg add（REG_SZ）；None → reg delete（值本就不存在也视为成功）。
/// 完成后广播环境变更；ZCode 运行中时请求重启（新值仅对新启动的进程生效）。
#[tauri::command]
#[cfg(target_os = "windows")]
pub async fn set_model_retry_config(
    app: AppHandle,
    config: ModelRetryConfig,
) -> Result<ModelRetryConfig, String> {
    validate(&config)?;
    let mut failed: Vec<&str> = Vec::new();
    let entries: [(&str, Option<String>); 4] = [
        (V_MAX_RETRIES, config.max_retries.map(|v| v.to_string())),
        (V_BASE_DELAY, config.base_delay_ms.map(|v| v.to_string())),
        (V_BACKOFF, config.backoff_factor.map(|v| v.to_string())),
        (V_MAX_DELAY, config.max_delay_ms.map(|v| v.to_string())),
    ];
    for (name, v) in entries {
        let ok = match v {
            Some(v) => reg(&[
                "add", r"HKCU\Environment", "/v", name, "/t", "REG_SZ", "/d", &v, "/f",
            ])
            .1,
            // 清除：值不存在时 reg delete 报错，同样达成「未设置」的目标状态
            None => {
                let _ = reg(&["delete", r"HKCU\Environment", "/v", name, "/f"]);
                true
            }
        };
        if !ok {
            failed.push(name);
        }
    }
    if !failed.is_empty() {
        return Err(format!("写入环境变量失败: {}", failed.join(", ")));
    }

    broadcast_env_change();

    // 新值只对新启动的进程生效；ZCode 在跑则走全局重启确认弹窗
    if crate::zcode::paths::is_zcode_running() {
        let _ = app.emit(
            "zcode://restart-requested",
            serde_json::json!({ "reason": "重试配置已写入，重启 ZCode 后生效" }),
        );
    }
    Ok(config)
}

#[tauri::command]
#[cfg(not(target_os = "windows"))]
pub async fn set_model_retry_config(
    _app: AppHandle,
    _config: ModelRetryConfig,
) -> Result<ModelRetryConfig, String> {
    Err("仅支持在 Windows 上配置".into())
}

/// 广播环境变量变更（WM_SETTINGCHANGE）。不广播的话，资源管理器不会刷新
/// 环境，之后从任务栏 / 开始菜单启动的 ZCode 拿到的还是旧值（直至重新登录）。
#[cfg(target_os = "windows")]
fn broadcast_env_change() {
    use windows::core::w;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(w!("Environment").as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            3000,
            None,
        );
    }
}
