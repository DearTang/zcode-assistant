//! ZCode 美化：外置主题目录 + file:// 外链注入（借鉴 zai-floating-monitor/ZBar
//! 的 agent_theme 架构，注入管线保留本项目自研的 asar 原地补丁）。
//!
//! 注入策略：在 `out/renderer/index.html` 的 `</head>` 前插入一个
//! `<!--ZQ-THEME-BEGIN--> … <!--ZQ-THEME-END-->` 标记块，内含三个外链——
//! `zq-vars.css`（热文件，每秒被运行时重读）+ `zq-theme.css`（静态结构模板）
//! 两个 link 与 `zq-effects.js`（运行时脚本）一个 defer script，全部 file://
//! 指向 zcode-assistant 自身 app data 的 `beautify/theme/` 目录。asar 内不再
//! 打包任何 CSS/JS/背景图（旧版注入的 asar 内资产在应用时自动清除）。
//!
//! 由此获得的热更新能力（ZBar 验证过的架构）：
//! - 改参数（主题/颜色/字体/透明度/壁纸/滤镜）→ 只重写 `zq-vars.css`，
//!   zq-effects.js 每秒热重载，约 1 秒生效——见 [`save_params`]，不触碰 asar、
//!   不关闭 ZCode；
//! - 「应用美化」只剩两件事：注入/刷新外链块（首次或 ZCode 升级后）+ 重启
//!   ZCode 让外链被加载；静态模板升级也只需重启（模板版本化落盘）。
//!
//! 还原与备份策略不变：版本感知备份（ZCode 升级后首次应用自动重建）、还原前
//! 校验备份版本与当前安装一致（防旧版本 asar 覆盖新版本安装）。
use crate::zcode::asar;
use crate::zcode::process;
use crate::zcode::theme_assets;
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const INDEX_HTML: &str = "out/renderer/index.html";
const ASSETS_DIR: &str = "out/renderer/assets";

/// 注入块起止标记（head 单块，剥离后重插保证幂等）
pub const INJECT_BEGIN: &str = "<!--ZQ-THEME-BEGIN-->";
pub const INJECT_END: &str = "<!--ZQ-THEME-END-->";

/// 旧版注入方案的 asar 内资产（应用新方案时从 index.html 与资产表一并清除）
const LEGACY_CSS: &str = "zcode-custom.css";
const LEGACY_JS: &str = "zcode-custom.js";
const LEGACY_BG_PREFIX: &str = "zcode-bg.";

/// 美化配置。持久化到 zcode-assistant app data，不写 ZCode 的 setting.json。
/// 全部参数走 `save_params` 热生效；`apply` 仅负责把外链块写进 asar（首次 /
/// ZCode 升级后 / 手动修复）。
#[derive(Serialize, Deserialize, Clone)]
pub struct BeautifyConfig {
    /// 是否启用（应用美化）。
    #[serde(default)]
    pub enabled: bool,
    /// 预设主题 id（midnight / nord / dracula / gruvbox / tokyo-night / rose-pine / none）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// UI 字体族（覆盖 --font-sans），如 "Microsoft YaHei UI"。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_font: Option<String>,
    /// 等宽字体族（覆盖 --font-mono），如 "Cascadia Code"。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mono_font: Option<String>,
    /// 自定义背景色（覆盖 --color-background），如 "#0d1117"。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg_color: Option<String>,
    /// 自定义主色调（覆盖 --color-primary），如 "#58a6ff"。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_color: Option<String>,
    /// 毛玻璃：让主要表面半透明，透出 ZCode 在 Windows 上默认启用的 acrylic 材质。
    #[serde(default)]
    pub acrylic: bool,
    /// 全局表面/氛围不透明度（0.2–1.0），毛玻璃或壁纸启用时生效；越大越实。
    #[serde(default = "default_surface_opacity")]
    pub surface_opacity: f32,
    /// 左栏透明度（0–1）；None = 跟随 surface_opacity。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_opacity: Option<f32>,
    /// 对话区透明度（0–1）；None = 跟随 surface_opacity。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel_opacity: Option<f32>,
    /// 右栏透明度（0–1）；None = 跟随 surface_opacity。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidebar_right_opacity: Option<f32>,
    /// 文字描边强度（0–1，0=关）：壁纸过亮/过暗时把前景文字从背景里托出来。
    #[serde(default)]
    pub text_shadow: f32,
    /// 壁纸（本地绝对路径，图片或视频 mp4/webm/mov）；优先于旧字段 bg_image。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallpaper: Option<String>,
    /// 旧字段（背景图路径），保留兼容旧 config.json 与模板，读取时被 wallpaper 覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg_image: Option<String>,
    /// 壁纸图层不透明度（0.1–1.0）。
    #[serde(default = "default_bg_image_opacity")]
    pub bg_image_opacity: f32,
    /// 壁纸亮度滤镜（0.2–2.0，默认 1.1）。
    #[serde(default = "default_wp_brightness")]
    pub wp_brightness: f32,
    /// 壁纸饱和度滤镜（0–2.0，默认 1.4）。
    #[serde(default = "default_wp_saturate")]
    pub wp_saturate: f32,
    /// 壁纸模糊滤镜（0–30px，默认 0）。
    #[serde(default = "default_wp_blur")]
    pub wp_blur: f32,
    /// 压暗遮罩强度（0–0.9）：壁纸过亮时整体压暗保证前景可读。
    #[serde(default)]
    pub mask_strength: f32,
    /// 视频壁纸播放速率（0.25–4.0，默认 1）。
    #[serde(default = "default_playback_rate")]
    pub playback_rate: f32,
}

