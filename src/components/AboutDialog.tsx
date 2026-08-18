import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  IconRefresh,
  IconDownload,
  IconExternal,
  IconSparkle,
} from "./icons";
import { onUpdateDownloadProgress, updater } from "../api";
import type { UpdateInfo } from "../types";
// `?raw` 在构建期把文件内容作为字符串打包（见 vite-env.d.ts）。
// 离线可用、随安装包锁定版本 —— 展示更新日志无需联网。
// CHANGELOG.md 位于仓库根目录，相对 src/components/ 上两级。
import changelog from "../../CHANGELOG.md?raw";

// CHANGELOG.md 顶部带有面向维护者的 HTML 注释，不应出现在应用内渲染中 ——
// 先剥离所有 `<!-- … -->`（跨行安全）并压缩多余空行，再交给 ReactMarkdown。
const cleanChangelog = changelog
  .replace(/<!--[\s\S]*?-->/g, "")
  .replace(/\n{3,}/g, "\n\n")
  .trim();

type Phase = "idle" | "downloading" | "ready" | "failed";

interface Props {
  version: string;
  updateInfo: UpdateInfo | null;
  /** 正在强制检查更新（禁用「检查更新」按钮） */
  checking: boolean;
  onClose: () => void;
  /** 强制重新检查更新（绕过会话节流） */
  onCheckUpdates: () => void;
  /** 在系统浏览器打开发行版 / 下载页 */
  onDownload: (url: string) => void;
}

/**
 * 「关于」弹窗：点击侧边栏底部版本号打开（参考 myshell 的 AboutDialog）。
 *
 * 顶部为品牌与版本；中部是检查更新区 —— 发现新版本时变为更新横幅，
 * 可自动下载安装（Windows），非 Windows 回退为打开下载页；
 * 下方滚动展示内置的 CHANGELOG.md（更新日志）。
 */
