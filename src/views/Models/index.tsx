import { useEffect, useState, type CSSProperties } from "react";
import {
  importer,
  models,
  zcode,
  quota,
  templates,
  events,
  maskApiKey,
  formatUnits,
  pickBucketByName,
  usageColor,
} from "../../api";
import type { ImportResult } from "../../api";
import {
  IconPlus,
  IconTrash,
  IconRefresh,
  IconCheck,
  IconFolder,
  IconClose,
} from "../../components/icons";
import { RestartBar } from "../../components/RestartBar";
import { DualRing } from "../../components/DualRing";
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
} from "../../types";

const fieldStyle: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 4,
  fontSize: "var(--fs-sm)",
  color: "var(--text-secondary)",
};

/** 模型上下文缺省值 */
const DEFAULT_CONTEXT = 200_000;

export default function Models() {
  const [config, setConfig] = useState<ZcodeConfig | null>(null);
  const [setting, setSetting] = useState<ZcodeSetting | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [showAddModal, setShowAddModal] = useState(false);
  // 导入配置
  const [impSource, setImpSource] = useState("opencode");
  const [impPath, setImpPath] = useState("");
  const [impResults, setImpResults] = useState<ImportResult[] | null>(null);
  const [impError, setImpError] = useState<string | null>(null);
  // 本次会话新导入的 provider key（仅内存，退出应用即清，用于 NEW 标记）
  const [newKeys, setNewKeys] = useState<Set<string>>(() => new Set());
  // 双击打开的供应商编辑弹窗 key
  const [editKey, setEditKey] = useState<string | null>(null);
  // db 标记为 Coding Plan 的 provider key（配额查询优先用其 apiKey）
  const [cpKeys, setCpKeys] = useState<Set<string>>(() => new Set());
  // 每个供应商的最新配额（key -> QuotaOverview | null），null=查询失败/未配置
  const [quotaMap, setQuotaMap] = useState<Record<string, QuotaOverview | null>>({});

  const reload = async () => {
    try {
      const [c, s, cp] = await Promise.all([
        zcode.getConfig(),
        zcode.getSetting(),
        models.listCodingPlan(),
      ]);
      setConfig(c);
      setSetting(s);
      setCpKeys(new Set(cp));
      setSelected(
        (cur) =>
          cur ??
          Object.keys(c.provider).find((k) => !k.startsWith("builtin:")) ??
          null
      );
    } catch (e: unknown) {
      setMsg(typeof e === "string" ? e : "读取 zcode 配置失败");
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

  // 每 60s 轮询每个非 builtin 供应商的配额（智谱走内置接口，其余走用量模板）。
  // 失败/未配置模板 → null，前端卡片显示「未知」/「未配置」。
  useEffect(() => {
    if (!config) return;
    const keys = Object.keys(config.provider).filter(
      (k) => !k.startsWith("builtin:")
    );
    let cancelled = false;
    const poll = async () => {
      const entries = await Promise.all(
        keys.map(async (k): Promise<[string, QuotaOverview | null]> => {
          try {
            return [k, await quota.getProviderQuota(k)];
          } catch {
            return [k, null];
          }
        })
      );
      if (!cancelled) setQuotaMap(Object.fromEntries(entries));
    };
    poll();
    const t = setInterval(poll, 60_000);
    return () => {
      cancelled = true;
      clearInterval(t);
    };
  }, [config]);

  const family = setting?.providerFamilyDomain;
  const currentKey = family
    ? setting?.modelProviderFamilySelectedKeys?.[family]
    : undefined;
  // 隐藏系统未授权的 provider 和智谱内置账号 provider（builtin: 前缀，
  // 通过登录智谱账号获取，无法在此手动管理）；用户禁用的保留显示（灰显，可重新启用）
  const providers = config
    ? Object.entries(config.provider).filter(
        ([key, p]) =>
          !p.systemDisabledReason && !key.startsWith("builtin:")
      )
    : [];

  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    setMsg(null);
    try {
      await fn();
      await reload();
    } catch (e: unknown) {
      setMsg(typeof e === "string" ? e : "操作失败");
    } finally {
      setBusy(false);
    }
  };

  const handleImport = async () => {
    setBusy(true);
    setImpError(null);
    try {
      const results = await importer.from(
        impSource,
        impPath.trim() || undefined
      );
      setImpResults(results);
      // 记录新导入的 provider key（仅 success），用于本次会话 NEW 标记
      const added = results
        .filter((r) => r.status === "success" && r.providerKey)
        .map((r) => r.providerKey);
      if (added.length > 0) {
        setNewKeys((prev) => {
          const next = new Set(prev);
          added.forEach((k) => next.add(k));
          return next;
        });
      }
      await reload();
    } catch (e: unknown) {
      setImpError(typeof e === "string" ? e : "导入失败");
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
    });

  const handleSetCurrent = (key: string) =>
    run(async () => {
      if (!family) return;
      await zcode.selectProvider(family, key);
    });

  // 弹窗中操作的 provider 数据
  const editProvider = editKey ? config?.provider[editKey] : undefined;

  return (
    <>
      <RestartBar hint="供应商 / 模型变更后需重启 zcode 生效" />
      {msg && (
        <div
          className="za-panel za-card-pad za-mono"
          style={{
            fontSize: "var(--fs-sm)",
            borderLeft: "3px solid var(--accent)",
          }}
        >
          {msg}
        </div>
      )}

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
        {impError && (
          <div
            className="za-mono"
            style={{
              marginTop: 10,
              fontSize: "var(--fs-sm)",
              color: "#ef4444",
            }}
          >
            ✕ {impError}
          </div>
        )}
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
          单击选中，双击打开编辑弹窗
        </p>

        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {providers.length === 0 && (
            <div className="za-empty">未读取到 provider（请确认 zcode 已配置）</div>
          )}
          {providers.map(([key, p]) => {
            const enabled = p.enabled !== false;
            const isCurrent = currentKey === key;
            return (
              <div
                key={key}
                style={{
                  padding: "8px 12px",
                  borderRadius: 8,
                  cursor: "pointer",
                  background:
                    selected === key ? "var(--accent-subtle)" : "transparent",
                  border: "1px solid var(--glass-border)",
                  opacity: enabled ? 1 : 0.5,
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
                    <span className="za-badge za-badge-neutral">{p.kind}</span>
                    {p.source === "custom" && (
                      <span className="za-badge">自定义</span>
                    )}
                  </div>
                  <div className="za-row" style={{ gap: 8 }}>
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

      {/* 双击供应商 → 编辑弹窗 */}
      {editKey && editProvider && (
        <ProviderEditModal
          providerKey={editKey}
          provider={editProvider}
          isCurrent={currentKey === editKey}
          family={family}
          busy={busy}
          isCodingPlan={cpKeys.has(editKey)}
          onToggleCodingPlan={(v) => {
            models
              .setCodingPlan(editKey, v)
              .catch((e) =>
                setMsg(typeof e === "string" ? e : "标记 Coding Plan 失败")
              );
            setCpKeys((prev) => {
              const next = new Set(prev);
              if (v) next.add(editKey);
              else next.delete(editKey);
              return next;
            });
          }}
          onClose={() => setEditKey(null)}
          onSetCurrent={() => handleSetCurrent(editKey)}
          onRun={run}
          onMsg={setMsg}
        />
      )}

      {showAddModal && (
        <ProviderAddModal
          onClose={() => setShowAddModal(false)}
          onAdded={(m) => {
            setShowAddModal(false);
            setMsg(m);
            reload();
          }}
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
  // 智谱 BigModel：5H + 每周
  if (b5 || bW) {
    const u5 = b5 && b5.total > 0 ? (b5.used / b5.total) * 100 : null;
    const uW = bW && bW.total > 0 ? (bW.used / bW.total) * 100 : null;
    const summary =
      b5 && bW
        ? `5H 剩${Math.round(b5.remaining)}% · 每周 剩${Math.round(
            bW.remaining
          )}%`
        : b5
          ? `5H 剩${Math.round(b5.remaining)}%`
          : bW
            ? `每周 剩${Math.round(bW.remaining)}%`
            : "无额度数据";
    const ok = (b5?.remaining ?? 0) > 0 && (bW?.remaining ?? 0) > 0;
    const reset = b5?.periodEnd || bW?.periodEnd;
    return (
      <div
        className="za-row"
        style={{ gap: 8, alignItems: "center", paddingLeft: 22 }}
      >
        <DualRing size={28} usedOuter={u5} usedInner={uW} />
        <span className="za-mono" style={{ fontSize: "var(--fs-xs)" }}>
          {summary}
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
      b.unit === "%"
        ? `剩 ${Math.round(b.remaining)}%`
        : `剩 ${formatUnits(b.remaining)}`;
    const ok = b.remaining > 0;
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
  family,
  busy,
  isCodingPlan,
  onToggleCodingPlan,
  onClose,
  onSetCurrent,
  onRun,
  onMsg,
}: {
  providerKey: string;
  provider: ZcProvider;
  isCurrent: boolean;
  family?: string;
  busy: boolean;
  isCodingPlan: boolean;
  onToggleCodingPlan: (enabled: boolean) => void;
  onClose: () => void;
  onSetCurrent: () => void;
  onRun: (fn: () => Promise<void>) => Promise<void>;
  onMsg: (m: string | null) => void;
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

  // 拉取可用模型的候选
  const [fetchCands, setFetchCands] = useState<ModelSpec[] | null>(null);
  const [fetchSel, setFetchSel] = useState<Set<string>>(() => new Set());

  // 用量查询模板（仅非智谱 BigModel 显示配置区，智谱走内置配额接口）
  const [tTmpl, setTTmpl] = useState<QuotaTemplate>({ providerKey, method: "GET" });
  const [tHasTmpl, setTHasTmpl] = useState(false);
  const isBigmodelModal = (provider.options.baseURL ?? "")
    .toLowerCase()
    .includes("bigmodel");

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
      onMsg("供应商信息已保存");
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
    });

  const handleApplySelected = () =>
    onRun(async () => {
      if (!fetchCands) return;
      const picked = fetchCands.filter((s) => fetchSel.has(s.id));
      if (picked.length === 0) {
        setFetchCands(null);
        setFetchSel(new Set());
        return;
      }
      const n = await models.applyModels(providerKey, picked);
      onMsg(`已写入 ${n} 个模型到 config.json`);
      setFetchCands(null);
      setFetchSel(new Set());
    });

  // 加载用量查询模板（仅非智谱 BigModel；模板区在 JSX 中按 isBigmodelModal 隐藏）
  useEffect(() => {
    if (isBigmodelModal) return;
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
  }, [providerKey, isBigmodelModal]);

  const handleSaveTmpl = () =>
    onRun(async () => {
      await templates.upsert(tTmpl);
      setTHasTmpl(true);
      onMsg("用量查询模板已保存");
    });

  const handleRemoveTmpl = () =>
    onRun(async () => {
      await templates.remove(providerKey);
      setTTmpl({ providerKey, method: "GET" });
      setTHasTmpl(false);
      onMsg("用量查询模板已清除");
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
              {isCurrent && (
                <span
                  className="za-muted"
                  style={{ fontSize: "var(--fs-xs)" }}
                >
                  当前供应商信息不可修改
                </span>
              )}
              <label
                className="za-row"
                style={{
                  gap: 6,
                  alignItems: "center",
                  cursor: "pointer",
                  fontSize: "var(--fs-xs)",
                  color: "var(--text-secondary)",
                }}
                title="标记此供应商为智谱 Coding Plan 订阅，配额查询优先使用其 API Key"
              >
                <input
                  type="checkbox"
                  checked={isCodingPlan}
                  onChange={(e) => onToggleCodingPlan(e.target.checked)}
                />
                Coding Plan 订阅
              </label>
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
                disabled={isCurrent}
                onChange={(e) => setEName(e.target.value)}
                placeholder="Provider Name"
              />
            </label>
            <label style={fieldStyle}>
              协议
              <select
                className="za-select"
                value={eKind}
                disabled={isCurrent}
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
                disabled={isCurrent}
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
                  disabled={isCurrent}
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
                  disabled={isCurrent}
                  onClick={handleReveal}
                  title={revealedKey != null ? "隐藏 Key" : "查看明文 Key"}
                >
                  {revealedKey != null ? "隐藏" : "显示"}
                </button>
              </div>
            </label>
          </div>

          {/* 用量查询模板（仅非智谱 BigModel 显示，智谱走内置配额接口） */}
          {!isBigmodelModal && (
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
                  {tHasTmpl && (
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
              <p
                className="za-muted"
                style={{ fontSize: "var(--fs-xs)", margin: "0 0 8px" }}
              >
                URL 支持 <span className="za-mono">{"{{apiKey}}/{{baseURL}}"}</span>{" "}
                占位；按 dot path（如 <span className="za-mono">data.balance</span>）
                提取总额/已用/剩余。
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
                  Headers (JSON，值支持 {"{{apiKey}}"})
                  <textarea
                    className="za-textarea za-mono"
                    value={tTmpl.headersJson ?? ""}
                    onChange={(e) =>
                      setTTmpl({ ...tTmpl, headersJson: e.target.value })
                    }
                    placeholder='{"Authorization":"Bearer {{apiKey}}"}'
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
            </div>
          )}

          {/* 模型列表 */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              marginBottom: 8,
            }}
          >
            <span style={{ fontWeight: 600, fontSize: "var(--fs-md)" }}>
              模型（{selModels.length}）
            </span>
            <button
              className="za-btn za-btn-sm za-btn-primary"
              disabled={busy}
              onClick={handleFetch}
            >
              <IconRefresh width={13} height={13} />{" "}
              {busy ? "处理中…" : "拉取可用模型"}
            </button>
          </div>

          {fetchCands && (
            <div
              className="za-panel"
              style={{ padding: 12, marginBottom: 10 }}
            >
              <div className="za-row-between" style={{ marginBottom: 8 }}>
                <div className="za-row" style={{ gap: 8 }}>
                  <span style={{ fontWeight: 500 }}>选择要添加的模型</span>
                  <span
                    className="za-muted"
                    style={{ fontSize: "var(--fs-xs)" }}
                  >
                    已选 {fetchSel.size} / {fetchCands.length}
                  </span>
                </div>
                <div className="za-row" style={{ gap: 6 }}>
                  <button
                    className="za-btn za-btn-sm"
                    onClick={() =>
                      setFetchSel(new Set(fetchCands.map((s) => s.id)))
                    }
                  >
                    全选
                  </button>
                  <button
                    className="za-btn za-btn-sm"
                    onClick={() => setFetchSel(new Set())}
                  >
                    清空
                  </button>
                </div>
              </div>
              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: 4,
                  maxHeight: 200,
                  overflowY: "auto",
                }}
              >
                {fetchCands.map((s) => {
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
              </div>
              <div
                className="za-row"
                style={{
                  gap: 8,
                  justifyContent: "flex-end",
                  marginTop: 10,
                }}
              >
                <button
                  className="za-btn za-btn-sm"
                  onClick={() => {
                    setFetchCands(null);
                    setFetchSel(new Set());
                  }}
                >
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
          )}

          {selModels.length === 0 ? (
            <div className="za-empty">
              无模型，点击「拉取可用模型」自动填充上下文长度
            </div>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
              {selModels.map(([name, m]) => (
                <ModalModelRow
                  key={name}
                  name={name}
                  model={m}
                  busy={busy}
                  onSave={(c, o) =>
                    onRun(async () => {
                      await models.updateModelLimit(providerKey, name, c, o);
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
              ))}
            </div>
          )}
        </div>

        {/* 底部操作栏 */}
        <div className="za-modal-footer">
          {!isCurrent && (
            <button
              className="za-btn za-btn-sm"
              disabled={busy || !family}
              onClick={onSetCurrent}
              style={{ marginRight: "auto" }}
              title={family ? "设为当前 provider" : "setting.json 无 family"}
            >
              设为当前
            </button>
          )}
          <button className="za-btn za-btn-sm" onClick={onClose}>
            关闭
          </button>
          <button
            className="za-btn za-btn-sm za-btn-primary"
            disabled={busy || isCurrent}
            onClick={handleSaveProvider}
          >
            保存供应商
          </button>
        </div>
      </div>
    </div>
  );
}

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