fn default_surface_opacity() -> f32 {
    0.72
}

fn default_bg_image_opacity() -> f32 {
    1.0
}

fn default_wp_brightness() -> f32 {
    1.1
}

fn default_wp_saturate() -> f32 {
    1.4
}

fn default_wp_blur() -> f32 {
    0.0
}

fn default_playback_rate() -> f32 {
    1.0
}

impl Default for BeautifyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            theme: None,
            ui_font: None,
            mono_font: None,
            bg_color: None,
            primary_color: None,
            acrylic: false,
            surface_opacity: default_surface_opacity(),
            sidebar_opacity: None,
            panel_opacity: None,
            sidebar_right_opacity: None,
            text_shadow: 0.0,
            wallpaper: None,
            bg_image: None,
            bg_image_opacity: default_bg_image_opacity(),
            wp_brightness: default_wp_brightness(),
            wp_saturate: default_wp_saturate(),
            wp_blur: default_wp_blur(),
            mask_strength: 0.0,
            playback_rate: default_playback_rate(),
        }
    }
}

// ───────────────────────── 预设主题 ─────────────────────────

/// 返回某预设主题的 (CSS 变量名, 值) 列表。覆盖背景 / 背景alt / 表面 / 卡片 /
/// 前景 / 主色 / 边框 / 品牌色 / 强调色 + 面板 / 侧栏 / 头部 / 输入栏
/// （后四者默认引用 neutral-100/200 等浅色 token，不覆盖会留下浅色侧栏/输入栏）。
pub fn preset_vars(theme: &str) -> Option<Vec<(&'static str, &'static str)>> {
    // 各 token: 背景 / 背景alt / 表面 / 卡片 / 前景 / 主色 / 边框 / 品牌色 / 强调色
    //          / 面板 / 侧栏 / 头部 / 输入栏
    let v = match theme {
        "midnight" => vec![
            ("--color-background", "#0b1020"),
            ("--color-background-alt", "#121a33"),
            ("--color-surface", "#151d3b"),
            ("--color-card", "#1a2348"),
            ("--color-foreground", "#c9d4f0"),
            ("--color-primary", "#e8ecff"),
            ("--color-border", "#2a3558"),
            ("--color-brand", "#7c8cff"),
            ("--color-accent", "#5b8cff"),
            ("--color-panel", "var(--color-surface)"),
            ("--color-sidebar", "var(--color-surface)"),
            ("--color-header", "var(--color-surface)"),
            ("--color-input", "var(--color-surface)"),
        ],
        "nord" => vec![
            ("--color-background", "#2e3440"),
            ("--color-background-alt", "#3b4252"),
            ("--color-surface", "#3b4252"),
            ("--color-card", "#434c5e"),
            ("--color-foreground", "#d8dee9"),
            ("--color-primary", "#eceff4"),
            ("--color-border", "#434c5e"),
            ("--color-brand", "#88c0d0"),
            ("--color-accent", "#81a1c1"),
            ("--color-panel", "var(--color-surface)"),
            ("--color-sidebar", "var(--color-surface)"),
            ("--color-header", "var(--color-surface)"),
            ("--color-input", "var(--color-surface)"),
        ],
        "dracula" => vec![
            ("--color-background", "#282a36"),
            ("--color-background-alt", "#2e3140"),
            ("--color-surface", "#343746"),
            ("--color-card", "#383b4d"),
            ("--color-foreground", "#f8f8f2"),
            ("--color-primary", "#ffffff"),
            ("--color-border", "#44475a"),
            ("--color-brand", "#bd93f9"),
            ("--color-accent", "#ff79c6"),
            ("--color-panel", "var(--color-surface)"),
            ("--color-sidebar", "var(--color-surface)"),
            ("--color-header", "var(--color-surface)"),
            ("--color-input", "var(--color-surface)"),
        ],
        "gruvbox" => vec![
            ("--color-background", "#282828"),
            ("--color-background-alt", "#32302f"),
            ("--color-surface", "#3c3836"),
            ("--color-card", "#45403d"),
            ("--color-foreground", "#ebdbb2"),
            ("--color-primary", "#fbf1c7"),
            ("--color-border", "#504945"),
            ("--color-brand", "#fabd2f"),
            ("--color-accent", "#fe8019"),
            ("--color-panel", "var(--color-surface)"),
            ("--color-sidebar", "var(--color-surface)"),
            ("--color-header", "var(--color-surface)"),
            ("--color-input", "var(--color-surface)"),
        ],
        "tokyo-night" => vec![
            ("--color-background", "#1a1b26"),
            ("--color-background-alt", "#16161e"),
            ("--color-surface", "#24283b"),
            ("--color-card", "#292e42"),
            ("--color-foreground", "#c0caf5"),
            ("--color-primary", "#c0caf5"),
            ("--color-border", "#414868"),
            ("--color-brand", "#7aa2f7"),
            ("--color-accent", "#bb9af7"),
            ("--color-panel", "var(--color-surface)"),
            ("--color-sidebar", "var(--color-surface)"),
            ("--color-header", "var(--color-surface)"),
            ("--color-input", "var(--color-surface)"),
        ],
        "rose-pine" => vec![
            ("--color-background", "#191724"),
            ("--color-background-alt", "#1f1d2e"),
            ("--color-surface", "#26233a"),
            ("--color-card", "#2a2740"),
            ("--color-foreground", "#e0def4"),
            ("--color-primary", "#e0def4"),
            ("--color-border", "#403d52"),
            ("--color-brand", "#c4a7e7"),
            ("--color-accent", "#ebbcba"),
            ("--color-panel", "var(--color-surface)"),
            ("--color-sidebar", "var(--color-surface)"),
            ("--color-header", "var(--color-surface)"),
            ("--color-input", "var(--color-surface)"),
        ],
        _ => return None,
    };
    Some(v)
}

