import { useCallback, useEffect, useRef, useState } from "react";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import Dashboard from "./views/Dashboard";
import Models from "./views/Models";
import AutoSwitch from "./views/AutoSwitch";
import Usage from "./views/Usage";
import Projects from "./views/Projects";
import Accounts from "./views/Accounts";
import Proxy from "./views/Proxy";
import Beautify from "./views/Beautify";
import Settings from "./views/Settings";
import { IconActivity } from "./components/icons";
import { RestartDialog } from "./components/RestartDialog";
import { SecondInstanceDialog } from "./components/SecondInstanceDialog";
import { UpdateNotification } from "./components/UpdateNotification";
import { AboutDialog } from "./components/AboutDialog";
import { StatsConsentDialog } from "./components/StatsConsentDialog";
import { ToastContainer, toast } from "./components/Toast";
import { useUpdateCheck } from "./hooks/useUpdateCheck";
import { events, win, app, updater, quota, health, prefs as prefsApi, pickRingBuckets, usageColor, models } from "./api";
import {
  checkReportNeeded,
  markVersionHandled,
  reportVersion,
  setStatsConsent,
} from "./lib/usageStats";
import type { AppPrefs, QuotaOverview, ViewId } from "./types";
import "./styles/layout.css";

const META: Record<ViewId, { title: string; sub: string }> = {
  dashboard: { title: "总览", sub: "配额 · 当前模型 · 账号状态" },
  models: { title: "模型管理", sub: "获取可用模型 · 配置上下文 · 写入 zcode" },
  autoswitch: { title: "自动切换", sub: "定时切换 · 配额耗尽自动切换" },
  usage: { title: "用量查询", sub: "按供应商 / 模型 / 日期统计 token 用量与速度" },
  projects: { title: "项目管理", sub: "管理项目与会话 · 查看消耗 · 改名 · 批量删除" },
  accounts: { title: "智谱账号", sub: "多账号捕获与切换" },
  proxy: { title: "网络代理", sub: "HTTP / SOCKS5 代理" },
  beautify: { title: "ZCode 美化", sub: "换肤 · 换字体 · 自定义配色（可还原）" },
  settings: { title: "设置", sub: "主题 · 关于" },
};

