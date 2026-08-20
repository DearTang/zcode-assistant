//! 轻量自动更新（不集成 tauri-plugin-updater，参考 myshell）。
//!
//! 后端做一次 GET 拉取发行版列表，按 semver 取最大 tag 与当前版本比较；
//! 前端拿到 `UpdateInfo` 后弹窗提示「更新 / 忽略」，确认后由后端流式下载
//! 安装包并启动安装器、退出本进程。无需签名密钥、无需 CI manifest，信任
//! 来自 HTTPS + 用户显式确认。
//!
//! 走 Rust reqwest 而非前端 fetch：本项目 csp 为 null 虽不会拦截，但 reqwest
//! 规避 CORS 且可复用超时/流式能力，与 myshell 一致。更新源为国内 Gitee，不
//! 走应用代理（应用代理专供 bigmodel API）。

use crate::version::APP_VERSION;
use serde::Serialize;
use std::time::Duration;
use tauri::{Emitter, Manager, WebviewWindow};

/// 发行版列表接口（默认 Gitee）。
///
/// **配置项**：把下面的 `owner/repo` 改成你自己的 Gitee 仓库即可。
/// 占位状态下 `check` 会因 404 / 解析失败返回 `error`，前端表现为「无更新」，
/// 不会报错。GitHub 的 `/repos/{owner}/{repo}/releases` 返回结构兼容，亦可替换。
const RELEASES_ENDPOINT: &str =
    "https://gitee.com/api/v5/repos/argustang/zcode-assistant/releases?per_page=100&page=1";

/// 发行说明正文字符上限，避免超大 changelog 撑爆 webview。
const MAX_NOTES_CHARS: usize = 2000;

/// 下载的安装包固定文件名（覆盖式，位于应用配置目录）。
const SETUP_FILENAME: &str = "zcode-assistant-update-setup.exe";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    /// release 页面地址（「下载」按钮的兜底）
    pub release_url: String,
    /// 第一个匹配平台 asset 的下载地址，缺省回退到 release_url
    pub download_url: String,
    /// 截断后的发行说明（Markdown）
    pub notes: String,
    /// 发行时间（API 原始字符串）
    pub published_at: String,
    /// 本次检查的 unix 秒，前端可显示「N 分钟前检查」
    pub checked_at: u64,
    /// 检查失败时设置；前端遇非空 error 视为「无更新，保持沉默」
    pub error: Option<String>,
    /// `"auto"` = 应用内下载 + 启动安装器（Windows）；`"browser"` = 打开浏览器手动下载
    pub update_strategy: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
}

fn unix_now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 构造一个标记失败的 UpdateInfo。永不返回 Result —— 命令契约是「总是 resolve
/// 一个结构体」，瞬时网络问题不会冒泡成前端的未捕获 promise 拒绝。
pub fn update_info_error(current_version: &str, message: impl Into<String>) -> UpdateInfo {
    UpdateInfo {
        current_version: current_version.to_string(),
        latest_version: String::new(),
        has_update: false,
        release_url: String::new(),
        download_url: String::new(),
        notes: String::new(),
        published_at: String::new(),
        checked_at: unix_now_secs(),
        error: Some(message.into()),
        update_strategy: String::new(),
    }
}

/// 解析点分版本（如 "v1.4.5" / "1.4.5"）为数值段。非数字尾随字符在段内截断
/// （如 "1.4.5-rc1" → [1,4,5]）。空/垃圾输入返回 `vec![0]`。
fn parse_version(raw: &str) -> Vec<u64> {
    let cleaned = raw.trim().trim_start_matches('v').trim_start_matches('V');
    cleaned
        .split('.')
        .map(|seg| {
            let digits: String = seg.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u64>().unwrap_or(0)
        })
        .collect()
}

/// `latest` 是否严格新于 `current`（点分数值比较），相等或更旧返回 false。
pub fn is_newer(latest: &str, current: &str) -> bool {
    let l = parse_version(latest);
    let c = parse_version(current);
    let n = l.len().max(c.len());
    for i in 0..n {
        let lv = l.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if lv != cv {
            return lv > cv;
        }
    }
    false
}