/// 可选预设 id 列表（供前端渲染选择）。
pub fn preset_list() -> Vec<(&'static str, &'static str)> {
    vec![
        ("midnight", "午夜蓝"),
        ("nord", "Nord 极地"),
        ("dracula", "Dracula 吸血鬼"),
        ("gruvbox", "Gruvbox 复古"),
        ("tokyo-night", "东京夜"),
        ("rose-pine", "Rose Pine 玫瑰松"),
    ]
}

/// UI 字体 font-family 栈（theme_assets 渲染 vars 用）。
pub fn font_stack(font: &str) -> String {
    format!(
        "\"{}\", ui-sans-serif, system-ui, -apple-system, \"Segoe UI\", sans-serif",
        font
    )
}

/// 等宽字体 font-family 栈。
pub fn mono_stack(font: &str) -> String {
    format!(
        "\"{}\", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
        font
    )
}

// ───────────────────────── 注入 ─────────────────────────

/// 当前 app.asar 是否已注入新方案外链块（index.html 含块标记）。
/// 旧版方案（asar 内打包 css/js）不再视为已注入——重新「应用美化」即自动升级。
pub fn is_installed(asar: &Path) -> bool {
    asar::read_file(asar, INDEX_HTML)
        .map(|b| String::from_utf8_lossy(&b).contains(INJECT_BEGIN))
        .unwrap_or(false)
}

