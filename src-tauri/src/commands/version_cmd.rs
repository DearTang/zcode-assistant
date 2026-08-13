//! 版本号命令：把编译期版本号返回给前端
use crate::version;

/// 返回当前应用版本号（取自 Cargo.toml，编译期确定）
#[tauri::command]
pub fn get_app_version() -> String {
    version::APP_VERSION.to_string()
}