/// 按 Unicode 字符截断到 max_chars，被截断时追加省略号（避免拆分多字节 UTF-8）。
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push_str("\n…(已截断)");
    out
}

/// 平台更新策略：Windows 走应用内下载+安装；其余平台打开浏览器手动下载。
pub fn update_strategy() -> &'static str {
    if cfg!(target_os = "windows") {
        "auto"
    } else {
        "browser"
    }
}

/// 拉取发行版列表并判断是否有更新。每条路径都 resolve 一个 UpdateInfo；
/// 失败编码进 `error` 字段而非 reject。
pub async fn check() -> UpdateInfo {
    let current_version = APP_VERSION.to_string();

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => return update_info_error(&current_version, format!("客户端构建失败: {e}")),
    };

    let resp = match client.get(RELEASES_ENDPOINT).send().await {
        Ok(r) => r,
        Err(e) => return update_info_error(&current_version, format!("网络请求失败: {e}")),
    };
    if !resp.status().is_success() {
        return update_info_error(
            &current_version,
            format!("接口返回状态 {}", resp.status().as_u16()),
        );
    }
    let json: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return update_info_error(&current_version, format!("解析响应失败: {e}")),
    };

    // 列表接口返回数组，用 is_newer 取最大 semver tag，避免依赖创建顺序/latest 标记。
    let releases = match json.as_array() {
        Some(arr) if !arr.is_empty() => arr,
        _ => return update_info_error(&current_version, "未找到任何发布版本".to_string()),
    };
    let latest_json = releases
        .iter()
        .max_by(|a, b| {
            let av = a
                .get("tag_name")
                .and_then(|v| v.as_str())
                .or_else(|| a.get("name").and_then(|v| v.as_str()))
                .unwrap_or("0");
            let bv = b
                .get("tag_name")
                .and_then(|v| v.as_str())
                .or_else(|| b.get("name").and_then(|v| v.as_str()))
                .unwrap_or("0");
            if is_newer(av, bv) {
                std::cmp::Ordering::Greater
            } else if is_newer(bv, av) {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .expect("non-empty array checked above");

    let tag = latest_json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            latest_json
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
    let Some(latest_version) = tag else {
        return update_info_error(&current_version, "未找到版本信息".to_string());
    };

    let release_url = latest_json
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://gitee.com/")
        .to_string();

    // 按平台后缀挑 asset：Windows 找 .exe（NSIS 安装器），其余平台留空→回退 release 页。
    let asset_suffix = if cfg!(target_os = "windows") {
        ".exe"
    } else if cfg!(target_os = "linux") {
        ".deb"
    } else {
        ""
    };
    let download_url = latest_json
        .get("assets")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter().find_map(|asset| {
                let url = asset.get("browser_download_url").and_then(|v| v.as_str())?;
                if !asset_suffix.is_empty() {
                    if url.to_ascii_lowercase().ends_with(asset_suffix) {
                        Some(url.to_string())
                    } else {
                        None
                    }
                } else {
                    Some(url.to_string())
                }
            })
        })
        .unwrap_or_else(|| release_url.clone());

    let published_at = latest_json
        .get("created_at")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let notes_raw = latest_json
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let notes = truncate_chars(&notes_raw, MAX_NOTES_CHARS);
    let has_update = is_newer(&latest_version, &current_version);

    UpdateInfo {
        current_version,
        latest_version,
        has_update,
        release_url,
        download_url,
        notes,
        published_at,
        checked_at: unix_now_secs(),
        error: None,
        update_strategy: update_strategy().to_string(),
    }
}