/// 构建注入块：三个 file:// 外链（vars / theme / effects），指向主题目录。
fn inject_block() -> Result<String> {
    let (vars, theme, effects) = theme_assets::link_urls()?;
    Ok(format!(
        "{INJECT_BEGIN}\n    <link rel=\"stylesheet\" data-zq-vars href=\"{vars}\">\n    <link rel=\"stylesheet\" href=\"{theme}\">\n    <script defer src=\"{effects}\"></script>\n    {INJECT_END}\n"
    ))
}

/// 剥离全部既有注入块（幂等重装的基础）。
fn strip_inject_blocks(html: &str) -> String {
    let mut out = html.to_string();
    while let Some(b) = out.find(INJECT_BEGIN) {
        match out[b..].find(INJECT_END) {
            Some(e) => {
                let end = b + e + INJECT_END.len();
                out.replace_range(b..end, "");
            }
            None => {
                // 标记残缺（有 BEGIN 无 END，写盘截断等极端场景）：
                // 只剥 BEGIN 标记本身，其余内容保留（随后正常走锚点插入）
                out.replace_range(b..b + INJECT_BEGIN.len(), "");
                break;
            }
        }
    }
    out
}

/// 在 index.html 的 `</head>` 前插入注入块。幂等：先剥离旧块与旧版方案的
/// 两个 asar 内资产标签，再插入新块——旧版 asar 应用一次即完成升级。
fn inject_html(html: &str, block: &str) -> Result<String> {
    let stripped = strip_inject_blocks(html);
    // 旧版注入的两个标签按当年写出的精确格式剥离（见旧版 inject_html）
    let legacy_css_tag = format!("    <link rel=\"stylesheet\" href=\"./assets/{LEGACY_CSS}\">\n");
    let legacy_js_tag = format!("    <script defer src=\"./assets/{LEGACY_JS}\"></script>\n");
    let stripped = stripped.replace(&legacy_css_tag, "").replace(&legacy_js_tag, "");
    let Some(idx) = stripped.find("</head>") else {
        return Err(anyhow!("index.html 未找到 </head> 注入锚点"));
    };
    let mut new_html = String::with_capacity(stripped.len() + block.len());
    new_html.push_str(&stripped[..idx]);
    new_html.push_str(block);
    new_html.push_str(&stripped[idx..]);
    Ok(new_html)
}

/// 在内存中构建 asar 原地补丁的修改集（不解包、不落盘）：
/// - index.html 注入外链块；
/// - 删除旧版方案打入 asar 的资产（zcode-custom.css / zcode-custom.js / zcode-bg.*）。
fn build_mods(asar_path: &Path) -> Result<Vec<(String, asar::FileMod)>> {
    let mut mods: Vec<(String, asar::FileMod)> = Vec::new();

    let html = asar::read_file(asar_path, INDEX_HTML)
        .with_context(|| format!("读取 {} 失败", INDEX_HTML))?;
    let block = inject_block()?;
    let new_html = inject_html(&String::from_utf8_lossy(&html), &block)?;
    mods.push((
        INDEX_HTML.to_string(),
        asar::FileMod::Write(new_html.into_bytes()),
    ));

    // 清理旧版注入的 asar 内资产（不存在则跳过）
    if let Ok(names) = asar::list_dir(asar_path, ASSETS_DIR) {
        for name in names {
            if name == LEGACY_CSS || name == LEGACY_JS || name.starts_with(LEGACY_BG_PREFIX) {
                mods.push((format!("{ASSETS_DIR}/{name}"), asar::FileMod::Remove));
            }
        }
    }
    Ok(mods)
}

