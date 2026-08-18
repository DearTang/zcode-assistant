//! ZCode 美化：根据配置生成 zcode-custom.css 并注入解包后的 index.html。
//!
//! 注入策略（最小侵入、最易还原）：在 `out/renderer/index.html` 的 `</head>` 前
//! 插入 `<link rel="stylesheet" href="./assets/zcode-custom.css">`，并把自定义 CSS
//! 写到 `out/renderer/assets/zcode-custom.css`。因该 link 在主样式表之后加载，
//! CSS 源顺序取胜，可覆盖 ZCode 自带样式。
//!
//! 换肤靠覆盖 CSS 变量（ZCode 主样式表用 ~501 个 token，亮色 `:root,:host`、暗色 `.dark`）。
//! 完整性校验已关闭，重打包安全（详见 asar.rs）。
//!
//! 毛玻璃：ZCode 在 Windows 上默认启用 acrylic 窗口材质，但被不透明的表面 token
//! （--color-background/-panel/-sidebar/-header）盖住；这里用 color-mix 混入透明让其透出。
//! 背景图：复制到 assets/zcode-bg.<ext> 后用 body::before 固定图层承载，透过半透明表面显现。
//!
//! 原则上同时覆盖 `:root,:host` 与 `.dark` 两个作用域，保证亮/暗模式都生效。
use crate::zcode::asar;
use crate::zcode::process;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const INDEX_HTML: &str = "out/renderer/index.html";
const CUSTOM_CSS_NAME: &str = "zcode-custom.css";
const INJECT_MARK: &str = "zcode-custom.css"; // 用于判断是否已注入

/// 美化配置。持久化到 zcode-assistant app data，不写 ZCode 的 setting.json。
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
    /// 表面不透明度（0.2–1.0），毛玻璃或背景图启用时生效；越大越实。
    #[serde(default = "default_surface_opacity")]
    pub surface_opacity: f32,
    /// 背景图源文件（本地绝对路径），应用时复制进 asar 的 assets/zcode-bg.<ext>。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg_image: Option<String>,
    /// 背景图图层不透明度（0.1–1.0）。
    #[serde(default = "default_bg_image_opacity")]
    pub bg_image_opacity: f32,
}

fn default_surface_opacity() -> f32 {
    0.72
}

fn default_bg_image_opacity() -> f32 {
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
            bg_image: None,
            bg_image_opacity: default_bg_image_opacity(),
        }
    }
}

// ───────────────────────── 预设主题 ─────────────────────────

/// 返回某预设主题的 (CSS 变量名, 值) 列表。核心 token 集合，覆盖背景/表面/前景/主色/边框/品牌色。
fn preset_vars(theme: &str) -> Option<Vec<(&'static str, &'static str)>> {
    // 各 token: 背景 / 背景alt / 表面 / 卡片 / 前景 / 主色 / 边框 / 品牌色 / 强调色
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

fn font_stack(font: &str) -> String {
    format!(
        "\"{}\", ui-sans-serif, system-ui, -apple-system, \"Segoe UI\", sans-serif",
        font
    )
}

fn mono_stack(font: &str) -> String {
    format!(
        "\"{}\", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
        font
    )
}

// ───────────────────────── CSS 生成 ─────────────────────────

/// 允许的背景图扩展名（复制进 asar 后由 CSS 引用）。
pub const BG_IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif"];

/// 背景图在 assets/ 内的文件名（沿用源扩展名）。非法扩展名返回 None。
pub fn bg_image_asset_name(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    BG_IMAGE_EXTS
        .contains(&ext.as_str())
        .then(|| format!("zcode-bg.{ext}"))
}

/// 校验背景图：文件存在 + 扩展名受支持。成功返回 asar 内资源文件名。
pub fn validate_bg_image(path: &Path) -> Result<String> {
    if !path.exists() {
        anyhow::bail!("背景图不存在：{}", path.display());
    }
    bg_image_asset_name(path).ok_or_else(|| {
        anyhow::anyhow!("不支持的背景图格式（支持 png / jpg / jpeg / webp / gif）")
    })
}

/// 取预设主题的背景色（供毛玻璃混色用）。
fn preset_bg_color(theme: &str) -> Option<&'static str> {
    if theme == "none" {
        return None;
    }
    preset_vars(theme)?
        .iter()
        .find(|(k, _)| *k == "--color-background")
        .map(|(_, v)| *v)
}

