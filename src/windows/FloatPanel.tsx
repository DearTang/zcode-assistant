import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { win, events, pickBucketByName, usageColor, feLog } from "../api";
import { IconClose, IconExternal, IconRefresh } from "../components/icons";
import type { QuotaOverview } from "../types";

/** 展开的迷你面板：账号 / 套餐 / 配额进度 + 快捷操作。 */
export default function FloatPanel() {
  const [q, setQ] = useState<QuotaOverview | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = () => {
    // 转发给 Dashboard 统一查询，结果经 quota://updated 回流
    setLoading(true);
    events.emitRefreshRequested();
    setTimeout(() => setLoading(false), 800);
  };

  useEffect(() => {
    feLog("mounted, label=" + getCurrentWindow().label);
    // 仅监听 Dashboard 广播，不再自行定时查询
    let un: (() => void) | undefined;
    events.onQuotaUpdated((q) => setQ(q)).then((fn) => (un = fn));
    return () => {
      un?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const b5 = pickBucketByName(q, "5小时");
  const bW = pickBucketByName(q, "每周");
  const used5 = b5 && b5.total > 0 ? (b5.used / b5.total) * 100 : null;
  const usedW = bW && bW.total > 0 ? (bW.used / bW.total) * 100 : null;

  return (
    <div className="fp-root">
      <div className="fp-card">
        <div
          className="fp-header"
          onMouseDown={() => getCurrentWindow().startDragging()}
        >
          <span className="fp-title">zcode-assistant</span>
          <button
            className="za-icon-btn"
            style={{ width: 26, height: 26 }}
            onClick={() => win.hideFloatPanel()}
          >
            <IconClose width={14} height={14} />
          </button>
        </div>

        <div className="fp-row">
          <span className="fp-row-label">当前账号</span>
          <span className="fp-row-value">{q?.accountLabel ?? "—"}</span>
        </div>
        <div className="fp-row">
          <span className="fp-row-label">套餐</span>
          <span className="fp-row-value">{q?.planName ?? "—"}</span>
        </div>

        <div className="fp-quota">
          <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
            <QuotaBlock label="每5小时" usedPct={used5} />
            <QuotaBlock label="每周" usedPct={usedW} />
          </div>
        </div>

        <div className="fp-actions">
          <button
            className="za-btn za-btn-sm"
            onClick={() => refresh()}
            disabled={loading}
          >
            <IconRefresh width={13} height={13} /> {loading ? "…" : "刷新"}
          </button>
          <button
            className="za-btn za-btn-sm za-btn-primary"
            onClick={() => win.showMain()}
          >
            <IconExternal width={13} height={13} /> 主界面
          </button>
        </div>
      </div>
    </div>
  );
}

/** 单行额度块：小环 + 标签 + 可用占比（颜色按已用度分级） */
function QuotaBlock({
  label,
  usedPct,
}: {
  label: string;
  usedPct: number | null;
}) {
  const color = usedPct != null ? usageColor(usedPct) : "var(--text-tertiary)";
  const remaining =
    usedPct != null ? Math.max(0, Math.round(100 - usedPct)) : null;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
      <Ring size={32} usedPct={usedPct} />
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 2,
          minWidth: 0,
        }}
      >
        <span
          style={{ fontSize: "var(--fs-xs)", color: "var(--text-secondary)" }}
        >
          {label}
        </span>
        <span
          className="za-mono"
          style={{ fontSize: "var(--fs-md)", fontWeight: 600, color }}
        >
          {remaining != null ? `剩余 ${remaining}%` : "—"}
        </span>
      </div>
    </div>
  );
}

/** 单环：弧长=剩余可用，颜色按已用占比分级（绿/黄/红） */
function Ring({ size, usedPct }: { size: number; usedPct: number | null }) {
  const c = size / 2;
  const r = c - 3;
  const C = 2 * Math.PI * r;
  const remaining =
    usedPct != null ? Math.max(0, Math.min(100, 100 - usedPct)) : null;
  const offset = remaining != null ? C * (1 - remaining / 100) : C;
  const color =
    usedPct != null ? usageColor(usedPct) : "rgba(255,255,255,0.25)";
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