// ───────────────────────── 配置持久化 ─────────────────────────

/// 配置文件路径（zcode-assistant app data / beautify/config.json）。
pub fn config_path() -> Result<std::path::PathBuf> {
    Ok(asar::backup_dir()?.join("config.json"))
}

/// 读取美化配置；不存在则返回默认空配置。
pub fn read_config() -> Result<BeautifyConfig> {
    let p = config_path()?;
    if !p.exists() {
        return Ok(BeautifyConfig::default());
    }
    let txt = fs::read_to_string(&p)?;
    let cfg: BeautifyConfig = serde_json::from_str(&txt).context("美化配置解析失败")?;
    Ok(cfg)
}

/// 写入美化配置。
pub fn write_config(cfg: &BeautifyConfig) -> Result<()> {
    let p = config_path()?;
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    let txt = serde_json::to_string_pretty(cfg)?;
    fs::write(&p, format!("{txt}\n"))?;
    Ok(())
}

// ───────────────────────── 模板（命名配置快照）─────────────────────────

/// 美化模板：一份带名称的配置快照。
#[derive(Serialize, Deserialize, Clone)]
pub struct BeautifyTemplate {
    pub name: String,
    pub config: BeautifyConfig,
}

/// 模板文件路径（beautify/templates.json）。
pub fn templates_path() -> Result<std::path::PathBuf> {
    Ok(asar::backup_dir()?.join("templates.json"))
}

/// 读取模板列表；文件不存在或损坏时返回空列表。
pub fn read_templates() -> Vec<BeautifyTemplate> {
    templates_path()
        .ok()
        .filter(|p| p.exists())
        .and_then(|p| fs::read_to_string(&p).ok())
        .and_then(|txt| serde_json::from_str(&txt).ok())
        .unwrap_or_default()
}

/// 写入模板列表。
pub fn write_templates(list: &[BeautifyTemplate]) -> Result<()> {
    let p = templates_path()?;
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    let txt = serde_json::to_string_pretty(list)?;
    fs::write(&p, format!("{txt}\n"))?;
    Ok(())
}

// ───────────────────────── 高层 apply / save_params / restore ─────────────────────────

/// 校验并规整壁纸源（apply 与 save_params 共用的前置检查）。
fn validated_wallpaper(cfg: &BeautifyConfig) -> Result<Option<String>> {
    let src = theme_assets::wallpaper_source(cfg);
    if let Some(p) = &src {
        theme_assets::validate_wallpaper(Path::new(p))?;
    }
    Ok(src)
}

/// 完整应用美化（把外链注入块写进 app.asar，秒级原地补丁）：
/// 壁纸预校验 → 版本感知备份 → 主题资产落盘（模板 + vars + 壁纸副本）→
/// kill ZCode 解锁 → asar 原地补丁 → 原子替换。
/// 不负责重启 ZCode（由命令层 emit restart 事件，前端走全局 RestartDialog）。
pub fn apply(cfg: &BeautifyConfig) -> Result<()> {
    let wp_src = validated_wallpaper(cfg)?;

    // 1. 版本感知备份（ZCode 升级后首次应用会用官方包自动重建）
    ensure_backup_versioned()?;

    // 2. 主题资产落盘（先于 asar 补丁：任何失败都不触碰 asar）
    theme_assets::ensure_static_templates()?;
    let asset = theme_assets::sync_wallpaper(wp_src.as_deref().map(Path::new))?;
    theme_assets::write_vars_css(&theme_assets::render_vars_css(cfg, asset.as_deref()))?;

    // 3. 构建修改集（读 asar 内 index.html + 注入块，全部在内存完成）
    let asar_path = asar::asar_path()?;
    let resources_dir = asar_path
        .parent()
        .ok_or_else(|| anyhow!("app.asar 无父目录"))?;
    let mods = build_mods(&asar_path)?;

    // 4. 关闭 ZCode，释放 app.asar 文件锁（放在修改集构建之后，缩短停机窗口）
    let _ = process::kill_zcode();

    // 5. 原地补丁到同目录临时文件（同卷，便于原子 rename）→ 原子替换
    let new_asar = resources_dir.join("app.asar.new");
    asar::patch(&asar_path, &mods, &new_asar)?;
    fs::rename(&new_asar, &asar_path)
        .with_context(|| "替换 app.asar 失败（可能 ZCode 仍在运行占用文件）")?;
    Ok(())
}

