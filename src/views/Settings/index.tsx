import { useEffect, useState, type CSSProperties } from "react";
import { useTheme } from "../../hooks/useTheme";
import { useAppearance } from "../../hooks/useAppearance";
import { IconSun, IconMoon, IconCheck, IconRefresh } from "../../components/icons";
import { UpdateNotification } from "../../components/UpdateNotification";
import { Switch } from "../../components/Switch";
import { toast } from "../../components/Toast";
import { zcode, app, updater, prefs as prefsApi, events, beautify } from "../../api";
import type { AppPrefs, UpdateInfo, UsageDisplayMode } from "../../types";

export default function Settings() {
  const { theme, setTheme } = useTheme();
  const { appearance, setAppearance, reset: resetAppearance, bgDataUrl } =
    useAppearance();
  const [probe, setProbe] = useState<{
    exePath: string | null;
    running: boolean;
    configDir: string;
  } | null>(null);
  const [version, setVersion] = useState<string>("");
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [updateDismissed, setUpdateDismissed] = useState(false);
  const [prefs, setPrefs] = useState<AppPrefs | null>(null);

  useEffect(() => {
    zcode.probe().then(setProbe).catch(() => setProbe(null));
    app.getVersion().then(setVersion).catch(() => {});
    // 偏好：初始读取 + 监听变更（托盘菜单切悬浮球时这里同步刷新）
    prefsApi.get().then(setPrefs).catch(() => {});
    let un: (() => void) | undefined;
    events.onPrefsUpdated(setPrefs).then((fn) => (un = fn));
    return () => un?.();
  }, []);

  const setFloatBallVisible = async (visible: boolean) => {
    setPrefs((p) => (p ? { ...p, floatBallVisible: visible } : p));
    try {
      await prefsApi.setFloatBallVisible(visible);
    } catch (e: unknown) {
      toast.error(String(e));
      prefsApi.get().then(setPrefs).catch(() => {});
    }
  };

  const setUsageDisplay = async (mode: UsageDisplayMode) => {
    setPrefs((p) => (p ? { ...p, usageDisplay: mode } : p));
    try {
      await prefsApi.setUsageDisplay(mode);
    } catch (e: unknown) {
      toast.error(String(e));
      prefsApi.get().then(setPrefs).catch(() => {});
    }
  };

  const setSwitchRestart = async (enabled: boolean) => {
    setPrefs((p) => (p ? { ...p, switchRestartZcode: enabled } : p));
    try {
      await prefsApi.setSwitchRestart(enabled);
    } catch (e: unknown) {
      toast.error(String(e));
      prefsApi.get().then(setPrefs).catch(() => {});
    }
  };

  const setAutostart = async (enabled: boolean) => {
    setPrefs((p) => (p ? { ...p, autostart: enabled } : p));
    try {
      await prefsApi.setAutostart(enabled);
    } catch (e: unknown) {
      toast.error(String(e));
      prefsApi.get().then(setPrefs).catch(() => {});
    }
  };

  // 外观定制：选背景图（复用美化页的文件选择器）；还原默认
  const pickBgImage = async () => {
    try {
      const p = await beautify.pickImage();
      if (p) setAppearance({ bgImage: p });
    } catch (e: unknown) {
      toast.error(String(e));
    }
  };
  const resetAppearanceAll = () => {
    resetAppearance();
    toast.success("已还原默认主题");
  };

  const checkUpdates = async () => {
    setChecking(true);
    try {
      const info = await updater.checkForUpdates();
      setUpdateInfo(info);
      if (info.hasUpdate) setUpdateDismissed(false);
    } catch {
      setUpdateInfo((prev) => prev);
    } finally {
      setChecking(false);
    }
  };

  // 关于页检查更新状态文案
  const updateStatus = (() => {
    if (checking) return "检查中…";
    if (!updateInfo) return null;
    if (updateInfo.error) return `检查失败：${updateInfo.error}`;
    if (updateInfo.hasUpdate) return `发现新版本 v${updateInfo.latestVersion.replace(/^v/, "")}`;
    return "已是最新版本";
  })();

  return (
    <>
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>通用</h3>
        </div>

        {/* 开机自启动 */}
        <div
          className="za-row-between"
          style={{ gap: 10, alignItems: "center" }}
        >
          <div style={{ minWidth: 0 }}>
            <div style={{ fontSize: "var(--fs-sm)" }}>开机自启动</div>
            <div className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
              登录系统后自动启动并驻留托盘
            </div>
          </div>
          <Switch
            on={prefs?.autostart ?? false}
            onChange={setAutostart}
            title="开机自启动"
          />
        </div>
      </div>

      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>外观</h3>
        </div>
        <div className="za-row" style={{ gap: "var(--space-3)" }}>
          <button
            className="za-btn"
            onClick={() => setTheme("dark")}
            style={
              theme === "dark"
                ? { borderColor: "var(--accent)", color: "var(--accent)" }
                : undefined
            }
          >
            <IconMoon width={16} height={16} />
            深色（液态玻璃）
          </button>
          <button
            className="za-btn"
            onClick={() => setTheme("light")}
            style={
              theme === "light"
                ? { borderColor: "var(--accent)", color: "var(--accent)" }
                : undefined
            }
          >
            <IconSun width={16} height={16} />
            浅色
          </button>
        </div>
      </div>

      {/* 外观定制：主题色 / 透明度 / 背景图（参考「ZCode 美化」），即时生效 */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>外观定制</h3>
          <button className="za-btn za-btn-sm" onClick={resetAppearanceAll}>
            还原默认主题
          </button>
        </div>
        <p className="za-muted" style={{ margin: "0 0 12px" }}>
          参考「ZCode 美化」的思路给本应用换肤：主题色更换强调色系，透明度让光晕 /
          背景图更透出，背景图铺满窗口底层。全部即时生效，仅主窗口（悬浮球 / 托盘面板不套用）。
        </p>

        <div style={subLabel}>主题色</div>
        <div className="za-row" style={{ gap: 14, flexWrap: "wrap", marginBottom: 6 }}>
          {ACCENT_PRESETS.map((p) => (
            <AccentChip
              key={p.label}
              label={p.label}
              hue={p.hue}
              selected={appearance.accentHue === p.hue}
              onClick={() => setAppearance({ accentHue: p.hue })}
            />
          ))}
        </div>
        <div className="za-row" style={{ gap: 10, alignItems: "center" }}>
          <input
            type="range"
            min={0}
            max={360}
            value={appearance.accentHue ?? 180}
            onChange={(e) => setAppearance({ accentHue: Number(e.target.value) })}
            style={{ flex: 1, accentColor: "var(--accent)", cursor: "pointer" }}
            title="自定义色相（OKLCH hue）"
          />
          <span className="za-mono za-faint" style={{ width: 44, textAlign: "right" }}>
            {appearance.accentHue ?? 180}°
          </span>
        </div>

        <div style={{ ...subLabel, marginTop: 14 }}>透明度</div>
        <Slider
          value={Math.round(appearance.surfaceOpacity * 100)}
          min={40}
          max={100}
          onChange={(v) => setAppearance({ surfaceOpacity: v / 100 })}
        />
        <div className="za-faint" style={{ fontSize: "var(--fs-xs)", marginTop: 2 }}>
          面板与表面不透明度；调低让光晕 / 背景图更透出（100% = 默认实底）
        </div>

        <div style={{ ...subLabel, marginTop: 14 }}>背景图</div>
        <div className="za-row" style={{ gap: 8, alignItems: "center", flexWrap: "wrap" }}>
          <button className="za-btn za-btn-sm" onClick={pickBgImage}>
            选择图片…
          </button>
          {appearance.bgImage && (
            <button
              className="za-btn za-btn-sm"
              onClick={() => setAppearance({ bgImage: null })}
            >
              移除
            </button>
          )}
          {appearance.bgImage && !bgDataUrl && (
            <span className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
              图片不可用（超过 8MB 或格式不支持）
            </span>
          )}
        </div>
        {bgDataUrl && (
          <div style={{ marginTop: 8 }}>
            <div
              style={{
                height: 84,
                borderRadius: "var(--radius-md)",
                border: "1px solid var(--glass-border)",
                background: `url("${bgDataUrl}") center / cover no-repeat`,
                opacity: appearance.bgOpacity,
                marginBottom: 6,
              }}
            />
            <Slider
              value={Math.round(appearance.bgOpacity * 100)}
              min={10}
              max={100}
              onChange={(v) => setAppearance({ bgOpacity: v / 100 })}
            />
          </div>
        )}
      </div>

      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>悬浮球与托盘</h3>
        </div>

        {/* 悬浮球显示/隐藏 */}
        <div
          className="za-row-between"
          style={{ gap: 10, alignItems: "center" }}
        >
          <div style={{ minWidth: 0 }}>
            <div style={{ fontSize: "var(--fs-sm)" }}>显示悬浮球</div>
            <div className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
              常驻桌面的配额监控小球，托盘菜单里也可以切换
            </div>
          </div>
          <Switch
            on={prefs?.floatBallVisible ?? true}
            onChange={setFloatBallVisible}
            title="显示/隐藏悬浮球"
          />
        </div>
      </div>

      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>模型用量展示方案</h3>
        </div>

        <div className="za-row" style={{ gap: 8 }}>
          <button
            className="za-btn"
            onClick={() => setUsageDisplay("used")}
            style={
              (prefs?.usageDisplay ?? "used") === "used"
                ? { borderColor: "var(--accent)", color: "var(--accent)" }
                : undefined
            }
          >
            展示已用量
          </button>
          <button
            className="za-btn"
            onClick={() => setUsageDisplay("remaining")}
            style={
              prefs?.usageDisplay === "remaining"
                ? { borderColor: "var(--accent)", color: "var(--accent)" }
                : undefined
            }
          >
            展示剩余用量
          </button>
        </div>
        <p className="za-faint" style={{ margin: "8px 0 0", fontSize: "var(--fs-xs)" }}>
          统一控制总览、供应商管理额度行、悬浮球、悬浮面板与托盘菜单 / tooltip
          的百分比口径；颜色始终按已用度分级。
        </p>
      </div>

      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>切换行为</h3>
        </div>

        {/* 切换后提示重启 */}
        <div
          className="za-row-between"
          style={{ gap: 10, alignItems: "center" }}
        >
          <div style={{ minWidth: 0 }}>
            <div style={{ fontSize: "var(--fs-sm)" }}>切换后重启 ZCode</div>
            <div className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
              自动切换写入配置与各会话模型选择并自动重启；账号切换完成后弹窗确认
            </div>
          </div>
          <Switch
            on={prefs?.switchRestartZcode ?? true}
            onChange={setSwitchRestart}
            title="切换后重启 ZCode"
          />
        </div>
        <p className="za-faint" style={{ margin: "10px 0 0", fontSize: "var(--fs-xs)" }}>
          开启时自动切换写入配置与全部符合条件的会话（模型选择 + 供应商配置）并自动重启
          ZCode，全部对话统一生效；关闭时不重启，各对话在恢复 / 新开时使用新模型，
          账号切换会直接重启 ZCode。
        </p>
      </div>

      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>zcode 连接</h3>
        </div>
        {probe ? (
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <div className="za-row">
              <IconCheck width={16} height={16} style={{ color: "var(--success)" }} />
              <span>已检测到 zcode</span>
              {probe.running && <span className="za-badge">运行中</span>}
            </div>
            <div className="za-muted za-mono" style={{ fontSize: "var(--fs-sm)" }}>
              {probe.exePath ?? "未找到 ZCode.exe"}
            </div>
            <div className="za-faint za-mono" style={{ fontSize: "var(--fs-xs)" }}>
              配置目录：{probe.configDir}
            </div>
          </div>
        ) : (
          <p className="za-muted" style={{ margin: 0 }}>
            未检测到 zcode。
          </p>
        )}
      </div>


      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>关于</h3>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 6, fontSize: "var(--fs-sm)" }}>
          <Row k="应用" v={version ? `zcode-assistant v${version}` : "zcode-assistant"} />
          <Row k="技术栈" v="Tauri 2 · React 18 · TypeScript · Vite" />
          <Row k="设计" v="Sequoia-X 液态玻璃" />
          <Row k="数据" v="本地读写 ~/.zcode/v2，不外传" />
        </div>
        <div
          className="za-row-between"
          style={{ marginTop: 12, gap: 8, flexWrap: "wrap" }}
        >
          <button
            className="za-btn za-btn-sm"
            disabled={checking}
            onClick={checkUpdates}
          >
            <IconRefresh width={15} height={15} />
            {checking ? "检查中…" : "检查更新"}
          </button>
          {updateStatus && (
            <span
              className={updateInfo?.hasUpdate ? "" : "za-muted"}
              style={{
                fontSize: "var(--fs-xs)",
                color: updateInfo?.hasUpdate ? "var(--accent)" : undefined,
              }}
            >
              {updateStatus}
            </span>
          )}
        </div>
      </div>

      {updateInfo?.hasUpdate && !updateDismissed && (
        <UpdateNotification
          updateInfo={updateInfo}
          onIgnored={() => setUpdateDismissed(true)}
        />
      )}
    </>
  );
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="za-row-between">
      <span className="za-muted">{k}</span>
      <span className="za-mono" style={{ fontSize: "var(--fs-xs)" }}>
        {v}
      </span>
    </div>
  );
}

