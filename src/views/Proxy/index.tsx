import { useEffect, useState, type CSSProperties } from "react";
import { proxy as proxyApi } from "../../api";
import type { ProxyConfig } from "../../types";
import { Switch } from "../../components/Switch";
import { toast } from "../../components/Toast";

const field: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 4,
  fontSize: "var(--fs-sm)",
  color: "var(--text-secondary)",
};

export default function Proxy() {
  const [cfg, setCfg] = useState<ProxyConfig>({
    enabled: false,
    type: "http",
    host: "",
    port: 7890,
    username: "",
    hasPassword: false,
  });
  const [pw, setPw] = useState("");
  const [busy, setBusy] = useState(false);
  const [test, setTest] = useState<unknown>(null);

  const load = async () => {
    try {
      setCfg(await proxyApi.get());
    } catch {
      /* 未配置 */
    }
  };
  useEffect(() => {
    load();
  }, []);

  const save = async () => {
    setBusy(true);
    try {
      await proxyApi.set({ ...cfg, hasPassword: pw.length > 0 }, pw || undefined);
      toast.success("已保存");
      setPw("");
    } catch (e: unknown) {
      toast.error(typeof e === "string" ? e : "保存失败");
    } finally {
      setBusy(false);
    }
  };
  const doTest = async () => {
    setBusy(true);
    try {
      setTest(await proxyApi.test());
    } catch (e: unknown) {
      setTest({ ok: false, error: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const t = test as { ok?: boolean; latencyMs?: number; status?: number; error?: string };

  return (
    <div className="za-panel za-card-pad">
      <div className="za-section-title">
        <h3>网络代理</h3>
        <div className="za-row">
          <Switch
            on={cfg.enabled}
            onChange={(on) => setCfg({ ...cfg, enabled: on })}
          />
          <span className="za-muted" style={{ fontSize: "var(--fs-sm)" }}>
            {cfg.enabled ? "已启用" : "已禁用"}
          </span>
        </div>
      </div>
      <p className="za-muted" style={{ margin: "0 0 12px" }}>
        所有对外请求（配额查询、模型列表拉取）均走此代理。密码存 OS keyring。
      </p>

      <div className="za-grid za-grid-2" style={{ marginBottom: 12 }}>
        <label style={field}>
          协议
          <select
            className="za-select"
            value={cfg.type}
            onChange={(e) =>
              setCfg({
                ...cfg,
                type: e.target.value as ProxyConfig["type"],
              })
            }
            disabled={!cfg.enabled}
          >
            <option value="http">HTTP</option>
            <option value="socks5">SOCKS5</option>
          </select>
        </label>
        <label style={field}>
          端口
          <input
            className="za-input"
            type="number"
            value={cfg.port || ""}
            onChange={(e) => setCfg({ ...cfg, port: Number(e.target.value) })}
            disabled={!cfg.enabled}
            placeholder="7890"
          />
        </label>
        <label style={field}>
          主机
          <input
            className="za-input"
            value={cfg.host}
            onChange={(e) => setCfg({ ...cfg, host: e.target.value })}
            disabled={!cfg.enabled}
            placeholder="127.0.0.1"
          />
        </label>
        <label style={field}>
          用户名（可选）
          <input
            className="za-input"
            value={cfg.username || ""}
            onChange={(e) => setCfg({ ...cfg, username: e.target.value })}
            disabled={!cfg.enabled}
          />
        </label>
      </div>
      <label style={{ ...field, marginBottom: 12 }}>
        密码（可选，存 keyring；留空不修改）
        <input
          className="za-input"
          type="password"
          value={pw}
          onChange={(e) => setPw(e.target.value)}
          placeholder={
            cfg.hasPassword ? "••••（已设置，留空保留）" : ""
          }
          disabled={!cfg.enabled}
        />
      </label>

      <div className="za-row" style={{ gap: 8 }}>
        <button
          className="za-btn za-btn-primary"
          disabled={busy}
          onClick={save}
        >
          保存
        </button>
        <button className="za-btn" disabled={busy} onClick={doTest}>
          测试连通性
        </button>
      </div>

      {t && (
        <div
          className="za-muted za-mono"
          style={{ fontSize: "var(--fs-xs)", marginTop: 10 }}
        >
          {t.ok
            ? `✓ 连通（${t.latencyMs}ms，status ${t.status}）`
            : `✗ ${t.error || "失败"}`}
        </div>
      )}
    </div>
  );
}