/// 流式下载安装器到应用配置目录下的固定文件名，并周期性 emit
/// `update_download_progress` 事件（每 ~32KB + 收尾 100%）。返回绝对路径。
///
/// 无签名校验（那需要完整 tauri-plugin-updater 管线），信任来自 HTTPS + 用户确认。
pub async fn download(app: &tauri::AppHandle, window: &WebviewWindow, url: &str) -> Result<String, String> {
    let lower = url.trim().to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err("仅支持 http(s) 链接".to_string());
    }

    // 比检查更长的超时：安装器（~数 MB）在慢链路上耗时较久，需要流持续可用。
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| format!("客户端构建失败: {e}"))?;

    let resp = client
        .get(url.trim())
        .send()
        .await
        .map_err(|e| format!("下载请求失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败: HTTP {}", resp.status().as_u16()));
    }
    let total = resp.content_length().unwrap_or(0);

    // 固定文件名覆盖式下载。用应用配置目录而非系统 temp（部分 Windows 环境 temp
    // 路径不存在或在网络盘上）。
    let dest = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("获取应用目录失败: {e}"))?
        .join(SETUP_FILENAME);

    use tokio::io::AsyncWriteExt;
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let mut file = tokio::fs::File::create(&dest)
        .await
        .map_err(|e| format!("创建临时文件失败: {e}"))?;

    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emitted: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("读取数据失败: {e}"))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("写入文件失败: {e}"))?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        // 每 ~32KB emit 一次，进度条顺滑又不淹没 webview。
        if downloaded - last_emitted >= 32 * 1024 || total == 0 {
            let _ = window.emit(
                "update_download_progress",
                DownloadProgress {
                    downloaded,
                    total,
                },
            );
            last_emitted = downloaded;
        }
    }
    file.flush().await.map_err(|e| format!("写入文件失败: {e}"))?;
    drop(file);

    // 收尾 100% 事件，即使最后一块未达阈值 UI 也能翻到「就绪」。
    let _ = window.emit(
        "update_download_progress",
        DownloadProgress {
            downloaded,
            total: if total == 0 { downloaded } else { total },
        },
    );

    dest.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "临时路径含非法字符".to_string())
}

/// 启动下载好的安装器并退出本进程，让安装器替换正在运行的文件。
///
/// 安装包为 perMachine NSIS（装到 Program Files），要求管理员权限，普通
/// spawn 会报 os error 740（ERROR_ELEVATION_REQUIRED）。Windows 用
/// ShellExecuteW 携带 "runas" verb 提权启动（触发 UAC 确认），其余平台普通启动。
pub fn install(app: &tauri::AppHandle, path: &str) -> Result<(), String> {
    if path.bytes().any(|b| b == 0) {
        return Err("无效路径".to_string());
    }
    let meta = std::fs::metadata(path).map_err(|_| "安装包不存在".to_string())?;
    if !meta.is_file() {
        return Err("安装包路径无效".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        if !path.to_ascii_lowercase().ends_with(".exe") {
            return Err("仅支持 .exe 安装包".to_string());
        }
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let to_wide = |s: &str| -> Vec<u16> {
            std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect()
        };
        let verb = to_wide("runas");
        let file = to_wide(path);
        // 返回值 > 32 才算成功；用户在 UAC 确认框点「否」也会落到失败分支
        let code = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(verb.as_ptr()),
                PCWSTR(file.as_ptr()),
                None,
                None,
                SW_SHOWNORMAL,
            )
        };
        if code.0 as isize <= 32 {
            return Err(format!(
                "启动安装器失败（ShellExecute {}；若在 UAC 确认框点了「否」，重试即可）",
                code.0 as isize
            ));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new(path)
            .spawn()
            .map_err(|e| format!("启动安装器失败: {e}"))?;
    }

    // 退出以便安装器覆盖当前二进制。exit(0) 会跑 RunEvent::Exit 清理后终止。
    app.exit(0);
    Ok(())
}

/// 在系统默认浏览器打开发行版页面（mac/linux 的更新兜底）。
///
/// 不依赖 tauri-plugin-shell，用平台原生命令零依赖实现。仅放行 http(s)。
pub fn open_release_page(url: &str) -> Result<(), String> {
    let lower = url.trim().to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err("仅支持 http(s) 链接".to_string());
    }
    let url = url.trim().to_string();

    #[cfg(target_os = "windows")]
    {
        // start 的第一个引号串会被当作窗口标题，故显式传空标题 ""。
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| format!("打开链接失败: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("打开链接失败: {e}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("打开链接失败: {e}"))?;
    }
    Ok(())
}
