import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { win, events, prefs as prefsApi, pickRingBuckets, feLog } from "../api";
import { DualRing } from "../components/DualRing";
import type { AppPrefs, QuotaOverview } from "../types";

const clamp = (n: number) => Math.max(0, Math.min(100, n));

/**
 * 悬浮球：悬停 → 展开信息面板（float-panel 独立窗口），双击 → 打开主界面，拖拽 → 移动。
 * 鼠标离开时通过 float://ball-leave 通知面板延迟隐藏（球与面板是两个 OS 窗口，
 * 跨窗口 hover 需事件总线协调，否则移到面板途中会触发隐藏）。
 */
export default function FloatBall() {
  const [data, setData] = useState<QuotaOverview | null>(null);
  const [prefs, setPrefs] = useState<AppPrefs>({
    floatBallVisible: true,
    usageDisplay: "used",
    switchRestartZcode: true,
    autostart: false,
  });

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
    // 展示方案（已用 / 剩余）随设置联动
    prefsApi.get().then(setPrefs).catch(() => {});
    let unPrefs: (() => void) | undefined;
    events.onPrefsUpdated(setPrefs).then((fn) => (unPrefs = fn));
    return () => {
      un?.();
      unPrefs?.();
    };
  }, []);

  // 双环 bucket：智谱取「每5小时 / 每周」，其余供应商（用量模板）回退前两个 bucket
  const { b5, bW } = pickRingBuckets(data);
  const used5 = b5 && b5.total > 0 ? clamp((b5.used / b5.total) * 100) : null;
  const usedW = bW && bW.total > 0 ? clamp((bW.used / bW.total) * 100) : null;
  // 中心数字按展示方案：已用占比 或 剩余占比（环弧长同步）
  const showRemaining = prefs.usageDisplay === "remaining";
  const shown5 = used5 != null ? (showRemaining ? 100 - used5 : used5) : null;
  const shownW = usedW != null ? (showRemaining ? 100 - usedW : usedW) : null;

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

  // 悬停 → 显示面板；离开 → 通知面板延迟隐藏（面板自身决定是否真的隐藏）
  const onMouseEnter = () => {
    feLog("hover → show_float_panel");
    win
      .showFloatPanel()
      .catch((e) => feLog("showFloatPanel ERR: " + String(e), "error"));
  };
  const onMouseLeave = () => {
    feLog("leave → emit ball-leave");
    events.emitBallLeave().catch(() => {});
  };

  // 双击 → 打开主界面（单击已由悬停展示面板，不再 toggle）
  const onDoubleClick = () => {
    feLog("dblclick → showMain");
    win.showMain().catch((e) => feLog("showMain ERR: " + String(e), "error"));
  };

  return (
    <div className="fb-stage">
      <div
        className="fb-ball"
        onMouseDown={onMouseDown}
        onMouseEnter={onMouseEnter}
        onMouseLeave={onMouseLeave}
        onDoubleClick={onDoubleClick}
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
            <DualRing
              size={52}
              usedOuter={used5}
              usedInner={usedW}
              showRemaining={showRemaining}
            />
          </div>
          <div className="fb-center">
            {shown5 != null || shownW != null ? (
              <>
                {/* 上行：每 5 小时（对应外环，按展示方案为已用或剩余） */}
                <div className="fb-line">
                  <span className="fb-pct">
                    {shown5 != null ? Math.round(shown5) : "—"}
                  </span>
                  <span className="fb-unit">%</span>
                </div>
                {/* 下行：每周（对应内环） */}
                <div className="fb-line fb-line-sub">
                  <span className="fb-pct-sm">
                    {shownW != null ? Math.round(shownW) : "—"}
                  </span>
                  <span className="fb-unit-sm">%</span>
                </div>
              </>
            ) : (
              <span className="fb-dot" />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
