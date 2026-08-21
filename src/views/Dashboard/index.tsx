import type { ReactNode } from "react";
import { Progress } from "../../components/Progress";
import {
  IconRefresh,
  IconZap,
  IconCpu,
  IconUser,
} from "../../components/icons";
import { formatUnits } from "../../api";
import type { QuotaOverview, UsageDisplayMode } from "../../types";

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

/**
 * 总览视图：配额数据由 App 的全局轮询（唯一查询源）下发，本组件纯展示。
 * 刷新按钮触发 App 的查询（non-silent，会广播给悬浮窗 / 托盘）。
 */
export default function Dashboard({
  data,
  loading,
  error,
  onRefresh,
  usageDisplay = "used",
}: {
  data: QuotaOverview | null;
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  /** 模型用量展示方案（与悬浮球/托盘同源）：已用 / 剩余 */
  usageDisplay?: UsageDisplayMode;
}) {
  // 展示方案：数字与进度条宽度随方案（已用 / 剩余），颜色始终按已用度分级
  const showRemaining = usageDisplay === "remaining";
  return (
    <>
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
          label="供应商"
          value={
            data?.providerName ?? (loading ? "加载中…" : "自动 · 智谱 Coding Plan")
          }
          hint={data ? `数据源 ${data.source}` : "未设置主供应商时自动识别"}
        />
      </div>

      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>主供应商配额</h3>
          <button
            className="za-btn za-btn-sm"
            onClick={() => onRefresh()}
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
              const shown = showRemaining ? 100 - pct : pct;
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
                  <Progress value={shown} colorValue={pct} />
                  <div
                    className="za-row-between"
                    style={{
                      fontSize: "var(--fs-xs)",
                      marginTop: "var(--space-1)",
                    }}
                  >
                    <span className="za-faint">
                      {showRemaining ? "剩余" : "已用"} {Math.round(shown)}%
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
            <div className="za-empty">
              {data.source === "none"
                ? "未配置主供应商且未登录智谱 Coding Plan——设置主供应商或登录智谱账号后可查看用量"
                : "暂无配额数据"}
            </div>
          )}
          {error && (
            <div className="za-empty" style={{ color: "var(--danger)" }}>
              配额查询失败：{error}
            </div>
          )}
          {!data && !error && (
            <div className="za-empty">{loading ? "加载中…" : "暂无数据"}</div>
          )}
        </div>
      </div>
    </>
  );
}
