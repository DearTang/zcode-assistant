import { useEffect, useState } from "react";
import type { CSSProperties } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { IconDownload, IconSparkle, IconExternal } from "./icons";
import {
  onUpdateDownloadProgress,
  updater,
} from "../api";
import type { UpdateInfo } from "../types";

interface Props {
  updateInfo: UpdateInfo;
  /** 用户点击「忽略此版本」后回调（App 据此隐藏本弹窗） */
  onIgnored?: () => void;
}

type Phase = "prompt" | "downloading" | "ready" | "failed";

const IGNORE_KEY = "za.ignoredUpdateVersion";

/**
 * 居中模态更新弹窗（参考 myshell，4 阶段：prompt/downloading/ready/failed）。
 *
 * 启动后后台检查发现新版本时显示一次：
 *   - 「更新」→ 自动下载安装器 → 进度条 → 「安装并重启」
 *   - 「忽略」→ 把该版本记到 localStorage，同版本不再提示；更高版本会再次触发。
 *
 * 非 Windows（update_strategy==="browser"）跳过下载/安装阶段，只给一个「打开下载页」
 * 按钮，由系统浏览器手动下载安装。
 */
export function UpdateNotification({ updateInfo, onIgnored }: Props) {
  const latest = updateInfo.latestVersion;
  const downloadUrl = updateInfo.downloadUrl || updateInfo.releaseUrl || "";
  // latestVersion 来自 release tag，已带 "v"（如 "v1.6.1"），原样展示避免双 v。
  const latestDisplay = latest.startsWith("v") ? latest : `v${latest}`;
  const isBrowserMode = updateInfo.updateStrategy === "browser";

  const [phase, setPhase] = useState<Phase>("prompt");
  const [progress, setProgress] = useState<{ downloaded: number; total: number }>(
    { downloaded: 0, total: 0 }
  );
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

  const handleIgnore = () => {
    try {
      localStorage.setItem(IGNORE_KEY, latest);
    } catch {
      /* best-effort */
    }
    onIgnored?.();
  };

  const pct =
    progress.total > 0
      ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
      : null;

  return (
    <div className="rd-overlay">
      <div className="za-glass-strong" style={card}>
        {/* 标题 */}
        <div style={header}>
          <div style={iconCircle}>
            <IconSparkle width={22} height={22} />
          </div>
          <strong style={{ fontSize: "var(--fs-md)" }}>
            zcode-assistant 有新版本可用
          </strong>
          <span className="za-badge" style={versionBadge}>
            {latestDisplay}
          </span>
        </div>

        {/* 副标题 / 状态 */}
        <div className="za-muted" style={subtitle}>
          {phase === "prompt" &&
            (isBrowserMode
              ? "检测到新版本。当前系统暂不支持应用内自动更新，请前往下载页手动下载安装。"
              : "新版本已就绪，是否立即更新？")}
          {phase === "downloading" &&
            (pct !== null ? `正在下载… ${pct}%` : "正在下载…")}
          {phase === "ready" && "下载完成，点击安装并重启应用"}
          {phase === "failed" && (error || "下载出现问题")}
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

        {/* 操作区 */}
        <div className="za-row" style={{ justifyContent: "flex-end", gap: 8 }}>
          {phase === "prompt" && (
            <>
              <button className="za-btn za-btn-sm" onClick={handleIgnore}>
                忽略
              </button>
              {isBrowserMode ? (
                <button
                  className="za-btn za-btn-sm za-btn-primary"
                  onClick={() => downloadUrl && updater.openReleasePage(downloadUrl)}
                >
                  <IconExternal width={15} height={15} />
                  打开下载页
                </button>
              ) : (
                <button
                  className="za-btn za-btn-sm za-btn-primary"
                  onClick={handleUpdate}
                >
                  <IconDownload width={15} height={15} />
                  更新
                </button>
              )}
            </>
          )}
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
                onClick={() => downloadUrl && updater.openReleasePage(downloadUrl)}
              >
                浏览器下载
              </button>
              <button className="za-btn za-btn-sm za-btn-primary" onClick={handleUpdate}>
                重试
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

const card: CSSProperties = {
  width: 380,
  maxWidth: "90vw",
  padding: 20,
  display: "flex",
  flexDirection: "column",
  gap: 12,
};

const header: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  gap: 8,
};

const iconCircle: CSSProperties = {
  width: 44,
  height: 44,
  borderRadius: "var(--radius-full)",
  background: "var(--accent-subtle)",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  color: "var(--accent)",
};

const versionBadge: CSSProperties = {
  background: "var(--accent-subtle)",
  color: "var(--accent)",
  letterSpacing: "0.02em",
};

const subtitle: CSSProperties = {
  textAlign: "center",
  fontSize: "var(--fs-sm)",
  lineHeight: 1.5,
};
