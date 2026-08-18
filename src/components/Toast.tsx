import { useEffect, useState } from "react";
import { IconClose } from "./icons";

/* ============ 全局 Toast 通知 ============ */
/* 用法：任意组件中 import { toast } from "../components/Toast";
   toast.success("已保存"); toast.error("失败：xxx"); toast.warning("注意：xxx");
   特性：成功绿/错误红/警告黄、4s 自动消失、悬停暂停（离开后再计时）、可复制、可手动关闭 */

type ToastKind = "success" | "error" | "warning";

interface ToastItem {
  id: number;
  kind: ToastKind;
  text: string;
}

const listeners = new Set<(items: ToastItem[]) => void>();
let items: ToastItem[] = [];
let nextId = 1;

function emit() {
  listeners.forEach((l) => l([...items]));
}

function push(kind: ToastKind, text: string) {
  const id = nextId++;
  items = [...items, { id, kind, text }];
  // 最多同时显示 5 条，超出丢最旧的
  if (items.length > 5) items = items.slice(-5);
  emit();
}

function dismiss(id: number) {
  items = items.filter((t) => t.id !== id);
  emit();
}

export const toast = {
  success: (text: string) => push("success", text),
  error: (text: string) => push("error", text),
  warning: (text: string) => push("warning", text),
};

/** 挂载在应用根节点；固定右上角，浮于所有内容之上 */
export function ToastContainer() {
  const [list, setList] = useState<ToastItem[]>([]);
  useEffect(() => {
    const fn = (next: ToastItem[]) => setList(next);
    listeners.add(fn);
    return () => {
      listeners.delete(fn);
    };
  }, []);
  if (list.length === 0) return null;
  return (
    <div
      style={{
        position: "fixed",
        top: 16,
        right: 16,
        zIndex: 99999,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        maxWidth: 440,
        pointerEvents: "none",
      }}
    >
      {list.map((t) => (
        <ToastCard key={t.id} item={t} onClose={() => dismiss(t.id)} />
      ))}
    </div>
  );
}

const KIND_STYLE: Record<
  ToastKind,
  { bg: string; border: string; color: string; icon: string }
> = {
  success: {
    bg: "rgba(34,197,94,0.12)",
    border: "rgba(34,197,94,0.55)",
    color: "#22C55E",
    icon: "✓",
  },
  error: {
    bg: "rgba(239,68,68,0.12)",
    border: "rgba(239,68,68,0.55)",
    color: "#EF4444",
    icon: "✕",
  },
  warning: {
    bg: "rgba(245,158,11,0.12)",
    border: "rgba(245,158,11,0.55)",
    color: "#F59E0B",
    icon: "⚠",
  },
};

function ToastCard({
  item,
  onClose,
}: {
  item: ToastItem;
  onClose: () => void;
}) {
  const [hovered, setHovered] = useState(false);
  const [copied, setCopied] = useState(false);
  const s = KIND_STYLE[item.kind];

  // 自动消失：4s；悬停暂停计时，鼠标离开后重新计 4s
  useEffect(() => {
    if (hovered) return;
    const t = setTimeout(onClose, 4000);
    return () => clearTimeout(t);
  }, [hovered, onClose]);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(item.text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      /* 剪贴板不可用，忽略 */
    }
  };

  return (
    <div
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        pointerEvents: "auto",
        display: "flex",
        alignItems: "flex-start",
        gap: 8,
        padding: "10px 12px",
        borderRadius: 8,
        background: s.bg,
        border: `1px solid ${s.border}`,
        backdropFilter: "blur(12px)",
        boxShadow: "0 4px 16px rgba(0,0,0,0.25)",
        fontSize: "var(--fs-sm)",
        lineHeight: 1.5,
      }}
    >
      <span
        style={{ color: s.color, fontWeight: 700, flexShrink: 0, lineHeight: 1.5 }}
      >
        {s.icon}
      </span>
      <span
        className="za-mono"
        style={{
          color: "var(--text-primary)",
          wordBreak: "break-all",
          whiteSpace: "pre-wrap",
          flex: 1,
        }}
      >
        {item.text}
      </span>
      <button
        type="button"
        onClick={handleCopy}
        title="复制消息"
        style={{
          flexShrink: 0,
          border: "none",
          background: "transparent",
          color: copied ? s.color : "var(--text-tertiary)",
          cursor: "pointer",
          fontSize: "var(--fs-xs)",
          padding: "2px 4px",
          borderRadius: 4,
          lineHeight: 1.5,
        }}
      >
        {copied ? "已复制" : "复制"}
      </button>
      <button
        type="button"
        onClick={onClose}
        title="关闭"
        style={{
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          border: "none",
          background: "transparent",
          color: "var(--text-tertiary)",
          cursor: "pointer",
          padding: 0,
          lineHeight: 1,
        }}
      >
        <IconClose width={12} height={12} />
      </button>
    </div>
  );
}
