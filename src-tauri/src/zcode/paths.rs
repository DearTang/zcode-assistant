//! zcode 配置路径定位与进程探测
use std::path::PathBuf;

pub fn zcode_v2_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".zcode").join("v2"))
}

pub fn config_path() -> Option<PathBuf> {
    zcode_v2_dir().map(|d| d.join("config.json"))
}

pub fn setting_path() -> Option<PathBuf> {
    zcode_v2_dir().map(|d| d.join("setting.json"))
}

pub fn credentials_path() -> Option<PathBuf> {
    zcode_v2_dir().map(|d| d.join("credentials.json"))
}

/// zcode CLI 每次模型调用的逐行用量记录目录（model-io-sess_*.jsonl）
pub fn rollout_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".zcode").join("cli").join("rollout"))
}

/// zcode CLI 的 SQLite 库（model_usage 等历史用量表，保留完整历史）
pub fn zcode_cli_db_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".zcode").join("cli").join("db").join("db.sqlite"))
}

pub fn config_dir_str() -> String {
    zcode_v2_dir()
        .map(|d| d.to_string_lossy().to_string())
        .unwrap_or_else(|| "(未找到 ~/.zcode/v2)".to_string())
}

/// 探测 ZCode 安装路径（Windows 优先）
pub fn find_zcode_exe() -> Option<String> {
    let candidates: Vec<PathBuf> = vec![
        PathBuf::from(r"C:\Program Files\ZCode\ZCode.exe"),
        PathBuf::from(r"C:\Program Files (x86)\ZCode\ZCode.exe"),
        PathBuf::from(r"D:\Program Files\ZCode\ZCode.exe"),
    ];
    for c in &candidates {
        if c.exists() {
            return Some(c.to_string_lossy().to_string());
        }
    }
    if let Some(local) = dirs::data_dir() {
        let p = local.join("Programs").join("ZCode").join("ZCode.exe");
        if p.exists() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    None
}

/// ZCode 是否正在运行
pub fn is_zcode_running() -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq ZCode.exe", "/NH"])
            .output()
            .map(|o| {
                let s = String::from_utf8_lossy(&o.stdout);
                s.contains("ZCode.exe")
            })
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}
