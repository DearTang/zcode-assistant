import { useEffect, useRef, useState, type CSSProperties } from "react";
import {
  importer,
  models,
  zcode,
  quota,
  templates,
  quotaToken,
  events,
  health,
  maskApiKey,
  formatUnits,
  openUrl,
  pickBucketByName,
  usageColor,
} from "../../api";
import type { ImportResult, ProviderPreview } from "../../api";
import {
  PRESET_PROVIDERS,
  PRESET_CATEGORY_LABELS,
  type ProviderPreset,
} from "../../presets/providerPresets";
import {
  IconPlus,
  IconTrash,
  IconRefresh,
  IconCheck,
  IconFolder,
  IconClose,
  IconZap,
  IconStar,
} from "../../components/icons";
import { RestartBar } from "../../components/RestartBar";
import { DualRing } from "../../components/DualRing";
import { toast } from "../../components/Toast";
import type {
  ProviderKind,
  ZcModel,
  ZcProvider,
  ZcodeConfig,
  ZcodeSetting,
  ModelSpec,
  QuotaOverview,
  QuotaBucket,
  QuotaTemplate,
  QuotaTokenStatus,
} from "../../types";

/** 供应商/模型变更后的统一黄色提醒：不自动重启，由用户在顶部「重启 zcode」按钮自行操作 */
const RESTART_HINT = "供应商/模型修改需要重启zcode才可以生效哟!";

const fieldStyle: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 4,
  fontSize: "var(--fs-sm)",
  color: "var(--text-secondary)",
};

/** 模型上下文缺省值 */
const DEFAULT_CONTEXT = 200_000;

/** Token Plan 供应商识别（与后端 coding_plan::detect 口径一致，对齐 cc-switch
 *  codingPlanProviders：按 baseURL 子串匹配；命中即自动用该供应商 API Key + Base URL 查额度） */
export function detectCodingPlan(base: string): {
  id: "kimi" | "zhipu" | "minimax" | "zenmux" | "volcengine";
  label: string;
} | null {
  const u = (base || "").toLowerCase();
  if (u.includes("api.kimi.com/coding")) return { id: "kimi", label: "Kimi For Coding" };
  if (u.includes("bigmodel.cn") || u.includes("api.z.ai"))
    return { id: "zhipu", label: "Zhipu GLM(智谱)" };
  if (u.includes("api.minimaxi.com") || u.includes("api.minimax.io"))
    return { id: "minimax", label: "MiniMax" };
  if (u.includes("zenmux")) return { id: "zenmux", label: "ZenMux" };
  if (u.includes("volces.com/api/coding"))
    return { id: "volcengine", label: "火山方舟(Volcengine)" };
  return null;
}

