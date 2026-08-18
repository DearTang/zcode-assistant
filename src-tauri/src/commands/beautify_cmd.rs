//! ZCode 美化（侵入式改造 app.asar）命令：状态 / 预设 / 应用 / 还原。
//! 底层逻辑在 `zcode::asar`（asar 操作）与 `zcode::beautify`（注入 + CSS 生成）。
use crate::zcode::{asar, beautify, beautify::BeautifyConfig, beautify::BeautifyTemplate};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

const INDEX_HTML_PATH: &str = "out/renderer/index.html";
const INJECT_MARK: &str = "zcode-custom.css";

/// 美化状态：是否已注入 / 是否有备份 / 当前配置 / ZCode 版本。
#[derive(Serialize)]
pub struct BeautifyStatus {
    /// app.asar 当前是否已注入美化 CSS（index.html 含注入标记）。
    pub installed: bool,
    /// 是否已有原始 app.asar 备份（可用于还原）。
    pub has_backup: bool,
    /// 当前美化配置。
    pub config: BeautifyConfig,
    /// ZCode 版本号（asar 根 package.json）。
    pub zcode_version: Option<String>,
    /// app.asar 绝对路径（展示用）。
    pub asar_path: Option<String>,
}

/// 读取美化状态。async：要解包读 asar 内文件（美化页挂载即调），避免阻塞主线程。
#[tauri::command]
pub async fn get_beautify_status() -> Result<BeautifyStatus, String> {
    let asar_path = asar::asar_path().ok();
    let installed = asar_path
        .as_ref()
        .and_then(|p| asar::read_file(p, INDEX_HTML_PATH).ok())
        .map(|b| String::from_utf8_lossy(&b).contains(INJECT_MARK))
        .unwrap_or(false);
    let has_backup = asar::origin_backup_path()
        .map(|p| p.exists())
        .unwrap_or(false);
    let config = beautify::read_config().unwrap_or_default();
    let zcode_version = asar_path.as_ref().and_then(|p| asar::read_zcode_version(p));
    Ok(BeautifyStatus {
        installed,
        has_backup,
        config,
        zcode_version,
        asar_path: asar_path.map(|p| p.display().to_string()),
    })
}

/// 预设主题项。
#[derive(Serialize)]
pub struct PresetInfo {
    pub id: String,
    pub name: String,
}

/// 列出可选预设主题。async：页面挂载即调。
#[tauri::command]
pub async fn get_beautify_presets() -> Vec<PresetInfo> {
    beautify::preset_list()
        .into_iter()
        .map(|(id, name)| PresetInfo {
            id: id.to_string(),
            name: name.to_string(),
        })
        .collect()
}

/// 弹出系统文件选择器挑选背景图，返回选中图片的绝对路径（取消返回 None）。
#[tauri::command]
pub fn pick_beautify_image() -> Option<String> {
    rfd::FileDialog::new()
        .add_filter("图片", &["png", "jpg", "jpeg", "webp", "gif"])
        .add_filter("所有文件", &["*"])
        .pick_file()
        .map(|p| p.to_string_lossy().to_string())
}

/// 读取选中背景图返回 base64 data URL，供前端预览。
/// 仅接受受支持的图片格式；超过 8MB 返回 None（避免大文件阻塞 IPC）。
/// async：读文件 + base64 编码耗时，避免阻塞主线程。
#[tauri::command]
pub async fn read_beautify_image_preview(path: String) -> Option<String> {
    use base64::Engine;
    let p = std::path::Path::new(&path);
    beautify::bg_image_asset_name(p)?; // 扩展名校验
    let meta = std::fs::metadata(p).ok()?;
    if meta.len() > 8 * 1024 * 1024 {
        return None;
    }
    let bytes = std::fs::read(p).ok()?;
    let mime = match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => return None,
    };
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// 应用美化：写配置 → 备份 → kill → extract → 注入 → pack → 替换 app.asar → 请求重启。
/// async：asar 备份/解包/重打包为重 IO 操作（秒级），同步执行会长时间冻结主线程。
#[tauri::command]
pub async fn apply_beautify(app: AppHandle, config: BeautifyConfig) -> Result<(), String> {
    beautify::write_config(&config).map_err(|e| e.to_string())?;
    beautify::apply(&config).map_err(|e| e.to_string())?;
    let _ = app.emit(
        "zcode://restart-requested",
        serde_json::json!({ "reason": "美化已应用，需重启 ZCode 生效" }),
    );
    Ok(())
}

/// 还原：kill → 用备份覆盖 app.asar → 请求重启。
/// async：文件覆盖为重 IO 操作，避免阻塞主线程。
#[tauri::command]
pub async fn restore_beautify(app: AppHandle) -> Result<(), String> {
    beautify::restore().map_err(|e| e.to_string())?;
    // 还原后清掉 enabled 标记，前端状态随之归位
    if let Ok(mut cfg) = beautify::read_config() {
        cfg.enabled = false;
        let _ = beautify::write_config(&cfg);
    }
    let _ = app.emit(
        "zcode://restart-requested",
        serde_json::json!({ "reason": "已还原 ZCode 官方外观，需重启 ZCode 生效" }),
    );
    Ok(())
}

// ───────────────────────── 模板（命名配置快照）─────────────────────────

/// 列出全部美化模板。async：页面挂载即调。
#[tauri::command]
pub async fn list_beautify_templates() -> Vec<BeautifyTemplate> {
    beautify::read_templates()
}

/// 保存模板（同名覆盖），返回最新模板列表。async：文件读写。
#[tauri::command]
pub async fn save_beautify_template(
    name: String,
    config: BeautifyConfig,
) -> Result<Vec<BeautifyTemplate>, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("模板名称不能为空".to_string());
    }
    let mut list = beautify::read_templates();
    if let Some(t) = list.iter_mut().find(|t| t.name == name) {
        t.config = config; // 同名覆盖
    } else {
        list.push(BeautifyTemplate { name, config });
    }
    beautify::write_templates(&list).map_err(|e| e.to_string())?;
    Ok(list)
}

/// 删除模板，返回最新模板列表。async：文件读写。
#[tauri::command]
pub async fn delete_beautify_template(name: String) -> Result<Vec<BeautifyTemplate>, String> {
    let mut list = beautify::read_templates();
    list.retain(|t| t.name != name);
    beautify::write_templates(&list).map_err(|e| e.to_string())?;
    Ok(list)
}
