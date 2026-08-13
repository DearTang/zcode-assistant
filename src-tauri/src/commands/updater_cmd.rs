//! 自动更新命令：薄包装，调用 updater 模块。需 app 句柄用于 emit/exit。
use crate::updater;
use tauri::AppHandle;

/// 检查是否有新版本（GET 发行版列表 + semver 比较）。永不返回 Err，失败编码进
/// 返回结构的 `error` 字段。
#[tauri::command]
pub async fn check_for_updates() -> updater::UpdateInfo {
    updater::check().await
}

/// 流式下载安装器到应用配置目录固定文件名，周期性 emit 进度事件。返回绝对路径。
#[tauri::command]
pub async fn download_update(
    app: AppHandle,
    window: tauri::WebviewWindow,
    url: String,
) -> Result<String, String> {
    updater::download(&app, &window, &url).await
}

/// 启动下载好的安装器并退出本进程（让安装器替换运行中的文件）。
#[tauri::command]
pub fn install_update(app: AppHandle, path: String) -> Result<(), String> {
    updater::install(&app, &path)
}

/// 在系统默认浏览器打开发行版页面（非 Windows 平台更新兜底）。
#[tauri::command]
pub fn open_release_page(url: String) -> Result<(), String> {
    updater::open_release_page(&url)
}
