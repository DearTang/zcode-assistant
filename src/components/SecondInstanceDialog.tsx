import { useEffect, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { events, win } from "../api";

/**
 * 单实例弹窗：第二个应用实例尝试启动时，已有实例收到 app://second-instance
 * 广播（后端已唤出主窗口、新实例已自动退出），由用户选择：
 * - 覆盖启动：重启当前进程（restart_app），等效用新实例替换旧实例
 * - 退出：保持现有实例，仅关闭弹窗
 */
export function SecondInstanceDialog() {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let un: UnlistenFn | undefined;
    events.onSecondInstance(() => setOpen(true)).then((fn) => (un = fn));
    return () => un?.();
  }, []);

  if (!open) return null;

  return (
    <div className="rd-overlay">
      <div className="za-glass-strong rd-card">
        <strong>应用已在运行</strong>
        <p className="za-muted" style={{ fontSize: "var(--fs-sm)", margin: 0 }}>
          检测到另一次启动。覆盖启动会结束当前实例并重新启动应用；
          退出则保持当前实例继续运行（本次启动已自动结束）。
        </p>
        <div
          className="za-row"
          style={{ justifyContent: "flex-end", gap: 8, marginTop: 10 }}
        >
          <button
            className="za-btn za-btn-sm"
            onClick={() => setOpen(false)}
          >
            退出
          </button>
          <button
            className="za-btn za-btn-sm za-btn-primary"
            onClick={() => win.restartApp().catch(() => {})}
          >
            覆盖启动
          </button>
        </div>
      </div>
    </div>
  );
}
