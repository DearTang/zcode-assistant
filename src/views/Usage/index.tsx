/**
 * 用量查询 —— 解析 zcode 的模型调用记录，按供应商 / 模型 / 日期聚合 token 用量，
 * 并统计输出速度（最快 / 平均 / 最慢）。
 *
 * 数据源：~/.zcode/cli/rollout/model-io-sess_*.jsonl（只读，由后端 usage_sync 解析）。
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  usage,
  formatUnits,
  usageColor,
  type UsageQuery,
} from "../../api";
import type {
  UsageAggRow,
  UsageFilters,
  UsageGroupBy,
  UsageOverview,
  UsageRecord,
  UsageSyncResult,
} from "../../types";
import { IconRefresh } from "../../components/icons";

type RangePreset = "today" | "7d" | "30d" | "all" | "custom";
type SortKey =
  | "label"
  | "calls"
  | "input"
  | "output"
  | "cache"
  | "total"
  | "avgTps"
  | "maxTps"
  | "minTps"
  | "avgDuration";

const PRESETS: { id: RangePreset; label: string }[] = [
  { id: "today", label: "今天" },
  { id: "7d", label: "近 7 天" },
  { id: "30d", label: "近 30 天" },
  { id: "all", label: "全部" },
  { id: "custom", label: "自定义" },
];

const GROUPS: { id: UsageGroupBy; label: string }[] = [
  { id: "provider", label: "按供应商" },
  { id: "model", label: "按模型" },
  { id: "date", label: "按日期" },
];

/** YYYY-MM-DD（本地） */
function todayStr(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate()
  ).padStart(2, "0")}`;
}
/** 相对今天前 n 天的 YYYY-MM-DD */
function daysAgoStr(n: number): string {
  const d = new Date();
  d.setDate(d.getDate() - n);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate()
  ).padStart(2, "0")}`;
}

/** 去掉 builtin: 前缀，便于展示 */
function prettyKey(k: string): string {
  if (!k) return k;
  if (k.startsWith("builtin:")) return k.slice("builtin:".length);
  return k;
}

/** 速度格式化：tok/s，保留 1 位小数 */
function fmtTps(n?: number | null): string {
  if (n == null || !Number.isFinite(n)) return "—";
  return `${n.toFixed(1)}`;
}

/** 毫秒 → 友好耗时 */
function fmtMs(n?: number | null): string {
  if (n == null || !Number.isFinite(n)) return "—";
  if (n >= 1000) return `${(n / 1000).toFixed(1)}s`;
  return `${Math.round(n)}ms`;
}

/** ISO(UTC) 时间 → 本地时区简短显示 MM-DD HH:mm */
function fmtTime(s?: string): string {
  if (!s) return "—";
  const d = new Date(s);
  if (Number.isNaN(d.getTime())) {
    // 解析失败：退回原始字符串（去 T/Z）
    return s.replace("T", " ").replace("Z", "").slice(5, 16);
  }
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  const hh = String(d.getHours()).padStart(2, "0");
  const mi = String(d.getMinutes()).padStart(2, "0");
  return `${mm}-${dd} ${hh}:${mi}`;
}

