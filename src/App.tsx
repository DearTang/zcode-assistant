import { useEffect, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import Dashboard from "./views/Dashboard";
import Models from "./views/Models";
import AutoSwitch from "./views/AutoSwitch";
import Usage from "./views/Usage";
import Accounts from "./views/Accounts";
import Proxy from "./views/Proxy";
import Settings from "./views/Settings";
import { IconActivity } from "./components/icons";
import { RestartDialog } from "./components/RestartDialog";
import { UpdateNotification } from "./components/UpdateNotification";
import { StatsConsentDialog } from "./components/StatsConsentDialog";
import { useUpdateCheck } from "./hooks/useUpdateCheck";
import { events, win, app, pickBucketByName, usageColor } from "./api";
import {
  checkReportNeeded,
  markVersionHandled,
  reportVersion,
  setStatsConsent,
} from "./lib/usageStats";
import type { QuotaOverview, ViewId } from "./types";
import "./styles/layout.css";

const META: Record<ViewId, { title: string; sub: string }> = {
  dashboard: { title: "总览", sub: "配额 · 当前模型 · 账号状态" },
  models: { title: "模型管理", sub: "获取可用模型 · 配置上下文 · 写入 zcode" },
  autoswitch: { title: "自动切换", sub: "定时切换 · 配额耗尽自动切换" },
  usage: { title: "用量查询", sub: "按供应商 / 模型 / 日期统计 token 用量与速度" },
  accounts: { title: "智谱账号", sub: "多账号捕获与切换" },
  proxy: { title: "网络代理", sub: "HTTP / SOCKS5 代理" },
  settings: { title: "设置", sub: "主题 · 关于" },
};

export default function App() {
  const [view, setView] = useState<ViewId>("dashboard");
  const meta = META[view];

  // 后台更新检查：应用启动后本会话自动检查一次
  const { info: updateInfo } = useUpdateCheck(true);
  const [updateDismissed, setUpdateDismissed] = useState(false);
  // 用户「忽略」的版本（localStorage），同版本不再弹
  const ignoredVersion =
    updateInfo?.latestVersion &&
    localStorage.getItem("za.ignoredUpdateVersion") === updateInfo.latestVersion;

  // 统计同意弹窗（每个新版本首次启动、用户未同意时显示）
  const [statsPrompt, setStatsPrompt] = useState<{ version: string } | null>(
    null
  );

  // 应用启动后：取版本号 → 决定是否上报统计 / 弹同意框
  useEffect(() => {
    let cancelled = false;
    app.getVersion()
      .then((v) => {
        if (cancelled) return;
        const { shouldReport, hasConsent } = checkReportNeeded(v);
        if (shouldReport) {
          if (hasConsent) {
            void reportVersion(v, navigator.platform);
          } else {
            setStatsPrompt({ version: v });
          }
        }
      })
      .catch(() => {
        /* 取版本号失败静默，不影响统计/更新 */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // 监听配额更新 → 重绘任务栏托盘图标（进度环，颜色随用量变化）
  useEffect(() => {
    const applyQuota = async (q: QuotaOverview) => {
      const b5 = pickBucketByName(q, "5小时");
      const bW = pickBucketByName(q, "每周");
      const u5 = b5 && b5.total > 0 ? (b5.used / b5.total) * 100 : null;
      const uW = bW && bW.total > 0 ? (bW.used / bW.total) * 100 : null;
      const icon = drawTrayIcon(u5, uW);
      if (icon.rgba.length > 0) {
        try {
          await win.setTrayIcon(icon.rgba, icon.width, icon.height);
        } catch {
          /* ignore */
        }
      }
    };
    // 仅监听 Dashboard 广播的配额更新（单一数据源）
    let un: (() => void) | undefined;
    events.onQuotaUpdated(applyQuota).then((fn) => (un = fn));
    return () => un?.();
  }, []);

  return (
    <div className="za-app">
      <Sidebar current={view} onSelect={setView} />
      <div className="za-main">
        <TopBar
          title={meta.title}
          subtitle={meta.sub}
          actions={
            <button
              className="za-icon-btn"
              title="显示悬浮球"
              onClick={() => win.showFloatBall()}
            >
              <IconActivity width={18} height={18} />
            </button>
          }
        />
        <main className="za-content" key={view}>
          <div className="za-content-inner za-fade-in">
            {view === "dashboard" && <Dashboard />}
            {view === "models" && <Models />}
            {view === "autoswitch" && <AutoSwitch />}
            {view === "usage" && <Usage />}
            {view === "accounts" && <Accounts />}
            {view === "proxy" && <Proxy />}
            {view === "settings" && <Settings />}
          </div>
        </main>
      </div>
      <RestartDialog />
      {updateInfo?.hasUpdate && !ignoredVersion && !updateDismissed && (
        <UpdateNotification
          updateInfo={updateInfo}
          onIgnored={() => setUpdateDismissed(true)}
        />
      )}
      {statsPrompt && (
        <StatsConsentDialog
          version={statsPrompt.version}
          onAgree={() => {
            setStatsConsent(true);
            // 注意：此处不调 markVersionHandled。reportVersion 只在发送成功后才盖戳，
            // 这样发送失败时下次启动会静默重试（不再弹窗）。
            void reportVersion(statsPrompt.version, navigator.platform);
            setStatsPrompt(null);
          }}
          onDecline={() => {
            setStatsConsent(false);
            markVersionHandled(statsPrompt.version);
            setStatsPrompt(null);
          }}
        />
      )}
    </div>
  );
}

/** 用 canvas 绘制 32x32 双环托盘图标：外圈=每5小时、内圈=每周；
 *  弧长=剩余可用，颜色按已用占比分级（绿/黄/红），返回 RGBA 像素 */
function drawTrayIcon(
  used5h: number | null,
  usedWeekly: number | null
): { rgba: number[]; width: number; height: number } {
  const size = 32;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d");
  if (!ctx) return { rgba: [], width: size, height: size };
  ctx.clearRect(0, 0, size, size);
  drawTrayRing(ctx, 16, 16, 12, 3, used5h); // 外圈 = 每5小时
  drawTrayRing(ctx, 16, 16, 6.5, 2.5, usedWeekly); // 内圈 = 每周
  return {
    rgba: Array.from(ctx.getImageData(0, 0, size, size).data),
    width: size,
    height: size,
  };
}

/** 画一道配额环：背景底环 + 剩余可用弧（颜色按已用占比分级） */
function drawTrayRing(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  r: number,
  lw: number,
  usedPct: number | null
) {
  ctx.beginPath();
  ctx.arc(cx, cy, r, 0, Math.PI * 2);
  ctx.strokeStyle = "rgba(255,255,255,0.15)";
  ctx.lineWidth = lw;
  ctx.stroke();
  if (usedPct == null) return;
  const used = Math.max(0, Math.min(100, usedPct));
  ctx.beginPath();
  ctx.arc(cx, cy, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * (used / 100));
  ctx.strokeStyle = usageColor(usedPct);
  ctx.lineWidth = lw;
  ctx.lineCap = "round";
  ctx.stroke();
}
