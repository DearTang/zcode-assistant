//! 美化主题资产（外置主题目录 + 热重载，借鉴 zai-floating-monitor/ZBar 的 agent_theme 方案）。
//!
//! 核心思想：app.asar 内只注入几行 file:// 外链引用，真正的主题资产全部放在
//! zcode-assistant 自身 app data 的 `beautify/theme/` 目录下，由三部分组成：
//!
//! - `zq-vars.css`（热文件）：**只含 CSS 变量声明、不含任何元素规则**，由
//!   BeautifyConfig 渲染生成。zq-effects.js 每秒以追加时间戳的方式 cache-bust
//!   重读本文件并比对变量快照——面板改参数约 1 秒生效，无需重打 asar、无需重启
//!   ZCode。变量-only 是热重载无闪烁的关键（元素规则热重载会经历"旧样式表卸载
//!   → 异步加载"窗口，背景会周期性闪回原生底色，ZBar 实测教训）；
//! - `zq-theme.css`（静态模板）：全部分区/元素级结构规则，统一由
//!   `html[data-zq-translucent="1"]` 属性门控（属性由 zq-effects.js 依据热变量
//!   开关，改开关即时生效）。规则内部消费 `--zq-*` 变量并带无副作用兜底值；
//! - `zq-effects.js`（静态模板）：壁纸运行时（黑底占位层 / 壁纸媒体层
//!   视频·图片二选一 / 压暗遮罩层）+ 每秒热重载引擎 + 表面透明化补丁
//!   （MutationObserver 改写字面量 Tailwind 底色 + backdrop-filter 磨砂）。
//!
//! 壁纸文件复制为本目录下的 `wallpaper.<ext>`（图片与视频同目录同名规则），
//! 不引用用户原路径——用户移动/删除源文件不影响已生效主题。
//!
//! 空值防御（ZBar 踩坑）：热重载轮询读到任一变量为空串说明正处于样式表卸载
//! 窗口，本轮直接跳过，避免把用户参数误判为"被重置为默认值"。

use crate::zcode::beautify::BeautifyConfig;
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::Path;

const THEME_DIR_NAME: &str = "theme";
const VARS_CSS_NAME: &str = "zq-vars.css";
const THEME_CSS_NAME: &str = "zq-theme.css";
const EFFECTS_JS_NAME: &str = "zq-effects.js";
const WALLPAPER_PREFIX: &str = "wallpaper.";

/// 壁纸支持的扩展名（图片 + 视频）。视频由 zq-effects.js 按扩展名挂 <video>。
pub const WALLPAPER_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "mp4", "webm", "mov"];

// ───────────────────────── 路径 ─────────────────────────

/// 主题目录：`<appdata>/com.zcode-assistant.app/beautify/theme/`。
/// 与 asar 备份（beautify/ 根）同源，随配置一并持久化。
pub fn theme_dir() -> Result<std::path::PathBuf> {
    Ok(crate::zcode::asar::backup_dir()?.join(THEME_DIR_NAME))
}

fn vars_css_path() -> Result<std::path::PathBuf> {
    Ok(theme_dir()?.join(VARS_CSS_NAME))
}

fn theme_css_path() -> Result<std::path::PathBuf> {
    Ok(theme_dir()?.join(THEME_CSS_NAME))
}

fn effects_js_path() -> Result<std::path::PathBuf> {
    Ok(theme_dir()?.join(EFFECTS_JS_NAME))
}

/// 壁纸在主题目录内的资产名（沿用源扩展名）。非法扩展名返回 None。
pub fn wallpaper_asset_name(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    WALLPAPER_EXTS
        .contains(&ext.as_str())
        .then(|| format!("{WALLPAPER_PREFIX}{ext}"))
}

/// 校验壁纸：文件存在 + 扩展名受支持（图片或视频）。成功返回资产名。
pub fn validate_wallpaper(path: &Path) -> Result<String> {
    if !path.exists() {
        return Err(anyhow!("壁纸文件不存在：{}", path.display()));
    }
    wallpaper_asset_name(path)
        .ok_or_else(|| anyhow!("不支持的壁纸格式（支持 png / jpg / jpeg / webp / gif / mp4 / webm / mov）"))
}

// ───────────────────────── file:// 外链 ─────────────────────────

/// 把本地路径转为 file:// URL（百分号编码，保留 `/` 与 `:`）。
/// 路径中的空格、中文、括号等必须编码，否则 Chromium 解析 href 失败。
fn file_url(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let mut out = String::with_capacity(s.len() + 8);
    out.push_str("file:///");
    for ch in s.chars() {
        match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | '/' | ':' => out.push(ch),
            _ => {
                for b in ch.to_string().as_bytes() {
                    out.push_str(&format!("%{b:02X}"));
                }
            }
        }
    }
    out
}

/// 三个外链地址（index.html 注入块用）。
pub fn link_urls() -> Result<(String, String, String)> {
    Ok((
        file_url(&vars_css_path()?),
        file_url(&theme_css_path()?),
        file_url(&effects_js_path()?),
    ))
}

// ───────────────────────── 静态模板（版本化落盘）─────────────────────────

pub const THEME_CSS_VERSION: u32 = 1;
pub const EFFECTS_JS_VERSION: u32 = 1;

