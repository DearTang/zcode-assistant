import { useEffect, useState, type ReactNode } from "react";
import { Progress } from "../../components/Progress";
import {
  IconRefresh,
  IconZap,
  IconCpu,
  IconUser,
} from "../../components/icons";
import { quota, events, formatUnits } from "../../api";
import type { QuotaOverview } from "../../types";

function StatCard({
  icon,
  label,
  value,
  hint,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  hint?: string;
}) {
  return (
    <div className="za-panel za-card-pad">
      <div
        className="za-row"
        style={{
          color: "var(--text-secondary)",
          marginBottom: "var(--space-3)",
        }}
      >
        {icon}
        <span className="za-muted" style={{ fontSize: "var(--fs-sm)" }}>
          {label}
        </span>
      </div>
      <div
        style={{
          fontSize: "var(--fs-xl)",
          fontWeight: 600,
          letterSpacing: "-0.01em",
        }}
      >
        {value}
      </div>
      {hint && (
        <div
          className="za-faint"
          style={{ fontSize: "var(--fs-xs)", marginTop: "var(--space-1)" }}
        >
          {hint}
        </div>
      )}
    </div>
  );
}

export default function Dashboard() {
  const [data, setData] = useState<QuotaOverview | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = async (silent = false) => {
    if (!silent) setLoading(true);
    setError(null);
    try {
      const q = await quota.getCodingPlan();
      setData(q);
      // 作为唯一数据源，把结果广播给悬浮窗 / 悬浮球 / 托盘
      events.emitQuotaUpdated(q);
    } catch (e: unknown) {
      setError(
        typeof e === "string" ? e : (e as Error)?.message ?? "配额查询失败"
      );
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => {
    refresh();
    // 每 5 秒自动静默刷新（唯一查询源，结果广播给所有消费方）
    const t = setInterval(() => refresh(true), 5000);
    // 监听刷新请求（托盘菜单 / 悬浮窗刷新按钮转发而来）
    let un: (() => void) | undefined;
    events.onRefreshRequested(() => refresh()).then((fn) => (un = fn));
    return () => {
      clearInterval(t);
      un?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <>
      {error && (
        <div
          className="za-panel za-card-pad"
          style={{ borderColor: "var(--danger)" }}
        >
          <div className="za-row" style={{ gap: 8, marginBottom: 6 }}>
            <IconZap width={16} height={16} style={{ color: "var(--danger)" }} />
            <strong>配额查询失败</strong>
          </div>
          <div className="za-muted za-mono" style={{ fontSize: "var(--fs-xs)" }}>
            {error}
          </div>
          <div
            className="za-faint"
            style={{ fontSize: "var(--fs-xs)", marginTop: 6 }}
          >
            提示：配额查询使用 config.json 中「BigModel - Coding Plan」的
            apiKey（非 zcodejwttoken）。若失败可能是未启用该 provider、apiKey
            过期或网络/代理问题。
          </div>
        </div>
      )}

      <div className="za-grid za-grid-3">
        <StatCard
          icon={<IconUser width={16} height={16} />}
          label="当前账号"
          value={data?.accountLabel ?? (loading ? "加载中…" : "-")}
        />
        <StatCard
          icon={<IconCpu width={16} height={16} />}
          label="套餐"
          value={data?.planName ?? "-"}
          hint={
            data
              ? `更新于 ${new Date(data.fetchedAt).toLocaleTimeString()}`
              : undefined
          }
        />
        <StatCard
          icon={<IconZap width={16} height={16} />}
          label="数据源"
          value={data?.source ?? "-"}
          hint="BigModel 使用统计"
        />
      </div>

      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>Coding Plan 配额</h3>
          <button
            className="za-btn za-btn-sm"
            onClick={() => refresh()}
            disabled={loading}
          >
            <IconRefresh width={14} height={14} />{" "}
            {loading ? "刷新中" : "刷新"}
          </button>
        </div>

        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: "var(--space-5)",
          }}
        >
          {data &&
            data.buckets.map((b) => {
              const pct = b.total > 0 ? (b.used / b.total) * 100 : 0;
              return (
                <div key={b.name}>
                  <div
                    className="za-row-between"
                    style={{ marginBottom: "var(--space-2)" }}
                  >
                    <span style={{ fontWeight: 500 }}>{b.name}</span>
                    <span
                      className="za-muted za-mono"
                      style={{ fontSize: "var(--fs-sm)" }}
                    >
                      {formatUnits(b.used, b.unit)} /{" "}
                      {formatUnits(b.total, b.unit)}
                      <span
                        style={{
                          marginLeft: "var(--space-2)",
                          color: "var(--accent)",
                        }}
                      >
                        剩 {formatUnits(b.remaining, b.unit)}
                      </span>
                    </span>
                  </div>
                  <Progress value={pct} />
                  <div
                    className="za-row-between"
                    style={{
                      fontSize: "var(--fs-xs)",
                      marginTop: "var(--space-1)",
                    }}
                  >
                    <span className="za-faint">
                      已用 {pct.toFixed(1)}%
                    </span>
                    {b.periodEnd && (
                      <span className="za-faint">
                        重置时间 {new Date(b.periodEnd).toLocaleString()}
                      </span>
                    )}
                  </div>
                </div>
              );
            })}
          {data && data.buckets.length === 0 && (
            <div className="za-empty">暂无配额数据</div>
          )}
          {!data && !error && (
            <div className="za-empty">{loading ? "加载中…" : "暂无数据"}</div>
          )}
        </div>
      </div>
    </>
  );
}