/// 生成某一作用域（亮/暗）的表面半透明覆盖声明。
/// 基色优先级：自定义/主题背景色（统一染色 panel/sidebar/header）> ZCode 默认色板。
/// 不直接引用自身 token（`--color-background: ... var(--color-background)` 会构成循环），
/// 而是引用底层调色板 token 或已知的自定义色。
fn frosted_block(light: bool, base: Option<&str>, alpha_pct: i32) -> String {
    let mix = |c: &str| format!("color-mix(in oklab, {} {}%, transparent)", c, alpha_pct);
    // ZCode 默认：亮 bg=neutral-50、panel/sidebar/header=neutral-100；
    //            暗 bg/panel/header=neutral-900、sidebar=neutral-950。
    let (bg, side, head) = match (light, base) {
        (_, Some(c)) => (mix(c), mix(c), mix(c)),
        (true, None) => (
            mix("var(--color-neutral-50)"),
            mix("var(--color-neutral-100)"),
            mix("var(--color-neutral-100)"),
        ),
        (false, None) => (
            mix("var(--color-neutral-900)"),
            mix("var(--color-neutral-950)"),
            mix("var(--color-neutral-900)"),
        ),
    };
    format!(
        "  --color-background: {bg};\n  --color-panel: {side};\n  --color-sidebar: {side};\n  --color-header: {head};\n"
    )
}

/// 根据配置生成 zcode-custom.css 内容。
pub fn generate_css(cfg: &BeautifyConfig) -> String {
    let mut vars: Vec<(String, String)> = Vec::new();

    if let Some(t) = &cfg.theme {
        if t == "none" {
            // 显式不应用主题
        } else if let Some(preset) = preset_vars(t) {
            for (k, v) in preset {
                vars.push((k.to_string(), v.to_string()));
            }
        }
    }
    if let Some(f) = &cfg.ui_font {
        vars.push(("--font-sans".to_string(), font_stack(f)));
    }
    if let Some(f) = &cfg.mono_font {
        vars.push(("--font-mono".to_string(), mono_stack(f)));
    }
    if let Some(c) = &cfg.bg_color {
        vars.push(("--color-background".to_string(), c.clone()));
    }
    if let Some(c) = &cfg.primary_color {
        vars.push(("--color-primary".to_string(), c.clone()));
    }

    // 毛玻璃或背景图任一启用时，都需要让表面半透明（否则亚克力/背景图被不透明底色盖住）
    let translucent = cfg.acrylic || cfg.bg_image.is_some();
    let bg_asset = cfg
        .bg_image
        .as_deref()
        .map(Path::new)
        .and_then(bg_image_asset_name);

    if vars.is_empty() && !translucent && bg_asset.is_none() {
        return "/* zcode-custom.css：当前无生效配置 */\n".to_string();
    }

    let mut s = String::new();
    s.push_str("/* zcode-custom.css — 由 zcode-assistant 生成，请勿手动编辑。*/\n");
    s.push_str("/* 同时覆盖 :root,:host 与 .dark，保证亮/暗模式都生效。*/\n");

    if !vars.is_empty() {
        s.push_str(":root, :host {\n");
        for (k, v) in &vars {
            s.push_str(&format!("  {}: {};\n", k, v));
        }
        s.push_str("}\n");
        s.push_str(".dark {\n");
        for (k, v) in &vars {
            s.push_str(&format!("  {}: {};\n", k, v));
        }
        s.push_str("}\n");
    }

    if translucent {
        // 毛玻璃：ZCode 在 Windows 上默认启用 acrylic 窗口材质，但被不透明的
        // --color-background 等表面 token 盖住；这里按 surface_opacity 混入透明。
        // 注意：必须排在上方主题/自定义色块之后，才能覆盖它们设置的 --color-background。
        let alpha = (cfg.surface_opacity.clamp(0.2, 1.0) * 100.0).round() as i32;
        let base = cfg.bg_color.as_deref().or_else(|| {
            cfg.theme
                .as_deref()
                .and_then(preset_bg_color)
        });
        s.push_str("/* 毛玻璃：主要表面半透明，透出 Windows acrylic / 背景图。*/\n");
        s.push_str(":root, :host {\n");
        s.push_str(&frosted_block(true, base, alpha));
        s.push_str("}\n");
        s.push_str(".dark {\n");
        s.push_str(&frosted_block(false, base, alpha));
        s.push_str("}\n");
    }

    if let Some(name) = &bg_asset {
        // 背景图层：固定在内容之下（z-index:-1），透过上方半透明表面显现。
        let op = cfg.bg_image_opacity.clamp(0.1, 1.0);
        s.push_str("/* 背景图层：固定在内容之下，透过上方半透明表面显现。*/\n");
        s.push_str(&format!(
            "body::before {{\n  content: \"\";\n  position: fixed;\n  inset: 0;\n  z-index: -1;\n  background: url(\"./{name}\") center / cover no-repeat;\n  opacity: {op:.2};\n  pointer-events: none;\n}}\n"
        ));
    }
    s
}