/// 热保存参数（本方案的日常路径）：只落盘主题资产（zq-vars.css + 壁纸副本），
/// **完全不触碰 app.asar、不关闭 ZCode**。已注入的前提下 zq-effects.js 每秒
/// 热重载 zq-vars.css，参数约 1 秒生效；未注入时仅持久化，待 apply 后生效。
pub fn save_params(cfg: &BeautifyConfig) -> Result<()> {
    let wp_src = validated_wallpaper(cfg)?;
    theme_assets::ensure_static_templates()?;
    let asset = theme_assets::sync_wallpaper(wp_src.as_deref().map(Path::new))?;
    theme_assets::write_vars_css(&theme_assets::render_vars_css(cfg, asset.as_deref()))?;
    write_config(cfg)?;
    Ok(())
}

/// 原始备份对应的 ZCode 版本（读备份 asar 的 package.json）。
/// 无备份或备份损坏读不出均为 None（状态层配合 has_backup 区分展示）。
pub fn backup_version() -> Option<String> {
    let p = asar::origin_backup_path().ok()?;
    if !p.exists() {
        return None;
    }
    asar::read_zcode_version(&p)
}

/// 版本感知备份（apply 前调用，此时 app.asar 尚未被本次注入）：
/// - 无备份 → 备份当前 app.asar；
/// - 备份版本与当前一致 → 跳过；
/// - 版本不一致（ZCode 升级替换了 app.asar）且当前未注入（新版本官方纯净包）
///   → 用当前包重建备份。升级后第一次「应用美化」自动完成备份换新；
/// - 版本不一致但当前已注入（历史遗留：升级后注入过、备份没跟上）→ 无法安全
///   重建（当前非纯净包），保留旧备份；还原由 restore 的版本防护拦下。
fn ensure_backup_versioned() -> Result<()> {
    let src = asar::asar_path()?;
    if asar::ensure_backup()? {
        return Ok(()); // 本次新建了备份（必然取自当前包）
    }
    let cur = asar::read_zcode_version(&src);
    let bak = backup_version();
    if cur.is_some() && cur != bak && !is_installed(&src) {
        let origin = asar::origin_backup_path()?;
        fs::copy(&src, &origin).with_context(|| {
            format!("重建备份失败: {} -> {}", src.display(), origin.display())
        })?;
    }
    Ok(())
}

