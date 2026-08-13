import { useEffect, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { events, zcode } from "../api";

/** 全局重启确认弹窗：监听后端 zcode://restart-requested，确认后重启 zcode */
export function RestartDialog() {
  const [reason, setReason] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let un: UnlistenFn | undefined;
    events.onRestartRequested((p) => setReason(p.reason)).then((fn) => (un = fn));
    return () => un?.();
  }, []);

  if (!reason) return null;

  const confirm = async () => {
    setBusy(true);
    try {
      await zcode.restartZcode();
      setReason(null);
    } catch (e: unknown) {
      alert(typeof e === "string" ? e : "重启失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="rd-overlay">
      <div className="za-glass-strong rd-card">
        <strong>需要重启 zcode 生效</strong>
        <p className="za-muted" style={{ fontSize: "var(--fs-sm)", margin: 0 }}>
          {reason}
        </p>
        <div
          className="za-row"
          style={{ justifyContent: "flex-end", gap: 8, marginTop: 10 }}
        >
          <button
            className="za-btn za-btn-sm"
            disabled={busy}
            onClick={() => setReason(null)}
          >
            稍后
          </button>
          <button
            className="za-btn za-btn-sm za-btn-primary"
            disabled={busy}
            onClick={confirm}
          >
            {busy ? "重启中…" : "立即重启"}
          </button>
        </div>
      </div>
    </div>
  );
}