/// 从模板文本头部提取版本号（`ZQ-THEME-V<n>` / `ZQ-EFFECTS-V<n>` 标记）。
fn template_version_of(text: &str, marker: &str) -> Option<u32> {
    let pos = text.find(marker)?;
    let digits: String = text[pos + marker.len()..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// 版本化落盘：文件不存在或版本低于 current 时写入当前模板；
/// 已是当前版本（可能被真机调优）不覆盖。
fn ensure_versioned(path: &Path, marker: &str, current: u32, template: &str) -> Result<()> {
    if path.exists() {
        if let Ok(text) = fs::read_to_string(path) {
            if template_version_of(&text, marker).is_some_and(|v| v >= current) {
                return Ok(());
            }
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, template).with_context(|| format!("写入主题模板失败: {}", path.display()))?;
    Ok(())
}

/// 确保静态模板（zq-theme.css / zq-effects.js）已落盘且为当前版本。
/// apply 与 save_params 都调用（模板升级后无需重打 asar——ZCode 下次启动即加载新模板）。
pub fn ensure_static_templates() -> Result<()> {
    let dir = theme_dir()?;
    fs::create_dir_all(&dir).with_context(|| format!("创建主题目录失败: {}", dir.display()))?;
    ensure_versioned(
        &dir.join(THEME_CSS_NAME),
        "ZQ-THEME-V",
        THEME_CSS_VERSION,
        THEME_CSS_TEMPLATE,
    )?;
    ensure_versioned(
        &dir.join(EFFECTS_JS_NAME),
        "ZQ-EFFECTS-V",
        EFFECTS_JS_VERSION,
        EFFECTS_JS_TEMPLATE,
    )?;
    Ok(())
}

// ───────────────────────── 壁纸同步 ─────────────────────────

/// 把壁纸源文件同步进主题目录（`wallpaper.<ext>`），返回资产名（无壁纸返回 None）。
/// - 先清理目录内全部旧 `wallpaper.*`（换图/换视频/移除后不残留、不累积）；
/// - 同名同大小的目标已存在时跳过复制（热保存拖滑块时避免反复拷贝大视频）。
pub fn sync_wallpaper(src: Option<&Path>) -> Result<Option<String>> {
    let dir = theme_dir()?;
    fs::create_dir_all(&dir)?;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(WALLPAPER_PREFIX) {
            let _ = fs::remove_file(entry.path());
        }
    }
    let Some(src) = src else {
        return Ok(None);
    };
    let asset = validate_wallpaper(src)?;
    let dest = dir.join(&asset);
    let needs_copy = match fs::metadata(&dest) {
        Ok(m) => m.len() != fs::metadata(src)?.len(),
        Err(_) => true,
    };
    if needs_copy {
        fs::copy(src, &dest)
            .with_context(|| format!("复制壁纸失败: {} -> {}", src.display(), dest.display()))?;
    }
    Ok(Some(asset))
}

/// 壁纸资产的 file:// URL（无壁纸返回 None）。
pub fn wallpaper_url(asset: Option<&str>) -> Result<Option<String>> {
    let Some(asset) = asset else {
        return Ok(None);
    };
    Ok(Some(file_url(&theme_dir()?.join(asset))))
}

// ───────────────────────── zq-vars.css 渲染（热文件）─────────────────────────

fn num(v: f32) -> String {
    format!("{v:.3}")
}

/// 是否处于"透出"模式（毛玻璃或壁纸任一启用）。
pub fn translucent(cfg: &BeautifyConfig) -> bool {
    cfg.acrylic || cfg.wallpaper.is_some() || cfg.bg_image.is_some()
}

/// 壁纸源路径：新字段 wallpaper 优先，旧字段 bg_image 兼容。
pub fn wallpaper_source(cfg: &BeautifyConfig) -> Option<String> {
    cfg.wallpaper.clone().or_else(|| cfg.bg_image.clone())
}

/// 混色基准色（毛玻璃/调色板覆盖的染色源）：自定义背景色 > 预设主题背景色。
/// 与旧 generate_css 的 base 取值语义一致（亮暗两档共用同一基准）。
fn base_color(cfg: &BeautifyConfig) -> Option<String> {
    if let Some(c) = &cfg.bg_color {
        return Some(c.clone());
    }
    let theme = cfg.theme.as_deref()?;
    if theme == "none" {
        return None;
    }
    crate::zcode::beautify::preset_vars(theme)?
        .iter()
        .find(|(k, _)| *k == "--color-background")
        .map(|(_, v)| v.to_string())
}

/// 生成 zq-vars.css 内容。**只允许 CSS 变量声明，禁止元素规则**——
/// 本文件每秒被 zq-effects.js 热重载，元素规则会带来背景闪烁（见模块头）。
pub fn render_vars_css(cfg: &BeautifyConfig, wallpaper_asset: Option<&str>) -> String {
    let mut root: Vec<(String, String)> = Vec::new();

    // ---- 预设主题 / 字体 / 自定义色（与旧注入方案同一套 token）----
    if let Some(t) = &cfg.theme {
        if t != "none" {
            if let Some(preset) = crate::zcode::beautify::preset_vars(t) {
                for (k, v) in preset {
                    root.push((k.to_string(), v.to_string()));
                }
            }
        }
    }
    if let Some(f) = &cfg.ui_font {
        root.push((
            "--font-sans".to_string(),
            crate::zcode::beautify::font_stack(f),
        ));
    }
    if let Some(f) = &cfg.mono_font {
        root.push((
            "--font-mono".to_string(),
            crate::zcode::beautify::mono_stack(f),
        ));
    }
    if let Some(c) = &cfg.bg_color {
        root.push(("--color-background".to_string(), c.clone()));
    }
    if let Some(c) = &cfg.primary_color {
        root.push(("--color-primary".to_string(), c.clone()));
    }

    let tl = translucent(cfg);
    let surface = cfg.surface_opacity.clamp(0.2, 1.0);
    let region = |v: Option<f32>| -> String { num(v.unwrap_or(surface).clamp(0.0, 1.0)) };

    // ---- 运行时参数（zq-theme.css / zq-effects.js 消费）----
    root.push((
        "--zq-wallpaper-url".to_string(),
        match wallpaper_url(wallpaper_asset).ok().flatten() {
            Some(u) => format!("url(\"{u}\")"),
            None => "none".to_string(),
        },
    ));
    root.push((
        "--zq-wallpaper-opacity".to_string(),
        num(cfg.bg_image_opacity.clamp(0.1, 1.0)),
    ));
    root.push(("--zq-wp-brightness".to_string(), num(cfg.wp_brightness.clamp(0.2, 2.0))));
    root.push(("--zq-wp-saturate".to_string(), num(cfg.wp_saturate.clamp(0.0, 2.0))));
    root.push(("--zq-wp-blur".to_string(), format!("{}px", num(cfg.wp_blur.clamp(0.0, 30.0)))));
    root.push(("--zq-mask-strength".to_string(), num(cfg.mask_strength.clamp(0.0, 0.9))));
    root.push(("--zq-playback-rate".to_string(), num(cfg.playback_rate.clamp(0.25, 4.0))));
    root.push(("--zq-translucent".to_string(), if tl { "1" } else { "0" }.to_string()));
    root.push(("--zq-surface-alpha".to_string(), num(surface)));
    root.push(("--zq-sidebar-alpha".to_string(), region(cfg.sidebar_opacity)));
    root.push(("--zq-panel-alpha".to_string(), region(cfg.panel_opacity)));
    root.push(("--zq-sidebar-right-alpha".to_string(), region(cfg.sidebar_right_opacity)));
    root.push(("--zq-text-shadow".to_string(), num(cfg.text_shadow.clamp(0.0, 1.0))));
    // 表面透明化补丁开关（zq-effects.js 内 MutationObserver 改写字面量底色）：
    // 透出模式开启时 = surface alpha，否则 0（脚本空转）
    root.push((
        "--zq-alpha".to_string(),
        num(if tl { surface } else { 0.0 }),
    ));
    root.push(("--zq-blur".to_string(), "22px".to_string()));

    // 混色基准（zq-theme.css 的 token 覆写消费；未设置时回退 ZCode 原生 neutral 档）
    if let Some(c) = base_color(cfg) {
        root.push(("--zq-base-color-light".to_string(), c.clone()));
        root.push(("--zq-base-color-dark".to_string(), c));
    }

    // 主题/字体类变量亮暗双作用域（.dark 同值覆盖，与旧注入方案一致）；
    // 运行时参数仅 :root,:host（zq-effects.js 读 documentElement 计算值）
    let theme_vars: Vec<(String, String)> = root
        .iter()
        .filter(|(k, _)| k.starts_with("--color") || k.starts_with("--font"))
        .cloned()
        .collect();
    let decls = |items: &[(String, String)]| -> String {
        items
            .iter()
            .map(|(k, v)| format!("  {k}: {v};"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut s = String::new();
    s.push_str("/* zq-vars.css — 由 zcode-assistant 生成，请勿手动编辑。\n");
    s.push_str(" * 仅含 CSS 变量声明（无元素规则）；zq-effects.js 每秒热重载本文件，\n");
    s.push_str(" * 参数变更约 1 秒生效，无需重打 asar / 重启 ZCode。*/\n");
    s.push_str(":root, :host {\n");
    s.push_str(&decls(&root));
    s.push_str("\n}\n");
    if !theme_vars.is_empty() {
        s.push_str(".dark {\n");
        s.push_str(&decls(&theme_vars));
        s.push_str("\n}\n");
    }
    s
}

/// 写入 zq-vars.css（热保存的唯一落盘动作）。
pub fn write_vars_css(content: &str) -> Result<()> {
    let dir = theme_dir()?;
    fs::create_dir_all(&dir)?;
    let p = dir.join(VARS_CSS_NAME);
    fs::write(&p, content).with_context(|| format!("写入变量文件失败: {}", p.display()))
}

// ───────────────────────── 静态模板正文 ─────────────────────────

/// zq-theme.css：静态结构模板。全部规则由 `html[data-zq-translucent="1"]` 门控，
/// 属性由 zq-effects.js 依据 --zq-translucent 热变量开关——未开启透出时本文件
/// 对界面零影响；开启/关闭即时生效（无需重启）。
/// 分区选择器组照抄 ZBar agent_theme theme.css V9 的实机结论：
/// - 左栏：#sidebar；
/// - 对话区四面板组：常驻 workspace-main + 多面板视图才挂载的
///   conversation-column / conversation / terminal；
/// - 右栏：其余 [data-pane-id]（:not 反选，自动覆盖未来新增面板）+
///   右栏空态选择面板 .side-pane-open-tab-shell（无面板属性）。
const THEME_CSS_TEMPLATE: &str = r##"/* ============================================================
 * ZQ-THEME-V1
 * zcode-assistant 美化静态结构模板（由 zcode-assistant 落盘并随版本升级覆盖）
 * ============================================================
 * 全部规则由 html[data-zq-translucent="1"] 门控（属性由 zq-effects.js 依据
 * zq-vars.css 的 --zq-translucent 热变量开关），未开启透出（毛玻璃/壁纸）时
 * 本文件对界面零影响。
 *
 * 规则消费的变量全部来自 zq-vars.css（每秒热重载），拖滑块约 1 秒生效：
 *   --zq-surface-alpha        全局底/氛围透明度（0.2–1）
 *   --zq-sidebar-alpha        左栏透明度
 *   --zq-panel-alpha          对话区透明度
 *   --zq-sidebar-right-alpha  右栏透明度
 *   --zq-text-shadow          文字描边强度（0–1，0=关）
 *   --zq-base-color-light/dark 混色基准色（未设置回退 ZCode 原生 neutral 档）
 * ============================================================ */

/* ---- 壳与根透明：让壁纸层（zq-effects.js 挂载的负 z-index 层）透出 ---- */
html[data-zq-translucent="1"],
html[data-zq-translucent="1"] body {
  background: transparent !important;
}
html[data-zq-translucent="1"] #root {
  background: transparent !important;
}
/* Windows 全视口应用壳容器：ZCode 3.10.x 实测自带不透明底色 rgb(43 43 43)，
 * 会把全部负 z-index 壁纸层整体盖住（"参数生效但壁纸永不显示"的根因），
 * 必须置透明。选择器随 ZCode 版本可能失效——失效只是不再透出，无副作用。 */
html[data-zq-translucent="1"] div.flex.h-dvh.flex-col.overflow-hidden {
  background: transparent !important;
}

/* ---- 全局底色 token 半透明化（毛玻璃主路径）----
 * 混色基准：--zq-base-color-light/dark（自定义/主题背景色），
 * 未设置时回退 ZCode 原生 neutral 档（亮 bg=50/侧栏=100，暗 bg=900/侧栏=950）。
 * 兜底 alpha=1 → 等价原色，变量缺失时无副作用。 */
html[data-zq-translucent="1"]:not(.dark) {
  --color-background: color-mix(in oklab, var(--zq-base-color-light, var(--color-neutral-50)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-background-alt: color-mix(in oklab, var(--zq-base-color-light, var(--color-neutral-100)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-card: color-mix(in oklab, var(--zq-base-color-light, var(--color-neutral-100)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-popover: color-mix(in oklab, var(--zq-base-color-light, var(--color-neutral-100)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-secondary: color-mix(in oklab, var(--zq-base-color-light, var(--color-neutral-100)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-panel: color-mix(in oklab, var(--zq-base-color-light, var(--color-neutral-100)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-sidebar: color-mix(in oklab, var(--zq-base-color-light, var(--color-neutral-100)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-header: color-mix(in oklab, var(--zq-base-color-light, var(--color-neutral-100)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-input: color-mix(in oklab, var(--zq-base-color-light, var(--color-neutral-100)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
}
html[data-zq-translucent="1"].dark {
  --color-background: color-mix(in oklab, var(--zq-base-color-dark, var(--color-neutral-900)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-background-alt: color-mix(in oklab, var(--zq-base-color-dark, var(--color-neutral-800)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-card: color-mix(in oklab, var(--zq-base-color-dark, var(--color-neutral-900)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-popover: color-mix(in oklab, var(--zq-base-color-dark, var(--color-neutral-900)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-secondary: color-mix(in oklab, var(--zq-base-color-dark, var(--color-neutral-900)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-panel: color-mix(in oklab, var(--zq-base-color-dark, var(--color-neutral-900)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-sidebar: color-mix(in oklab, var(--zq-base-color-dark, var(--color-neutral-950)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-header: color-mix(in oklab, var(--zq-base-color-dark, var(--color-neutral-900)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-input: color-mix(in oklab, var(--zq-base-color-dark, var(--color-neutral-900)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
}

/* ---- 调色板 token 半透明覆写（.bg-* 工具类均引用；按亮/暗分档避免误伤文字色）---- */
html[data-zq-translucent="1"]:not(.dark) {
  --color-neutral-50: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-neutral-100: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-neutral-200: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-neutral-300: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-zinc-50: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-zinc-100: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-zinc-200: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-slate-50: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-slate-100: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-slate-200: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-gray-50: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-gray-100: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-white: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
}
html[data-zq-translucent="1"].dark {
  --color-neutral-800: color-mix(in oklab, var(--zq-base-color-dark, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-neutral-900: color-mix(in oklab, var(--zq-base-color-dark, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-neutral-950: color-mix(in oklab, var(--zq-base-color-dark, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-zinc-800: color-mix(in oklab, var(--zq-base-color-dark, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-zinc-900: color-mix(in oklab, var(--zq-base-color-dark, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-zinc-950: color-mix(in oklab, var(--zq-base-color-dark, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-slate-800: color-mix(in oklab, var(--zq-base-color-dark, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-slate-900: color-mix(in oklab, var(--zq-base-color-dark, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-slate-950: color-mix(in oklab, var(--zq-base-color-dark, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-gray-800: color-mix(in oklab, var(--zq-base-color-dark, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-gray-900: color-mix(in oklab, var(--zq-base-color-dark, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
  --color-gray-950: color-mix(in oklab, var(--zq-base-color-dark, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent);
}

/* ---- 直写 Tailwind 容器背景双保险（防御个别工具类未走 token 引用）---- */
html[data-zq-translucent="1"] .bg-neutral-50,
html[data-zq-translucent="1"] .bg-neutral-100,
html[data-zq-translucent="1"] .bg-neutral-200,
html[data-zq-translucent="1"] .bg-zinc-50,
html[data-zq-translucent="1"] .bg-zinc-100,
html[data-zq-translucent="1"] .bg-zinc-200,
html[data-zq-translucent="1"] .bg-slate-50,
html[data-zq-translucent="1"] .bg-slate-100,
html[data-zq-translucent="1"] .bg-slate-200,
html[data-zq-translucent="1"] .bg-gray-50,
html[data-zq-translucent="1"] .bg-gray-100,
html[data-zq-translucent="1"] .bg-white {
  background-color: color-mix(in oklab, var(--zq-base-color-light, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent) !important;
}
html[data-zq-translucent="1"].dark .bg-neutral-900,
html[data-zq-translucent="1"].dark .bg-neutral-950,
html[data-zq-translucent="1"].dark .bg-neutral-800,
html[data-zq-translucent="1"].dark .bg-zinc-900,
html[data-zq-translucent="1"].dark .bg-zinc-950,
html[data-zq-translucent="1"].dark .bg-slate-900,
html[data-zq-translucent="1"].dark .bg-slate-950,
html[data-zq-translucent="1"].dark .bg-gray-900,
html[data-zq-translucent="1"].dark .bg-gray-950 {
  background-color: color-mix(in oklab, var(--zq-base-color-dark, var(--color-surface)) calc(var(--zq-surface-alpha, 1) * 100%), transparent) !important;
}

/* ---- 分区透明度（三区域各自独立滑块，互不牵连）----
 * 左栏 #sidebar → --zq-sidebar-alpha；
 * 对话区四面板组 → --zq-panel-alpha；
 * 右栏其余面板（:not 反选自动覆盖新增面板）与空态选择面板 → --zq-sidebar-right-alpha。
 * 注：全局 token 覆写已按 --zq-surface-alpha 生效，本组元素规则以更高优先级
 * 精确控制三块主区域；不想分区时把三个滑块与"桌面不透明度"保持一致即可。 */
html[data-zq-translucent="1"] #sidebar {
  background-color: color-mix(in oklab, var(--zq-base-color-light, var(--color-neutral-50)) calc(var(--zq-sidebar-alpha, 1) * 100%), transparent) !important;
}
html[data-zq-translucent="1"].dark #sidebar {
  background-color: color-mix(in oklab, var(--zq-base-color-dark, var(--color-neutral-900)) calc(var(--zq-sidebar-alpha, 1) * 100%), transparent) !important;
}

html[data-zq-translucent="1"] [data-pane-id="workspace-main"],
html[data-zq-translucent="1"] [data-pane-id="conversation-column"],
html[data-zq-translucent="1"] [data-pane-id="conversation"],
html[data-zq-translucent="1"] [data-pane-id="terminal"] {
  background-color: color-mix(in oklab, var(--zq-base-color-light, var(--color-neutral-50)) calc(var(--zq-panel-alpha, 1) * 100%), transparent) !important;
}
html[data-zq-translucent="1"].dark [data-pane-id="workspace-main"],
html[data-zq-translucent="1"].dark [data-pane-id="conversation-column"],
html[data-zq-translucent="1"].dark [data-pane-id="conversation"],
html[data-zq-translucent="1"].dark [data-pane-id="terminal"] {
  background-color: color-mix(in oklab, var(--zq-base-color-dark, var(--color-neutral-900)) calc(var(--zq-panel-alpha, 1) * 100%), transparent) !important;
}

html[data-zq-translucent="1"] [data-pane-id]:not([data-pane-id="workspace-main"]):not([data-pane-id="conversation-column"]):not([data-pane-id="conversation"]):not([data-pane-id="terminal"]),
html[data-zq-translucent="1"] .side-pane-open-tab-shell {
  background-color: color-mix(in oklab, var(--zq-base-color-light, var(--color-neutral-50)) calc(var(--zq-sidebar-right-alpha, 1) * 100%), transparent) !important;
}
html[data-zq-translucent="1"].dark [data-pane-id]:not([data-pane-id="workspace-main"]):not([data-pane-id="conversation-column"]):not([data-pane-id="conversation"]):not([data-pane-id="terminal"]),
html[data-zq-translucent="1"].dark .side-pane-open-tab-shell {
  background-color: color-mix(in oklab, var(--zq-base-color-dark, var(--color-neutral-900)) calc(var(--zq-sidebar-right-alpha, 1) * 100%), transparent) !important;
}

/* ---- 文字可读性：壁纸过亮/过暗时给前景文字补描边 ----
 * 浅色主题补白描边、深色主题补黑描边；强度 0 时 alpha=0 自然不可见。
 * 选择器组与分区背景规则同批，描边设在容器上由文字继承。 */
html[data-zq-translucent="1"] #sidebar,
html[data-zq-translucent="1"] [data-pane-id="workspace-main"],
html[data-zq-translucent="1"] [data-pane-id="conversation-column"],
html[data-zq-translucent="1"] [data-pane-id="conversation"],
html[data-zq-translucent="1"] [data-pane-id="terminal"],
html[data-zq-translucent="1"] [data-pane-id]:not([data-pane-id="workspace-main"]):not([data-pane-id="conversation-column"]):not([data-pane-id="conversation"]):not([data-pane-id="terminal"]),
html[data-zq-translucent="1"] .side-pane-open-tab-shell {
  text-shadow: 0 0 calc(3px + (var(--zq-text-shadow, 0) * 5px)) rgba(255, 255, 255, var(--zq-text-shadow, 0));
}
html[data-zq-translucent="1"].dark #sidebar,
html[data-zq-translucent="1"].dark [data-pane-id="workspace-main"],
html[data-zq-translucent="1"].dark [data-pane-id="conversation-column"],
html[data-zq-translucent="1"].dark [data-pane-id="conversation"],
html[data-zq-translucent="1"].dark [data-pane-id="terminal"],
html[data-zq-translucent="1"].dark [data-pane-id]:not([data-pane-id="workspace-main"]):not([data-pane-id="conversation-column"]):not([data-pane-id="conversation"]):not([data-pane-id="terminal"]),
html[data-zq-translucent="1"].dark .side-pane-open-tab-shell {
  text-shadow: 0 0 calc(3px + (var(--zq-text-shadow, 0) * 5px)) rgba(0, 0, 0, var(--zq-text-shadow, 0));
}
"##;

/// zq-effects.js：壁纸运行时 + 热重载引擎 + 表面透明化补丁（ES5，Electron 旧内核兼容）。
/// 移植自 ZBar agent_theme effects.js V5（壁纸三层/热重载/空值防御/自愈），
/// 并合并本项目原有的 zcode-custom.js 表面透明化补丁（--zq-alpha 开关支持热切换）。
const EFFECTS_JS_TEMPLATE: &str = r##"/*! ZQ-EFFECTS-V1
 * zcode-assistant 美化运行时（由 zcode-assistant 落盘并随版本升级覆盖）。
 * 三层结构：黑底占位层 z:-2 → 壁纸媒体层 z:-2（视频/图片按扩展名二选一）→ 压暗遮罩层 z:-1。
 * 热重载：每 1000ms 追加时间戳强制重读 zq-vars.css，比对 --zq-* 快照，
 * 壁纸 URL / 滤镜 / 遮罩 / 透出开关 / 补丁 alpha 变化即时应用。
 * 空值防御：任一变量读到空串 = 样式表卸载窗口，本轮跳过（防止参数被误清零）。
 * 表面透明化补丁：MutationObserver 持续把计算样式为不透明的背景改写为
 * --zq-alpha 透明度并给大面块加 backdrop-filter；alpha 热切换时全量还原重扫。
 */
(function () {
  "use strict";
  if (window.__ZQ_EFFECTS__) return;
  window.__ZQ_EFFECTS__ = true;

  var VAR_NAMES = [
    "--zq-wallpaper-url",
    "--zq-wallpaper-opacity",
    "--zq-wp-brightness",
    "--zq-wp-saturate",
    "--zq-wp-blur",
    "--zq-mask-strength",
    "--zq-playback-rate",
    "--zq-translucent",
    "--zq-alpha",
    "--zq-blur"
  ];

  function cssVar(name) {
    var v = "";
    try { v = getComputedStyle(document.documentElement).getPropertyValue(name) || ""; } catch (e) {}
    return v.trim();
  }
  function num(value, fallback) {
    var n = parseFloat(value);
    return isFinite(n) ? n : fallback;
  }
  /* 从 url("file://…") 变量值提取纯地址；"none" → ""（无壁纸） */
  function urlOf(value) {
    var m = /^\s*url\(\s*(['"]?)(.*?)\1\s*\)\s*$/.exec(value || "");
    var u = m ? m[2] : "";
    return !u || u === "none" ? "" : u;
  }

  /* ==================== 壁纸三层（移植自 ZBar effects.js） ==================== */
  var placeholder = document.createElement("div");
  placeholder.setAttribute("data-zq-wallpaper", "placeholder");
  placeholder.style.cssText =
    "position:fixed;top:0;left:0;width:100%;height:100%;" +
    "z-index:-2;background:#000;pointer-events:none;";

  var mask = document.createElement("div");
  mask.setAttribute("data-zq-wallpaper", "mask");
  mask.style.cssText =
    "position:fixed;top:0;left:0;width:100%;height:100%;" +
    "z-index:-1;pointer-events:none;";

  var media = null, mediaKind = "", currentUrl = "", dead = false, ready = false;
  var wpOpacity = 1, phTimer = 0;

  function onReady() {
    if (mediaKind !== "image") {
      applyRate();
      var p = media.play();
      if (p && p.catch) p.catch(function () {});
    }
    ready = true;
    media.style.opacity = String(wpOpacity); /* CSS 过渡淡入（可重复调用） */
    if (phTimer) clearTimeout(phTimer);
    phTimer = setTimeout(function () {
      if (placeholder.parentNode) placeholder.parentNode.removeChild(placeholder);
      phTimer = 0;
    }, 500);
  }
  function onDead() { unmountMedia(); dead = true; }

  function createVideo() {
    var v = document.createElement("video");
    v.setAttribute("data-zq-wallpaper", "video");
    v.muted = true; v.loop = true; v.playsInline = true; v.autoplay = true;
    v.style.cssText =
      "position:fixed;top:0;left:0;width:100%;height:100%;" +
      "object-fit:cover;z-index:-2;pointer-events:none;" +
      "opacity:0;transition:opacity .35s ease;";
    v.addEventListener("canplay", onReady);
    v.addEventListener("error", onDead);
    return v;
  }
  function createImage() {
    var i = document.createElement("img");
    i.setAttribute("data-zq-wallpaper", "image");
    i.alt = "";
    i.style.cssText =
      "position:fixed;top:0;left:0;width:100%;height:100%;" +
      "object-fit:cover;z-index:-2;pointer-events:none;" +
      "opacity:0;transition:opacity .35s ease;";
    i.addEventListener("load", onReady);
    i.addEventListener("error", onDead);
    return i;
  }
  function kindOf(url) {
    var u = (url || "").toLowerCase().split("?")[0];
    if (/\.mp4$/.test(u)) return "video/mp4";
    if (/\.webm$/.test(u)) return "video/webm";
    if (/\.mov$/.test(u)) return "video/quicktime";
    if (/\.(jpe?g|png|webp|gif)$/.test(u)) return "image";
    return "video/mp4";
  }
  function mounted() { return !!(media && media.parentNode); }
  function mount() {
    if (!media) return;
    if (!media.parentNode) document.body.appendChild(media);
    if (!placeholder.parentNode) document.body.insertBefore(placeholder, media);
    if (!mask.parentNode) document.body.appendChild(mask);
  }
  function unmountMedia() {
    if (placeholder.parentNode) placeholder.parentNode.removeChild(placeholder);
    if (media && media.parentNode) media.parentNode.removeChild(media);
    if (mask.parentNode) mask.parentNode.removeChild(mask);
    media = null; mediaKind = ""; ready = false;
  }
  function setWallpaper(url) {
    currentUrl = url; dead = false; ready = false;
    var kind = kindOf(url);
    /* 类型变化（视频↔图片）时移除旧元素重建，避免元素属性串味（ZBar 同款） */
    if (media && mediaKind !== kind) {
      if (media.parentNode) media.parentNode.removeChild(media);
      media = null; mediaKind = "";
    }
    if (!media) {
      if (kind === "image") { mediaKind = "image"; media = createImage(); }
      else { mediaKind = kind; media = createVideo(); }
    }
    mount();
    media.style.opacity = "0"; /* 重置占位态，就绪后淡入 */
    media.src = url;
    if (mediaKind !== "image") media.load();
    applyFilter(); applyMask();
  }
  function applyFilter() {
    if (!media) return;
    var b = num(cssVar("--zq-wp-brightness"), 1.1);
    var s = num(cssVar("--zq-wp-saturate"), 1.4);
    var l = num(cssVar("--zq-wp-blur"), 0);
    media.style.filter = "brightness(" + b + ") saturate(" + s + ") blur(" + l + "px)";
  }
  function applyRate() {
    if (mediaKind === "image" || !media) return;
    try { media.playbackRate = num(cssVar("--zq-playback-rate"), 1); } catch (e) {}
  }
  function applyMask() {
    mask.style.background = "rgba(0,0,0," + num(cssVar("--zq-mask-strength"), 0) + ")";
  }

  /* ==================== 表面透明化补丁（原 zcode-custom.js） ==================== */
  var patchAlpha = 0;   /* 当前生效 alpha（0 或 ≥1 = 关闭） */
  var patchBlur = 22;
  var blurred = [];
  var obs = null, scheduled = false;
  var OBS_OPTS = { childList: true, subtree: true, attributes: true, attributeFilter: ["class", "style"] };

  function parseColor(s) {
    var m = /^rgba?\(([^)]+)\)$/i.exec(s);
    if (m) {
      var p = m[1].split(/[,\s/]+/).filter(Boolean).map(Number);
      if (p.length >= 3 && p.every(isFinite)) {
        return { r: Math.round(p[0]), g: Math.round(p[1]), b: Math.round(p[2]), a: p.length > 3 ? p[3] : 1 };
      }
      return null;
    }
    m = /^color\(\s*srgb\s+([0-9.]+)\s+([0-9.]+)\s+([0-9.]+)(?:\s*\/\s*([0-9.]+))?\s*\)$/i.exec(s);
    if (m) {
      return { r: Math.round(+m[1] * 255), g: Math.round(+m[2] * 255), b: Math.round(+m[3] * 255), a: m[4] === undefined ? 1 : +m[4] };
    }
    return null;
  }
  function excluded(el) {
    if (el.id === "loading") return true;
    var t = el.tagName;
    if (t === "PRE" || t === "CODE" || t === "SVG" || t === "IMG" || t === "VIDEO" || t === "CANVAS" || t === "IFRAME" || t === "PICTURE") return true;
    if (el.closest && el.closest("pre,code,#loading")) return true;
    return false;
  }
  function patch(el) {
    if (el.dataset && el.dataset.zqBg) return;
    if (excluded(el)) return;
    var bg;
    try { bg = getComputedStyle(el).backgroundColor; } catch (e) { return; }
    var c = bg ? parseColor(bg) : null;
    if (!c || c.a < 0.99) return;
    el.style.backgroundColor = "rgba(" + c.r + "," + c.g + "," + c.b + "," + patchAlpha + ")";
    if (el.dataset) el.dataset.zqBg = "1";
  }
  function scan(root) {
    if (!root || root.nodeType !== 1 || !root.querySelectorAll) return;
    patch(root);
    var list = root.querySelectorAll("*");
    for (var i = 0; i < list.length; i++) patch(list[i]);
    blurPass();
  }
  function hasBlurredAncestor(el) {
    for (var p = el.parentElement; p; p = p.parentElement) {
      if (p.dataset && p.dataset.zqBlur) return true;
    }
    return false;
  }
  function blurPass() {
    for (var i = 0; i < blurred.length; i++) {
      var b = blurred[i];
      b.style.backdropFilter = "";
      b.style.webkitBackdropFilter = "";
      if (b.dataset) delete b.dataset.zqBlur;
    }
    blurred = [];
    var vw = window.innerWidth, vh = window.innerHeight;
    if (!vw || !vh) return;
    var marked = document.querySelectorAll('[data-zq-bg="1"]');
    var cand = [];
    for (var j = 0; j < marked.length; j++) {
      var el = marked[j];
      var r = el.getBoundingClientRect();
      if (r.width < 40 || r.height < 40) continue;
      if (r.width * r.height < vw * vh * 0.12) continue;
      if (r.bottom <= 0 || r.top >= vh || r.right <= 0 || r.left >= vw) continue;
      cand.push({ el: el, area: r.width * r.height });
    }
    cand.sort(function (a, b2) { return b2.area - a.area; });
    for (var k = 0; k < cand.length && blurred.length < 6; k++) {
      var el2 = cand[k].el;
      if (hasBlurredAncestor(el2)) continue;
      var v = "blur(" + patchBlur + "px)";
      el2.style.backdropFilter = v;
      el2.style.webkitBackdropFilter = v;
      if (el2.dataset) el2.dataset.zqBlur = "1";
      blurred.push(el2);
    }
  }
  /* 全量还原补丁（关闭 / alpha 变化时调用）：清内联背景与磨砂，交还原生观感 */
  function unpatchAll() {
    var marked = document.querySelectorAll('[data-zq-bg="1"]');
    for (var i = 0; i < marked.length; i++) {
      var el = marked[i];
      el.style.backgroundColor = "";
      if (el.dataset) delete el.dataset.zqBg;
    }
    for (var j = 0; j < blurred.length; j++) {
      var b = blurred[j];
      b.style.backdropFilter = "";
      b.style.webkitBackdropFilter = "";
      if (b.dataset) delete b.dataset.zqBlur;
    }
    blurred = [];
  }
  function scheduleScan() {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(function () {
      scheduled = false;
      if (obs) obs.disconnect();
      try { scan(document.body); } finally { if (obs) obs.observe(document.documentElement, OBS_OPTS); }
    });
  }
  function startObserver() {
    if (obs) return;
    obs = new MutationObserver(scheduleScan);
    scheduleScan();
    obs.observe(document.documentElement, OBS_OPTS);
  }
  function stopObserver() {
    if (obs) { obs.disconnect(); obs = null; }
    unpatchAll();
  }
  /* alpha 热切换：开启 → 启动观察器全量重扫；关闭 → 全量还原并停扫 */
  function setPatch(alpha, blur) {
    var active = alpha > 0 && alpha < 1;
    var wasActive = patchAlpha > 0 && patchAlpha < 1;
    patchBlur = blur || 22;
    if (active === wasActive && (!active || alpha === patchAlpha)) { patchAlpha = alpha; return; }
    patchAlpha = alpha;
    if (active && !wasActive) startObserver();
    else if (active && wasActive) { unpatchAll(); scheduleScan(); }
    else if (!active && wasActive) stopObserver();
  }

  /* ==================== 热重载引擎（移植自 ZBar effects.js） ==================== */
  function findVarsLink() {
    return (
      document.querySelector("link[data-zq-vars]") ||
      document.querySelector('link[href*="zq-vars.css"]')
    );
  }
  var snapshot = {};
  function snapshotOf() {
    var cs = getComputedStyle(document.documentElement);
    var o = {};
    for (var i = 0; i < VAR_NAMES.length; i++) {
      o[VAR_NAMES[i]] = (cs.getPropertyValue(VAR_NAMES[i]) || "").trim();
    }
    return o;
  }
  function sameSnapshot(a, b) {
    for (var i = 0; i < VAR_NAMES.length; i++) {
      if (a[VAR_NAMES[i]] !== b[VAR_NAMES[i]]) return false;
    }
    return true;
  }
  function reloadVarsLink() {
    var link = findVarsLink();
    if (!link) return;
    var base = (link.getAttribute("href") || "").split("?")[0];
    link.setAttribute("href", base + "?t=" + Date.now());
  }
  function applySnapshot(now) {
    /* 透出门控属性：zq-theme.css 全部规则的选择器前缀 */
    if (now["--zq-translucent"] === "1") {
      document.documentElement.setAttribute("data-zq-translucent", "1");
    } else {
      document.documentElement.removeAttribute("data-zq-translucent");
    }
    /* 表面补丁 alpha */
    setPatch(num(now["--zq-alpha"], 0), num(now["--zq-blur"], 22));
    /* 壁纸 */
    wpOpacity = num(now["--zq-wallpaper-opacity"], 1);
    var url = urlOf(now["--zq-wallpaper-url"]);
    if (url) {
      if (url !== currentUrl || dead || !media) setWallpaper(url);
      else {
        applyFilter(); applyRate(); applyMask();
        /* 已就绪的媒体层实时跟随不透明度滑块；未就绪的由 onReady 淡入到目标值 */
        if (ready && media) media.style.opacity = String(wpOpacity);
      }
    } else if (currentUrl) {
      unmountMedia();
      currentUrl = "";
    } else {
      applyMask();
    }
  }
  function poll() {
    try {
      /* 注入层被页面意外清掉时自愈（dead 态除外：等换源后重建） */
      if (!dead && currentUrl && !mounted()) { setWallpaper(currentUrl); return; }
      /* 先取快照后重读：href 变更会让样式表立即进入"卸载失效 → 异步加载"窗口，
       * 先取快照读到的恒为上一轮稳定值（ZBar 踩坑结论） */
      var now = snapshotOf();
      /* 空值防御：任一变量为空串 = 本轮处于重载窗口，不可信，直接跳过 */
      for (var i = 0; i < VAR_NAMES.length; i++) {
        if (now[VAR_NAMES[i]] === "") return;
      }
      reloadVarsLink();
      if (sameSnapshot(snapshot, now)) return;
      snapshot = now;
      applySnapshot(now);
    } catch (e) { /* 单轮失败静默，下一轮重试 */ }
  }

  function start() {
    poll(); /* 立即建层，不等首个周期 */
    setInterval(poll, 1000);
  }
  if (document.body) start();
  else document.addEventListener("DOMContentLoaded", start, { once: true });
})();
"##;

// ───────────────────────── 单元测试 ─────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> BeautifyConfig {
        BeautifyConfig {
            enabled: true,
            theme: Some("tokyo-night".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn file_url_编码_空格与中文与反斜杠() {
        let p = Path::new(r"C:\Users\张 三\AppData\com.zcode-assistant.app\theme\zq-vars.css");
        let url = file_url(p);
        assert!(url.starts_with("file:///C:/Users/"));
        assert!(url.ends_with("/theme/zq-vars.css"));
        assert!(!url.contains(' '), "空格必须编码：{url}");
        assert!(!url.contains('张'), "中文必须编码：{url}");
        assert!(url.contains("%20"), "空格 → %20：{url}");
        assert!(url.contains("com.zcode-assistant.app"), "点号保留：{url}");
    }

    #[test]
    fn render_vars_仅含变量声明_含全部运行时变量() {
        let c = cfg();
        let css = render_vars_css(&c, Some("wallpaper.mp4"));
        for v in [
            "--zq-wallpaper-url",
            "--zq-wallpaper-opacity",
            "--zq-wp-brightness",
            "--zq-wp-saturate",
            "--zq-wp-blur",
            "--zq-mask-strength",
            "--zq-playback-rate",
            "--zq-translucent",
            "--zq-surface-alpha",
            "--zq-sidebar-alpha",
            "--zq-panel-alpha",
            "--zq-sidebar-right-alpha",
            "--zq-text-shadow",
            "--zq-alpha",
            "--zq-blur",
        ] {
            assert!(css.contains(v), "缺少 {v}：\n{css}");
        }
        // 无元素规则（热重载防闪烁的关键约束）：vars 文件不允许出现规则值语法
        assert!(!css.contains("color-mix"), "vars 不应含元素规则：\n{css}");
        assert!(!css.contains("!important"), "vars 不应含元素规则：\n{css}");
        assert!(css.contains("url(\"file:///"), "壁纸应为 url(file://...) 形式");
        // 主题预设色进入
        assert!(css.contains("--color-background: #1a1b26"));
    }

    #[test]
    fn render_vars_透出开关与补丁alpha联动() {
        let mut c = cfg();
        c.acrylic = false;
        c.surface_opacity = 0.6;
        let css = render_vars_css(&c, None);
        assert!(css.contains("--zq-translucent: 0"), "无毛玻璃无壁纸 → 关：\n{css}");
        assert!(css.contains("--zq-alpha: 0.000"), "补丁关闭：\n{css}");
        c.acrylic = true;
        let css = render_vars_css(&c, None);
        assert!(css.contains("--zq-translucent: 1"));
        assert!(css.contains("--zq-alpha: 0.600"));
    }

    #[test]
    fn render_vars_分区透明度回退surface() {
        let mut c = cfg();
        c.acrylic = true;
        c.surface_opacity = 0.5;
        // 三分区未设置 → 回退 surface
        let css = render_vars_css(&c, None);
        assert!(css.contains("--zq-sidebar-alpha: 0.500"));
        assert!(css.contains("--zq-panel-alpha: 0.500"));
        assert!(css.contains("--zq-sidebar-right-alpha: 0.500"));
        // 单独设置生效
        c.panel_opacity = Some(0.2);
        let css = render_vars_css(&c, None);
        assert!(css.contains("--zq-panel-alpha: 0.200"));
        assert!(css.contains("--zq-sidebar-alpha: 0.500"));
    }

    #[test]
    fn 模板版本解析与落盘幂等() {
        assert_eq!(template_version_of(THEME_CSS_TEMPLATE, "ZQ-THEME-V"), Some(1));
        assert_eq!(template_version_of(EFFECTS_JS_TEMPLATE, "ZQ-EFFECTS-V"), Some(1));
        assert_eq!(template_version_of("/* 无标记 */", "ZQ-THEME-V"), None);
    }

    #[test]
    fn 壁纸资产名校验() {
        assert_eq!(
            wallpaper_asset_name(Path::new("C:/a/demo.MP4")).as_deref(),
            Some("wallpaper.mp4")
        );
        assert_eq!(wallpaper_asset_name(Path::new("C:/a/demo.bmp")), None);
        assert!(validate_wallpaper(Path::new("C:/no-such-file.png")).is_err());
    }
}