/// 还原：备份版本与当前安装一致才放行（防止旧版本 asar 覆盖新版本安装、
/// 导致 ZCode 主程序与渲染层版本错配而损坏）→ kill ZCode → 备份覆盖 app.asar。
pub fn restore() -> Result<()> {
    let src = asar::asar_path()?;
    let cur = asar::read_zcode_version(&src);
    let bak = backup_version();
    if let (Some(c), Some(b)) = (&cur, &bak) {
        if c != b {
            anyhow::bail!(
                "原始备份属于 ZCode v{b}，与当前安装的 v{c} 不一致：还原会把旧版本文件覆盖到新版本上，可能导致 ZCode 无法启动。\
                 请先重装/修复 ZCode 恢复官方文件，再重新应用美化（会自动重建匹配版本的备份）。"
            );
        }
    }
    let _ = process::kill_zcode();
    asar::restore_from_backup()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成 asar（含旧版注入残留），build_mods + patch 全链路离线验证：
    /// 外链块注入（file:/// 链接 + 标记）、旧版 css/js 标签剥离、旧版资产
    /// （zcode-custom.css/js、zcode-bg.*）条目删除、幂等重装。
    #[test]
    fn build_mods_and_patch_offline() {
        let work = std::env::temp_dir().join("zcode_beautify_patch_test");
        let _ = fs::remove_dir_all(&work);

        // 合成 asar：index.html（带旧版注入标签）+ assets（旧资产 + 旧背景图）
        let src_dir = work.join("src");
        fs::create_dir_all(src_dir.join(ASSETS_DIR)).unwrap();
        let old_html = format!(
            "<!doctype html><html><head><title>ZCode</title>{}{}</head><body><div id=\"root\"></div></body></html>",
            format!("    <link rel=\"stylesheet\" href=\"./assets/{LEGACY_CSS}\">\n"),
            format!("    <script defer src=\"./assets/{LEGACY_JS}\"></script>\n"),
        );
        fs::write(src_dir.join(INDEX_HTML), old_html).unwrap();
        fs::write(src_dir.join(ASSETS_DIR).join(LEGACY_CSS), b"old css").unwrap();
        fs::write(src_dir.join(ASSETS_DIR).join(LEGACY_JS), b"old js").unwrap();
        fs::write(src_dir.join(ASSETS_DIR).join("zcode-bg.jpg"), b"old bg").unwrap();
        fs::write(src_dir.join(ASSETS_DIR).join("styles-abc.css"), b"").unwrap();
        let asar = work.join("app.asar");
        asar::pack(&src_dir, &asar, &std::collections::HashSet::new()).unwrap();

        // 第一次应用
        let mods = build_mods(&asar).unwrap();
        let patched = work.join("patched.asar");
        asar::patch(&asar, &mods, &patched).unwrap();

        let read = |p: &str, a: &std::path::Path| {
            String::from_utf8_lossy(&asar::read_file(a, p).unwrap()).to_string()
        };
        assert!(is_installed(&patched), "注入标记未写入");
        let html = read(INDEX_HTML, &patched);
        assert!(html.contains("file:///"), "外链应为 file:// URL：{html}");
        assert!(html.contains("zq-vars.css"), "变量外链缺失");
        assert!(html.contains("zq-theme.css"), "主题外链缺失");
        assert!(html.contains("zq-effects.js"), "运行时外链缺失");
        assert!(html.contains("data-zq-vars"), "热重载定位标记缺失");
        assert!(
            html.find(INJECT_BEGIN).unwrap() < html.find("</head>").unwrap(),
            "注入块应在 </head> 之前"
        );
        assert!(!html.contains(LEGACY_CSS), "旧版 css 标签未剥离：{html}");
        assert!(!html.contains(LEGACY_JS), "旧版 js 标签未剥离：{html}");
        assert!(
            asar::read_file(&patched, &format!("{ASSETS_DIR}/{LEGACY_CSS}")).is_err(),
            "旧版 css 资产条目未删除"
        );
        assert!(
            asar::read_file(&patched, &format!("{ASSETS_DIR}/{LEGACY_JS}")).is_err(),
            "旧版 js 资产条目未删除"
        );
        assert!(
            asar::read_file(&patched, &format!("{ASSETS_DIR}/zcode-bg.jpg")).is_err(),
            "旧背景图条目未删除"
        );
        assert!(
            asar::read_file(&patched, &format!("{ASSETS_DIR}/styles-abc.css")).is_ok(),
            "ZCode 自有资产不应被误删"
        );

        // 幂等：对已注入包重复 build_mods + patch，外链块不重复
        let mods2 = build_mods(&patched).unwrap();
        let patched2 = work.join("patched2.asar");
        asar::patch(&patched, &mods2, &patched2).unwrap();
        let html2 = read(INDEX_HTML, &patched2);
        assert_eq!(
            html2.matches(INJECT_BEGIN).count(),
            1,
            "注入块重复：{html2}"
        );
        assert_eq!(html2.matches("zq-vars.css").count(), 1);
        assert_eq!(html2.matches("zq-effects.js").count(), 1);

        let _ = fs::remove_dir_all(&work);
    }

    /// 注入块剥离的边界：残缺块（有 BEGIN 无 END）也能安全剥离，不死循环。
    #[test]
    fn inject_html_残缺块安全剥离() {
        let html = "<html><head><!--ZQ-THEME-BEGIN--><link></head><body></body></html>";
        let out = inject_html(html, "<!--ZQ-THEME-BEGIN-->x<!--ZQ-THEME-END-->\n").unwrap();
        assert_eq!(out.matches(INJECT_BEGIN).count(), 1, "{out}");
        assert!(out.contains("</head>"), "文档结构不应被破坏：{out}");
        // 缺 </head> 锚点报错
        assert!(inject_html("<html><body></body></html>", "x").is_err());
    }

    /// 真机验证①：应用美化到真实 ZCode 并启动（需人工观察）。
    /// 手动跑：`cargo test --lib apply_real_beautify -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn apply_real_beautify() {
        let cfg = BeautifyConfig {
            enabled: true,
            theme: Some("tokyo-night".to_string()),
            acrylic: true,
            ..Default::default()
        };
        apply(&cfg).expect("应用美化失败");
        println!("✅ 已注入外链并落盘主题资产。启动 ZCode 确认效果...");
        let _ = process::launch_zcode();
        println!("确认后运行 restore_real 还原：cargo test --lib restore_real -- --ignored --nocapture");
    }

    /// 真机验证②：还原真实 ZCode 到官方 app.asar 并启动。
    #[test]
    #[ignore]
    fn restore_real() {
        println!("还原真实 ZCode 到官方 app.asar...");
        restore().expect("还原失败");
        println!("✅ 已还原。启动 ZCode 确认恢复官方外观...");
        let _ = process::launch_zcode();
    }

    /// 只读校验：真实 app.asar 内 index.html 存在 `</head>` 注入锚点。
    /// 若 ZCode 未安装则跳过（不视为失败）。防止未来 ZCode 版本变更 index.html
    /// 结构导致注入锚点失效而未被察觉。
    #[test]
    fn real_index_html_has_inject_anchor() {
        let Ok(asar) = asar::asar_path() else {
            println!("未找到 ZCode 安装，跳过锚点校验");
            return;
        };
        if !asar.exists() {
            println!("app.asar 不存在，跳过锚点校验: {}", asar.display());
            return;
        }
        let bytes = asar::read_file(&asar, INDEX_HTML).expect("读取 index.html 失败");
        let html = String::from_utf8_lossy(&bytes);
        assert!(
            html.contains("</head>"),
            "index.html 未找到 </head> 注入锚点，注入策略需要调整"
        );
        assert!(
            html.contains("styles-") && html.contains(".css"),
            "index.html 未见主样式表引用，结构可能已变化"
        );
        println!("✓ index.html 含 </head> 锚点（{} 字节）", bytes.len());
    }

    /// 配置兼容：旧版 config.json（无新字段）能解析且新字段取默认值。
    #[test]
    fn config_旧版json兼容() {
        let old = r#"{
  "enabled": true,
  "theme": "nord",
  "acrylic": true,
  "surface_opacity": 0.5,
  "bg_image": "C:/pic/a.jpg",
  "bg_image_opacity": 0.8
}"#;
        let cfg: BeautifyConfig = serde_json::from_str(old).unwrap();
        assert_eq!(cfg.theme.as_deref(), Some("nord"));
        assert_eq!(cfg.surface_opacity, 0.5);
        assert_eq!(cfg.bg_image.as_deref(), Some("C:/pic/a.jpg"));
        assert_eq!(cfg.bg_image_opacity, 0.8);
        assert_eq!(cfg.sidebar_opacity, None, "新分区字段缺省为 None");
        assert_eq!(cfg.wp_brightness, 1.1);
        assert_eq!(cfg.playback_rate, 1.0);
        // wallpaper_source：wallpaper 优先，bg_image 兜底
        assert_eq!(
            theme_assets::wallpaper_source(&cfg).as_deref(),
            Some("C:/pic/a.jpg")
        );
        let mut with_wp = cfg.clone();
        with_wp.wallpaper = Some("C:/v/a.mp4".to_string());
        assert_eq!(
            theme_assets::wallpaper_source(&with_wp).as_deref(),
            Some("C:/v/a.mp4")
        );
    }
}
