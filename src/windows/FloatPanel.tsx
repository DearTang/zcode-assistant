import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  win,
  events,
  prefs as prefsApi,
  pickRingBuckets,
  usageColor,
  feLog,
} from "../api";
import type { AppPrefs, QuotaOverview } from "../types";

const HIDE_DELAY = 250;

/**
 * 悬浮球触发的信息面板（独立 float-panel 窗口）：供应商 / 套餐 / 配额进度。
 * 纯展示（无按钮）。hover 联动隐藏由 float://ball-leave 事件协调：
 *   球离开 → 发 ball-leave → 本面板 250ms 后隐藏；
 *   鼠标进入本面板 → 取消隐藏；离开本面板 → 立即隐藏。
 * （球与面板是两个 OS 窗口，跨窗口 hover 必须用事件总线，否则移到面板途中会被隐藏）
 */
export default function FloatPanel() {
  const [q, setQ] = useState<QuotaOverview | null>(null);
  const [prefs, setPrefs] = useState<AppPrefs>({
    floatBallVisible: true,
    usageDisplay: "used",
    switchRestartZcode: true,
    autostart: false,
  });
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const cancelHide = () => {
    if (hideTimer.current) {
      clearTimeout(hideTimer.current);
      hideTimer.current = null;
    }
  };
  const scheduleHide = () => {
    cancelHide();
    hideTimer.current = setTimeout(() => {
      feLog("panel: hide by scheduled timer");
      win.hideFloatPanel();
    }, HIDE_DELAY);
  };
  const hideNow = () => {
    cancelHide();
    win.hideFloatPanel();
  };

  useEffect(() => {
    feLog("mounted, label=" + getCurrentWindow().label);
    // 配额（主窗口 App 每 5s 全局轮询广播，唯一数据源；不依赖总览视图是否打开）
    let unQ: (() => void) | undefined;
    events.onQuotaUpdated((v) => setQ(v)).then((fn) => (unQ = fn));
    // 展示方案（已用 / 剩余）随设置联动
    prefsApi.get().then(setPrefs).catch(() => {});
    let unPrefs: (() => void) | undefined;
    events.onPrefsUpdated(setPrefs).then((fn) => (unPrefs = fn));
    // 球离开 → 延迟隐藏
    let unLeave: (() => void) | undefined;
    events.onBallLeave(() => scheduleHide()).then((fn) => (unLeave = fn));
    return () => {
      unQ?.();
      unPrefs?.();
      unLeave?.();
      cancelHide();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 双环 bucket：智谱取「每5小时 / 每周」，其余供应商（用量模板）回退前两个 bucket
  const { b5, bW } = pickRingBuckets(q);
  const used5 = b5 && b5.total > 0 ? (b5.used / b5.total) * 100 : null;
  const usedW = bW && bW.total > 0 ? (bW.used / bW.total) * 100 : null;
  const end5 = b5?.periodEnd;
  const endW = bW?.periodEnd;

  return (
    <div className="fp-root" onMouseEnter={cancelHide} onMouseLeave={hideNow}>
      <div className="fp-card">
        <div className="fp-header">
          <span className="fp-title">zcode-assistant</span>
        </div>

        <div className="fp-row">
          <span className="fp-row-label">供应商</span>
          <span className="fp-row-value">{q?.providerName ?? "自动 · 智谱"}</span>
        </div>
        <div className="fp-row">
          <span className="fp-row-label">套餐</span>
          <span className="fp-row-value">{q?.planName ?? "—"}</span>
        </div>

        <div className="fp-quota">
          <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            <QuotaBlock
              label={shortBucketName(b5?.name) ?? "每5小时"}
              usedPct={used5}
              periodEnd={end5}
              showRemaining={prefs.usageDisplay === "remaining"}
            />
            {bW && (
              <QuotaBlock
                label={shortBucketName(bW.name) ?? "每周"}
                usedPct={usedW}
                periodEnd={endW}
                showRemaining={prefs.usageDisplay === "remaining"}
              />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/** 展示名压缩：每5小时使用额度 → 每5小时、每周使用额度 → 每周（托盘/悬浮窗悬停口径）；
 *  其余名称（模板供应商自定义桶名）原样返回 */
function shortBucketName(name?: string): string | undefined {
  if (!name) return undefined;
  if (name.includes("5小时")) return "每5小时";
  if (name.includes("每周")) return "每周";
  return name;
}

/** 重置时间紧凑显示：今天 → HH:mm；跨天 → MM-dd HH:mm */
function fmtReset(iso?: string): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  const now = new Date();
  const hh = String(d.getHours()).padStart(2, "0");
  const mi = String(d.getMinutes()).padStart(2, "0");
  if (d.toDateString() === now.toDateString()) return `${hh}:${mi}`;
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${mm}-${dd} ${hh}:${mi}`;
}

/** 单行额度块：小环 + 标签 + 占比（按展示方案为已用或剩余）+ 下次重置时间（颜色按已用度分级） */
function QuotaBlock({
  label,
  usedPct,
  periodEnd,
  showRemaining,
}: {
  label: string;
  usedPct: number | null;
  periodEnd?: string;
  showRemaining?: boolean;
}) {
  const color = usedPct != null ? usageColor(usedPct) : "var(--text-tertiary)";
  const used = usedPct != null ? Math.max(0, Math.round(usedPct)) : null;
  const shown = used != null ? (showRemaining ? 100 - used : used) : null;
  const word = showRemaining ? "剩余" : "已用";
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
      <Ring size={32} usedPct={usedPct} showRemaining={showRemaining} />
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 2,
          minWidth: 0,
          flex: 1,
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "baseline",
            justifyContent: "space-between",
            gap: 8,
          }}
        >
          <span
            style={{ fontSize: "var(--fs-xs)", color: "var(--text-secondary)" }}
          >
            {label}
          </span>
          {periodEnd && (
            <span
              className="za-mono za-faint"
              style={{ fontSize: "var(--fs-xs)" }}
            >
              重置 {fmtReset(periodEnd)}
            </span>
          )}
        </div>
        <span
          className="za-mono"
          style={{ fontSize: "var(--fs-md)", fontWeight: 600, color }}
        >
          {shown != null ? `${word} ${shown}%` : "—"}
        </span>
      </div>
    </div>
  );
}

/** 单环：弧长随展示方案（已用 / 剩余占比），颜色始终按已用占比分级（绿/黄/红） */
function Ring({
  size,
  usedPct,
  showRemaining,
}: {
  size: number;
  usedPct: number | null;
  showRemaining?: boolean;
}) {
  const c = size / 2;
  const r = c - 3;
  const C = 2 * Math.PI * r;
  const used = usedPct != null ? Math.max(0, Math.min(100, usedPct)) : null;
  const frac =
    used != null ? (showRemaining ? 100 - used : used) / 100 : 0;
  const offset = used != null ? C * (1 - frac) : C;
  const color = usedPct != null ? usageColor(usedPct) : "rgba(255,255,255,0.25)";
  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      style={{ transform: "rotate(-90deg)", display: "block" }}
    >
      <circle
        cx={c}
        cy={c}
        r={r}
        fill="none"
        stroke="rgba(255,255,255,0.1)"
        strokeWidth={3}
      />
      <circle
        cx={c}
        cy={c}
        r={r}
        fill="none"
        stroke={color}
        strokeWidth={3}
        strokeLinecap="round"
        strokeDasharray={C}
        strokeDashoffset={offset}
        style={{ transition: "stroke-dashoffset 0.5s ease, stroke 0.3s ease" }}
      />
    </svg>
  );
}