export function AboutDialog({
  version,
  updateInfo,
  checking,
  onClose,
  onCheckUpdates,
  onDownload,
}: Props) {
  const hasUpdate = !!updateInfo?.hasUpdate;
  const downloadUrl = updateInfo?.downloadUrl || updateInfo?.releaseUrl || "";
  // latestVersion 来自 release tag，已带 "v"（如 "v1.6.1"），原样展示避免双 v。
  const latestDisplay = updateInfo
    ? updateInfo.latestVersion.startsWith("v")
      ? updateInfo.latestVersion
      : `v${updateInfo.latestVersion}`
    : "";
  const isBrowserMode = updateInfo?.updateStrategy === "browser";

  // 自动下载 + 安装状态（与 UpdateNotification 同构的四阶段）
  const [phase, setPhase] = useState<Phase>("idle");
  const [progress, setProgress] = useState<{ downloaded: number; total: number }>({
    downloaded: 0,
    total: 0,
  });
  const [error, setError] = useState("");
  const [downloadedPath, setDownloadedPath] = useState("");

  // 订阅下载进度事件（仅 downloading 阶段）
  useEffect(() => {
    if (phase !== "downloading") return;
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;
    onUpdateDownloadProgress((p) => {
      setProgress({ downloaded: p.downloaded, total: p.total });
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [phase]);

  const pct =
    progress.total > 0
      ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
      : null;

  const handleUpdate = async () => {
    setPhase("downloading");
    setProgress({ downloaded: 0, total: 0 });
    setError("");
    try {
      const path = await updater.downloadUpdate(downloadUrl);
      setDownloadedPath(path);
      setPhase("ready");
    } catch (e) {
      setError(String(e));
      setPhase("failed");
    }
  };

  const handleInstall = async () => {
    if (!downloadedPath) {
      setError("安装包路径丢失");
      setPhase("failed");
      return;
    }
    try {
      await updater.installUpdate(downloadedPath);
    } catch (e) {
      setError(String(e));
      setPhase("failed");
    }
  };

  return (
    <div className="rd-overlay" onClick={onClose}>
      <div
        className="za-glass-strong"
        onClick={(e) => e.stopPropagation()}
        style={card}
      >
        {/* 头部：品牌 + 版本 */}
        <div style={header}>
          <div className="za-logo-mark" style={{ width: 40, height: 40 }}>
            <svg
              width="22"
              height="22"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M3 17l5-6 4 3 8-9" />
            </svg>
          </div>
          <div style={{ flex: 1 }}>
            <div style={{ fontWeight: 700, letterSpacing: "-0.02em" }}>
              zcode-assistant
            </div>
            <div className="za-faint" style={{ fontSize: "var(--fs-xs)", marginTop: 2 }}>
              {version ? `版本 v${version}` : "版本获取中…"}
            </div>
          </div>
          <button className="za-btn za-btn-ghost za-btn-sm" onClick={onClose}>
            关闭
          </button>
        </div>

        {/* 检查更新区 */}
        <div style={{ padding: "0 20px 14px" }}>
          {hasUpdate ? (
            <div style={banner}>
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <span style={{ color: "var(--accent)", display: "inline-flex" }}>
                  <IconSparkle width={18} height={18} />
                </span>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: "var(--fs-md)", fontWeight: 600 }}>
                    发现新版本 {latestDisplay}
                  </div>
                  <div className="za-faint" style={{ fontSize: "var(--fs-xs)", marginTop: 2 }}>
                    {phase === "idle" &&
                      (isBrowserMode
                        ? "当前系统暂不支持应用内自动更新，请前往下载页手动下载安装"
                        : "可自动下载安装，也可前往网页下载")}
                    {phase === "downloading" &&
                      (pct !== null ? `正在下载… ${pct}%` : "正在下载…")}
                    {phase === "ready" && "下载完成，可安装"}
                    {phase === "failed" && (error || "下载失败")}
                  </div>
                </div>
              </div>
              {/* 进度条 */}
              {phase === "downloading" && (
                <div className="za-progress">
                  <div
                    className="za-progress-fill"
                    style={{ width: pct !== null ? `${pct}%` : "0%" }}
                  />
                </div>
              )}
              {/* 操作按钮 */}
              <div className="za-row" style={{ justifyContent: "flex-end", gap: 8 }}>
                {phase === "idle" &&
                  (isBrowserMode ? (
                    <button
                      className="za-btn za-btn-sm za-btn-primary"
                      onClick={() => downloadUrl && onDownload(downloadUrl)}
                    >
                      <IconExternal width={15} height={15} />
                      打开下载页
                    </button>
                  ) : (
                    <>
                      <button
                        className="za-btn za-btn-sm za-btn-primary"
                        onClick={handleUpdate}
                      >
                        <IconDownload width={15} height={15} />
                        更新
                      </button>
                      <button
                        className="za-btn za-btn-sm"
                        onClick={() => downloadUrl && onDownload(downloadUrl)}
                      >
                        网页下载
                      </button>
                    </>
                  ))}
                {phase === "downloading" && (
                  <span className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
                    请稍候，下载完成后将自动提示…
                  </span>
                )}
                {phase === "ready" && (
                  <button
                    className="za-btn za-btn-sm za-btn-primary"
                    style={{ flex: 1 }}
                    onClick={handleInstall}
                  >
                    安装并重启
                  </button>
                )}
                {phase === "failed" && (
                  <>
                    <button
                      className="za-btn za-btn-sm"
                      onClick={() => downloadUrl && onDownload(downloadUrl)}
                    >
                      浏览器下载
                    </button>
                    <button
                      className="za-btn za-btn-sm za-btn-primary"
                      onClick={handleUpdate}
                    >
                      重试
                    </button>
                  </>
                )}
              </div>
            </div>
          ) : (
            <div className="za-row" style={{ gap: 10 }}>
              <button
                className="za-btn za-btn-sm"
                disabled={checking}
                onClick={onCheckUpdates}
              >
                <IconRefresh width={15} height={15} />
                {checking ? "检查中…" : "检查更新"}
              </button>
              <span className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
                {updateInfo?.error
                  ? "上次检查失败，可重试"
                  : updateInfo && !hasUpdate
                  ? "当前已是最新版本"
                  : ""}
              </span>
            </div>
          )}
        </div>

        {/* 更新日志（滚动区） */}
        <div className="about-changelog" style={changelogBox}>
          <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
            {cleanChangelog}
          </ReactMarkdown>
        </div>
      </div>
    </div>
  );
}

/** ReactMarkdown 组件映射 —— 用项目 token 给标题/列表/链接一个干净的外观 */
const markdownComponents = {
  h1: ({ children }: { children?: ReactNode }) => (
    <h2 style={{ margin: "14px 0 8px", fontSize: 16, color: "var(--text-primary)" }}>
      {children}
    </h2>
  ),
  h2: ({ children }: { children?: ReactNode }) => (
    <h2 style={{ margin: "14px 0 8px", fontSize: 15, color: "var(--text-primary)" }}>
      {children}
    </h2>
  ),
  h3: ({ children }: { children?: ReactNode }) => (
    <h3 style={{ margin: "10px 0 6px", fontSize: 13, color: "var(--text-primary)" }}>
      {children}
    </h3>
  ),
  p: ({ children }: { children?: ReactNode }) => <p style={{ margin: "6px 0" }}>{children}</p>,
  ul: ({ children }: { children?: ReactNode }) => (
    <ul style={{ margin: "6px 0", paddingLeft: 20 }}>{children}</ul>
  ),
  ol: ({ children }: { children?: ReactNode }) => (
    <ol style={{ margin: "6px 0", paddingLeft: 20 }}>{children}</ol>
  ),
  li: ({ children }: { children?: ReactNode }) => <li style={{ margin: "3px 0" }}>{children}</li>,
  a: ({ children, href }: { children?: ReactNode; href?: string }) => (
    <a
      href={href}
      style={{ color: "var(--accent)", textDecoration: "none" }}
      // Tauri webview 内不直接跳转外链，统一交给系统浏览器打开
      onClick={(e) => {
        e.preventDefault();
        if (href) void updater.openReleasePage(href);
      }}
    >
      {children}
    </a>
  ),
  hr: () => (
    <hr
      style={{ border: "none", borderTop: "1px solid var(--glass-border)", margin: "14px 0" }}
    />
  ),
  code: ({ children }: { children?: ReactNode }) => (
    <code
      className="za-mono"
      style={{
        fontSize: 12,
        background: "var(--bg-elevated)",
        padding: "1px 5px",
        borderRadius: 4,
      }}
    >
      {children}
    </code>
  ),
  blockquote: ({ children }: { children?: ReactNode }) => (
    <blockquote
      style={{
        margin: "8px 0",
        padding: "8px 12px",
        borderLeft: "3px solid var(--glass-border-strong)",
        background: "var(--bg-elevated)",
        borderRadius: "var(--radius-sm)",
        color: "var(--text-tertiary)",
        fontSize: 12,
      }}
    >
      {children}
    </blockquote>
  ),
};

/* ===== 布局样式（内联，配合 .za-glass-strong 玻璃卡片） ===== */
const card: React.CSSProperties = {
  width: 520,
  maxWidth: "92vw",
  maxHeight: "86vh",
  display: "flex",
  flexDirection: "column",
  overflow: "hidden",
};

const header: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 12,
  padding: "18px 20px 14px",
};

const banner: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 10,
  padding: "12px 14px",
  background: "var(--accent-subtle)",
  border: "1px solid var(--glass-border-strong)",
  borderRadius: "var(--radius-md)",
};

const changelogBox: React.CSSProperties = {
  flex: 1,
  overflowY: "auto",
  padding: "12px 20px 18px",
  borderTop: "1px solid var(--glass-border)",
  fontSize: 13,
  color: "var(--text-secondary)",
  lineHeight: 1.6,
};