export default function Models() {
  const [config, setConfig] = useState<ZcodeConfig | null>(null);
  const [setting, setSetting] = useState<ZcodeSetting | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [showAddModal, setShowAddModal] = useState(false);
  // 导入配置
  const [impSource, setImpSource] = useState("opencode");
  const [impPath, setImpPath] = useState("");
  const [impResults, setImpResults] = useState<ImportResult[] | null>(null);
  // 预览弹窗：解析出的待导入 provider 列表（null=弹窗关闭）
  const [impPreview, setImpPreview] = useState<ProviderPreview[] | null>(null);
  // 本次会话新导入的 provider key（仅内存，退出应用即清，用于 NEW 标记）
  const [newKeys, setNewKeys] = useState<Set<string>>(() => new Set());
  // 双击打开的供应商编辑弹窗 key
  const [editKey, setEditKey] = useState<string | null>(null);
  // 主供应商 key（总览 / 悬浮窗 / 托盘展示其配额；null=未设置，回退自动识别智谱 Coding Plan）
  const [primary, setPrimary] = useState<string | null>(null);
  // 每个供应商的最新配额（key -> QuotaOverview | null），null=查询失败/未配置
  const [quotaMap, setQuotaMap] = useState<Record<string, QuotaOverview | null>>({});
  // 拖拽排序：dragIdx=拖动源索引（驱动视觉反馈）；gripDownRef=手柄按下标记（同步 ref，避免 state 重渲染延迟导致 draggable 时序错位）
  const [dragIdx, setDragIdx] = useState<number | null>(null);
  const gripDownRef = useRef(false);

  const reload = async () => {
    try {
      const [c, s, p] = await Promise.all([
        zcode.getConfig(),
        zcode.getSetting(),
        models.getPrimary(),
      ]);
      setConfig(c);
      setSetting(s);
      setPrimary(p ?? null);
      setSelected(
        (cur) =>
          cur ??
          Object.keys(c.provider).find((k) => !k.startsWith("builtin:")) ??
          null
      );
    } catch (e: unknown) {
      toast.error(typeof e === "string" ? e : "读取 zcode 配置失败");
    }
  };
  useEffect(() => {
    reload();
    // 后端 select_provider 广播「当前模型已切换」→ 多窗口同步刷新当前选中态
    let un: (() => void) | undefined;
    events.onModelSwitched(() => reload()).then((fn) => (un = fn));
    return () => {
      un?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const family = setting?.providerFamilyDomain;
  const currentKey = family
    ? setting?.modelProviderFamilySelectedKeys?.[family]
    : undefined;
  // setting.json 中选中值可能带 "coding-plan:" 前缀（如 "coding-plan:builtin:bigmodel-coding-plan"）
  const isCurrentKey = (key: string) =>
    currentKey === key || currentKey === `coding-plan:${key}`;

  // 智谱账号对应的 builtin 供应商：优先取当前 family 选中的 builtin，
  // 兜底取启用的、模型最多的 builtin。固定展示在供应商列表首位。
  const builtinKey = (() => {
    if (!config) return undefined;
    if (currentKey) {
      const k = currentKey.startsWith("coding-plan:")
        ? currentKey.slice("coding-plan:".length)
        : currentKey;
      if (k.startsWith("builtin:") && config.provider[k]) return k;
    }
    const fb = Object.entries(config.provider)
      .filter(
        ([k, p]) =>
          k.startsWith("builtin:") &&
          !p.systemDisabledReason &&
          p.enabled !== false
      )
      .sort(
        (a, b) => Object.keys(b[1].models).length - Object.keys(a[1].models).length
      )[0];
    return fb?.[0];
  })();
  const builtinEntry: [string, ZcProvider] | undefined =
    builtinKey && config ? [builtinKey, config.provider[builtinKey]] : undefined;

  // 每 60s 轮询供应商配额：智谱账号（builtin）走内置 Coding Plan 接口；
  // Token Plan 供应商（Kimi/智谱/MiniMax/ZenMux/火山）按 baseURL 自动识别查询，
  // 其余走用量模板。失败/未配置 → null，前端卡片显示「未知」/「未配置」。
  useEffect(() => {
    if (!config) return;
    const customKeys = Object.keys(config.provider).filter(
      (k) => !k.startsWith("builtin:")
    );
    let cancelled = false;
    const poll = async () => {
      const entries = await Promise.all([
        ...customKeys.map(async (k): Promise<[string, QuotaOverview | null]> => {
          try {
            return [k, await quota.getProviderQuota(k)];
          } catch {
            return [k, null];
          }
        }),
        ...(builtinKey
          ? [(async (): Promise<[string, QuotaOverview | null]> => {
              try {
                return [builtinKey, await quota.getCodingPlan()];
              } catch {
                return [builtinKey, null];
              }
            })()]
          : []),
      ]);
      if (!cancelled) setQuotaMap(Object.fromEntries(entries));
    };
    poll();
    const t = setInterval(poll, 60_000);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, [config, builtinKey]);

  // ⚡ 立即检测指定供应商连接（GET /models 免费探测，绕过冷却），
  // 结果走通知栏 toast；卡片本身不再单独占行展示检测状态（可用性由额度行徽标呈现）
  const checkProvider = async (key: string, name: string) => {
    toast.success(`正在检测「${name}」连接…`);
    try {
      const r = await health.checkProvider(key);
      if (r.ok) toast.success(`「${name}」连接可用：${r.message}`);
      else toast.warning(`「${name}」连接不可用：${r.message}`);
    } catch (e: unknown) {
      toast.error(typeof e === "string" ? e : "检测失败");
    }
  };

  // ⭐ 设为/取消主供应商（全局唯一）：列表卡片右侧快捷切换；
  // 立即触发 App 全局配额刷新，总览 / 悬浮窗 / 托盘马上切换数据源
  const togglePrimary = (key: string, name: string) => {
    const v = primary !== key;
    models
      .setPrimary(key, v)
      .then(() => {
        events.emitRefreshRequested();
        setPrimary(v ? key : null);
        toast.success(
          v
            ? `已设「${name}」为主供应商，总览 / 悬浮窗 / 托盘将展示其配额`
            : "已取消主供应商，展示回退自动识别"
        );
      })
      .catch((e) =>
        toast.error(typeof e === "string" ? e : "设置主供应商失败")
      );
  };

  // 隐藏系统未授权的 provider；builtin 由 builtinEntry 单独置顶展示；
  // 用户禁用的保留显示（灰显，可重新启用）
  const providers = config
    ? Object.entries(config.provider).filter(
        ([key, p]) =>
          !p.systemDisabledReason && !key.startsWith("builtin:")
      )
    : [];

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    try {
      await fn();
      await reload();
    } catch (e: unknown) {
      toast.error(typeof e === "string" ? e : "操作失败");
    } finally {
      setBusy(false);
    }
  };

  // 点「导入」→ 先预览解析（只读不写），弹窗勾选后再执行导入
  const handleImport = async () => {
    setBusy(true);
    try {
      const items = await importer.preview(impSource, impPath.trim() || undefined);
      setImpPreview(items);
    } catch (e: unknown) {
      toast.error(typeof e === "string" ? e : "解析配置失败");
    } finally {
      setBusy(false);
    }
  };

  // 预览弹窗确认：只导入选中的条目
  const handleImportConfirm = async (ids: string[]) => {
    setBusy(true);
    try {
      const results = await importer.from(
        impSource,
        impPath.trim() || undefined,
        ids
      );
      setImpPreview(null);
      setImpResults(results);
      // 记录新导入的 provider key（仅 success），用于本次会话 NEW 标记
      const added = results
        .filter((r) => r.status === "success" && r.providerKey)
        .map((r) => r.providerKey);
      const updated = results.filter((r) => r.status === "updated");
      if (added.length > 0) {
        setNewKeys((prev) => {
          const next = new Set(prev);
          added.forEach((k) => next.add(k));
          return next;
        });
      }
      await reload();
      if (added.length > 0) {
        toast.success(`成功导入 ${added.length} 个供应商`);
        toast.warning(RESTART_HINT);
      } else if (updated.length > 0) {
        toast.success(`已覆盖更新 ${updated.length} 个供应商`);
        toast.warning(RESTART_HINT);
      } else {
        toast.error("未成功导入任何供应商");
      }
    } catch (e: unknown) {
      toast.error(typeof e === "string" ? e : "导入失败");
    } finally {
      setBusy(false);
    }
  };

  const handlePickFile = async () => {
    try {
      const picked = await importer.pickFile(impSource, impPath || undefined);
      if (picked) setImpPath(picked);
    } catch {
      /* 用户取消或不可用，忽略 */
    }
  };

  const handleRemove = (key: string) =>
    run(async () => {
      await models.removeProvider(key);
      if (selected === key) setSelected(null);
      if (editKey === key) setEditKey(null);
      toast.success("已删除供应商");
      toast.warning(RESTART_HINT);
    });

  // 拖拽排序：把 dragIdx 项移到 targetIdx 位置，写回 config.json
  const handleReorder = async (targetIdx: number) => {
    const src = dragIdx;
    setDragIdx(null);
    gripDownRef.current = false;
    if (src === null || src === targetIdx) return;
    const keys = providers.map(([k]) => k);
    const [moved] = keys.splice(src, 1);
    keys.splice(targetIdx, 0, moved);
    setBusy(true);
    try {
      await models.reorderProviders(keys);
      await reload();
      toast.success("供应商顺序已更新");
    } catch (e: unknown) {
      toast.error(typeof e === "string" ? e : "排序失败");
    } finally {
      setBusy(false);
    }
  };

  // 弹窗中操作的 provider 数据
  const editProvider = editKey ? config?.provider[editKey] : undefined;

  return (
    <>
      <RestartBar hint="供应商 / 模型变更后需重启 zcode 生效" />

      {/* 从其他工具导入配置 */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>导入配置</h3>
        </div>
        <p
          className="za-muted"
          style={{ margin: "0 0 10px", fontSize: "var(--fs-sm)" }}
        >
          从 opencode / Claude Code / Codex 配置文件导入 provider。留空路径用各工具默认位置；也可手动指定。
        </p>
        <div className="za-row" style={{ gap: 8, flexWrap: "wrap", alignItems: "center" }}>
          <select
            className="za-select"
            style={{ height: 30, width: 250 }}
            value={impSource}
            onChange={(e) => setImpSource(e.target.value)}
          >
            <option value="opencode">opencode (opencode.json)</option>
            <option value="claude">Claude Code (settings.json)</option>
            <option value="codex">Codex (config.toml)</option>
            <option value="zcode">ZCode (config.json)</option>
          </select>
          <div
            className="za-input za-btn-sm"
            style={{
              height: 30,
              width: 320,
              display: "flex",
              alignItems: "center",
              gap: 6,
              padding: "0 8px",
              cursor: "pointer",
            }}
            onClick={handlePickFile}
            title="点击选择配置文件"
          >
            <IconFolder width={14} height={14} style={{ color: "var(--accent)", flexShrink: 0 }} />
            <span
              className="za-mono"
              style={{
                fontSize: "var(--fs-xs)",
                color: impPath ? "var(--text-primary)" : "var(--text-tertiary)",
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
                flex: 1,
              }}
            >
              {impPath || "点击选择配置文件，或留空使用默认路径"}
            </span>
          </div>
          {impPath && (
            <button
              className="za-icon-btn"
              style={{ width: 30, height: 30, flexShrink: 0 }}
              onClick={() => setImpPath("")}
              title="清除路径"
            >
              <IconClose width={13} height={13} />
            </button>
          )}
          <button
            className="za-btn za-btn-sm za-btn-primary"
            disabled={busy}
            onClick={handleImport}
          >
            导入
          </button>
        </div>
        {impResults && impResults.length > 0 && (() => {
          const succ = impResults.filter((r) => r.status === "success");
          const upd = impResults.filter((r) => r.status === "updated");
          const dup = impResults.filter((r) => r.status === "duplicate");
          const fail = impResults.filter((r) => r.status === "failed");
          const detail = [...upd, ...dup, ...fail]; // 清单列覆盖/重复/失败
          return (
            <div style={{ marginTop: 10 }}>
              <div
                className="za-row"
                style={{ gap: 14, fontSize: "var(--fs-sm)" }}
              >
                <span style={{ color: "var(--accent)" }}>
                  ✓ 成功 {succ.length}
                </span>
                <span style={{ color: "var(--accent)" }}>
                  ↻ 覆盖 {upd.length}
                </span>
                {dup.length > 0 && (
                  <span style={{ color: "var(--text-tertiary)" }}>
                    ↻ 重复 {dup.length}
                  </span>
                )}
                <span style={{ color: "#ef4444" }}>✕ 失败 {fail.length}</span>
              </div>
              {detail.length > 0 && (
                <div
                  className="za-mono"
                  style={{
                    fontSize: "var(--fs-xs)",
                    marginTop: 6,
                    display: "flex",
                    flexDirection: "column",
                    gap: 3,
                  }}
                >
                  {detail.map((r, i) => (
                    <div
                      // eslint-disable-next-line react/no-array-index-key
                      key={`${r.name}-${i}`}
                      className="za-row"
                      style={{ gap: 6 }}
                    >
                      <span
                        style={{
                          color:
                            r.status === "failed"
                              ? "#ef4444"
                              : r.status === "updated"
                                ? "var(--accent)"
                                : "var(--text-tertiary)",
                        }}
                      >
                        {r.status === "failed" ? "✕" : "↻"}
                      </span>
                      <span style={{ color: "var(--text-primary)" }}>
                        {r.name}
                      </span>
                      <span className="za-muted">— {r.message}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          );
        })()}
      </div>

      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>供应商</h3>
          <button
            className="za-btn za-btn-sm za-btn-primary"
            onClick={() => setShowAddModal(true)}
          >
            <IconPlus width={14} height={14} /> 添加供应商
          </button>
        </div>
        <p
          className="za-muted"
          style={{ margin: "0 0 10px", fontSize: "var(--fs-xs)" }}
        >
          单击选中，双击编辑，拖动 ⠿ 手柄调整顺序（智谱账号固定首位）
        </p>

        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {/* 智谱账号（builtin）：固定第一位，不可拖拽/禁用/删除/改信息，仅可管理其模型 */}
          {builtinEntry &&
            (() => {
              const [bKey, bProv] = builtinEntry;
              return (
                <div
                  key={bKey}
                  style={{
                    padding: "8px 12px",
                    borderRadius: 8,
                    cursor: "pointer",
                    background:
                      selected === bKey
                        ? "var(--accent-subtle)"
                        : "transparent",
                    border: "1px solid var(--accent)",
                    opacity: bProv.enabled !== false ? 1 : 0.5,
                    display: "flex",
                    flexDirection: "column",
                    gap: 6,
                  }}
                  onClick={() => setSelected(bKey)}
                  onDoubleClick={() => setEditKey(bKey)}
                  title="双击管理套餐模型（供应商信息不可修改）"
                >
                  <div className="za-row-between">
                    <div className="za-row" style={{ gap: 8 }}>
                      <span
                        style={{
                          color: "var(--accent)",
                          userSelect: "none",
                          lineHeight: 1,
                          flexShrink: 0,
                        }}
                        title="固定第一位"
                      >
                        📌
                      </span>
                      {isCurrentKey(bKey) && (
                        <IconCheck
                          width={14}
                          height={14}
                          style={{ color: "var(--accent)" }}
                        />
                      )}
                      <div
                        style={{
                          display: "flex",
                          flexDirection: "column",
                          gap: 1,
                        }}
                      >
                        <span style={{ fontWeight: 500 }}>{bProv.name}</span>
                        <span
                          className="za-mono za-faint"
                          style={{
                            fontSize: "var(--fs-xs)",
                            wordBreak: "break-all",
                          }}
                        >
                          标识 {bKey}
                        </span>
                      </div>
                      <span
                        className="za-badge"
                        style={{
                          background: "rgba(34,197,94,0.15)",
                          color: "#22C55E",
                        }}
                        title="智谱 Coding Plan 订阅（登录态托管）"
                      >
                        智谱CodingPlan
                      </span>
                      {primary === bKey && (
                        <span
                          className="za-badge"
                          style={{
                            background: "rgba(34,197,94,0.15)",
                            color: "#22C55E",
                          }}
                          title="主供应商：总览 / 悬浮窗 / 托盘展示此供应商的配额"
                        >
                          主供应商
                        </span>
                      )}
                      <span className="za-badge za-badge-neutral">
                        {bProv.kind}
                      </span>
                    </div>
                    <div className="za-row" style={{ gap: 8 }}>
                      <span
                        className="za-faint"
                        style={{ fontSize: "var(--fs-xs)", flexShrink: 0 }}
                      >
                        模型数：{Object.keys(bProv.models).length}
                      </span>
                      <button
                        className="za-icon-btn"
                        style={{
                          width: 26,
                          height: 26,
                          color: primary === bKey ? "#F5B301" : undefined,
                        }}
                        onClick={(e) => {
                          e.stopPropagation();
                          togglePrimary(bKey, bProv.name);
                        }}
                        title={
                          primary === bKey
                            ? "取消主供应商"
                            : "设为主供应商：总览 / 悬浮窗 / 托盘展示此供应商的配额"
                        }
                      >
                        <IconStar
                          width={13}
                          height={13}
                          fill={primary === bKey ? "currentColor" : "none"}
                        />
                      </button>
                      <button
                        className="za-icon-btn"
                        style={{ width: 26, height: 26 }}
                        onClick={async (e) => {
                          e.stopPropagation();
                          await checkProvider(bKey, bProv.name);
                        }}
                        title="立即检测此供应商连接（GET /models，不消耗 token）"
                      >
                        <IconZap width={13} height={13} />
                      </button>
                    </div>
                  </div>
                  <ProviderQuotaRow
                    quota={quotaMap[bKey]}
                    isBigmodel={(bProv.options?.baseURL ?? "")
                      .toLowerCase()
                      .includes("bigmodel")}
                  />
                </div>
              );
            })()}
          {providers.length === 0 && !builtinEntry && (
            <div className="za-empty">未读取到 provider（请确认 zcode 已配置）</div>
          )}
          {providers.map(([key, p], idx) => {
            const enabled = p.enabled !== false;
            const isCurrent = isCurrentKey(key);
            return (
              <div
                key={key}
                draggable
                onDragStart={(e) => {
                  // 只有手柄按下时才允许拖拽，否则阻止（避免误触单击/双击触发的拖拽）
                  if (!gripDownRef.current) {
                    e.preventDefault();
                    return;
                  }
                  setDragIdx(idx);
                }}
                onDragOver={(e) => e.preventDefault()}
                onDrop={() => handleReorder(idx)}
                onDragEnd={() => {
                  gripDownRef.current = false;
                  setDragIdx(null);
                }}
                style={{
                  padding: "8px 12px",
                  borderRadius: 8,
                  cursor: "pointer",
                  background:
                    selected === key ? "var(--accent-subtle)" : "transparent",
                  border:
                    dragIdx === idx
                      ? "1px dashed var(--accent)"
                      : "1px solid var(--glass-border)",
                  opacity: dragIdx === idx ? 0.5 : enabled ? 1 : 0.5,
                  display: "flex",
                  flexDirection: "column",
                  gap: 6,
                }}
                onClick={() => setSelected(key)}
                onDoubleClick={() => setEditKey(key)}
                title="双击编辑"
              >
                <div className="za-row-between">
                  <div className="za-row" style={{ gap: 8 }}>
                    {/* 拖拽手柄：按下时设标记，卡片 onDragStart 据此放行；松开重置 */}
                    <span
                      onMouseDown={() => {
                        gripDownRef.current = true;
                      }}
                      onMouseUp={() => {
                        gripDownRef.current = false;
                      }}
                      style={{
                        cursor: "grab",
                        color: "var(--text-tertiary)",
                        fontSize: "var(--fs-md)",
                        lineHeight: 1,
                        userSelect: "none",
                        flexShrink: 0,
                      }}
                      title="拖动调整顺序"
                    >
                      ⠿
                    </span>
                    {isCurrent && (
                      <IconCheck
                        width={14}
                        height={14}
                        style={{ color: "var(--accent)" }}
                      />
                    )}
                    <div
                      style={{
                        display: "flex",
                        flexDirection: "column",
                        gap: 1,
                      }}
                    >
                      <span style={{ fontWeight: 500 }}>{p.name}</span>
                      <span
                        className="za-mono za-faint"
                        style={{
                          fontSize: "var(--fs-xs)",
                          wordBreak: "break-all",
                        }}
                      >
                        标识 {key}
                      </span>
                    </div>
                    {newKeys.has(key) && (
                      <span className="za-badge za-badge-new">NEW</span>
                    )}
                    {primary === key && (
                      <span
                        className="za-badge"
                        style={{
                          background: "rgba(34,197,94,0.15)",
                          color: "#22C55E",
                        }}
                        title="主供应商：总览 / 悬浮窗 / 托盘展示此供应商的配额"
                      >
                        主供应商
                      </span>
                    )}
                    <span className="za-badge za-badge-neutral">{p.kind}</span>
                    {p.source === "custom" && (
                      <span className="za-badge">自定义</span>
                    )}
                  </div>
                  <div className="za-row" style={{ gap: 8 }}>
                    <span
                      className="za-faint"
                      style={{ fontSize: "var(--fs-xs)", flexShrink: 0 }}
                      title="该供应商当前配置的模型数"
                    >
                      模型数：{Object.keys(p.models).length}
                    </span>
                    <button
                      className="za-icon-btn"
                      style={{
                        width: 26,
                        height: 26,
                        color: primary === key ? "#F5B301" : undefined,
                      }}
                      onClick={(e) => {
                        e.stopPropagation();
                        togglePrimary(key, p.name);
                      }}
                      title={
                        primary === key
                          ? "取消主供应商"
                          : "设为主供应商：总览 / 悬浮窗 / 托盘展示此供应商的配额"
                      }
                    >
                      <IconStar
                        width={13}
                        height={13}
                        fill={primary === key ? "currentColor" : "none"}
                      />
                    </button>
                    <button
                      className="za-icon-btn"
                      style={{ width: 26, height: 26 }}
                      onClick={async (e) => {
                        e.stopPropagation();
                        await checkProvider(key, p.name);
                      }}
                      title="立即检测此供应商连接（GET /models，不消耗 token）"
                    >
                      <IconZap width={13} height={13} />
                    </button>
                    <Toggle
                      checked={enabled}
                      title={enabled ? "已启用" : "已禁用"}
                      onChange={(v) =>
                        run(async () => {
                          await models.setProviderEnabled(key, v);
                        })
                      }
                    />
                    <button
                      className="za-icon-btn"
                      style={{ width: 26, height: 26 }}
                      onClick={(e) => {
                        e.stopPropagation();
                        handleRemove(key);
                      }}
                      title="删除供应商"
                    >
                      <IconTrash width={13} height={13} />
                    </button>
                  </div>
                </div>
                <ProviderQuotaRow
                  quota={quotaMap[key]}
                  isBigmodel={(p.options?.baseURL ?? "")
                    .toLowerCase()
                    .includes("bigmodel")}
                />
              </div>
            );
          })}
        </div>
      </div>

      {/* 双击供应商 → 编辑弹窗（主供应商改在列表卡片 ⭐ 快捷切换） */}
      {editKey && editProvider && (
        <ProviderEditModal
          providerKey={editKey}
          provider={editProvider}
          isCurrent={isCurrentKey(editKey)}
          isBuiltin={editKey === builtinKey}
          busy={busy}
          onClose={() => setEditKey(null)}
          onRun={run}
        />
      )}

      {showAddModal && (
        <ProviderAddModal
          onClose={() => setShowAddModal(false)}
          onAdded={(m) => {
            setShowAddModal(false);
            toast.success(m);
            toast.warning(RESTART_HINT);
            reload();
          }}
        />
      )}

      {/* 导入预览：勾选要导入的供应商后再执行写入 */}
      {impPreview && (
        <ImportPreviewModal
          items={impPreview}
          busy={busy}
          onClose={() => setImpPreview(null)}
          onConfirm={handleImportConfirm}
        />
      )}
    </>
  );
}

/* ============ 供应商卡片额度行（智谱 vs 模板 统一展示） ============ */
function ProviderQuotaRow({
  quota,
  isBigmodel,
}: {
  quota: QuotaOverview | null | undefined;
  isBigmodel: boolean;
}) {
  if (quota === undefined) return <QuotaPlaceholder text="额度查询中…" />;
  if (quota === null) {
    return (
      <QuotaPlaceholder
        text={
          isBigmodel ? "智谱配额查询失败" : "未配置用量查询（双击详情配置）"
        }
      />
    );
  }
  const b5 = pickBucketByName(quota, "5小时");
  const bW = pickBucketByName(quota, "每周");
  // 月限额桶（可选）：名为「每月…」且排除智谱的「MCP每月额度」（那是工具调用次数，不是模型月限额）
  const bM = quota.buckets.find(
    (b) => b.name.includes("每月") && !b.name.includes("MCP")
  );
  const monthlyPart = bM
    ? ` · 每月 剩${
        bM.unit === "%"
          ? `${Math.round(bM.remaining)}%`
          : formatUnits(bM.remaining)
      }`
    : "";
  const monthlyOk = !bM || bM.remaining > 0;
  // 智谱 BigModel：每5小时 + 每周（部分供应商再追加每月）
  if (b5 || bW) {
    const u5 = b5 && b5.total > 0 ? (b5.used / b5.total) * 100 : null;
    const uW = bW && bW.total > 0 ? (bW.used / bW.total) * 100 : null;
    const summary =
      b5 && bW
        ? `每5小时 剩${Math.round(b5.remaining)}% · 每周 剩${Math.round(
            bW.remaining
          )}%`
        : b5
          ? `每5小时 剩${Math.round(b5.remaining)}%`
          : bW
            ? `每周 剩${Math.round(bW.remaining)}%`
            : "无额度数据";
    // 可用性只判定实际存在的 bucket：部分供应商没有周限额（只有 5 小时窗口），
    // 缺失的 bucket 不参与判定，否则会被误标「不可用」
    const ok =
      (b5 ? b5.remaining > 0 : true) &&
      (bW ? bW.remaining > 0 : true) &&
      monthlyOk;
    const reset = b5?.periodEnd || bW?.periodEnd;
    return (
      <div
        className="za-row"
        style={{ gap: 8, alignItems: "center", paddingLeft: 22 }}
      >
        <DualRing size={28} usedOuter={u5} usedInner={uW} />
        <span className="za-mono" style={{ fontSize: "var(--fs-xs)" }}>
          {summary}
          {monthlyPart}
        </span>
        <AvailabilityBadge ok={ok} />
        {reset && (
          <span
            className="za-faint za-mono"
            style={{ fontSize: "var(--fs-xs)" }}
          >
            重置 {formatReset(reset)}
          </span>
        )}
      </div>
    );
  }
  // 模板：单 bucket
  const b = quota.buckets[0];
  if (b) {
    const usedPct = b.total > 0 ? (b.used / b.total) * 100 : null;
    const summary =
      (b.unit === "%"
        ? `剩 ${Math.round(b.remaining)}%`
        : `剩 ${formatUnits(b.remaining)}`) + monthlyPart;
    const ok = b.remaining > 0 && monthlyOk;
    return (
      <div
        className="za-row"
        style={{ gap: 8, alignItems: "center", paddingLeft: 22 }}
      >
        <DualRing size={28} usedOuter={usedPct} usedInner={null} />
        <span className="za-mono" style={{ fontSize: "var(--fs-xs)" }}>
          {summary}
        </span>
        <AvailabilityBadge ok={ok} />
        <span
          className="za-faint za-mono"
          style={{ fontSize: "var(--fs-xs)" }}
        >
          {b.periodEnd
            ? `重置 ${formatReset(b.periodEnd)}`
            : `更新 ${formatTime(quota.fetchedAt)}`}
        </span>
      </div>
    );
  }
  return <QuotaPlaceholder text="无额度数据" />;
}

function QuotaPlaceholder({ text }: { text: string }) {
  return (
    <div
      className="za-row"
      style={{ gap: 8, alignItems: "center", paddingLeft: 22 }}
    >
      <div
        style={{
          width: 28,
          height: 28,
          borderRadius: "50%",
          border: "2px solid var(--glass-border)",
          flexShrink: 0,
        }}
      />
      <span
        className="za-faint za-mono"
        style={{ fontSize: "var(--fs-xs)" }}
      >
        {text}
      </span>
    </div>
  );
}

function AvailabilityBadge({ ok }: { ok: boolean }) {
  return (
    <span
      className="za-badge"
      style={{
        background: ok ? "rgba(34,197,94,0.15)" : "rgba(239,68,68,0.15)",
        color: ok ? "#22C55E" : "#EF4444",
        fontSize: "var(--fs-xs)",
      }}
    >
      {ok ? "可用" : "不可用"}
    </span>
  );
}

function formatReset(iso: string) {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "";
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(
    d.getMinutes()
  )}`;
}

function formatTime(iso: string) {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "";
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/* ============ 供应商编辑弹窗 ============ */
function ProviderEditModal({
  providerKey,
  provider,
  isCurrent,
  isBuiltin,
  busy,
  onClose,
  onRun,
}: {
  providerKey: string;
  provider: ZcProvider;
  isCurrent: boolean;
  isBuiltin: boolean;
  busy: boolean;
  onClose: () => void;
  onRun: (fn: () => Promise<void>) => Promise<void>;
}) {
  // 供应商信息（编辑态）
  const [eName, setEName] = useState(provider.name);
  const [eKind, setEKind] = useState<ProviderKind>(provider.kind);
  const [eUrl, setEUrl] = useState(provider.options.baseURL ?? "");
  // apiKey 来自后端时是脱敏的 <REDACTED>，输入框显示占位提示
  const [eKey, setEKey] = useState("");
  const [keyTouched, setKeyTouched] = useState(false);
  // 「查看 key」：临时展示明文（不影响保存；保存仍以 keyTouched 为准）
  const [revealedKey, setRevealedKey] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  // 拉取可用模型的候选（非 null = 选择弹窗打开）+ 模糊过滤词
  const [fetchCands, setFetchCands] = useState<ModelSpec[] | null>(null);
  const [fetchSel, setFetchSel] = useState<Set<string>>(() => new Set());
  const [fetchQuery, setFetchQuery] = useState("");
  // 模型拖拽排序：dragModelIdx=拖动源索引；gripRef=手柄按下标记（避免误触拖拽）
  const [dragModelIdx, setDragModelIdx] = useState<number | null>(null);
  const modelGripRef = useRef(false);

  // 用量查询模板（Token Plan 供应商仅用 extraJson 存附加凭据，其余为完整模板表单）
  const [tTmpl, setTTmpl] = useState<QuotaTemplate>({ providerKey, method: "GET" });
  const [tHasTmpl, setTHasTmpl] = useState(false);
  // 全部已保存模板（「使用模板」一键复制内容用）+ 当前 provider 的 Token 状态
  const [allTmpls, setAllTmpls] = useState<QuotaTemplate[]>([]);
  const [tToken, setTToken] = useState<QuotaTokenStatus | null>(null);
  // 明文 token（点「显示」后从后端加载核对；fetchedAt 变化 = 新 token，自动收回明文）
  const [revealedToken, setRevealedToken] = useState<string | null>(null);
  const [tokenCopied, setTokenCopied] = useState(false);
  const isTokenMode = (tTmpl.authMode ?? "appkey") === "token";
  // Token Plan 供应商（按 baseURL 自动识别并查询；智谱团队/火山需附加凭据 → 模板 extraJson）
  const codingPlan = detectCodingPlan(provider.options.baseURL ?? "");
  // extraJson 附加凭据（团队版组织/项目 ID、火山 AK/SK）的表单读写
  const tExtra: Record<string, string> = (() => {
    try {
      return JSON.parse(tTmpl.extraJson || "{}");
    } catch {
      return {};
    }
  })();
  const setExtra = (k: string, v: string) => {
    setTTmpl({ ...tTmpl, extraJson: JSON.stringify({ ...tExtra, [k]: v }) });
  };
  // 附加凭据输入块的容器样式
  const credBoxStyle: CSSProperties = {
    marginTop: 10,
    padding: 10,
    borderRadius: 8,
    border: "1px solid var(--glass-border)",
  };

  // 供应商信息锁定：当前供应商 / 智谱账号 builtin 均不可改
  const infoLocked = isCurrent || isBuiltin;

  const selModels = Object.entries(provider.models);
  const apiKeyDisplay =
    revealedKey ?? (keyTouched ? eKey : maskApiKey(provider.options.apiKey));

  // 显示/隐藏明文 key（仅查看，未编辑则不参与保存）
  const handleReveal = async () => {
    if (revealedKey != null) {
      setRevealedKey(null);
      return;
    }
    const k = await models.getApiKey(providerKey);
    setRevealedKey(k);
  };

  // 复制供应商标识到剪贴板
  const handleCopyId = async () => {
    try {
      await navigator.clipboard.writeText(providerKey);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      /* 剪贴板不可用，忽略 */
    }
  };

  const handleSaveProvider = () =>
    onRun(async () => {
      await models.updateProvider(providerKey, {
        name: eName,
        kind: eKind,
        baseURL: eUrl,
        // 只有用户改过 apiKey 才提交
        apiKey: keyTouched ? eKey : undefined,
      });
      toast.success("供应商信息已保存");
      toast.warning(RESTART_HINT);
    });

  const handleFetch = () =>
    onRun(async () => {
      const specs = await models.fetchAvailable(providerKey);
      const builtin = await models.builtinSpecs();
      const merged = specs.map((s) => {
        const b = builtin.find((x) => x.id === s.id);
        return {
          ...s,
          contextLength: s.contextLength ?? b?.contextLength,
          maxOutput: s.maxOutput ?? b?.maxOutput,
        };
      });
      const existing = new Set(Object.keys(provider.models));
      const init = new Set(
        merged.filter((s) => !existing.has(s.id)).map((s) => s.id)
      );
      setFetchCands(merged);
      setFetchSel(init);
      setFetchQuery("");
    });

  // 拖拽排序：把 dragModelIdx 项移到 targetIdx 位置，写回 config.json
  const handleModelReorder = (targetIdx: number) => {
    const src = dragModelIdx;
    setDragModelIdx(null);
    modelGripRef.current = false;
    if (src === null || src === targetIdx) return;
    const names = selModels.map(([n]) => n);
    const [moved] = names.splice(src, 1);
    names.splice(targetIdx, 0, moved);
    onRun(async () => {
      await models.reorderModels(providerKey, names);
      toast.success("模型顺序已更新");
      toast.warning(RESTART_HINT);
    });
  };

  const handleApplySelected = () =>
    onRun(async () => {
      if (!fetchCands) return;
      const picked = fetchCands.filter((s) => fetchSel.has(s.id));
      if (picked.length === 0) {
        setFetchCands(null);
        setFetchSel(new Set());
        setFetchQuery("");
        return;
      }
      const n = await models.applyModels(providerKey, picked);
      toast.success(`已写入 ${n} 个模型到 config.json`);
      toast.warning(RESTART_HINT);
      setFetchCands(null);
      setFetchSel(new Set());
      setFetchQuery("");
    });

  /** 关闭拉取弹窗（连同已选与搜索词清空） */
  const closeFetchModal = () => {
    setFetchCands(null);
    setFetchSel(new Set());
    setFetchQuery("");
  };

  /** 模糊匹配：查询词逐字符按顺序出现在模型 id 中即命中（包含关系是子集） */
  const fuzzyMatch = (text: string, q: string) => {
    if (!q.trim()) return true;
    const t = text.toLowerCase();
    let i = 0;
    for (const ch of q.toLowerCase()) {
      i = t.indexOf(ch, i);
      if (i < 0) return false;
      i += 1;
    }
    return true;
  };
  // 弹窗内按搜索词过滤后的候选
  const filteredCands = (fetchCands ?? []).filter((s) =>
    fuzzyMatch(s.id, fetchQuery)
  );

  // 加载用量查询模板（Token Plan 供应商也加载：extraJson 存团队/火山附加凭据）
  useEffect(() => {
    let cancelled = false;
    templates
      .get(providerKey)
      .then((t) => {
        if (cancelled) return;
        if (t) {
          setTTmpl(t);
          setTHasTmpl(true);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [providerKey]);

  // 已保存模板列表 + 内置预设（「使用模板」下拉数据源；已保存的排除当前 provider 自己）
  const [builtinTmpls, setBuiltinTmpls] = useState<QuotaTemplate[]>([]);
  useEffect(() => {
    templates.list().then(setAllTmpls).catch(() => {});
    templates.builtin().then(setBuiltinTmpls).catch(() => {});
  }, []);
  // 内置预设分组：Token Plan（preset:cp- 前缀，自动查询）与余额查询
  const builtinCpTmpls = builtinTmpls.filter((t) =>
    t.providerKey.startsWith("preset:cp-")
  );
  const builtinBalTmpls = builtinTmpls.filter(
    (t) => !t.providerKey.startsWith("preset:cp-")
  );

  // Token 状态：进入弹窗读取 + 登录窗获取成功后广播刷新
  useEffect(() => {
    quotaToken
      .status(providerKey)
      .then(setTToken)
      .catch(() => setTToken(null));
  }, [providerKey]);
  useEffect(() => {
    let un: (() => void) | undefined;
    events
      .onTokenUpdated((p) => {
        if (p.providerKey === providerKey) {
          quotaToken
            .status(providerKey)
            .then(setTToken)
            .catch(() => {});
        }
      })
      .then((fn) => (un = fn));
    return () => un?.();
  }, [providerKey]);

  // 新 token 写入（fetchedAt 变化）后收回已显示的明文
  useEffect(() => {
    setRevealedToken(null);
    setTokenCopied(false);
  }, [tToken?.fetchedAt]);

  /** 显示/隐藏明文 token（显示时才从后端读取，平时不回传前端） */
  const handleRevealToken = async () => {
    if (revealedToken != null) {
      setRevealedToken(null);
      return;
    }
    const v = await quotaToken.value(providerKey);
    setRevealedToken(v ?? "");
  };

  /** 复制明文 token 到剪贴板 */
  const handleCopyToken = async () => {
    if (!revealedToken) return;
    try {
      await navigator.clipboard.writeText(revealedToken);
      setTokenCopied(true);
      setTimeout(() => setTokenCopied(false), 1200);
    } catch {
      /* 剪贴板不可用，忽略 */
    }
  };

  /** 使用模板：把模板内容字段一键复制进当前表单（providerKey 保留当前供应商）；
   *  数据源含内置预设（Token Plan 6 家 + 余额 5 家，cc-switch 实测端点）与自己已保存的模板 */
  const handleUseTmpl = (key: string) => {
    const t =
      allTmpls.find((x) => x.providerKey === key) ??
      builtinTmpls.find((x) => x.providerKey === key);
    if (!t) return;
    setTTmpl({
      providerKey,
      name: t.name,
      method: t.method,
      url: t.url,
      headersJson: t.headersJson,
      body: t.body,
      totalPath: t.totalPath,
      usedPath: t.usedPath,
      remainingPath: t.remainingPath,
      monthlyTotalPath: t.monthlyTotalPath,
      monthlyUsedPath: t.monthlyUsedPath,
      monthlyRemainingPath: t.monthlyRemainingPath,
      loginUrl: t.loginUrl,
      tokenSource: t.tokenSource,
      authMode: t.authMode,
      loginUsername: t.loginUsername,
      // 附加凭据（团队组织/项目 ID、火山 AK/SK）不属于「模板内容」：
      // 所选模板没带就保留用户已填的，避免一键套用预设时静默清空密钥
      extraJson: t.extraJson ?? tTmpl.extraJson,
    });
    toast.success("模板内容已复制，可修改后点「保存模板」");
  };

  const handleSaveTmpl = () =>
    onRun(async () => {
      await templates.upsert(tTmpl);
      setTHasTmpl(true);
      toast.success("用量查询模板已保存");
    });

  const handleRemoveTmpl = () =>
    onRun(async () => {
      await templates.remove(providerKey);
      setTTmpl({ providerKey, method: "GET" });
      setTHasTmpl(false);
      toast.success("用量查询模板已清除");
    });

  /** 登录获取 Token：先保存模板（后端读库里的 loginUrl/提取规则）再弹登录窗 */
  const handleLoginToken = () =>
    onRun(async () => {
      if (!tTmpl.loginUrl?.trim() || !tTmpl.tokenSource?.trim()) {
        toast.error("请先填写「登录页 URL」和「Token 提取方式」并保存模板");
        return;
      }
      await templates.upsert(tTmpl);
      setTHasTmpl(true);
      await quotaToken.startLogin(providerKey);
      toast.success("登录窗口已打开，完成登录（含两步验证）后自动获取 Token");
    });

  const handleClearToken = () =>
    onRun(async () => {
      await quotaToken.clear(providerKey);
      setTToken((t) => (t ? { ...t, hasToken: false, fetchedAt: undefined } : null));
      toast.success("Token 已清除");
    });

  return (
    <div className="za-modal-overlay" onClick={onClose}>
      <div
        className="za-glass-strong za-modal"
        onClick={(e) => e.stopPropagation()}
      >
        {/* 头部 */}
        <div className="za-modal-header">
          <div className="za-row" style={{ gap: 8, alignItems: "center" }}>
            <h3 style={{ margin: 0, fontSize: "var(--fs-lg)", fontWeight: 600 }}>
              {provider.name}
            </h3>
            <span className="za-badge za-badge-neutral">{provider.kind}</span>
            {isBuiltin && (
              <span
                className="za-badge"
                style={{ background: "rgba(34,197,94,0.15)", color: "#22C55E" }}
              >
                智谱CodingPlan
              </span>
            )}
            {isCurrent && (
              <span className="za-badge" style={{ background: "var(--accent-subtle)", color: "var(--accent)" }}>
                当前
              </span>
            )}
          </div>
          <button
            className="za-icon-btn"
            style={{ width: 28, height: 28 }}
            onClick={onClose}
            title="关闭"
          >
            <IconClose width={14} height={14} />
          </button>
        </div>

        {/* 内容区 */}
        <div className="za-modal-body">
          {/* 供应商信息（置顶，不在最下方） */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              marginBottom: 10,
            }}
          >
            <span style={{ fontWeight: 600, fontSize: "var(--fs-md)" }}>
              供应商信息
            </span>
            <div className="za-row" style={{ gap: 12, alignItems: "center" }}>
              {infoLocked && (
                <span
                  className="za-muted"
                  style={{ fontSize: "var(--fs-xs)" }}
                >
                  {isBuiltin
                    ? "智谱账号供应商不可修改，仅可管理其模型"
                    : "当前供应商信息不可修改"}
                </span>
              )}
            </div>
          </div>
          <div className="za-modal-grid" style={{ marginBottom: 16 }}>
            {/* 供应商标识（provider key，只读不可修改，可复制） */}
            <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
              供应商标识
              <div className="za-row" style={{ gap: 6 }}>
                <input
                  className="za-input za-mono"
                  style={{ flex: 1 }}
                  value={providerKey}
                  readOnly
                  title="供应商标识不可修改"
                />
                <button
                  type="button"
                  className="za-btn za-btn-sm"
                  onClick={handleCopyId}
                  title="复制标识"
                >
                  {copied ? "已复制" : "复制"}
                </button>
              </div>
            </label>
            <label style={fieldStyle}>
              名称
              <input
                className="za-input"
                value={eName}
                disabled={infoLocked}
                onChange={(e) => setEName(e.target.value)}
                placeholder="Provider Name"
              />
            </label>
            <label style={fieldStyle}>
              协议
              <select
                className="za-select"
                value={eKind}
                disabled={infoLocked}
                onChange={(e) => setEKind(e.target.value as ProviderKind)}
              >
                <option value="openai-compatible">OpenAI 兼容</option>
                <option value="openai">OpenAI</option>
                <option value="anthropic">Anthropic</option>
              </select>
            </label>
            <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
              Base URL
              <input
                className="za-input za-mono"
                value={eUrl}
                disabled={infoLocked}
                onChange={(e) => setEUrl(e.target.value)}
                placeholder="https://api.example.com/v1"
              />
            </label>
            <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
              API Key
              <div className="za-row" style={{ gap: 6 }}>
                <input
                  className="za-input za-mono"
                  style={{ flex: 1 }}
                  value={apiKeyDisplay}
                  disabled={infoLocked}
                  onChange={(e) => {
                    setEKey(e.target.value);
                    setKeyTouched(true);
                    setRevealedKey(null);
                  }}
                  placeholder={provider.options.apiKey ? "••••（如需修改请输入新 Key）" : "sk-..."}
                />
                <button
                  type="button"
                  className="za-btn za-btn-sm"
                  disabled={infoLocked}
                  onClick={handleReveal}
                  title={revealedKey != null ? "隐藏 Key" : "查看明文 Key"}
                >
                  {revealedKey != null ? "隐藏" : "显示"}
                </button>
              </div>
            </label>
          </div>

          {/* 模型列表 */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              marginBottom: 8,
            }}
          >
            <div className="za-row" style={{ gap: 8, alignItems: "baseline" }}>
              <span style={{ fontWeight: 600, fontSize: "var(--fs-md)" }}>
                模型（{selModels.length}）
              </span>
              {selModels.length > 0 && (
                <span className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
                  拖动 ⠿ 手柄调整顺序
                </span>
              )}
            </div>
            <button
              className="za-btn za-btn-sm za-btn-primary"
              disabled={busy}
              onClick={handleFetch}
            >
              <IconRefresh width={13} height={13} />{" "}
              {busy ? "处理中…" : "拉取可用模型"}
            </button>
          </div>

          {/* 拉取可用模型：独立弹窗（模糊搜索 + 勾选添加）；非 null 即打开 */}
          {fetchCands && (
            <div className="za-modal-overlay" onClick={closeFetchModal}>
              <div
                className="za-glass-strong za-modal"
                style={{ maxWidth: 540 }}
                onClick={(e) => e.stopPropagation()}
              >
                {/* 头部 */}
                <div className="za-modal-header">
                  <div className="za-row" style={{ gap: 8, alignItems: "center" }}>
                    <h3
                      style={{
                        margin: 0,
                        fontSize: "var(--fs-lg)",
                        fontWeight: 600,
                      }}
                    >
                      选择要添加的模型
                    </h3>
                    <span
                      className="za-muted"
                      style={{ fontSize: "var(--fs-xs)" }}
                    >
                      已选 {fetchSel.size} / 匹配 {filteredCands.length}（共{" "}
                      {fetchCands.length}）
                    </span>
                  </div>
                  <button
                    className="za-icon-btn"
                    style={{ width: 28, height: 28 }}
                    onClick={closeFetchModal}
                    title="关闭"
                  >
                    <IconClose width={14} height={14} />
                  </button>
                </div>

                <div className="za-modal-body">
                  {/* 模糊搜索 + 批量选择 */}
                  <div
                    className="za-row-between"
                    style={{ gap: 8, marginBottom: 8, flexWrap: "wrap" }}
                  >
                    <input
                      className="za-input za-mono"
                      style={{ flex: 1, minWidth: 200 }}
                      value={fetchQuery}
                      onChange={(e) => setFetchQuery(e.target.value)}
                      placeholder="输入关键词模糊过滤，如 glm-4.7"
                      autoFocus
                    />
                    <div className="za-row" style={{ gap: 6 }}>
                      <button
                        className="za-btn za-btn-sm"
                        onClick={() =>
                          setFetchSel((prev) => {
                            // 全选当前过滤结果（与已选合并）
                            const next = new Set(prev);
                            for (const s of filteredCands) next.add(s.id);
                            return next;
                          })
                        }
                        disabled={filteredCands.length === 0}
                      >
                        全选匹配
                      </button>
                      <button
                        className="za-btn za-btn-sm"
                        onClick={() => setFetchSel(new Set())}
                      >
                        清空
                      </button>
                    </div>
                  </div>

                  {/* 候选列表 */}
                  <div
                    style={{
                      display: "flex",
                      flexDirection: "column",
                      gap: 4,
                      maxHeight: 320,
                      overflowY: "auto",
                    }}
                  >
                    {filteredCands.map((s) => {
                      const exists = Object.prototype.hasOwnProperty.call(
                        provider.models,
                        s.id
                      );
                      const checked = fetchSel.has(s.id);
                      return (
                        <label
                          key={s.id}
                          className="za-row"
                          style={{
                            gap: 8,
                            padding: "4px 8px",
                            borderRadius: 6,
                            cursor: "pointer",
                            alignItems: "center",
                            background: checked
                              ? "var(--accent-subtle)"
                              : "transparent",
                            opacity: exists ? 0.6 : 1,
                          }}
                        >
                          <input
                            type="checkbox"
                            checked={checked}
                            onChange={(e) =>
                              setFetchSel((prev) => {
                                const next = new Set(prev);
                                if (e.target.checked) next.add(s.id);
                                else next.delete(s.id);
                                return next;
                              })
                            }
                          />
                          <span
                            className="za-mono"
                            style={{ fontSize: "var(--fs-sm)" }}
                          >
                            {s.id}
                          </span>
                          {s.contextLength && (
                            <span
                              className="za-muted"
                              style={{ fontSize: "var(--fs-xs)" }}
                            >
                              ctx {s.contextLength.toLocaleString()}
                            </span>
                          )}
                          {exists && (
                            <span className="za-badge za-badge-neutral">
                              已存在
                            </span>
                          )}
                        </label>
                      );
                    })}
                    {filteredCands.length === 0 && (
                      <div className="za-empty">无匹配模型，换个关键词试试</div>
                    )}
                  </div>
                </div>

                {/* 底部操作栏 */}
                <div className="za-modal-footer">
                  <button className="za-btn za-btn-sm" onClick={closeFetchModal}>
                    取消
                  </button>
                  <button
                    className="za-btn za-btn-sm za-btn-primary"
                    disabled={busy || fetchSel.size === 0}
                    onClick={handleApplySelected}
                  >
                    添加选中（{fetchSel.size}）
                  </button>
                </div>
              </div>
            </div>
          )}

          {selModels.length === 0 ? (
            <div className="za-empty">
              无模型，点击「拉取可用模型」自动填充上下文长度
            </div>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
              {selModels.map(([name, m], idx) => (
                <div
                  key={name}
                  draggable
                  onDragStart={(e) => {
                    // 只有手柄按下时才允许拖拽，否则阻止（避免误触输入框选中等操作）
                    if (!modelGripRef.current) {
                      e.preventDefault();
                      return;
                    }
                    setDragModelIdx(idx);
                  }}
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={() => handleModelReorder(idx)}
                  onDragEnd={() => {
                    modelGripRef.current = false;
                    setDragModelIdx(null);
                  }}
                  style={{
                    display: "flex",
                    gap: 6,
                    alignItems: "center",
                    opacity: dragModelIdx === idx ? 0.5 : 1,
                  }}
                >
                  <span
                    onMouseDown={() => {
                      modelGripRef.current = true;
                    }}
                    onMouseUp={() => {
                      modelGripRef.current = false;
                    }}
                    style={{
                      cursor: "grab",
                      color: "var(--text-tertiary)",
                      fontSize: "var(--fs-md)",
                      lineHeight: 1,
                      userSelect: "none",
                      flexShrink: 0,
                    }}
                    title="拖动调整模型顺序"
                  >
                    ⠿
                  </span>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <ModalModelRow
                      name={name}
                      model={m}
                      busy={busy}
                      onSave={(c, o) =>
                        onRun(async () => {
                          await models.updateModelLimit(providerKey, name, c, o);
                          toast.success(`模型「${name}」已保存`);
                          toast.warning(RESTART_HINT);
                        })
                      }
                      onToggleEnabled={(v) =>
                        onRun(async () => {
                          await models.setModelEnabled(providerKey, name, v);
                        })
                      }
                      onDelete={() =>
                        onRun(async () => {
                          await models.removeModel(providerKey, name);
                        })
                      }
                    />
                  </div>
                </div>
              ))}
            </div>
          )}

          {/* 用量查询模板：Token Plan 供应商显示自动查询面板（团队/火山附加凭据可选），
              其余显示通用模板表单；置于模型列表之后，模型管理在前、模板配置在后 */}
          <div
            className="za-panel"
            style={{ padding: 12, marginBottom: 12 }}
          >
            <div
              className="za-row-between"
              style={{ marginBottom: 8 }}
            >
              <div className="za-row" style={{ gap: 8 }}>
                <span
                  style={{ fontWeight: 600, fontSize: "var(--fs-md)" }}
                >
                  用量查询模板
                </span>
                {codingPlan
                  ? (tExtra.organizationId || tExtra.accessKeyId) && (
                      <span
                        className="za-badge"
                        style={{
                          background: "var(--accent-subtle)",
                          color: "var(--accent)",
                        }}
                      >
                        附加凭据已配置
                      </span>
                    )
                  : tHasTmpl && (
                      <span
                        className="za-badge"
                        style={{
                          background: "var(--accent-subtle)",
                          color: "var(--accent)",
                        }}
                      >
                        已配置
                      </span>
                    )}
              </div>
              <div className="za-row" style={{ gap: 6 }}>
                {/* 使用模板：内置预设（Token Plan 6 家 + 余额 5 家）+ 已保存模板一键复制内容 */}
                <select
                  className="za-select"
                  style={{ maxWidth: 190, fontSize: "var(--fs-xs)" }}
                  value=""
                  onChange={(e) => {
                    if (e.target.value) handleUseTmpl(e.target.value);
                  }}
                  title="选择模板（含内置预设），一键复制内容到当前表单"
                >
                  <option value="">使用模板…</option>
                  {builtinCpTmpls.length > 0 && (
                    <optgroup label="Token Plan 额度（内置，自动查询）">
                      {builtinCpTmpls.map((t) => (
                        <option key={t.providerKey} value={t.providerKey}>
                          {t.name?.trim() || t.providerKey}
                        </option>
                      ))}
                    </optgroup>
                  )}
                  {builtinBalTmpls.length > 0 && (
                    <optgroup label="余额查询（内置）">
                      {builtinBalTmpls.map((t) => (
                        <option key={t.providerKey} value={t.providerKey}>
                          {t.name?.trim() || t.providerKey}
                        </option>
                      ))}
                    </optgroup>
                  )}
                  {allTmpls.filter((t) => t.providerKey !== providerKey).length > 0 && (
                    <optgroup label="我的模板">
                      {allTmpls
                        .filter((t) => t.providerKey !== providerKey)
                        .map((t) => (
                          <option key={t.providerKey} value={t.providerKey}>
                            {t.name?.trim() || t.providerKey}
                          </option>
                        ))}
                    </optgroup>
                  )}
                </select>
                  {tHasTmpl && (
                    <button
                      type="button"
                      className="za-btn za-btn-sm"
                      onClick={handleRemoveTmpl}
                    >
                      清除
                    </button>
                  )}
                  <button
                    type="button"
                    className="za-btn za-btn-sm za-btn-primary"
                    onClick={handleSaveTmpl}
                  >
                    保存模板
                  </button>
                </div>
              </div>
              {codingPlan ? (
                <>
                  {/* ===== Token Plan 供应商：自动查询，无需配置模板 ===== */}
                  <div
                    style={{
                      display: "flex",
                      gap: 8,
                      alignItems: "flex-start",
                      padding: "8px 10px",
                      borderRadius: 8,
                      background: "rgba(34,197,94,0.08)",
                      border: "1px solid rgba(34,197,94,0.25)",
                    }}
                  >
                    <span
                      style={{ color: "#22C55E", lineHeight: "20px" }}
                    >
                      ✓
                    </span>
                    <p
                      style={{
                        margin: 0,
                        fontSize: "var(--fs-sm)",
                        lineHeight: 1.6,
                      }}
                    >
                      已内置 <b>{codingPlan.label}</b> 的 Token Plan 额度查询：
                      自动使用该供应商的 API Key 与 Base URL，无需配置模板（与 cc-switch 一致）。
                    </p>
                  </div>
                  {/* 智谱团队版：填写组织/项目 ID 后按团队接口查询 */}
                  {codingPlan.id === "zhipu" && (
                    <div style={credBoxStyle}>
                      <div
                        className="za-row-between"
                        style={{ marginBottom: 8, gap: 8 }}
                      >
                        <div className="za-row" style={{ gap: 8, alignItems: "center" }}>
                          <span
                            style={{
                              fontWeight: 600,
                              fontSize: "var(--fs-sm)",
                            }}
                          >
                            智谱团队版（可选）
                          </span>
                          {tExtra.organizationId && (
                            <span
                              className="za-badge"
                              style={{
                                background: "var(--accent-subtle)",
                                color: "var(--accent)",
                              }}
                            >
                              团队版已配置
                            </span>
                          )}
                        </div>
                        {/* 凭据就近保存：不用去面板顶部找「保存模板」 */}
                        <button
                          type="button"
                          className="za-btn za-btn-sm za-btn-primary"
                          disabled={busy}
                          onClick={handleSaveTmpl}
                          title="保存组织 / 项目 ID 到本机（写入模板附加凭据）"
                        >
                          保存
                        </button>
                      </div>
                      <div className="za-grid za-grid-2" style={{ gap: 8 }}>
                        <label style={fieldStyle}>
                          组织 ID（organizationId）
                          <input
                            className="za-input za-mono"
                            value={tExtra.organizationId ?? ""}
                            onChange={(e) =>
                              setExtra("organizationId", e.target.value)
                            }
                            placeholder="bigmodel 团队组织 ID"
                          />
                        </label>
                        <label style={fieldStyle}>
                          项目 ID（projectId）
                          <input
                            className="za-input za-mono"
                            value={tExtra.projectId ?? ""}
                            onChange={(e) =>
                              setExtra("projectId", e.target.value)
                            }
                            placeholder="bigmodel 团队项目 ID"
                          />
                        </label>
                      </div>
                      <p
                        className="za-faint"
                        style={{ margin: "6px 0 0", fontSize: "var(--fs-xs)" }}
                      >
                        填写并点本框「保存」后按团队接口（type=2 +
                        bigmodel-organization/project 头）查询；留空走个人版接口。
                      </p>
                    </div>
                  )}
                  {/* 火山方舟：控制面 OpenAPI 需账号级 AK/SK 签名 */}
                  {codingPlan.id === "volcengine" && (
                    <div style={credBoxStyle}>
                      <div
                        className="za-row-between"
                        style={{ marginBottom: 8, gap: 8 }}
                      >
                        <div className="za-row" style={{ gap: 8, alignItems: "center" }}>
                          <span
                            style={{
                              fontWeight: 600,
                              fontSize: "var(--fs-sm)",
                            }}
                          >
                            火山方舟 AccessKey（必需）
                          </span>
                          {tExtra.accessKeyId && (
                            <span
                              className="za-badge"
                              style={{
                                background: "var(--accent-subtle)",
                                color: "var(--accent)",
                              }}
                            >
                              AK/SK 已配置
                            </span>
                          )}
                        </div>
                        {/* 凭据就近保存：不用去面板顶部找「保存模板」 */}
                        <button
                          type="button"
                          className="za-btn za-btn-sm za-btn-primary"
                          disabled={busy}
                          onClick={handleSaveTmpl}
                          title="保存 AK/SK 到本机（写入模板附加凭据）"
                        >
                          保存
                        </button>
                      </div>
                      <div className="za-grid za-grid-2" style={{ gap: 8 }}>
                        <label style={fieldStyle}>
                          AccessKeyId
                          <input
                            className="za-input za-mono"
                            value={tExtra.accessKeyId ?? ""}
                            onChange={(e) =>
                              setExtra("accessKeyId", e.target.value)
                            }
                            placeholder="火山控制台 IAM AccessKey ID"
                          />
                        </label>
                        <label style={fieldStyle}>
                          SecretAccessKey
                          <input
                            type="password"
                            className="za-input za-mono"
                            value={tExtra.secretAccessKey ?? ""}
                            onChange={(e) =>
                              setExtra("secretAccessKey", e.target.value)
                            }
                            placeholder="火山控制台 IAM Secret Access Key"
                            autoComplete="new-password"
                          />
                        </label>
                      </div>
                      <p
                        className="za-faint"
                        style={{ margin: "6px 0 0", fontSize: "var(--fs-xs)" }}
                      >
                        火山用量查询需账号级 AccessKey ID / Secret（与推理 API Key
                        不同），填写后点本框「保存」保存到本机。请在火山引擎控制台右上角账号菜单
                        →「API访问密钥」中创建。
                        <br />
                        密钥创建地址：{" "}
                        <a
                          href="https://console.volcengine.com/iam/keymanage"
                          onClick={(e) => {
                            e.preventDefault();
                            openUrl("https://console.volcengine.com/iam/keymanage");
                          }}
                          style={{ color: "var(--accent)" }}
                        >
                          https://console.volcengine.com/iam/keymanage
                        </a>
                      </p>
                    </div>
                  )}
                </>
              ) : (
                <>
              {/* 用量查询方式：appkey=API Key（默认）| token=登录会话 Token（展开获取块） */}
              <div
                className="za-row"
                style={{ gap: 8, alignItems: "center", marginBottom: 10 }}
              >
                <span style={{ fontSize: "var(--fs-sm)", color: "var(--text-secondary)" }}>
                  用量查询方式
                </span>
                {(
                  [
                    { v: "appkey", label: "API Key" },
                    { v: "token", label: "登录 Token" },
                  ] as const
                ).map((o) => (
                  <button
                    key={o.v}
                    type="button"
                    className="za-btn za-btn-sm"
                    onClick={() => setTTmpl({ ...tTmpl, authMode: o.v })}
                    style={
                      (tTmpl.authMode ?? "appkey") === o.v
                        ? { borderColor: "var(--accent)", color: "var(--accent)" }
                        : undefined
                    }
                  >
                    {o.label}
                  </button>
                ))}
                <span className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
                  {isTokenMode
                    ? "用 {{token}} 引用登录获取的会话 Token"
                    : "用 {{apiKey}}/{{baseURL}} 引用供应商 API Key"}
                </span>
              </div>
              <p
                className="za-muted"
                style={{ fontSize: "var(--fs-xs)", margin: "0 0 8px" }}
              >
                按 dot path（如 <span className="za-mono">data.balance</span>）提取
                总额/已用/剩余。
              </p>
              <div className="za-grid za-grid-2" style={{ gap: 8 }}>
                <label style={fieldStyle}>
                  名称
                  <input
                    className="za-input"
                    value={tTmpl.name ?? ""}
                    onChange={(e) =>
                      setTTmpl({ ...tTmpl, name: e.target.value })
                    }
                  />
                </label>
                <label style={fieldStyle}>
                  方法
                  <select
                    className="za-select"
                    value={tTmpl.method ?? "GET"}
                    onChange={(e) =>
                      setTTmpl({ ...tTmpl, method: e.target.value })
                    }
                  >
                    <option value="GET">GET</option>
                    <option value="POST">POST</option>
                  </select>
                </label>
                <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
                  URL
                  <input
                    className="za-input za-mono"
                    value={tTmpl.url ?? ""}
                    onChange={(e) =>
                      setTTmpl({ ...tTmpl, url: e.target.value })
                    }
                    placeholder="{{baseURL}}/dashboard/billing/credit_grants"
                  />
                </label>
                <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
                  Headers (JSON，值支持 {"{{apiKey}}/{{token}}"})
                  <textarea
                    className="za-textarea za-mono"
                    value={tTmpl.headersJson ?? ""}
                    onChange={(e) =>
                      setTTmpl({ ...tTmpl, headersJson: e.target.value })
                    }
                    placeholder='{"Authorization":"Bearer {{token}}"}'
                  />
                </label>
                <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
                  Body（POST 时使用，JSON 字符串）
                  <textarea
                    className="za-textarea za-mono"
                    value={tTmpl.body ?? ""}
                    onChange={(e) =>
                      setTTmpl({ ...tTmpl, body: e.target.value })
                    }
                  />
                </label>
                <label style={fieldStyle}>
                  总额 path
                  <input
                    className="za-input za-mono"
                    value={tTmpl.totalPath ?? ""}
                    onChange={(e) =>
                      setTTmpl({ ...tTmpl, totalPath: e.target.value })
                    }
                    placeholder="data.total"
                  />
                </label>
                <label style={fieldStyle}>
                  已用 path
                  <input
                    className="za-input za-mono"
                    value={tTmpl.usedPath ?? ""}
                    onChange={(e) =>
                      setTTmpl({ ...tTmpl, usedPath: e.target.value })
                    }
                    placeholder="data.used"
                  />
                </label>
                <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
                  剩余 path
                  <input
                    className="za-input za-mono"
                    value={tTmpl.remainingPath ?? ""}
                    onChange={(e) =>
                      setTTmpl({ ...tTmpl, remainingPath: e.target.value })
                    }
                    placeholder="data.remaining"
                  />
                </label>
              </div>
              <p
                className="za-faint"
                style={{ margin: "10px 0 6px", fontSize: "var(--fs-xs)" }}
              >
                月限额（可选，仅部分供应商有）：从同一响应里再提取一组「每月使用额度」，
                额度行会追加「每月 剩xx%」；未配置或提取不到则不展示。
              </p>
              <div className="za-grid za-grid-2" style={{ gap: 8 }}>
                <label style={fieldStyle}>
                  每月总额 path
                  <input
                    className="za-input za-mono"
                    value={tTmpl.monthlyTotalPath ?? ""}
                    onChange={(e) =>
                      setTTmpl({ ...tTmpl, monthlyTotalPath: e.target.value })
                    }
                    placeholder="data.monthly_total"
                  />
                </label>
                <label style={fieldStyle}>
                  每月已用 path
                  <input
                    className="za-input za-mono"
                    value={tTmpl.monthlyUsedPath ?? ""}
                    onChange={(e) =>
                      setTTmpl({ ...tTmpl, monthlyUsedPath: e.target.value })
                    }
                    placeholder="data.monthly_used"
                  />
                </label>
                <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
                  每月剩余 path
                  <input
                    className="za-input za-mono"
                    value={tTmpl.monthlyRemainingPath ?? ""}
                    onChange={(e) =>
                      setTTmpl({ ...tTmpl, monthlyRemainingPath: e.target.value })
                    }
                    placeholder="data.monthly_remaining"
                  />
                </label>
              </div>

              {/* ===== Token 获取（仅「登录 Token」方式显示）：
                  弹登录窗自动填账号密码 → 完成登录/2FA → 提取 Token 存系统凭证库 ===== */}
              {isTokenMode && (
                <div
                  style={{
                    marginTop: 10,
                    padding: 10,
                    borderRadius: 8,
                    border: "1px solid var(--glass-border)",
                  }}
                >
                  <div
                    className="za-row-between"
                    style={{ marginBottom: 8, gap: 8, flexWrap: "wrap" }}
                  >
                    <span
                      style={{ fontWeight: 600, fontSize: "var(--fs-sm)" }}
                    >
                      Token 获取
                    </span>
                    <span style={{ fontSize: "var(--fs-sm)" }}>
                      Token：
                      {tToken?.hasToken ? (
                        <span style={{ color: "var(--success)" }}>
                          已获取
                          {tToken.fetchedAt
                            ? `（${new Date(tToken.fetchedAt).toLocaleString()}）`
                            : ""}
                        </span>
                      ) : (
                        <span className="za-faint">未获取</span>
                      )}
                    </span>
                  </div>
                  {/* 最终获取到的 token：默认掩码，点「显示」核对明文（可复制） */}
                  {tToken?.hasToken && (
                    <div
                      className="za-row"
                      style={{ gap: 6, marginBottom: 8, alignItems: "center" }}
                    >
                      <div
                        className="za-mono"
                        style={{
                          flex: 1,
                          minWidth: 0,
                          height: 28,
                          display: "flex",
                          alignItems: "center",
                          padding: "0 8px",
                          borderRadius: 6,
                          border: "1px solid var(--glass-border)",
                          fontSize: "var(--fs-xs)",
                          color:
                            revealedToken != null
                              ? "var(--text-primary)"
                              : "var(--text-tertiary)",
                          whiteSpace: "nowrap",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                        }}
                        title="最终获取到的 Token"
                      >
                        {revealedToken != null
                          ? revealedToken || "（空）"
                          : "••••••••••••"}
                      </div>
                      <button
                        type="button"
                        className="za-btn za-btn-sm"
                        onClick={handleRevealToken}
                        title={revealedToken != null ? "隐藏 Token" : "显示明文 Token"}
                      >
                        {revealedToken != null ? "隐藏" : "显示"}
                      </button>
                      {revealedToken != null && revealedToken && (
                        <button
                          type="button"
                          className="za-btn za-btn-sm"
                          onClick={handleCopyToken}
                          title="复制 Token"
                        >
                          {tokenCopied ? "已复制" : "复制"}
                        </button>
                      )}
                    </div>
                  )}
                  <div className="za-grid za-grid-2" style={{ gap: 8 }}>
                    <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
                      登录页 URL
                      <input
                        className="za-input"
                        value={tTmpl.loginUrl ?? ""}
                        onChange={(e) =>
                          setTTmpl({ ...tTmpl, loginUrl: e.target.value })
                        }
                        placeholder="https://platform.example.com/login"
                      />
                    </label>
                    <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
                      Token 提取方式
                      <input
                        className="za-input za-mono"
                        value={tTmpl.tokenSource ?? ""}
                        onChange={(e) =>
                          setTTmpl({ ...tTmpl, tokenSource: e.target.value })
                        }
                        placeholder="cookie:session_id 或 localstorage:user#token"
                      />
                    </label>
                  </div>
                  <div
                    className="za-row"
                    style={{ gap: 8, marginTop: 8, justifyContent: "flex-end" }}
                  >
                    {tToken?.hasToken && (
                      <button
                        type="button"
                        className="za-btn za-btn-sm"
                        onClick={handleClearToken}
                      >
                        清除 Token
                      </button>
                    )}
                    <button
                      type="button"
                      className="za-btn za-btn-sm"
                      onClick={handleLoginToken}
                    >
                      登录获取 Token
                    </button>
                  </div>
                  <p
                    className="za-faint"
                    style={{ margin: "8px 0 0", fontSize: "var(--fs-xs)" }}
                  >
                    点「登录获取 Token」弹出该平台登录页，在登录窗中完成登录
                    （含验证码 / 两步验证）即可；成功后自动提取 Token 存入系统凭证库，
                    模板中用 <span className="za-mono">{"{{token}}"}</span> 引用。提取方式：{" "}
                    <span className="za-mono">cookie:名称</span>（Windows 下支持 HttpOnly
                    cookie）或{" "}
                    <span className="za-mono">localstorage:key</span>
                    （值为 JSON 加 <span className="za-mono">#字段.路径</span>）。
                  </p>
                </div>
              )}
                </>
              )}
            </div>
        </div>

        {/* 底部操作栏 */}
        <div className="za-modal-footer">
          <button className="za-btn za-btn-sm" onClick={onClose}>
            关闭
          </button>
          {!isBuiltin && (
            <button
              className="za-btn za-btn-sm za-btn-primary"
              disabled={busy || isCurrent}
              onClick={handleSaveProvider}
            >
              保存供应商
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/* ============ 导入预览弹窗：列出解析出的全部供应商，默认全选，确认后导入选中项 ============ */
function ImportPreviewModal({
  items,
  busy,
  onClose,
  onConfirm,
}: {
  items: ProviderPreview[];
  busy: boolean;
  onClose: () => void;
  onConfirm: (ids: string[]) => void;
}) {
  // 默认全选
  const [sel, setSel] = useState<Set<string>>(
    () => new Set(items.map((i) => i.id))
  );
  const all = sel.size === items.length;
  const allIds = () => new Set(items.map((i) => i.id));
  const toggle = (id: string, on: boolean) =>
    setSel((prev) => {
      const next = new Set(prev);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });

  return (
    <div className="za-modal-overlay" onClick={onClose}>
      <div
        className="za-glass-strong za-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="za-modal-header">
          <div className="za-row" style={{ gap: 8, alignItems: "center" }}>
            <h3 style={{ margin: 0, fontSize: "var(--fs-lg)", fontWeight: 600 }}>
              导入预览
            </h3>
            <span className="za-muted" style={{ fontSize: "var(--fs-sm)" }}>
              共解析到 {items.length} 个供应商
            </span>
          </div>
          <button
            className="za-icon-btn"
            style={{ width: 28, height: 28 }}
            onClick={onClose}
            title="关闭"
          >
            <IconClose width={14} height={14} />
          </button>
        </div>

        <div className="za-modal-body">
          <div className="za-row" style={{ gap: 8, marginBottom: 8 }}>
            <button
              className="za-btn za-btn-sm"
              onClick={() => setSel(all ? new Set() : allIds())}
            >
              {all ? "取消全选" : "全选"}
            </button>
            <span className="za-muted" style={{ fontSize: "var(--fs-sm)" }}>
              已选 {sel.size} / {items.length}
            </span>
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            {items.map((it) => {
              const checked = sel.has(it.id);
              return (
                <label
                  key={it.id}
                  style={{
                    display: "flex",
                    gap: 8,
                    alignItems: "flex-start",
                    padding: "6px 8px",
                    borderRadius: 6,
                    cursor: "pointer",
                    background: checked
                      ? "var(--accent-subtle)"
                      : "transparent",
                    border: "1px solid var(--glass-border)",
                  }}
                >
                  <input
                    type="checkbox"
                    style={{ marginTop: 3, flexShrink: 0 }}
                    checked={checked}
                    onChange={(e) => toggle(it.id, e.target.checked)}
                  />
                  <div
                    style={{
                      display: "flex",
                      flexDirection: "column",
                      gap: 2,
                      minWidth: 0,
                      flex: 1,
                    }}
                  >
                    <div
                      className="za-row"
                      style={{ gap: 6, flexWrap: "wrap" }}
                    >
                      <span style={{ fontWeight: 500 }}>{it.name}</span>
                      <span className="za-badge za-badge-neutral">
                        {it.kind}
                      </span>
                      {it.duplicateOf ? (
                        <span
                          className="za-badge"
                          style={{
                            background: "rgba(245,158,11,0.15)",
                            color: "#F59E0B",
                          }}
                          title={`baseURL + apiKey 与已有供应商一致，导入将覆盖更新「${it.duplicateOf}」`}
                        >
                          覆盖 {it.duplicateOf}
                        </span>
                      ) : (
                        <span className="za-badge za-badge-new">新增</span>
                      )}
                      {!it.hasApiKey && (
                        <span
                          className="za-badge za-badge-neutral"
                          title="源配置中无 apiKey（如 Codex OAuth），仅导入 baseURL"
                        >
                          无 apiKey
                        </span>
                      )}
                    </div>
                    <span
                      className="za-mono za-faint"
                      style={{
                        fontSize: "var(--fs-xs)",
                        wordBreak: "break-all",
                      }}
                    >
                      {it.baseUrl}
                    </span>
                    <span className="za-faint" style={{ fontSize: "var(--fs-xs)" }}>
                      {it.models.length > 0
                        ? `${it.models.length} 个模型`
                        : "无模型"}
                    </span>
                  </div>
                </label>
              );
            })}
          </div>
        </div>

        <div className="za-modal-footer">
          <button className="za-btn za-btn-sm" disabled={busy} onClick={onClose}>
            取消
          </button>
          <button
            className="za-btn za-btn-sm za-btn-primary"
            disabled={busy || sel.size === 0}
            onClick={() => onConfirm([...sel])}
          >
            {busy ? "导入中…" : `导入选中（${sel.size}）`}
          </button>
        </div>
      </div>
    </div>
  );
}

/* ============ 当前供应商连接检测行（仅当前选中卡片显示）============ */
/* ============ 添加供应商弹窗（风格与编辑弹窗一致）============ */
function ProviderAddModal({
  onClose,
  onAdded,
}: {
  onClose: () => void;
  onAdded: (msg: string) => void;
}) {
  const [fId, setFId] = useState("");
  const [fName, setFName] = useState("");
  const [fKind, setFKind] = useState<ProviderKind>("openai-compatible");
  const [fUrl, setFUrl] = useState("");
  const [fKey, setFKey] = useState("");
  // 选中的预设供应商（cc-switch）：自动填充 名称/协议/Base URL/建议标识
  const [preset, setPreset] = useState<ProviderPreset | null>(null);
  // 预设自动填入的标识（标识为空或仍是上次自动值时可随预设切换覆盖，手改过则保留）
  const presetIdRef = useRef("");

  /** 应用预设：一键填充供应商名称、接口格式、Base URL 与建议标识 */
  const applyPreset = (p: ProviderPreset) => {
    setPreset(p);
    setFName(p.name);
    setFKind(p.kind);
    setFUrl(p.baseUrl);
    if (!fId.trim() || fId.trim() === presetIdRef.current) {
      setFId(p.id);
      presetIdRef.current = p.id;
    }
  };
  const [busy, setBusy] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{
    ok: boolean;
    message: string;
  } | null>(null);
  // 弹窗内顶部错误提示（校验 / 测试 / 添加失败均在此显示）
  const [error, setError] = useState<string | null>(null);

  const handleTest = async () => {
    setError(null);
    if (!fUrl.trim()) {
      setError("请先填写 Base URL");
      return;
    }
    setTesting(true);
    setTestResult(null);
    try {
      const r = await models.testConnection(fUrl, fKey, fKind);
      setTestResult({ ok: r.ok, message: r.message });
    } catch (e: unknown) {
      setTestResult({
        ok: false,
        message: typeof e === "string" ? e : "测试请求失败",
      });
    } finally {
      setTesting(false);
    }
  };

  const handleAdd = async () => {
    setError(null);
    if (!fId.trim() || !fName.trim() || !fUrl.trim()) {
      setError("请填写供应商标识、名称和 Base URL");
      return;
    }
    setBusy(true);
    try {
      await models.addProvider(fName, fKind, fUrl, fKey, fId);
      onAdded(`已添加供应商「${fName}」`);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : "添加失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="za-modal-overlay" onClick={onClose}>
      <div
        className="za-glass-strong za-modal"
        onClick={(e) => e.stopPropagation()}
      >
        {/* 头部 */}
        <div className="za-modal-header">
          <div className="za-row" style={{ gap: 8, alignItems: "center" }}>
            <h3 style={{ margin: 0, fontSize: "var(--fs-lg)", fontWeight: 600 }}>
              添加供应商
            </h3>
          </div>
          <button
            className="za-icon-btn"
            style={{ width: 28, height: 28 }}
            onClick={onClose}
            title="关闭"
          >
            <IconClose width={14} height={14} />
          </button>
        </div>

        {/* 内容区 */}
        <div className="za-modal-body">
          {/* 顶部错误提示（校验 / 添加失败） */}
          {error && (
            <div
              className="za-panel"
              style={{
                padding: "8px 12px",
                marginBottom: 12,
                borderLeft: "3px solid var(--danger)",
                color: "var(--danger)",
                fontSize: "var(--fs-sm)",
              }}
            >
              {error}
            </div>
          )}

          <div className="za-modal-grid" style={{ marginBottom: 16 }}>
            {/* 预设供应商：选 cc-switch 预设一键填充 */}
            <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
              预设供应商
              <select
                className="za-select"
                value={preset?.id ?? ""}
                onChange={(e) => {
                  const p = PRESET_PROVIDERS.find(
                    (x) => x.id === e.target.value
                  );
                  if (p) applyPreset(p);
                }}
                title="预设数据来自 cc-switch（github.com/farion1231/cc-switch，MIT），可在此基础上修改"
              >
                <option value="" disabled>
                  选择预设自动填充…
                </option>
                {PRESET_CATEGORY_LABELS.map(({ key, label }) => (
                  <optgroup key={key} label={label}>
                    {PRESET_PROVIDERS.filter((p) => p.category === key).map(
                      (p) => (
                        <option key={p.id} value={p.id}>
                          {p.name}
                        </option>
                      )
                    )}
                  </optgroup>
                ))}
              </select>
            </label>
            {/* 选中预设后展示官网 / 获取 API Key 链接（点击系统浏览器打开） */}
            {preset && (
              <div
                className="za-row"
                style={{
                  gridColumn: "1 / -1",
                  gap: 8,
                  alignItems: "center",
                  flexWrap: "wrap",
                  fontSize: "var(--fs-xs)",
                }}
              >
                <span className="za-faint">官网</span>
                <span
                  className="za-mono"
                  style={{ color: "var(--accent)", cursor: "pointer" }}
                  onClick={() => openUrl(preset.websiteUrl)}
                  title={preset.websiteUrl}
                >
                  {preset.websiteUrl}
                </span>
                {preset.apiKeyUrl && preset.apiKeyUrl !== preset.websiteUrl && (
                  <>
                    <span className="za-faint">获取 API Key</span>
                    <span
                      className="za-mono"
                      style={{ color: "var(--accent)", cursor: "pointer" }}
                      onClick={() => openUrl(preset.apiKeyUrl ?? "")}
                      title={preset.apiKeyUrl}
                    >
                      {preset.apiKeyUrl}
                    </span>
                  </>
                )}
              </div>
            )}
            <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
              供应商标识
              <input
                className="za-input za-mono"
                value={fId}
                onChange={(e) => setFId(e.target.value)}
                placeholder="如 my-glm（字母/数字/-/_，创建后不可修改）"
                autoFocus
              />
            </label>
            <label style={fieldStyle}>
              名称
              <input
                className="za-input"
                value={fName}
                onChange={(e) => setFName(e.target.value)}
                placeholder="My Provider"
              />
            </label>
            <label style={fieldStyle}>
              协议
              <select
                className="za-select"
                value={fKind}
                onChange={(e) => setFKind(e.target.value as ProviderKind)}
              >
                <option value="openai-compatible">OpenAI 兼容</option>
                <option value="openai">OpenAI</option>
                <option value="anthropic">Anthropic</option>
              </select>
            </label>
            <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
              Base URL
              <input
                className="za-input za-mono"
                value={fUrl}
                onChange={(e) => setFUrl(e.target.value)}
                placeholder="https://api.example.com/v1"
              />
            </label>
            <label style={{ ...fieldStyle, gridColumn: "1 / -1" }}>
              API Key
              <input
                className="za-input za-mono"
                value={fKey}
                onChange={(e) => setFKey(e.target.value)}
                placeholder="sk-..."
              />
            </label>
          </div>

          {/* 测试连接 */}
          <div
            className="za-row"
            style={{
              gap: 8,
              marginBottom: 12,
              alignItems: "center",
              flexWrap: "wrap",
            }}
          >
            <button
              className="za-btn za-btn-sm"
              disabled={testing}
              onClick={handleTest}
            >
              {testing ? "测试中…" : "测试连接"}
            </button>
            {testResult && (
              <span
                className="za-mono"
                style={{
                  fontSize: "var(--fs-xs)",
                  color: testResult.ok ? "var(--accent)" : "var(--danger)",
                }}
              >
                {testResult.ok ? "✓ " : "✕ "}
                {testResult.message}
              </span>
            )}
          </div>
        </div>

        {/* 底部操作栏 */}
        <div className="za-modal-footer">
          <button className="za-btn za-btn-sm" onClick={onClose}>
            取消
          </button>
          <button
            className="za-btn za-btn-sm za-btn-primary"
            disabled={busy}
            onClick={handleAdd}
          >
            {busy ? "添加中…" : "添加"}
          </button>
        </div>
      </div>
    </div>
  );
}

/* ============ 弹窗内的模型行（context 默认 200000，无删除按钮）============ */
function ModalModelRow({
  name,
  model,
  busy,
  onSave,
  onToggleEnabled,
  onDelete,
}: {
  name: string;
  model: ZcModel;
  busy: boolean;
  onSave: (ctx?: number, out?: number) => void;
  onToggleEnabled: (enabled: boolean) => void;
  onDelete: () => void;
}) {
  // 没有上下文大小时默认 200000
  const [ctx, setCtx] = useState(
    model.limit?.context?.toString() ?? String(DEFAULT_CONTEXT)
  );
  const [out, setOut] = useState(model.limit?.output?.toString() ?? "");
  const enabled = model.enabled !== false;
  return (
    <div
      className="za-row-between"
      style={{
        gap: 10,
        padding: "6px 0",
        borderBottom: "1px solid var(--glass-border)",
        opacity: enabled ? 1 : 0.5,
      }}
    >
      <span className="za-mono" style={{ fontSize: "var(--fs-sm)" }}>
        {name}
      </span>
      <div className="za-row" style={{ gap: 6, alignItems: "center" }}>
        <label
          style={{
            display: "flex",
            alignItems: "center",
            gap: 4,
            fontSize: "var(--fs-xs)",
            color: "var(--text-tertiary)",
          }}
        >
          ctx
          <input
            className="za-input za-btn-sm za-mono"
            style={{ height: 28, width: 110 }}
            value={ctx}
            onChange={(e) => setCtx(e.target.value)}
            placeholder={String(DEFAULT_CONTEXT)}
            title="上下文 token（留空默认 200000）"
          />
        </label>
        <label
          style={{
            display: "flex",
            alignItems: "center",
            gap: 4,
            fontSize: "var(--fs-xs)",
            color: "var(--text-tertiary)",
          }}
        >
          out
          <input
            className="za-input za-btn-sm za-mono"
            style={{ height: 28, width: 100 }}
            value={out}
            onChange={(e) => setOut(e.target.value)}
            placeholder="output"
            title="最大输出 token"
          />
        </label>
        <Toggle
          checked={enabled}
          title={enabled ? "已启用" : "已禁用"}
          onChange={onToggleEnabled}
        />
        <button
          className="za-btn za-btn-sm"
          disabled={busy}
          onClick={() =>
            onSave(
              ctx ? Number(ctx) : DEFAULT_CONTEXT,
              out ? Number(out) : undefined
            )
          }
        >
          保存
        </button>
        <button
          className="za-icon-btn"
          style={{ width: 28, height: 28 }}
          disabled={busy}
          onClick={onDelete}
          title="删除模型"
        >
          <IconTrash width={13} height={13} />
        </button>
      </div>
    </div>
  );
}

/** 小型开关（button 实现，无原生 checkbox） */
function Toggle({
  checked,
  onChange,
  title,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  title?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      title={title}
      className={`za-switch${checked ? " is-on" : ""}`}
      onClick={(e) => {
        e.stopPropagation();
        onChange(!checked);
      }}
    >
      <span className="za-switch-thumb" />
    </button>
  );
}
