import { useEffect, useState, type CSSProperties } from "react";
import { beautify as bf } from "../../api";
import type { BeautifyConfig, BeautifyPreset, BeautifyTemplate } from "../../types";
import { toast } from "../../components/Toast";
import { IconSparkle } from "../../components/icons";
import { Switch } from "../../components/Switch";

const field: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 4,
  fontSize: "var(--fs-sm)",
  color: "var(--text-secondary)",
};

/** 常用 UI（无衬线）字体候选，值为 CSS font-family 首项 */
const UI_FONTS: { v: string; label: string }[] = [
  { v: "", label: "跟随 ZCode 默认" },
  { v: "Microsoft YaHei UI", label: "微软雅黑" },
  { v: "Source Han Sans SC", label: "思源黑体" },
  { v: "Noto Sans SC", label: "Noto Sans SC" },
  { v: "PingFang SC", label: "苹方（macOS）" },
  { v: "HarmonyOS Sans SC", label: "鸿蒙黑体" },
  { v: "Segoe UI", label: "Segoe UI" },
];

/** 常用等宽（代码/终端）字体候选 */
const MONO_FONTS: { v: string; label: string }[] = [
  { v: "", label: "跟随 ZCode 默认" },
  { v: "Cascadia Code", label: "Cascadia Code" },
  { v: "JetBrains Mono", label: "JetBrains Mono" },
  { v: "Fira Code", label: "Fira Code" },
  { v: "Source Code Pro", label: "Source Code Pro" },
  { v: "Maple Mono", label: "Maple Mono" },
  { v: "Consolas", label: "Consolas" },
];

/** 预设主题色卡预览（背景色 + 品牌色），与后端 preset_vars 对应 */
const PREVIEW: Record<string, { bg: string; brand: string }> = {
  midnight: { bg: "#0b1020", brand: "#7c8cff" },
  nord: { bg: "#2e3440", brand: "#88c0d0" },
  dracula: { bg: "#282a36", brand: "#bd93f9" },
  gruvbox: { bg: "#282828", brand: "#fabd2f" },
  "tokyo-night": { bg: "#1a1b26", brand: "#7aa2f7" },
  "rose-pine": { bg: "#191724", brand: "#c4a7e7" },
};

/** 模拟桌面壁纸渐变（预览用，代表窗口外的桌面内容） */
const DESKTOP_GRADIENT = "linear-gradient(135deg, #2b5876 0%, #4e4376 45%, #b06ab3 100%)";

const defaultCfg = (): BeautifyConfig => ({
  enabled: true,
  theme: "tokyo-night",
});