export default function Usage() {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [syncInfo, setSyncInfo] = useState<UsageSyncResult | null>(null);

  const [filters, setFilters] = useState<UsageFilters | null>(null);
  const [overview, setOverview] = useState<UsageOverview | null>(null);
  const [rows, setRows] = useState<UsageAggRow[]>([]);

  const [preset, setPreset] = useState<RangePreset>("30d");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [selProvider, setSelProvider] = useState("");
  const [selModel, setSelModel] = useState("");
  const [selRole, setSelRole] = useState("");

  const [groupBy, setGroupBy] = useState<UsageGroupBy>("provider");
  const [sortKey, setSortKey] = useState<SortKey>("total");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("desc");

  // 明细
  const [detailOpen, setDetailOpen] = useState(false);
  const [detailRows, setDetailRows] = useState<UsageRecord[]>([]);
  const [detailLoading, setDetailLoading] = useState(false);

  // 预设 → 日期范围
  useEffect(() => {
    if (preset === "today") {
      setFrom(todayStr());
      setTo(todayStr());
    } else if (preset === "7d") {
      setFrom(daysAgoStr(6));
      setTo(todayStr());
    } else if (preset === "30d") {
      setFrom(daysAgoStr(29));
      setTo(todayStr());
    } else if (preset === "all") {
      setFrom("");
      setTo("");
    }
    // custom：沿用当前 from/to
  }, [preset]);

  const query: UsageQuery = useMemo(
    () => ({
      from: from || undefined,
      to: to || undefined,
      provider: selProvider || undefined,
      model: selModel || undefined,
      role: selRole || undefined,
    }),
    [from, to, selProvider, selModel, selRole]
  );

  /** 加载汇总 + 聚合（不含 filters） */
  const loadAgg = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [ov, ag] = await Promise.all([
        usage.overview(query),
        usage.aggregate(groupBy, query),
      ]);
      setOverview(ov);
      setRows(ag);
    } catch (e: any) {
      setError(e?.message ?? String(e));
    } finally {
      setLoading(false);
    }
  }, [query, groupBy]);

  /** 加载筛选项（去重的供应商 / 模型 / 角色 + 范围） */
  const loadFilters = useCallback(async () => {
    try {
      setFilters(await usage.filters());
    } catch {
      /* ignore */
    }
  }, []);

  /** 同步：默认增量解析最近 30 天；full=true 回填全部历史 */
  const doSync = useCallback(async (full = false) => {
    setSyncing(true);
    setError(null);
    try {
      const res = await usage.sync(full);
      setSyncInfo(res);
      await Promise.all([loadFilters(), loadAgg()]);
    } catch (e: any) {
      setError(e?.message ?? String(e));
    } finally {
      setSyncing(false);
    }
  }, [loadFilters, loadAgg]);

  // 挂载：先同步一次（拿到最新数据），再全量加载
  useEffect(() => {
    doSync();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 筛选条件或维度变化 → 重新查 overview + aggregate（不再重复 sync）
  // 首次运行由上面挂载 effect 中的 doSync 负责，这里跳过，避免重复请求
  const firstRun = useRef(true);
  useEffect(() => {
    if (firstRun.current) {
      firstRun.current = false;
      return;
    }
    loadAgg();
  }, [loadAgg]);

  // 明细展开时拉取
  useEffect(() => {
    if (!detailOpen) return;
    setDetailLoading(true);
    usage
      .records(query, 200, 0)
      .then(setDetailRows)
      .catch(() => setDetailRows([]))
      .finally(() => setDetailLoading(false));
  }, [detailOpen, query]);

  // 客户端排序
  const sortedRows = useMemo(() => {
    const dir = sortDir === "asc" ? 1 : -1;
    const pick = (r: UsageAggRow): number | string => {
      switch (sortKey) {
        case "label":
          return r.label;
        case "calls":
          return r.calls;
        case "input":
          return r.inputTokens;
        case "output":
          return r.outputTokens;
        case "cache":
          return r.cacheReadTokens;
        case "total":
          return r.totalTokens;
        case "avgTps":
          return r.avgTps ?? -1;
        case "maxTps":
          return r.maxTps ?? -1;
        case "minTps":
          return r.minTps ?? -1;
        case "avgDuration":
          return r.avgDurationMs ?? -1;
      }
    };
    return [...rows].sort((a, b) => {
      const pa = pick(a);
      const pb = pick(b);
      if (typeof pa === "string" || typeof pb === "string") {
        return String(pa).localeCompare(String(pb)) * dir;
      }
      return (pa - pb) * dir;
    });
  }, [rows, sortKey, sortDir]);

  const sumTotal = useMemo(
    () => rows.reduce((s, r) => s + r.totalTokens, 0),
    [rows]
  );
  const maxTotal = useMemo(
    () => rows.reduce((m, r) => Math.max(m, r.totalTokens), 0),
    [rows]
  );

  const onSort = (k: SortKey) => {
    if (k === sortKey) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(k);
      setSortDir(k === "label" ? "asc" : "desc");
    }
  };

  const sortArrow = (k: SortKey) =>
    sortKey === k ? (sortDir === "asc" ? " ▲" : " ▼") : "";

  return (
    <>
      {/* 筛选条 */}
      <div className="za-panel za-card-pad">
        <div className="za-row" style={{ flexWrap: "wrap", gap: "var(--space-2)" }}>
          {PRESETS.map((p) => (
            <button
              key={p.id}
              className="za-btn za-btn-sm"
              data-active={preset === p.id}
              onClick={() => setPreset(p.id)}
              style={
                preset === p.id
                  ? {
                      borderColor: "var(--accent)",
                      color: "var(--accent)",
                      background: "var(--accent-subtle)",
                    }
                  : undefined
              }
            >
              {p.label}
            </button>
          ))}
          {preset === "custom" && (
            <>
              <input
                type="date"
                className="za-input za-usage-date"
                value={from}
                onChange={(e) => setFrom(e.target.value)}
              />
              <span className="za-faint">至</span>
              <input
                type="date"
                className="za-input za-usage-date"
                value={to}
                onChange={(e) => setTo(e.target.value)}
              />
            </>
          )}
          <div style={{ flex: 1 }} />
          <button
            className="za-btn za-btn-sm za-btn-ghost"
            onClick={() => doSync(true)}
            disabled={syncing}
            title="清空本地缓存并从 zcode 用量库全量重新导入"
          >
            重新同步
          </button>
          <button
            className="za-btn za-btn-sm za-btn-primary"
            onClick={() => doSync(false)}
            disabled={syncing}
          >
            <IconRefresh width={14} height={14} />
            {syncing ? "同步中…" : "同步"}
          </button>
        </div>

        <div
          className="za-row"
          style={{ flexWrap: "wrap", gap: "var(--space-3)", marginTop: "var(--space-3)" }}
        >
          <label className="za-usage-filter">
            <span className="za-faint">供应商</span>
            <select
              className="za-select"
              value={selProvider}
              onChange={(e) => setSelProvider(e.target.value)}
            >
              <option value="">全部</option>
              {filters?.providers.map((p) => (
                <option key={p} value={p}>
                  {prettyKey(p)}
                </option>
              ))}
            </select>
          </label>
          <label className="za-usage-filter">
            <span className="za-faint">模型</span>
            <select
              className="za-select"
              value={selModel}
              onChange={(e) => setSelModel(e.target.value)}
            >
              <option value="">全部</option>
              {filters?.models.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          </label>
          <label className="za-usage-filter">
            <span className="za-faint">角色</span>
            <select
              className="za-select"
              value={selRole}
              onChange={(e) => setSelRole(e.target.value)}
            >
              <option value="">全部</option>
              <option value="main">主模型</option>
              <option value="lite">轻量</option>
              <option value="subagent">子代理</option>
            </select>
          </label>
        </div>

        {syncInfo && (
          <div
            className="za-faint za-mono"
            style={{ marginTop: "var(--space-3)", fontSize: "var(--fs-xs)" }}
          >
            共 {syncInfo.totalCount.toLocaleString()} 条记录
            {syncInfo.scannedFiles > 0 &&
              ` · 本次拉取 ${syncInfo.scannedFiles} 条 · 新增 ${syncInfo.newCount}`}
            {syncInfo.minDate && syncInfo.maxDate
              ? ` · 数据范围 ${syncInfo.minDate} ~ ${syncInfo.maxDate}`
              : ""}
          </div>
        )}
      </div>

      {error && (
        <div className="za-panel za-card-pad" style={{ color: "var(--danger)" }}>
          加载失败：{error}
        </div>
      )}

      {/* 汇总卡片 */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>用量汇总</h3>
          <span className="za-faint za-mono" style={{ fontSize: "var(--fs-xs)" }}>
            {loading ? "更新中…" : "随筛选条件实时统计"}
          </span>
        </div>
        <div className="za-grid za-grid-3">
          <Stat label="总调用" value={overview ? overview.calls.toLocaleString() : "—"} />
          <Stat
            label="输入 tokens"
            value={fmtTokens(overview?.inputTokens)}
            color="var(--text-primary)"
          />
          <Stat
            label="输出 tokens"
            value={fmtTokens(overview?.outputTokens)}
            color="var(--text-primary)"
          />
          <Stat
            label="缓存命中"
            value={fmtTokens(overview?.cacheReadTokens)}
            color="var(--text-primary)"
          />
          <Stat
            label="总量 tokens"
            value={fmtTokens(overview?.totalTokens)}
            color="var(--accent)"
            strong
          />
          <Stat
            label="平均耗时"
            value={fmtMs(overview?.avgDurationMs)}
          />
        </div>
      </div>

      {/* 速度卡片 */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>输出速度</h3>
          <span
            className="za-faint"
            title="输出 tokens ÷ 生成耗时（总耗时 − 首 token 等待 TTFB），即真实吐字速度；TTFB 缺失时退化为总耗时。"
            style={{ fontSize: "var(--fs-xs)", cursor: "help" }}
          >
            token/s · 口径说明 ⓘ
          </span>
        </div>
        <div className="za-grid za-grid-3">
          <SpeedStat label="最快" tps={overview?.maxTps} tone="good" />
          <SpeedStat label="平均" tps={overview?.avgTps} tone="normal" />
          <SpeedStat label="最慢" tps={overview?.minTps} tone="warn" />
        </div>
      </div>

      {/* 分组聚合表 */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>分组统计</h3>
          <div className="za-row">
            {GROUPS.map((g) => (
              <button
                key={g.id}
                className="za-btn za-btn-sm"
                data-active={groupBy === g.id}
                onClick={() => setGroupBy(g.id)}
                style={
                  groupBy === g.id
                    ? {
                        borderColor: "var(--accent)",
                        color: "var(--accent)",
                        background: "var(--accent-subtle)",
                      }
                    : undefined
                }
              >
                {g.label}
              </button>
            ))}
          </div>
        </div>

        {loading ? (
          <div className="za-empty">统计中…</div>
        ) : sortedRows.length === 0 ? (
          <div className="za-empty">所选范围内暂无用量数据</div>
        ) : (
          <div className="za-usage-scroll">
            <table className="za-usage-table">
              <thead>
                <tr>
                  <th className="za-ut-l" onClick={() => onSort("label")}>
                    {dimLabel(groupBy)}
                    {sortArrow("label")}
                  </th>
                  <th onClick={() => onSort("calls")}>
                    调用{sortArrow("calls")}
                  </th>
                  <th onClick={() => onSort("input")}>输入{sortArrow("input")}</th>
                  <th onClick={() => onSort("output")}>输出{sortArrow("output")}</th>
                  <th onClick={() => onSort("cache")}>缓存读{sortArrow("cache")}</th>
                  <th onClick={() => onSort("total")}>
                    总量{sortArrow("total")}
                  </th>
                  <th onClick={() => onSort("avgTps")}>
                    平均速{sortArrow("avgTps")}
                  </th>
                  <th onClick={() => onSort("maxTps")}>最快{sortArrow("maxTps")}</th>
                  <th onClick={() => onSort("minTps")}>最慢{sortArrow("minTps")}</th>
                  <th onClick={() => onSort("avgDuration")}>
                    均耗时{sortArrow("avgDuration")}
                  </th>
                  <th className="za-ut-share">占比</th>
                </tr>
              </thead>
              <tbody>
                {sortedRows.map((r) => {
                  const share =
                    sumTotal > 0 ? (r.totalTokens / sumTotal) * 100 : 0;
                  return (
                    <tr key={r.key}>
                      <td className="za-ut-l" title={r.key}>
                        {prettyKey(r.label)}
                      </td>
                      <td className="za-mono">{r.calls.toLocaleString()}</td>
                      <td className="za-mono za-faint">
                        {formatUnits(r.inputTokens)}
                      </td>
                      <td className="za-mono">{formatUnits(r.outputTokens)}</td>
                      <td className="za-mono za-faint">
                        {formatUnits(r.cacheReadTokens)}
                      </td>
                      <td className="za-mono" style={{ fontWeight: 600 }}>
                        {formatUnits(r.totalTokens)}
                      </td>
                      <td className="za-mono">{fmtTps(r.avgTps)}</td>
                      <td className="za-mono za-tps-good">{fmtTps(r.maxTps)}</td>
                      <td className="za-mono za-tps-warn">{fmtTps(r.minTps)}</td>
                      <td className="za-mono za-faint">
                        {fmtMs(r.avgDurationMs)}
                      </td>
                      <td className="za-ut-share">
                        <div className="za-usage-bar">
                          <div
                            className="za-usage-bar-fill"
                            style={{
                              width: `${Math.max(2, share).toFixed(1)}%`,
                              background: usageColor(
                                maxTotal > 0
                                  ? (r.totalTokens / maxTotal) * 100
                                  : 0
                              ),
                            }}
                          />
                          <span className="za-usage-bar-label">
                            {share.toFixed(1)}%
                          </span>
                        </div>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* 明细 */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>调用明细</h3>
          <button
            className="za-btn za-btn-sm za-btn-ghost"
            onClick={() => setDetailOpen((v) => !v)}
          >
            {detailOpen ? "收起" : "展开最近 200 条"}
          </button>
        </div>
        {!detailOpen ? (
          <div className="za-empty za-faint">点击右上角展开查看明细</div>
        ) : detailLoading ? (
          <div className="za-empty">加载中…</div>
        ) : detailRows.length === 0 ? (
          <div className="za-empty">暂无明细</div>
        ) : (
          <div className="za-usage-scroll">
            <table className="za-usage-table">
              <thead>
                <tr>
                  <th className="za-ut-l">时间</th>
                  <th>供应商</th>
                  <th>模型</th>
                  <th>角色</th>
                  <th>输入</th>
                  <th>输出</th>
                  <th>缓存</th>
                  <th>总量</th>
                  <th>耗时</th>
                  <th>速度</th>
                  <th>结束</th>
                </tr>
              </thead>
              <tbody>
                {detailRows.map((r) => (
                  <tr key={r.requestId}>
                    <td className="za-ut-l za-mono za-faint">
                      {fmtTime(r.startedAt)}
                    </td>
                    <td className="za-faint" title={r.providerId}>
                      {prettyKey(r.providerId)}
                    </td>
                    <td>{r.modelId}</td>
                    <td>
                      {r.role ? (
                        <span className="za-badge za-badge-neutral">{r.role}</span>
                      ) : (
                        "—"
                      )}
                    </td>
                    <td className="za-mono za-faint">
                      {formatUnits(r.inputTokens)}
                    </td>
                    <td className="za-mono">{formatUnits(r.outputTokens)}</td>
                    <td className="za-mono za-faint">
                      {formatUnits(r.cacheReadTokens)}
                    </td>
                    <td className="za-mono" style={{ fontWeight: 600 }}>
                      {formatUnits(r.totalTokens)}
                    </td>
                    <td className="za-mono za-faint">{fmtMs(r.durationMs)}</td>
                    <td className="za-mono">{fmtTps(r.tps)}</td>
                    <td className="za-faint">{r.finishReason ?? "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </>
  );
}

function dimLabel(g: UsageGroupBy): string {
  return g === "provider" ? "供应商" : g === "model" ? "模型" : "日期";
}

function fmtTokens(n?: number | null): string {
  if (n == null) return "—";
  return formatUnits(n);
}

function Stat({
  label,
  value,
  color,
  strong,
}: {
  label: string;
  value: string;
  color?: string;
  strong?: boolean;
}) {
  return (
    <div className="za-usage-stat">
      <div className="za-faint" style={{ fontSize: "var(--fs-sm)" }}>
        {label}
      </div>
      <div
        className="za-mono"
        style={{
          fontSize: "var(--fs-xl)",
          fontWeight: strong ? 700 : 600,
          color: color ?? "var(--text-primary)",
          marginTop: "var(--space-1)",
        }}
      >
        {value}
      </div>
    </div>
  );
}

function SpeedStat({
  label,
  tps,
  tone,
}: {
  label: string;
  tps?: number | null;
  tone: "good" | "normal" | "warn";
}) {
  const color =
    tone === "good"
      ? "#22C55E"
      : tone === "warn"
      ? "#F59E0B"
      : "var(--text-primary)";
  return (
    <div className="za-usage-stat">
      <div className="za-faint" style={{ fontSize: "var(--fs-sm)" }}>
        {label}
      </div>
      <div
        className="za-mono"
        style={{
          fontSize: "var(--fs-xl)",
          fontWeight: 700,
          color,
          marginTop: "var(--space-1)",
        }}
      >
        {fmtTps(tps)}
        <span className="za-faint" style={{ fontSize: "var(--fs-sm)", fontWeight: 400 }}>
          {" "}
          tok/s
        </span>
      </div>
    </div>
  );
}
