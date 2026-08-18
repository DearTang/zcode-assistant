import { useState } from "react";
import { zcode } from "../api";
import { IconPower } from "./icons";
import { toast } from "./Toast";

/**
 * 重启 zcode 提示条：说明文字 + 手动重启按钮。
 * 用于模型管理 / 账号切换等需要配置生效的页面顶部。
 */
export function RestartBar({ hint }: { hint: string }) {
  const [busy, setBusy] = useState(false);

  const handleRestart = async () => {
    setBusy(true);
    try {
      await zcode.restartZcode();
      toast.success("zcode 已重启");
    } catch (e: unknown) {
      toast.error(typeof e === "string" ? e : "重启失败");
    } finally {
      setBusy(false);
    }
  };

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
