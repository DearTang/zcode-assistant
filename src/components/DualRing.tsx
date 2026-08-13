import type { CSSProperties } from "react";
import { usageColor } from "../api";

const clamp = (n: number) => Math.max(0, Math.min(100, n));

/**
 * 双环配额图：外圈 = 每 5 小时额度、内圈 = 每周额度。
 *
 * - 弧长 = 剩余可用占比（绿色越长越健康）
 * - 颜色 = 按已用占比分级（<70% 绿 / ≥70% 黄 / ≥90% 红）
 * - usedOuter / usedInner 传 null 时该环仅显示底环（无数据）
 */
export function DualRing({
  size = 52,
  usedOuter,
  usedInner,
  strokeWidthOuter = 3,
  strokeWidthInner = 2.5,
  style,
}: {
  size?: number;
  usedOuter: number | null;
  usedInner: number | null;
  strokeWidthOuter?: number;
  strokeWidthInner?: number;
  style?: CSSProperties;
}) {
  const c = size / 2;
  const rO = c - strokeWidthOuter / 2 - 0.5;
  const rI = c - 9;
  const CO = 2 * Math.PI * rO;
  const CI = 2 * Math.PI * rI;
  // offset = C * (1 - used%/100) → 弧长 = 已用占比（已用越多弧越长、颜色越警戒）
  const offO = usedOuter != null ? CO * (1 - clamp(usedOuter) / 100) : CO;
  const offI = usedInner != null ? CI * (1 - clamp(usedInner) / 100) : CI;
  const colO = usedOuter != null ? usageColor(usedOuter) : "rgba(255,255,255,0.25)";
  const colI = usedInner != null ? usageColor(usedInner) : "rgba(255,255,255,0.25)";
  const arcTransition = "stroke-dashoffset 0.5s ease, stroke 0.3s ease";

  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      style={{ transform: "rotate(-90deg)", display: "block", ...style }}
    >
      <circle
        cx={c}
        cy={c}
        r={rO}
        fill="none"
        stroke="rgba(255,255,255,0.1)"
        strokeWidth={strokeWidthOuter}
      />
      <circle
        cx={c}
        cy={c}
        r={rO}
        fill="none"
        stroke={colO}
        strokeWidth={strokeWidthOuter}
        strokeLinecap="round"
        strokeDasharray={CO}
        strokeDashoffset={offO}
        style={{ transition: arcTransition }}
      />
      <circle
        cx={c}
        cy={c}
        r={rI}
        fill="none"
        stroke="rgba(255,255,255,0.1)"
        strokeWidth={strokeWidthInner}
      />
      <circle
        cx={c}
        cy={c}
        r={rI}
        fill="none"
        stroke={colI}
        strokeWidth={strokeWidthInner}
        strokeLinecap="round"
        strokeDasharray={CI}
        strokeDashoffset={offI}
        style={{ transition: arcTransition }}
      />
    </svg>
  );
}
