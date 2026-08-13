//! 版本号：编译期取自 Cargo.toml，单一真相源。
//!
//! `Cargo.toml` 的 `[package] version` 是唯一手写版本号的地方。
//! 这里通过 Cargo 内置的 `CARGO_PKG_VERSION` 环境变量在编译期读取，
//! 供后端命令、HTTP User-Agent 等复用。`tauri.conf.json` 不再写 version 字段，
//! Tauri v2 会自动读 Cargo.toml。前端通过 `get_app_version` 命令获取。
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
