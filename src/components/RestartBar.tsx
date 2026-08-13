import { useState } from "react";
import { zcode } from "../api";
import { IconPower } from "./icons";

/**
 * 重启 zcode 提示条：说明文字 + 手动重启按钮。
 * 用于模型管理 / 账号切换等需要配置生效的页面顶部。
 */
export function RestartBar({ hint }: { hint: string }) {
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  const handleRestart = async () => {
    setBusy(true);
    setMsg(null);
    try {
      await zcode.restartZcode();
      setMsg("zcode 已重启");
    } catch (e: unknown) {
      setMsg(typeof e === "string" ? e : "重启失败");
    } finally {
      setBusy(false);
    }
  };

  const isErr = msg && (msg.includes("失败") || msg.includes("未"));

  return (
    <div
      className="za-panel za-card-pad"
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 12,
      }}
    >
      <div style={{ display: "flex", flexDirection: "column", gap: 2, minWidth: 0 }}>
        <span className="za-muted" style={{ fontSize: "var(--fs-sm)" }}>
          {hint}
        </span>
        {msg && (
          <span
            className="za-mono"
            style={{
              fontSize: "var(--fs-xs)",
              color: isErr ? "#ef4444" : "var(--accent)",
            }}
          >
            {isErr ? "✕ " : "✓ "}
            {msg}
          </span>
        )}
      </div>
      <button
        className="za-btn za-btn-sm"
        disabled={busy}
        onClick={handleRestart}
        style={{ flexShrink: 0 }}
      >
        <IconPower width={13} height={13} /> {busy ? "重启中…" : "重启 zcode"}
      </button>
    </div>
  );
}