export default function App() {
  const [view, setView] = useState<ViewId>("dashboard");
  const meta = META[view];

  // 后台更新检查：应用启动后本会话自动检查一次；checkNow 供「关于」弹窗手动触发
  const {
    info: updateInfo,
    loading: updateChecking,
    checkNow,
  } = useUpdateCheck(true);
  const [updateDismissed, setUpdateDismissed] = useState(false);

  // 「关于」弹窗（点击侧边栏底部版本号打开）
  const [aboutOpen, setAboutOpen] = useState(false);
  const [appVersion, setAppVersion] = useState("");
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
        setAppVersion(v);
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

  // ============ 配额轮询（全局唯一查询源）============
  // Dashboard / 悬浮球 / 悬浮面板 / 托盘图标共用：每 5s 查询并广播 quota://updated。
  // 必须挂在常驻的 App 而非 Dashboard 视图——否则切到其他视图后组件卸载、轮询停止，
  // 悬浮窗与托盘全部断更（主窗口关闭只是隐藏，webview 常活，轮询持续）。
  const [quotaData, setQuotaData] = useState<QuotaOverview | null>(null);
  const [quotaLoading, setQuotaLoading] = useState(true);
  const [quotaError, setQuotaError] = useState<string | null>(null);
  // 供常驻监听回调读取的最新配额（避免闭包过期）
  const quotaRef = useRef<QuotaOverview | null>(null);
  // 5s 轮询可能连续失败：相同错误只弹一次，避免刷屏
  const lastQuotaErrRef = useRef<string | null>(null);
  // 上一次查询未返回时跳过本次，避免慢网络下请求堆积
  const quotaInFlightRef = useRef(false);

  const refreshQuota = useCallback(async (silent = false) => {
    if (quotaInFlightRef.current) return;
    quotaInFlightRef.current = true;
    if (!silent) setQuotaLoading(true);
    setQuotaError(null);
    try {
      // 总览配额：主供应商优先（总览 / 悬浮窗 / 托盘共用），未设置则自动识别智谱 Coding Plan
      const q = await quota.getOverview();
      setQuotaData(q);
      quotaRef.current = q;
      lastQuotaErrRef.current = null;
      // 作为唯一数据源，把结果广播给悬浮窗 / 悬浮球 / 托盘
      events.emitQuotaUpdated(q);
    } catch (e: unknown) {
      const m =
        typeof e === "string" ? e : (e as Error)?.message ?? "配额查询失败";
      setQuotaError(m);
      if (lastQuotaErrRef.current !== m) {
        toast.error(`配额查询失败：${m}`);
        lastQuotaErrRef.current = m;
      }
    } finally {
      quotaInFlightRef.current = false;
      setQuotaLoading(false);
    }
  }, []);

  useEffect(() => {
    // 启动引导主供应商：best-effort 自动选（智谱账号 → 第一个用户供应商），
    // 让总览/悬浮窗一启动就有数据源。无任何可选源时返回 null，后续空概览无报错。
    models.bootstrapPrimary().catch(() => {});
    refreshQuota();
    // 每 5 秒自动静默刷新（唯一查询源，结果广播给所有消费方）
    const t = setInterval(() => refreshQuota(true), 5000);
    // 监听刷新请求（托盘菜单 / 账号切换后转发而来）
    let un: (() => void) | undefined;
    events.onRefreshRequested(() => refreshQuota()).then((fn) => (un = fn));
    return () => {
      clearInterval(t);
      un?.();
    };
  }, [refreshQuota]);

  // ============ 当前模型可用性检测（全局唯一触发源）============
  // 每 30s 调 check(false)：命令内部做冷却判断，失败退避期内直接返回缓存、不发网络请求。
  // 检测结果由后端广播 health://updated，各视图订阅展示；手动「立即检测」由视图传 force=true。
  useEffect(() => {
    const check = () => health.check(false).catch(() => {});
    check();
    const t = setInterval(check, 30_000);
    // 切换模型后立即检测一次（后端对 provider 变更也会视同 force 立即探测）
    let un: (() => void) | undefined;
    events.onModelSwitched(() => check()).then((fn) => (un = fn));
    return () => {
      clearInterval(t);
      un?.();
    };
  }, []);

  // ============ 应用偏好（悬浮球显隐 / 用量展示方案）============
  const [prefs, setPrefs] = useState<AppPrefs>({
    floatBallVisible: true,
    usageDisplay: "used",
    switchRestartZcode: true,
    autostart: false,
  });
  const prefsRef = useRef(prefs);
  useEffect(() => {
    prefsRef.current = prefs;
  }, [prefs]);
  useEffect(() => {
    prefsApi.get().then(setPrefs).catch(() => {});
    let un: (() => void) | undefined;
    events.onPrefsUpdated(setPrefs).then((fn) => (un = fn));
    return () => un?.();
  }, []);

  // 监听配额更新 → 重绘任务栏托盘图标（进度环，颜色随用量变化；弧长随展示方案）
  const applyTrayIcon = useCallback(async (q: QuotaOverview | null) => {
    if (!q) return;
    const { b5, bW } = pickRingBuckets(q);
    const u5 = b5 && b5.total > 0 ? (b5.used / b5.total) * 100 : null;
    const uW = bW && bW.total > 0 ? (bW.used / bW.total) * 100 : null;
    const icon = drawTrayIcon(u5, uW, prefsRef.current.usageDisplay === "remaining");
    if (icon.rgba.length > 0) {
      try {
        await win.setTrayIcon(icon.rgba, icon.width, icon.height);
      } catch {
        /* ignore */
      }
    }
  }, []);
  useEffect(() => {
    // 仅监听上方全局轮询广播的配额更新（单一数据源，不重复查询）
    let un: (() => void) | undefined;
    events.onQuotaUpdated(applyTrayIcon).then((fn) => (un = fn));
    return () => un?.();
  }, [applyTrayIcon]);
  // 展示方案变更 → 用最近一次配额立即重绘，不等下一轮轮询
  useEffect(() => {
    void applyTrayIcon(quotaRef.current);
  }, [prefs.usageDisplay, applyTrayIcon]);

  return (
    <div className="za-app">
      <Sidebar
        current={view}
        onSelect={setView}
        updateAvailable={!!updateInfo?.hasUpdate}
        onOpenAbout={() => setAboutOpen(true)}
      />
      <div className="za-main">
        <TopBar
          title={meta.title}
          subtitle={meta.sub}
          actions={
            <button
              className="za-icon-btn"
              title="显示悬浮球"
              onClick={() => prefsApi.setFloatBallVisible(true).catch(() => {})}
            >
              <IconActivity width={18} height={18} />
            </button>
          }
        />
        <main className="za-content" key={view}>
          <div className="za-content-inner za-fade-in">
            {view === "dashboard" && (
              <Dashboard
                data={quotaData}
                loading={quotaLoading}
                error={quotaError}
                onRefresh={() => refreshQuota()}
                usageDisplay={prefs.usageDisplay}
              />
            )}
            {view === "models" && <Models usageDisplay={prefs.usageDisplay} />}
            {view === "autoswitch" && <AutoSwitch />}
            {view === "usage" && <Usage />}
            {view === "projects" && <Projects />}
            {view === "accounts" && <Accounts />}
            {view === "proxy" && <Proxy />}
            {view === "beautify" && <Beautify />}
            {view === "settings" && <Settings />}
          </div>
        </main>
      </div>
      <RestartDialog />
      <SecondInstanceDialog />
      <ToastContainer />
      {updateInfo?.hasUpdate && !ignoredVersion && !updateDismissed && (
        <UpdateNotification
          updateInfo={updateInfo}
          onIgnored={() => setUpdateDismissed(true)}
        />
      )}
      {aboutOpen && (
        <AboutDialog
          version={appVersion}
          updateInfo={updateInfo}
          checking={updateChecking}
          onClose={() => setAboutOpen(false)}
          onCheckUpdates={checkNow}
          onDownload={(url) => void updater.openReleasePage(url)}
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
 *  弧长随展示方案（已用 / 剩余），颜色始终按已用占比分级（绿/黄/红），返回 RGBA 像素 */
function drawTrayIcon(
  used5h: number | null,
  usedWeekly: number | null,
  showRemaining: boolean
): { rgba: number[]; width: number; height: number } {
  const size = 32;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d");
  if (!ctx) return { rgba: [], width: size, height: size };
  ctx.clearRect(0, 0, size, size);
  drawTrayRing(ctx, 16, 16, 12, 3, used5h, showRemaining); // 外圈 = 每5小时
  drawTrayRing(ctx, 16, 16, 6.5, 2.5, usedWeekly, showRemaining); // 内圈 = 每周
  return {
    rgba: Array.from(ctx.getImageData(0, 0, size, size).data),
    width: size,
    height: size,
  };
}

/** 画一道配额环：背景底环 + 数值弧（showRemaining 时弧长=剩余占比；颜色始终按已用占比分级） */
function drawTrayRing(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  r: number,
  lw: number,
  usedPct: number | null,
  showRemaining: boolean
) {
  ctx.beginPath();
  ctx.arc(cx, cy, r, 0, Math.PI * 2);
  ctx.strokeStyle = "rgba(255,255,255,0.15)";
  ctx.lineWidth = lw;
  ctx.stroke();
  if (usedPct == null) return;
  const used = Math.max(0, Math.min(100, usedPct));
  const frac = showRemaining ? (100 - used) / 100 : used / 100;
  ctx.beginPath();
  ctx.arc(cx, cy, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * frac);
  ctx.strokeStyle = usageColor(usedPct);
  ctx.lineWidth = lw;
  ctx.lineCap = "round";
  ctx.stroke();
}