/// 一段非常明显的测试 CSS（红色背景），用于真机验证注入链路是否生效。
#[allow(dead_code)]
pub fn test_css() -> &'static str {
    "\
/* === zcode-assistant 注入测试：成功则界面应明显偏红 === */
:root, :host, .dark {
  --color-background: #b71c1c;
  --color-background-alt: #c62828;
  --color-surface: #d32f2f;
  --color-card: #e53935;
  --color-surface-hover: #ef5350;
  --color-primary: #ffebee;
  --color-foreground: #ffebee;
  --color-border: #ef9a9a;
  --color-brand: #ffcdd2;
}
"
}

// ───────────────────────── 注入 ─────────────────────────

/// 判断解包目录的 index.html 是否已注入自定义 CSS link。
#[allow(dead_code)]
pub fn is_injected(extracted_dir: &Path) -> bool {
    fs::read_to_string(extracted_dir.join(INDEX_HTML))
        .map(|s| s.contains(INJECT_MARK))
        .unwrap_or(false)
}

/// 把 css_content 写入 assets/zcode-custom.css，并在 index.html 的 </head> 前注入 link。
/// 若提供 bg_image_src，则把图片复制为 assets/zcode-bg.<ext>（先清掉旧背景图）。
/// 幂等：link 已存在则不重复插入（仅更新 css 文件内容）。
pub fn inject(extracted_dir: &Path, css_content: &str, bg_image_src: Option<&Path>) -> Result<()> {
    let assets_dir = extracted_dir.join("out/renderer/assets");
    fs::create_dir_all(&assets_dir)?;
    fs::write(assets_dir.join(CUSTOM_CSS_NAME), css_content)?;

    // 清理上一次注入的背景图，避免配置移除后残留
    if let Ok(entries) = fs::read_dir(&assets_dir) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with("zcode-bg.") {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    if let Some(src) = bg_image_src {
        let asset = validate_bg_image(src)?;
        fs::copy(src, assets_dir.join(&asset))
            .with_context(|| format!("复制背景图失败：{}", src.display()))?;
    }

    let html_path = extracted_dir.join(INDEX_HTML);
    let html = fs::read_to_string(&html_path)
        .with_context(|| format!("读取 {} 失败", INDEX_HTML))?;
    if html.contains(INJECT_MARK) {
        return Ok(()); // 已注入 link
    }
    let Some(idx) = html.find("</head>") else {
        return Err(anyhow::anyhow!("index.html 未找到 </head> 注入锚点"));
    };
    let (before, after) = html.split_at(idx);
    let new_html = format!(
        "{}    <link rel=\"stylesheet\" href=\"./assets/{}\">\n{}",
        before, CUSTOM_CSS_NAME, after
    );
    fs::write(&html_path, new_html)?;
    Ok(())
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

// ───────────────────────── 高层 apply / restore ─────────────────────────

/// 完整应用美化：
/// 备份（首次）→ kill ZCode 解锁 → extract → 注入 → pack → 原子替换 app.asar。
/// 不负责重启 ZCode（由命令层 emit restart 事件，前端走全局 RestartDialog）。
pub fn apply(cfg: &BeautifyConfig) -> Result<()> {
    // 0. 背景图预校验（在关闭 ZCode 之前尽早失败）
    if let Some(p) = cfg.bg_image.as_deref() {
        validate_bg_image(Path::new(p))?;
    }

    let asar_path = asar::asar_path()?;
    let resources_dir = asar_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("app.asar 无父目录"))?;

    // 1. 首次备份原始 app.asar
    asar::ensure_backup()?;

    // 2. 关闭 ZCode，释放 app.asar 文件锁
    let _ = process::kill_zcode();

    // 3. 解包到临时目录
    let work = std::env::temp_dir().join("zcode_beautify_apply");
    if work.exists() {
        let _ = fs::remove_dir_all(&work);
    }
    let extracted = work.join("extracted");
    let unpacked = asar::extract(&asar_path, &extracted)?;

    // 4. 注入 CSS + 背景图 + link
    let bg_src = cfg.bg_image.as_deref().map(Path::new);
    inject(&extracted, &generate_css(cfg), bg_src)?;

    // 5. 重打包到同目录的临时文件（同卷，便于原子 rename）
    let new_asar = resources_dir.join("app.asar.new");
    asar::pack(&extracted, &new_asar, &unpacked)?;

    // 6. 原子替换：rename 会覆盖目标（Windows MoveFileEx REPLACE_EXISTING）
    fs::rename(&new_asar, &asar_path)
        .with_context(|| "替换 app.asar 失败（可能 ZCode 仍在运行占用文件）")?;

    // 7. 清理临时目录
    let _ = fs::remove_dir_all(&work);
    Ok(())
}

/// 用测试 CSS 应用到真实 ZCode（仅用于真机验证）。
#[allow(dead_code)]
pub fn apply_test_css() -> Result<()> {
    let asar_path = asar::asar_path()?;
    let resources_dir = asar_path.parent().unwrap();
    asar::ensure_backup()?;
    let _ = process::kill_zcode();
    let work = std::env::temp_dir().join("zcode_beautify_apply");
    if work.exists() {
        let _ = fs::remove_dir_all(&work);
    }
    let extracted = work.join("extracted");
    let unpacked = asar::extract(&asar_path, &extracted)?;
    inject(&extracted, test_css(), None)?;
    let new_asar = resources_dir.join("app.asar.new");
    asar::pack(&extracted, &new_asar, &unpacked)?;
    fs::rename(&new_asar, &asar_path)?;
    let _ = fs::remove_dir_all(&work);
    Ok(())
}

/// 还原：kill ZCode → 用备份覆盖 app.asar。
pub fn restore() -> Result<()> {
    let _ = process::kill_zcode();
    asar::restore_from_backup()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 离线验证 inject() 全链路（不触碰真实 app.asar）：
    /// CSS 写入、背景图复制、旧背景图清理、html link 注入与幂等。
    #[test]
    fn inject_offline_with_bg_image() {
        let work = std::env::temp_dir().join("zcode_beautify_inject_test");
        let _ = fs::remove_dir_all(&work);
        let extracted = work.join("extracted");
        let assets = extracted.join("out/renderer/assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(
            extracted.join("out/renderer/index.html"),
            "<!doctype html><html><head><title>ZCode</title></head><body><div id=\"root\"></div></body></html>",
        )
        .unwrap();

        // 模拟背景图源文件与上一次注入遗留的旧背景图
        let img_src = work.join("wallpaper.png");
        fs::write(&img_src, b"\x89PNG fake bytes").unwrap();
        fs::write(assets.join("zcode-bg.jpg"), b"old").unwrap();

        inject(&extracted, "/* test css */", Some(img_src.as_path())).unwrap();

        assert_eq!(
            fs::read_to_string(assets.join(CUSTOM_CSS_NAME)).unwrap(),
            "/* test css */"
        );
        assert!(assets.join("zcode-bg.png").exists(), "背景图未按源扩展名复制");
        assert!(!assets.join("zcode-bg.jpg").exists(), "旧背景图未清理");
        let html = fs::read_to_string(extracted.join(INDEX_HTML)).unwrap();
        assert!(html.contains(r#"<link rel="stylesheet" href="./assets/zcode-custom.css">"#));
        assert!(
            html.find("zcode-custom.css").unwrap() < html.find("</head>").unwrap(),
            "link 未注入到 </head> 之前"
        );

        // 幂等 + 移除背景图场景：link 不重复插入，旧背景图被清掉
        inject(&extracted, "/* test css 2 */", None).unwrap();
        let html = fs::read_to_string(extracted.join(INDEX_HTML)).unwrap();
        assert_eq!(html.matches("zcode-custom.css").count(), 1, "link 重复注入");
        assert!(!assets.join("zcode-bg.png").exists(), "移除背景图后未清理");

        let _ = fs::remove_dir_all(&work);
    }

    /// 真机验证①：注入测试 CSS（红色背景）到真实 ZCode 并启动。
    /// 手动跑：`cargo test --lib apply_test_to_real -- --ignored --nocapture`
    /// 跑完后请人工确认 ZCode 界面变红，然后跑 restore_real 还原。
    #[test]
    #[ignore]
    fn apply_test_to_real() {
        println!("应用测试 CSS（红色背景）到真实 ZCode...");
        apply_test_css().expect("应用测试 CSS 失败");
        println!("✅ 已注入测试 CSS。启动 ZCode 以确认界面变红...");
        // 启动 ZCode 供人工确认
        let _ = process::launch_zcode();
        println!("ZCode 已启动。请观察界面是否明显偏红。");
        println!("确认后运行 restore_real 测试还原：cargo test --lib restore_real -- --ignored --nocapture");
    }

    /// 真机验证②：还原真实 ZCode 到官方 app.asar 并启动。
    #[test]
    #[ignore]
    fn restore_real() {
        println!("还原真实 ZCode 到官方 app.asar...");
        restore().expect("还原失败");
        println!("✅ 已还原。启动 ZCode 确认恢复官方外观...");
        let _ = process::launch_zcode();
        println!("ZCode 已启动。请确认界面恢复原状。");
    }

    /// 生成测试：打印各预设与自定义组合的 CSS（不改动任何文件）。
    #[test]
    fn preview_css() {
        for (id, name) in preset_list() {
            let cfg = BeautifyConfig {
                enabled: true,
                theme: Some(id.to_string()),
                ..Default::default()
            };
            let css = generate_css(&cfg);
            println!("===== {} ({}) =====\n{}", name, id, css);
        }
        // 毛玻璃（无主题，走 ZCode 默认色板）
        let cfg = BeautifyConfig {
            enabled: true,
            acrylic: true,
            ..Default::default()
        };
        println!("===== 毛玻璃（默认色板）=====\n{}", generate_css(&cfg));
        // 毛玻璃 + 主题 + 背景图组合
        let cfg = BeautifyConfig {
            enabled: true,
            theme: Some("tokyo-night".to_string()),
            acrylic: true,
            surface_opacity: 0.65,
            bg_image: Some("C:/Pictures/demo.jpg".to_string()),
            bg_image_opacity: 0.8,
            ..Default::default()
        };
        println!("===== 毛玻璃+主题+背景图 =====\n{}", generate_css(&cfg));
    }

    /// 背景图资源名与校验逻辑。
    #[test]
    fn bg_image_asset_name_rules() {
        assert_eq!(
            bg_image_asset_name(Path::new("C:/a/photo.JPG")).as_deref(),
            Some("zcode-bg.jpg")
        );
        assert_eq!(bg_image_asset_name(Path::new("C:/a/photo.bmp")), None);
        assert!(validate_bg_image(Path::new("C:/no-such-file.png")).is_err());
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
}
