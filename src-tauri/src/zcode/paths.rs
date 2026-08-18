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

/// zcode CLI 每次模型调用的逐行用量记录目录（model-io-sess_*.jsonl）。
/// 注：rollout 会被 zcode 定期清理，历史用量已改用 `model_usage` 表；
/// 会话删除时用于顺带清理对应文件。
pub fn rollout_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".zcode").join("cli").join("rollout"))
}

/// zcode CLI 的 SQLite 库（model_usage 等历史用量表，保留完整历史）
pub fn zcode_cli_db_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".zcode").join("cli").join("db").join("db.sqlite"))
}

/// zcode 界面的任务索引库（tasks 表：archived / deleted 等界面级标记，
/// 「归档会话」的真实存储位置，与会话库按 task_id = session.id 关联）
pub fn tasks_index_db_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".zcode").join("v2").join("tasks-index.sqlite"))
}

pub fn config_dir_str() -> String {
    zcode_v2_dir()
        .map(|d| d.to_string_lossy().to_string())
        .unwrap_or_else(|| "(未找到 ~/.zcode/v2)".to_string())
}

/// 探测 ZCode 安装路径。优先复用正在运行的 ZCode.exe 进程路径（最可靠，
/// 适配任意安装盘符/目录），其次查常见安装位置 + 注册表卸载项。
pub fn find_zcode_exe() -> Option<String> {
    #[cfg(windows)]
    {
        // 1) 从正在运行的进程取真实路径（适配 E:\、自定义目录等任意位置）
        if let Some(p) = running_zcode_path() {
            return Some(p);
        }
        // 2) 从注册表卸载信息取安装目录
        if let Some(p) = exe_from_uninstall_registry() {
            return Some(p);
        }
    }
    // 3) 常见位置兜底
    let mut candidates: Vec<PathBuf> = vec![
        PathBuf::from(r"C:\Program Files\ZCode\ZCode.exe"),
        PathBuf::from(r"C:\Program Files (x86)\ZCode\ZCode.exe"),
        PathBuf::from(r"D:\Program Files\ZCode\ZCode.exe"),
        PathBuf::from(r"E:\Program Files\ZCode\ZCode.exe"),
    ];
    if let Some(local) = dirs::data_dir() {
        candidates.push(local.join("Programs").join("ZCode").join("ZCode.exe"));
    }
    if let Some(local) = dirs::data_local_dir() {
        candidates.push(local.join("Programs").join("ZCode").join("ZCode.exe"));
    }
    for c in &candidates {
        if c.exists() {
            return Some(c.to_string_lossy().to_string());
        }
    }
    None
}

/// 通过 wmic 查询正在运行的 ZCode.exe 的可执行文件路径（任意盘符/目录均可命中）
#[cfg(windows)]
fn running_zcode_path() -> Option<String> {
    use crate::zcode::process::no_window;
    let out = no_window(&mut std::process::Command::new("wmic"))
        .args(["process", "where", "name='ZCode.exe'", "get", "ExecutablePath", "/value"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .map(|l| l.trim())
        .filter(|l| l.starts_with("ExecutablePath="))
        .map(|l| l.trim_start_matches("ExecutablePath=").trim().to_string())
        .find(|p| !p.is_empty())
        .filter(|p| std::path::Path::new(p).exists())
}

/// 从 Windows 卸载注册表项里找 ZCode 的安装目录里的 ZCode.exe
#[cfg(windows)]
fn exe_from_uninstall_registry() -> Option<String> {
    use crate::zcode::process::no_window;
    let out = no_window(&mut std::process::Command::new("reg"))
        .args(["query", "HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall", "/s", "/f", "ZCode"])
        .output()
        .ok()?;
    let txt = String::from_utf8_lossy(&out.stdout);
    // 找 InstallLocation 或 DisplayIcon 字段，推导安装目录
    for line in txt.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("InstallLocation") {
            let dir = v.trim_start_matches([' ', '_']).trim().trim_matches('"');
            if !dir.is_empty() {
                let exe = PathBuf::from(dir).join("ZCode.exe");
                if exe.exists() {
                    return Some(exe.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

/// ZCode 是否正在运行
pub fn is_zcode_running() -> bool {
    #[cfg(windows)]
    {
        use crate::zcode::process::no_window;
        no_window(&mut std::process::Command::new("tasklist"))
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
