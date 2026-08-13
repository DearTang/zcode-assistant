import { useEffect, useState, type CSSProperties } from "react";
import { useTheme } from "../../hooks/useTheme";
import { IconSun, IconMoon, IconCheck, IconRefresh } from "../../components/icons";
import { UpdateNotification } from "../../components/UpdateNotification";
import { zcode, templates, app, updater } from "../../api";
import type { QuotaTemplate, UpdateInfo, ZcodeConfig } from "../../types";

const field: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 4,
  fontSize: "var(--fs-sm)",
  color: "var(--text-secondary)",
};

export default function Settings() {
  const { theme, setTheme } = useTheme();
  const [probe, setProbe] = useState<{
    exePath: string | null;
    running: boolean;
    configDir: string;
  } | null>(null);
  const [config, setConfig] = useState<ZcodeConfig | null>(null);
  const [selProvider, setSelProvider] = useState("");
  const [tmpl, setTmpl] = useState<QuotaTemplate | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [version, setVersion] = useState<string>("");
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [updateDismissed, setUpdateDismissed] = useState(false);

  useEffect(() => {
    zcode.probe().then(setProbe).catch(() => setProbe(null));
    zcode.getConfig().then(setConfig).catch(() => {});
    app.getVersion().then(setVersion).catch(() => {});
  }, []);

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

  const loadTemplate = async (key: string) => {
    setSelProvider(key);
    try {
      setTmpl(
        (await templates.get(key)) ?? {
          providerKey: key,
          method: "GET",
        }
      );
    } catch {
      setTmpl({ providerKey: key, method: "GET" });
    }
  };
  const saveTemplate = async () => {
    if (!tmpl) return;
    try {
      await templates.upsert(tmpl);
      setMsg("模板已保存");
    } catch (e: unknown) {
      setMsg(String(e));
    }
  };

  const providers = config ? Object.entries(config.provider) : [];

  return (
    <>
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

      {/* 配额查询模板（M3-2） */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>配额查询模板</h3>
        </div>
        <p className="za-muted" style={{ margin: "0 0 12px", fontSize: "var(--fs-sm)" }}>
          为非 Coding Plan 的自定义 provider 配置配额查询。支持{" "}
          {"{{apiKey}}/{{baseURL}}"} 变量，按 dot path（如{" "}
          <span className="za-mono">data.balance.total</span>）提取总额/已用/剩余。
        </p>
        <label style={{ ...field, marginBottom: 10 }}>
          选择 provider
          <select
            className="za-select"
            value={selProvider}
            onChange={(e) => loadTemplate(e.target.value)}
          >
            <option value="">— 选择 —</option>
            {providers.map(([key, p]) => (
              <option key={key} value={key}>
                {p.name}（{p.source === "custom" ? "自定义" : key}）
              </option>
            ))}
          </select>
        </label>

        {tmpl && (
          <div className="za-grid za-grid-2" style={{ gap: 10 }}>
            <label style={field}>
              名称
              <input
                className="za-input"
                value={tmpl.name ?? ""}
                onChange={(e) => setTmpl({ ...tmpl, name: e.target.value })}
              />
            </label>
            <label style={field}>
              方法
              <select
                className="za-select"
                value={tmpl.method ?? "GET"}
                onChange={(e) => setTmpl({ ...tmpl, method: e.target.value })}
              >
                <option value="GET">GET</option>
                <option value="POST">POST</option>
              </select>
            </label>
            <label style={{ ...field, gridColumn: "1 / -1" }}>
              URL（支持 {"{{baseURL}}/{{apiKey}}"}）
              <input
                className="za-input"
                value={tmpl.url ?? ""}
                onChange={(e) => setTmpl({ ...tmpl, url: e.target.value })}
                placeholder="{{baseURL}}/dashboard/billing/credit_grants"
              />
            </label>
            <label style={{ ...field, gridColumn: "1 / -1" }}>
              Headers（JSON，值支持 {"{{apiKey}}"}）
              <textarea
                className="za-textarea"
                value={tmpl.headersJson ?? ""}
                onChange={(e) => setTmpl({ ...tmpl, headersJson: e.target.value })}
                placeholder='{"Authorization": "Bearer {{apiKey}}"}'
              />
            </label>
            <label style={field}>
              总额 path
              <input
                className="za-input"
                value={tmpl.totalPath ?? ""}
                onChange={(e) => setTmpl({ ...tmpl, totalPath: e.target.value })}
                placeholder="data.total_grants"
              />
            </label>
            <label style={field}>
              已用 path
              <input
                className="za-input"
                value={tmpl.usedPath ?? ""}
                onChange={(e) => setTmpl({ ...tmpl, usedPath: e.target.value })}
                placeholder="data.used"
              />
            </label>
            <label style={field}>
              剩余 path
              <input
                className="za-input"
                value={tmpl.remainingPath ?? ""}
                onChange={(e) => setTmpl({ ...tmpl, remainingPath: e.target.value })}
                placeholder="data.remaining"
              />
            </label>
            <div style={{ gridColumn: "1 / -1", display: "flex", justifyContent: "flex-end" }}>
              <button className="za-btn za-btn-sm za-btn-primary" onClick={saveTemplate}>
                保存模板
              </button>
            </div>
          </div>
        )}
        {msg && (
          <div className="za-muted" style={{ marginTop: 8 }}>
            {msg}
          </div>
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
