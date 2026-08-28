import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { win, events, prefs as prefsApi, pickRingBuckets, feLog } from "../api";
import { DualRing } from "../components/DualRing";
import type { AppPrefs, QuotaOverview } from "../types";

const clamp = (n: number) => Math.max(0, Math.min(100, n));

/** 单击与双击的判定窗口：单击延迟固定面板，双击取消挂起的固定并打开主界面 */
const CLICK_DELAY = 260;

/**
 * 悬浮球：悬停 → 展开信息面板（float-panel 独立窗口），单击 → 固定/取消固定
 * 面板（固定后鼠标移开、点击窗口外都不收起），双击 → 打开主界面，拖拽 → 移动。
 * 鼠标离开时通过 float://ball-leave 通知面板延迟隐藏（球与面板是两个 OS 窗口，
 * 跨窗口 hover 需事件总线协调，否则移到面板途中会触发隐藏）。
 */
export default function FloatBall() {
  const [data, setData] = useState<QuotaOverview | null>(null);
  const [pinned, setPinned] = useState(false);
  const [prefs, setPrefs] = useState<AppPrefs>({
    floatBallVisible: true,
    usageDisplay: "used",
    switchRestartZcode: true,
    autostart: false,
  });
  const clickTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const wasDragged = useRef(false);

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
    // 面板固定态（单击球切换；面板 ✕ / 再次单击收起时复位）→ 显示指示点
    let unPin: (() => void) | undefined;
    events.onPanelPinned(setPinned).then((fn) => (unPin = fn));
    return () => {
      un?.();
      unPrefs?.();
      unPin?.();
      if (clickTimer.current) clearTimeout(clickTimer.current);
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
    // 新一次按下即取消挂起的单击固定（快速点一下又按住拖拽的场景不应触发固定）
    if (clickTimer.current) {
      clearTimeout(clickTimer.current);
      clickTimer.current = null;
    }
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
      wasDragged.current = dragged;
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

  // 单击 → 固定/取消固定面板。双击会先触发两次 click：第二次命中挂起定时器
  // 直接忽略，随后 dblclick 取消定时器并打开主界面，避免固定态被误切换。
  const onClick = () => {
    if (wasDragged.current) {
      wasDragged.current = false;
      return;
    }
    if (clickTimer.current) return;
    clickTimer.current = setTimeout(() => {
      clickTimer.current = null;
      feLog("click → toggle_float_panel_pin");
      win
        .toggleFloatPanelPin()
        .catch((e) => feLog("toggleFloatPanelPin ERR: " + String(e), "error"));
    }, CLICK_DELAY);
  };

  // 双击 → 打开主界面（取消挂起的单击固定）
  const onDoubleClick = () => {
    if (clickTimer.current) {
      clearTimeout(clickTimer.current);
      clickTimer.current = null;
    }
    feLog("dblclick → showMain");
    win.showMain().catch((e) => feLog("showMain ERR: " + String(e), "error"));
  };

  return (
    <div className="fb-stage">
      <div
        className={pinned ? "fb-ball fb-ball-pinned" : "fb-ball"}
        onMouseDown={onMouseDown}
        onMouseEnter={onMouseEnter}
        onMouseLeave={onMouseLeave}
        onClick={onClick}
        onDoubleClick={onDoubleClick}
        title={pinned ? "面板已固定：单击球收起，双击打开主界面" : "单击固定面板，双击打开主界面"}
      >
        {pinned && <span className="fb-pin-dot" />}
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
