import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { win, events, pickBucketByName, feLog } from "../api";
import { DualRing } from "../components/DualRing";
import type { QuotaOverview } from "../types";

const clamp = (n: number) => Math.max(0, Math.min(100, n));

/**
 * 悬浮球：单击展开面板，拖拽移动。
 * 自定义 tooltip（悬停时显示在球正下方两行：每5小时 / 每周），替掉位置偏右的原生 title。
 */
export default function FloatBall() {
  const [data, setData] = useState<QuotaOverview | null>(null);
  const [hovered, setHovered] = useState(false);

  useEffect(() => {
    feLog("mounted, label=" + getCurrentWindow().label);
    getCurrentWindow()
      .setIgnoreCursorEvents(false)
      .then(() => feLog("setIgnoreCursorEvents(false) ok"))
      .catch((e) => feLog("setIgnoreCursorEvents failed: " + String(e), "error"));
    let un: (() => void) | undefined;
    events.onQuotaUpdated((q) => setData(q)).then((fn) => {
      feLog("onQuotaUpdated listener registered");
      un = fn;
    });
    return () => un?.();
  }, []);

  const b5 = pickBucketByName(data, "5小时");
  const bW = pickBucketByName(data, "每周");
  const used5 = b5 && b5.total > 0 ? clamp((b5.used / b5.total) * 100) : null;
  const usedW = bW && bW.total > 0 ? clamp((bW.used / bW.total) * 100) : null;

  const onMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    const startX = e.screenX;
    const startY = e.screenY;
    feLog(
      `mousedown screen=(${startX},${startY}) client=(${e.clientX},${e.clientY})`
    );
    let dragged = false;
    const onMove = (ev: MouseEvent) => {
      if (
        !dragged &&
        (Math.abs(ev.screenX - startX) > 4 ||
          Math.abs(ev.screenY - startY) > 4)
      ) {
        dragged = true;
        feLog("drag threshold exceeded → startDragging");
        getCurrentWindow()
          .startDragging()
          .then(() => feLog("startDragging ok"))
          .catch((err) =>
            feLog("startDragging failed: " + String(err), "error")
          );
      }
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      feLog(`mouseup, dragged=${dragged}`);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  const onClick = async () => {
    feLog("click → toggleFloatPanel (invoking)");
    let settled = false;
    const timer = setTimeout(() => {
      if (!settled)
        feLog(
          "toggleFloatPanel: invoke 未在 2s 内返回 — 后端命令挂起/未响应",
          "warn"
        );
    }, 2000);
    try {
      await win.toggleFloatPanel();
      settled = true;
      feLog("toggleFloatPanel ok");
    } catch (e) {
      settled = true;
      feLog("toggleFloatPanel ERR: " + String(e), "error");
    } finally {
      clearTimeout(timer);
    }
  };

  const tip = (label: string, v: number | null) =>
    `${label}: ${v != null ? Math.round(v) + "%" : "—"}`;

  return (
    <div className="fb-stage">
      <div
        className="fb-ball"
        onMouseDown={onMouseDown}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        onClick={onClick}
      >
        <div className="fb-inner">
          <div
            style={{
              position: "absolute",
              inset: 0,
              display: "grid",
              placeItems: "center",
            }}
          >
            <DualRing size={52} usedOuter={used5} usedInner={usedW} />
          </div>
          <div className="fb-center">
            {used5 != null ? (
              <>
                <span className="fb-pct">{Math.round(used5)}</span>
                <span className="fb-unit">%</span>
              </>
            ) : (
              <span className="fb-dot" />
            )}
          </div>
        </div>
      </div>
      {hovered && (
        <div className="fb-tooltip">
          <div>{tip("每5小时", used5)}</div>
          <div>{tip("每周", usedW)}</div>
        </div>
      )}
    </div>
  );
}
