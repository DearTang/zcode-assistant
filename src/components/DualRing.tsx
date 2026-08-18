import type { CSSProperties } from "react";
import { usageColor } from "../api";

const clamp = (n: number) => Math.max(0, Math.min(100, n));

/**
 * 双环配额图：外圈 = 每 5 小时额度、内圈 = 每周额度。
 *
 * - 弧长 = 展示方案对应占比（默认已用占比；showRemaining 时为剩余占比）
 * - 颜色 = 始终按已用占比分级（<70% 绿 / ≥70% 黄 / ≥90% 红）
 * - usedOuter / usedInner 传 null 时该环仅显示底环（无数据）
 */
export function DualRing({
  size = 52,
  usedOuter,
  usedInner,
  showRemaining = false,
  strokeWidthOuter = 3,
  strokeWidthInner = 2.5,
  style,
}: {
  size?: number;
  usedOuter: number | null;
  usedInner: number | null;
  /** true=弧长展示剩余占比（颜色仍按已用度分级） */
  showRemaining?: boolean;
  strokeWidthOuter?: number;
  strokeWidthInner?: number;
  style?: CSSProperties;
}) {
  const c = size / 2;
  const rO = c - strokeWidthOuter / 2 - 0.5;
  const rI = c - 9;
  const CO = 2 * Math.PI * rO;
  const CI = 2 * Math.PI * rI;
  // 弧长占比随展示方案：已用 → used%；剩余 → 100 - used%
  const fracO =
    usedOuter != null
      ? (showRemaining ? 100 - clamp(usedOuter) : clamp(usedOuter)) / 100
      : 0;
  const fracI =
    usedInner != null
      ? (showRemaining ? 100 - clamp(usedInner) : clamp(usedInner)) / 100
      : 0;
  const offO = usedOuter != null ? CO * (1 - fracO) : CO;
  const offI = usedInner != null ? CI * (1 - fracI) : CI;
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