/* ===== 外观定制局部组件 ===== */

const subLabel: CSSProperties = {
  fontSize: "var(--fs-sm)",
  color: "var(--text-secondary)",
  fontWeight: 500,
  marginBottom: 6,
};

/** 主题色预设（OKLCH hue；null = 默认青绿，不覆盖任何 token） */
const ACCENT_PRESETS: { label: string; hue: number | null }[] = [
  { label: "青绿（默认）", hue: null },
  { label: "蔚蓝", hue: 250 },
  { label: "紫罗兰", hue: 300 },
  { label: "品红", hue: 350 },
  { label: "珊瑚", hue: 25 },
  { label: "琥珀", hue: 75 },
];

function AccentChip({
  label,
  hue,
  selected,
  onClick,
}: {
  label: string;
  hue: number | null;
  selected: boolean;
  onClick: () => void;
}) {
  const color = `oklch(0.68 0.15 ${hue ?? 180})`;
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: 4,
        background: "transparent",
        border: "none",
        padding: 0,
        cursor: "pointer",
      }}
    >
      <span
        style={{
          width: 24,
          height: 24,
          borderRadius: "50%",
          background: color,
          display: "block",
          boxShadow: selected
            ? `0 0 0 2px var(--bg-base), 0 0 0 4px ${color}`
            : undefined,
        }}
      />
      <span
        style={{
          fontSize: "var(--fs-xs)",
          color: selected ? "var(--text-primary)" : "var(--text-tertiary)",
          whiteSpace: "nowrap",
        }}
      >
        {label}
      </span>
    </button>
  );
}

function Slider({
  value,
  min,
  max,
  disabled,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  disabled?: boolean;
  onChange: (v: number) => void;
}) {
  return (
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
          accentColor: "var(--accent)",
          cursor: disabled ? "not-allowed" : "pointer",
          opacity: disabled ? 0.45 : 1,
        }}
      />
      <span className="za-mono" style={{ width: 44, textAlign: "right" }}>
        {value}%
      </span>
    </div>
  );
}