export default function Beautify() {
  const [presets, setPresets] = useState<BeautifyPreset[]>([]);
  const [cfg, setCfg] = useState<BeautifyConfig>(defaultCfg());
  const [installed, setInstalled] = useState(false);
  const [hasBackup, setHasBackup] = useState(false);
  const [zcodeVersion, setZcodeVersion] = useState<string>("");
  const [backupVersion, setBackupVersion] = useState<string>("");
  const [asarPath, setAsarPath] = useState<string>("");
  const [busy, setBusy] = useState<"apply" | "restore" | null>(null);
  const [loading, setLoading] = useState(true);
  const [preview, setPreview] = useState<string | null>(null);
  const [templates, setTemplates] = useState<BeautifyTemplate[]>([]);
  const [templateName, setTemplateName] = useState("");
  const [activeTemplate, setActiveTemplate] = useState("");

  const reload = async () => {
    try {
      const st = await bf.getStatus();
      setInstalled(st.installed);
      setHasBackup(st.has_backup);
      setZcodeVersion(st.zcode_version ?? "");
      setBackupVersion(st.backup_version ?? "");
      setAsarPath(st.asar_path ?? "");
      // 用已存配置覆盖默认值；无配置则保持默认（编辑态友好起点）
      const c = st.config;
      if (
        c &&
        (c.enabled ||
          c.theme ||
          c.ui_font ||
          c.mono_font ||
          c.bg_color ||
          c.primary_color ||
          c.acrylic ||
          c.bg_image)
      ) {
        setCfg({ ...defaultCfg(), ...c });
      }
    } catch (e: unknown) {
      toast.error(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    reload();
    bf.getPresets().then(setPresets).catch(() => {});
    bf.listTemplates().then(setTemplates).catch(() => {});
  }, []);

  // 背景图变化时加载预览（base64 data URL，>8MB 返回 null 则不显示）
  useEffect(() => {
    let cancelled = false;
    setPreview(null);
    if (cfg.bg_image) {
      bf.readImagePreview(cfg.bg_image)
        .then((d) => {
          if (!cancelled) setPreview(d);
        })
        .catch(() => {});
    }
    return () => {
      cancelled = true;
    };
  }, [cfg.bg_image]);

  const apply = async () => {
    setBusy("apply");
    try {
      await bf.apply({ ...cfg, enabled: true });
      toast.success("美化已应用，重启 ZCode 后生效");
      await reload();
    } catch (e: unknown) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  };

  const restore = async () => {
    if (!window.confirm("确定还原为 ZCode 官方外观？当前美化将被移除。")) return;
    setBusy("restore");
    try {
      await bf.restore();
      toast.success("已还原官方外观，重启 ZCode 后生效");
      await reload();
    } catch (e: unknown) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  };

  const pickImage = async () => {
    try {
      const p = await bf.pickImage();
      if (p) setCfg({ ...cfg, bg_image: p });
    } catch (e: unknown) {
      toast.error(String(e));
    }
  };

  // 默认模板名：方案N（取最小可用序号）
  const nextTemplateName = () => {
    for (let n = 1; ; n++) {
      const name = `方案${n}`;
      if (!templates.some((t) => t.name === name)) return name;
    }
  };

  const saveTemplate = async () => {
    const name = templateName.trim() || nextTemplateName();
    try {
      const list = await bf.saveTemplate(name, { ...cfg, enabled: true });
      setTemplates(list);
      setTemplateName("");
      setActiveTemplate(name);
      toast.success(`模板「${name}」已保存`);
    } catch (e: unknown) {
      toast.error(String(e));
    }
  };

  const loadTemplate = (t: BeautifyTemplate) => {
    setCfg({ ...defaultCfg(), ...t.config });
    setActiveTemplate(t.name);
    toast.success(`已载入模板「${t.name}」，点「应用美化」后生效`);
  };

  const removeTemplate = async (name: string) => {
    if (!window.confirm(`删除模板「${name}」？`)) return;
    try {
      const list = await bf.deleteTemplate(name);
      setTemplates(list);
      if (activeTemplate === name) setActiveTemplate("");
    } catch (e: unknown) {
      toast.error(String(e));
    }
  };

  const busyNow = busy !== null;
  // 备份版本与当前 ZCode 不一致 = 备份已过期（升级替换了 app.asar，备份没跟上）
  const backupStale =
    hasBackup && !!backupVersion && !!zcodeVersion && backupVersion !== zcodeVersion;
  const translucencyActive = !!cfg.acrylic || !!cfg.bg_image;
  // 预览用表面色：自定义背景色 > 主题背景色 > ZCode 暗色默认
  const surfaceColor = cfg.bg_color || PREVIEW[cfg.theme ?? ""]?.bg || "#171717";

  return (
    <>
      {/* 状态与还原 */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>状态</h3>
          <span
            className={
              "za-badge " + (installed ? "za-badge-success" : "za-badge-neutral")
            }
          >
            {loading ? "检测中…" : installed ? "已注入美化" : "官方外观"}
          </span>
        </div>
        <p className="za-muted" style={{ margin: "0 0 12px" }}>
          通过向 ZCode 的 <span className="za-mono">app.asar</span> 注入一段自定义
          CSS 实现换肤 / 换字体 / 毛玻璃 / 背景图。首次应用会自动备份原始包，随时可一键还原。
          ZCode 自动更新后美化会失效，需重新应用。
        </p>
        <div
          className="za-row"
          style={{ gap: 16, flexWrap: "wrap", fontSize: "var(--fs-sm)" }}
        >
          <span className="za-faint">
            ZCode 版本：<span className="za-mono">{zcodeVersion || "—"}</span>
          </span>
          <span className="za-faint">
            原始备份：
            {hasBackup
              ? backupVersion
                ? `已有（v${backupVersion}）`
                : "已有（版本未知）"
              : "无（首次应用时创建）"}
          </span>
        </div>
        {backupStale && (
          <p
            style={{
              margin: "10px 0 0",
              fontSize: "var(--fs-sm)",
              color: "var(--warning)",
            }}
          >
            备份属于 ZCode v{backupVersion}，与当前 v{zcodeVersion} 不一致：
            {installed
              ? "当前已注入且没有该版本官方包，无法安全还原；如需还原请先重装/修复 ZCode。"
              : "下次「应用美化」会用当前官方包自动重建备份。"}
          </p>
        )}
        {asarPath && (
          <div
            className="za-faint za-mono"
            style={{ fontSize: "var(--fs-xs)", marginTop: 6, wordBreak: "break-all" }}
            title={asarPath}
          >
            {asarPath}
          </div>
        )}
        <div className="za-row" style={{ gap: 8, marginTop: 14 }}>
          <button
            className="za-btn za-btn-sm"
            disabled={busyNow || !hasBackup || backupStale}
            onClick={restore}
            title={
              backupStale
                ? "备份版本与当前 ZCode 不一致，还原会把旧文件盖到新版本上，已禁止"
                : hasBackup
                  ? "用备份恢复官方 app.asar"
                  : "尚无备份"
            }
          >
            {busy === "restore" ? "还原中…" : "还原官方外观"}
          </button>
        </div>
      </div>

      {/* 预设主题 */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>配色主题</h3>
        </div>
        <p className="za-muted" style={{ margin: "0 0 12px" }}>
          选一个预设覆盖核心配色 token；选「跟随默认」则不改颜色。
        </p>
        <div className="za-grid" style={{ gridTemplateColumns: "repeat(auto-fill,minmax(150px,1fr))", gap: 10 }}>
          <ThemeCard
            name="跟随默认"
            selected={!cfg.theme || cfg.theme === "none"}
            swatch={{ bg: "transparent", brand: "var(--text-secondary)" }}
            onClick={() => setCfg({ ...cfg, theme: "none" })}
          />
          {presets.map((p) => (
            <ThemeCard
              key={p.id}
              name={p.name}
              selected={cfg.theme === p.id}
              swatch={PREVIEW[p.id] ?? { bg: "#1e1e1e", brand: "#888" }}
              onClick={() => setCfg({ ...cfg, theme: p.id })}
            />
          ))}
        </div>
      </div>

      {/* 字体 */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>字体</h3>
        </div>
        <p className="za-muted" style={{ margin: "0 0 12px" }}>
          UI 字体影响界面文字；等宽字体影响代码 / 终端显示。留空则沿用 ZCode 默认。
          （字号请在 ZCode 设置内调整。）
        </p>
        <div className="za-grid za-grid-2" style={{ gap: 10 }}>
          <label style={field}>
            UI 字体（--font-sans）
            <select
              className="za-select"
              value={cfg.ui_font ?? ""}
              onChange={(e) =>
                setCfg({ ...cfg, ui_font: e.target.value || undefined })
              }
            >
              {UI_FONTS.map((f) => (
                <option key={f.v} value={f.v}>
                  {f.label}
                </option>
              ))}
            </select>
          </label>
          <label style={field}>
            等宽字体（--font-mono）
            <select
              className="za-select"
              value={cfg.mono_font ?? ""}
              onChange={(e) =>
                setCfg({ ...cfg, mono_font: e.target.value || undefined })
              }
            >
              {MONO_FONTS.map((f) => (
                <option key={f.v} value={f.v}>
                  {f.label}
                </option>
              ))}
            </select>
          </label>
        </div>
      </div>

      {/* 自定义颜色 */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>自定义颜色（可选）</h3>
        </div>
        <p className="za-muted" style={{ margin: "0 0 12px" }}>
          在预设之上进一步覆盖。留空表示不覆盖。自定义优先级高于预设。
        </p>
        <div className="za-grid za-grid-2" style={{ gap: 10 }}>
          <label style={field}>
            背景色（--color-background）
            <ColorField
              value={cfg.bg_color}
              onChange={(v) => setCfg({ ...cfg, bg_color: v })}
            />
          </label>
          <label style={field}>
            主色调（--color-primary）
            <ColorField
              value={cfg.primary_color}
              onChange={(v) => setCfg({ ...cfg, primary_color: v })}
            />
          </label>
        </div>
      </div>

      {/* 毛玻璃 */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>毛玻璃</h3>
        </div>
        <p className="za-muted" style={{ margin: "0 0 12px" }}>
          ZCode 在 Windows 上默认启用了 acrylic 材质，但被不透明的界面底色盖住。
          开启后界面表面变半透明，透出原生毛玻璃模糊。
        </p>
        <div
          className="za-row"
          style={{ gap: 10, alignItems: "center", marginBottom: 12 }}
        >
          <Switch
            on={!!cfg.acrylic}
            onChange={(on) => setCfg({ ...cfg, acrylic: on })}
            title="毛玻璃（透出 Windows acrylic）"
          />
          <span style={{ fontSize: "var(--fs-sm)" }}>
            毛玻璃：{cfg.acrylic ? "开" : "关"}
          </span>
        </div>
        <RangeField
          label="桌面不透明度（越小越透）"
          value={Math.round((cfg.surface_opacity ?? 0.72) * 100)}
          min={20}
          max={100}
          disabled={!translucencyActive}
          onChange={(v) => setCfg({ ...cfg, surface_opacity: v / 100 })}
        />
        <FrostedPreview
          active={translucencyActive}
          surfaceColor={surfaceColor}
          surfaceOpacity={cfg.surface_opacity ?? 0.72}
          wallpaper={cfg.bg_image ? preview : null}
          wallpaperOpacity={cfg.bg_image_opacity ?? 1}
        />
      </div>

      {/* 背景图 */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>背景图</h3>
        </div>
        <p className="za-muted" style={{ margin: "0 0 12px" }}>
          选择本地图片作为窗口背景（应用时复制进 app.asar），透过半透明界面显现。
        </p>
        <div
          className="za-row"
          style={{ gap: 8, alignItems: "center", flexWrap: "wrap" }}
        >
          <button className="za-btn za-btn-sm" onClick={pickImage}>
            选择背景图…
          </button>
          {cfg.bg_image ? (
            <>
              <span
                className="za-mono"
                title={cfg.bg_image}
                style={{
                  fontSize: "var(--fs-xs)",
                  color: "var(--text-secondary)",
                  maxWidth: 320,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {cfg.bg_image.split(/[\\/]/).pop()}
              </span>
              <button
                className="za-btn za-btn-sm"
                onClick={() => setCfg({ ...cfg, bg_image: undefined })}
              >
                移除
              </button>
            </>
          ) : (
            <span className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
              未设置（png / jpg / webp / gif）
            </span>
          )}
        </div>
        <div style={{ marginTop: 12 }}>
          <RangeField
            label="背景图不透明度"
            value={Math.round((cfg.bg_image_opacity ?? 1) * 100)}
            min={10}
            max={100}
            disabled={!cfg.bg_image}
            onChange={(v) => setCfg({ ...cfg, bg_image_opacity: v / 100 })}
          />
        </div>
        {/* 背景图不透明度实时预览：按当前透明度叠在模拟桌面上 */}
        {cfg.bg_image && preview && (
          <div style={{ marginTop: 10 }}>
            <div
              style={{
                fontSize: "var(--fs-xs)",
                color: "var(--text-tertiary)",
                marginBottom: 4,
              }}
            >
              效果预览
            </div>
            <div
              style={{
                position: "relative",
                height: 96,
                borderRadius: 8,
                overflow: "hidden",
                border: "1px solid var(--glass-border)",
                background: DESKTOP_GRADIENT,
              }}
            >
              <img
                src={preview}
                alt="背景图预览"
                style={{
                  position: "absolute",
                  inset: 0,
                  width: "100%",
                  height: "100%",
                  objectFit: "cover",
                  opacity: cfg.bg_image_opacity ?? 1,
                }}
              />
            </div>
          </div>
        )}
      </div>

      {/* 应用 + 模板 */}
      <div className="za-panel za-card-pad">
        <div className="za-row" style={{ gap: 10, alignItems: "center" }}>
          <button
            className="za-btn za-btn-primary"
            disabled={busyNow}
            onClick={apply}
          >
            <IconSparkle width={15} height={15} />
            {busy === "apply" ? "正在写入 app.asar…" : installed ? "重新应用美化" : "应用美化"}
          </button>
          {busyNow && (
            <span className="za-faint" style={{ fontSize: "var(--fs-sm)" }}>
              正在解包并重打包 app.asar（约 300MB），需数秒~十几秒，请稍候…
            </span>
          )}
        </div>
        <p className="za-muted" style={{ margin: "10px 0 0", fontSize: "var(--fs-xs)" }}>
          应用过程会先关闭 ZCode 以释放文件锁，完成后会询问是否重启。
        </p>

        {/* 我的模板 */}
        <div
          style={{
            marginTop: 14,
            paddingTop: 12,
            borderTop: "1px solid var(--glass-border)",
          }}
        >
          <div className="za-section-title">
            <h3>我的模板</h3>
          </div>
          <p className="za-muted" style={{ margin: "0 0 10px", fontSize: "var(--fs-xs)" }}>
            把当前全部设置保存为模板，下次一键载入。不填名称默认「方案1」，序号依次递增。
          </p>
          <div className="za-row" style={{ gap: 8, alignItems: "center" }}>
            <input
              className="za-input"
              placeholder={nextTemplateName()}
              value={templateName}
              onChange={(e) => setTemplateName(e.target.value)}
              style={{ flex: 1, maxWidth: 200 }}
            />
            <button className="za-btn za-btn-sm" onClick={saveTemplate}>
              保存当前设置为模板
            </button>
          </div>
          {templates.length > 0 ? (
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                gap: 6,
                marginTop: 10,
              }}
            >
              {templates.map((t) => {
                const active = activeTemplate === t.name;
                return (
                  <div
                    key={t.name}
                    className="za-row"
                    style={{
                      gap: 8,
                      alignItems: "center",
                      padding: "6px 10px",
                      borderRadius: 8,
                      border: active
                        ? "1px solid var(--accent, #6366f1)"
                        : "1px solid var(--glass-border)",
                      background: active ? "rgba(99,102,241,0.06)" : undefined,
                    }}
                  >
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div
                        style={{
                          fontSize: "var(--fs-sm)",
                          fontWeight: active ? 600 : 400,
                        }}
                      >
                        {t.name}
                      </div>
                      <div
                        className="za-faint"
                        style={{
                          fontSize: "var(--fs-xs)",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                      >
                        {describeTemplate(t.config, presets)}
                      </div>
                    </div>
                    <button
                      className="za-btn za-btn-sm"
                      onClick={() => loadTemplate(t)}
                    >
                      载入
                    </button>
                    <button
                      className="za-btn za-btn-sm"
                      onClick={() => removeTemplate(t.name)}
                    >
                      删除
                    </button>
                  </div>
                );
              })}
            </div>
          ) : (
            <p className="za-faint" style={{ fontSize: "var(--fs-xs)", marginTop: 10 }}>
              暂无模板
            </p>
          )}
        </div>
      </div>
    </>
  );
}

/** 模板摘要：主题 / 字体 / 毛玻璃 / 背景图 等要点 */
function describeTemplate(c: BeautifyConfig, presets: BeautifyPreset[]): string {
  const parts: string[] = [];
  if (c.theme && c.theme !== "none") {
    parts.push(presets.find((p) => p.id === c.theme)?.name ?? c.theme);
  }
  if (c.ui_font) parts.push(c.ui_font);
  if (c.bg_color) parts.push(`背景色 ${c.bg_color}`);
  if (c.acrylic) parts.push("毛玻璃");
  if (c.bg_image) parts.push("背景图");
  return parts.length > 0 ? parts.join(" · ") : "默认外观";
}

/** 毛玻璃实时预览：模拟桌面 + 背景图层 + 半透明界面表面，合成最终观感 */
function FrostedPreview({
  active,
  surfaceColor,
  surfaceOpacity,
  wallpaper,
  wallpaperOpacity,
}: {
  active: boolean;
  surfaceColor: string;
  surfaceOpacity: number;
  wallpaper: string | null;
  wallpaperOpacity: number;
}) {
  return (
    <div style={{ marginTop: 10 }}>
      <div
        style={{
          fontSize: "var(--fs-xs)",
          color: "var(--text-tertiary)",
          marginBottom: 4,
        }}
      >
        效果预览（模拟桌面 + 半透明界面，随上方滑块实时变化）
      </div>
      <div
        style={{
          position: "relative",
          height: 96,
          borderRadius: 8,
          overflow: "hidden",
          border: "1px solid var(--glass-border)",
        }}
      >
        {/* 模拟桌面壁纸 */}
        <div
          style={{ position: "absolute", inset: 0, background: DESKTOP_GRADIENT }}
        />
        {/* 背景图层（如已设置） */}
        {wallpaper && (
          <img
            src={wallpaper}
            alt=""
            style={{
              position: "absolute",
              inset: 0,
              width: "100%",
              height: "100%",
              objectFit: "cover",
              opacity: wallpaperOpacity,
            }}
          />
        )}
        {/* 界面表面（半透明底色） */}
        <div
          style={{
            position: "absolute",
            inset: 0,
            background: surfaceColor,
            opacity: active ? surfaceOpacity : 1,
          }}
        />
        {/* 模拟窗口内容条，增强层次感 */}
        <div
          style={{
            position: "absolute",
            left: 12,
            top: 14,
            width: 110,
            height: 9,
            borderRadius: 4,
            background: "rgba(255,255,255,0.38)",
          }}
        />
        <div
          style={{
            position: "absolute",
            left: 12,
            top: 32,
            width: 180,
            height: 7,
            borderRadius: 4,
            background: "rgba(255,255,255,0.22)",
          }}
        />
        <div
          style={{
            position: "absolute",
            left: 12,
            top: 46,
            width: 150,
            height: 7,
            borderRadius: 4,
            background: "rgba(255,255,255,0.22)",
          }}
        />
        <div
          style={{
            position: "absolute",
            right: 12,
            top: 14,
            width: 64,
            height: 39,
            borderRadius: 6,
            background: "rgba(255,255,255,0.14)",
            border: "1px solid rgba(255,255,255,0.18)",
          }}
        />
      </div>
    </div>
  );
}

function ThemeCard({
  name,
  selected,
  swatch,
  onClick,
}: {
  name: string;
  selected: boolean;
  swatch: { bg: string; brand: string };
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="za-panel"
      style={{
        padding: 10,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        cursor: "pointer",
        border: selected
          ? "1.5px solid var(--accent, #6366f1)"
          : "1px solid var(--glass-border)",
        background: selected ? "rgba(99,102,241,0.06)" : undefined,
        textAlign: "left",
      }}
    >
      <div
        style={{
          height: 40,
          borderRadius: 6,
          border: "1px solid var(--glass-border)",
          background: swatch.bg,
          position: "relative",
          overflow: "hidden",
        }}
      >
        <span
          style={{
            position: "absolute",
            left: 8,
            bottom: 6,
            width: 26,
            height: 5,
            borderRadius: 3,
            background: swatch.brand,
            display: "block",
          }}
        />
      </div>
      <span style={{ fontSize: "var(--fs-sm)", fontWeight: selected ? 600 : 400 }}>
        {name}
      </span>
    </button>
  );
}

function ColorField({
  value,
  onChange,
}: {
  value?: string;
  onChange: (v?: string) => void;
}) {
  return (
    <div className="za-row" style={{ gap: 8, alignItems: "center" }}>
      <input
        type="color"
        value={value ?? "#000000"}
        disabled={!value}
        onChange={(e) => onChange(e.target.value)}
        style={{
          width: 40,
          height: 30,
          padding: 2,
          border: "1px solid var(--glass-border)",
          borderRadius: 6,
          background: "transparent",
          opacity: value ? 1 : 0.35,
          cursor: value ? "pointer" : "not-allowed",
        }}
      />
      <input
        className="za-input za-mono"
        placeholder="留空=不覆盖，如 #0d1117"
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value.trim() || undefined)}
        style={{ flex: 1 }}
      />
      {value && (
        <button className="za-btn za-btn-sm" onClick={() => onChange(undefined)}>
          清除
        </button>
      )}
    </div>
  );
}

function RangeField({
  label,
  value,
  min,
  max,
  disabled,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  disabled?: boolean;
  onChange: (v: number) => void;
}) {
  return (
    <label style={{ ...field, opacity: disabled ? 0.45 : 1 }}>
      {label}
      <div className="za-row" style={{ gap: 10, alignItems: "center" }}>
        <input
          type="range"
          min={min}
          max={max}
          value={value}
          disabled={disabled}
          onChange={(e) => onChange(Number(e.target.value))}
          style={{
            flex: 1,
            accentColor: "var(--accent, #6366f1)",
            cursor: disabled ? "not-allowed" : "pointer",
          }}
        />
        <span className="za-mono" style={{ width: 42, textAlign: "right" }}>
          {value}%
        </span>
      </div>
    </label>
  );
}
