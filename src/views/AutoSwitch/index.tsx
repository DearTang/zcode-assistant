import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { autoswitch as sw, zcode } from "../../api";
import type {
  AutoSwitchLog,
  AutoSwitchProject,
  AutoSwitchRule,
  ZcProvider,
  ZcodeConfig,
} from "../../types";
import { Switch } from "../../components/Switch";
import { IconPlus, IconTrash } from "../../components/icons";
import { toast } from "../../components/Toast";

/** 取路径最后一段作为项目名（兼容 / 与 \） */
const baseName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() || p;

/** 执行方式 → 中文标签 */
const TRIGGER_LABEL: Record<string, string> = {
  manual: "人工点击",
  cron: "定时切换",
  drain: "配额耗尽",
  appstart: "应用启动",
};

/** 规则类型徽标文案 */
const kindBadge = (kind: string) =>
  kind === "cron" ? "定时" : kind === "drain" ? "配额耗尽" : "应用启动";

const field: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 4,
  fontSize: "var(--fs-sm)",
  color: "var(--text-secondary)",
};

const WEEKDAYS = [
  { v: 1, label: "周一" },
  { v: 2, label: "周二" },
  { v: 3, label: "周三" },
  { v: 4, label: "周四" },
  { v: 5, label: "周五" },
  { v: 6, label: "周六" },
  { v: 7, label: "周日" },
];

const weekdayLabel = (n: number) =>
  WEEKDAYS.find((w) => w.v === n)?.label ?? String(n);

const blank = (): AutoSwitchRule => ({
  id: "",
  name: "",
  kind: "cron",
  enabled: true,
  timeStart: "09:00",
  weekdays: "1,2,3,4,5",
  fromProvider: "",
  toProvider: "",
  toModel: "",
  createdAt: "",
  projectDir: "",
  switchPrimary: false,
});

/** 星期下拉多选（按钮 + 弹出 checkbox 面板） */
function WeekdayPicker({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  // 点击面板外收起。不能用 fixed 全屏遮罩：编辑卡片 .za-panel 带 backdrop-filter，
  // 会成为 fixed 后代的 containing block，遮罩实际只盖住卡片区域，页面其他位置点不到。
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const selected = new Set(
    value
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean)
      .map(Number)
  );
  const toggle = (v: number) => {
    const next = new Set(selected);
    if (next.has(v)) next.delete(v);
    else next.add(v);
    onChange(
      WEEKDAYS.filter((w) => next.has(w.v))
        .map((w) => w.v)
        .sort((a, b) => a - b)
        .join(",")
    );
  };
  const labels = WEEKDAYS.filter((w) => selected.has(w.v)).map((w) => w.label);
  const summary =
    labels.length === 0
      ? "未选择"
      : labels.length === 7
        ? "每天"
        : labels.join("、");
  return (
    <div ref={rootRef} style={{ position: "relative" }}>
      <button
        type="button"
        className="za-input"
        style={{
          height: 30,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          cursor: "pointer",
        }}
        onClick={() => setOpen((o) => !o)}
      >
        <span>{summary}</span>
        <span style={{ fontSize: 10, opacity: 0.6 }}>▾</span>
      </button>
      {open && (
        <div
          className="za-panel"
          style={{
            position: "absolute",
            top: "100%",
            left: 0,
            zIndex: 10,
            marginTop: 2,
            padding: 4,
            minWidth: "100%",
            boxShadow: "0 4px 12px rgba(0,0,0,0.18)",
          }}
        >
          {WEEKDAYS.map((w) => (
            <label
              key={w.v}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "4px 8px",
                cursor: "pointer",
                fontSize: "var(--fs-sm)",
              }}
            >
              <input
                type="checkbox"
                checked={selected.has(w.v)}
                onChange={() => toggle(w.v)}
              />
              {w.label}
            </label>
          ))}
        </div>
      )}
    </div>
  );
}

/** 执行日志弹窗：任务名 / 操作方式 / 操作时间 / 结果 / 错误日志 */
function LogsDialog({ onClose }: { onClose: () => void }) {
  const [logs, setLogs] = useState<AutoSwitchLog[] | null>(null);

  useEffect(() => {
    sw.logs()
      .then(setLogs)
      .catch((e) => {
        toast.error(String(e));
        setLogs([]);
      });
  }, []);

  const fmtTime = (iso: string) => {
    const d = new Date(iso);
    return Number.isNaN(d.getTime())
      ? iso
      : d.toLocaleString("zh-CN", { hour12: false });
  };

  const th: CSSProperties = {
    textAlign: "left",
    padding: "6px 8px",
    color: "var(--text-secondary)",
    fontWeight: 500,
    borderBottom: "1px solid var(--glass-border)",
    whiteSpace: "nowrap",
  };
  const td: CSSProperties = {
    padding: "6px 8px",
    borderBottom: "1px solid var(--glass-border)",
    verticalAlign: "top",
  };

  return (
    <div className="rd-overlay" onClick={onClose}>
      <div
        className="za-glass-strong"
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 680,
          maxWidth: "92vw",
          maxHeight: "80vh",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "16px 20px 12px",
          }}
        >
          <h3 style={{ margin: 0 }}>自动切换执行日志</h3>
          <button className="za-btn za-btn-ghost za-btn-sm" onClick={onClose}>
            关闭
          </button>
        </div>
        <div
          style={{
            flex: 1,
            overflowY: "auto",
            padding: "0 20px 16px",
            fontSize: "var(--fs-sm)",
          }}
        >
          {logs === null ? (
            <div className="za-empty">加载中…</div>
          ) : logs.length === 0 ? (
            <div className="za-empty">暂无执行日志</div>
          ) : (
            <table style={{ width: "100%", borderCollapse: "collapse" }}>
              <thead>
                <tr>
                  <th style={th}>任务名</th>
                  <th style={th}>操作方式</th>
                  <th style={th}>操作时间</th>
                  <th style={th}>结果</th>
                  <th style={th}>错误日志</th>
                </tr>
              </thead>
              <tbody>
                {logs.map((l) => (
                  <tr key={l.id}>
                    <td style={td}>{l.ruleName}</td>
                    <td style={{ ...td, whiteSpace: "nowrap" }}>
                      {TRIGGER_LABEL[l.triggerType] ?? l.triggerType}
                    </td>
                    <td style={{ ...td, whiteSpace: "nowrap" }} className="za-mono">
                      {fmtTime(l.createdAt)}
                    </td>
                    <td
                      style={{
                        ...td,
                        whiteSpace: "nowrap",
                        color: l.success ? "#22C55E" : "#EF4444",
                      }}
                    >
                      {l.success ? "成功" : "失败"}
                    </td>
                    <td
                      style={{ ...td, color: "var(--text-secondary)" }}
                      title={l.message}
                    >
                      {l.message || "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}

export default function AutoSwitch() {
  const [rules, setRules] = useState<AutoSwitchRule[]>([]);
  const [config, setConfig] = useState<ZcodeConfig | null>(null);
  const [editing, setEditing] = useState<AutoSwitchRule | null>(null);
  const [dragIdx, setDragIdx] = useState<number | null>(null);
  const [showLogs, setShowLogs] = useState(false);
  const [testing, setTesting] = useState<string | null>(null);
  // 可限定项目列表（当前打开且有对话的项目），打开编辑器时刷新
  const [projects, setProjects] = useState<AutoSwitchProject[]>([]);

  const loadProjects = async () => {
    try {
      setProjects(await sw.projects());
    } catch {
      // 列表加载失败不阻塞编辑，仅无可选项
    }
  };

  const reload = async () => {
    try {
      const [rs, cfg] = await Promise.all([sw.listRules(), zcode.getConfig()]);
      setRules(rs);
      setConfig(cfg);
    } catch (e: unknown) {
      toast.error(String(e));
    }
  };
  useEffect(() => {
    reload();
  }, []);

  // 启用供应商（含智谱内置），排除系统禁用项；与"模型管理"页口径一致
  const providers = useMemo<[string, ZcProvider][]>(() => {
    if (!config) return [];
    return Object.entries(config.provider).filter(
      ([, p]) => !p.systemDisabledReason && p.enabled !== false
    );
  }, [config]);

  const providerLabel = (key?: string) => {
    if (!key) return "（任意）";
    return config?.provider?.[key]?.name || key;
  };
  const modelsOf = (providerKey?: string): string[] => {
    if (!providerKey || !config) return [];
    return Object.keys(config.provider?.[providerKey]?.models ?? {});
  };

  const newRule = () => {
    const b = blank();
    // 默认目标=第一个供应商 + 其第一个模型
    if (providers.length > 0) {
      const [firstKey] = providers[0];
      b.toProvider = firstKey;
      b.toModel = modelsOf(firstKey)[0] ?? "";
    }
    setEditing(b);
    void loadProjects();
  };

  const editRule = (r: AutoSwitchRule) => {
    setEditing(r);
    void loadProjects();
  };

  const save = async () => {
    if (!editing) return;
    const name = editing.name.trim();
    if (!name) {
      toast.error("请填写规则名称");
      return;
    }
    // 规则名称不允许重复（排除自身，支持改名保存）
    if (rules.some((x) => x.id !== editing.id && x.name === name)) {
      toast.error(`规则名称「${name}」已存在，请换一个`);
      return;
    }
    if (!editing.toProvider || !editing.toModel) {
      toast.error("请选择目标供应商与模型");
      return;
    }
    if (editing.kind === "cron" && !editing.timeStart) {
      toast.error("请填写执行时间");
      return;
    }
    try {
      await sw.upsertRule({ ...editing, name });
      setEditing(null);
      await reload();
      toast.success("规则已保存");
    } catch (e: unknown) {
      toast.error(String(e));
    }
  };
  const del = async (id: string) => {
    try {
      await sw.deleteRule(id);
      await reload();
    } catch (e: unknown) {
      toast.error(String(e));
    }
  };
  // 手动测试：跳过触发条件立即执行切换（结果记入执行日志）
  const test = async (r: AutoSwitchRule) => {
    setTesting(r.id);
    try {
      const msg = await sw.testRule(r.id);
      toast.success(msg);
    } catch (e: unknown) {
      toast.error(String(e));
    } finally {
      setTesting(null);
    }
  };
  const toggle = async (r: AutoSwitchRule) => {
    try {
      await sw.upsertRule({ ...r, enabled: !r.enabled });
      await reload();
    } catch (e: unknown) {
      toast.error(String(e));
    }
  };

  const onDrop = async (targetIdx: number) => {
    if (dragIdx === null || dragIdx === targetIdx) {
      setDragIdx(null);
      return;
    }
    const next = [...rules];
    const [moved] = next.splice(dragIdx, 1);
    next.splice(targetIdx, 0, moved);
    setRules(next);
    setDragIdx(null);
    try {
      await sw.reorder(next.map((r) => r.id));
    } catch (e: unknown) {
      toast.error(String(e));
      await reload();
    }
  };

  // 目标供应商切换 → 默认选中第一个模型
  const pickToProvider = (providerKey: string) => {
    const ms = modelsOf(providerKey);
    setEditing((e) =>
      e ? { ...e, toProvider: providerKey, toModel: ms[0] ?? "" } : e
    );
  };
  const pickFromProvider = (providerKey: string) => {
    setEditing((e) => (e ? { ...e, fromProvider: providerKey, fromModel: "" } : e));
  };

  // 下拉选项：已加载的项目列表；规则里的项目已不在列表（已关闭）时补一项保留原值
  const projectOptions = useMemo(() => {
    const list = [...projects];
    const cur = editing?.projectDir;
    if (cur && !list.some((p) => p.dir === cur)) {
      list.push({ dir: cur, name: `${baseName(cur)}（已关闭）`, sessions: 0 });
    }
    return list;
  }, [projects, editing?.projectDir]);

  const weekdaySummary = (wds: string) => {
    const arr = wds.split(",").filter(Boolean).map(Number);
    if (arr.length === 0) return "每天";
    return arr.map(weekdayLabel).join("、");
  };

  return (
    <>
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>自动切换规则</h3>
          <div className="za-row" style={{ gap: 8 }}>
            <button className="za-btn za-btn-sm" onClick={() => setShowLogs(true)}>
              执行日志
            </button>
            <button
              className="za-btn za-btn-sm za-btn-primary"
              onClick={newRule}
            >
              <IconPlus width={13} height={13} /> 新建规则
            </button>
          </div>
        </div>
        <p className="za-muted" style={{ margin: "0 0 12px" }}>
          ① 定时（指定执行时间 / 星期 → 切到目标）② 配额耗尽（剩余 ≤ 阈值 →
          切到目标）③ 应用启动（本应用启动后自动执行一次）。列表顺序即优先级，拖动手柄可调整。
          可点「测试」立即手动执行一次切换。切换会写入配置与全部符合条件的会话（规则限定项目时仅该项目）：
          开启「设置 → 切换后重启 ZCode」（默认）时自动重启 ZCode，全部对话立即生效；
          关闭时不重启，各对话在恢复 / 新开时生效。
          规则可限定项目：仅当最近对话发生在该项目时才触发，不影响在其他项目的工作。
        </p>
        {rules.length === 0 ? (
          <div className="za-empty">暂无规则</div>
        ) : (
          rules.map((r, idx) => (
            <div
              key={r.id}
              draggable
              onDragStart={() => setDragIdx(idx)}
              onDragOver={(e) => e.preventDefault()}
              onDrop={() => onDrop(idx)}
              onDragEnd={() => setDragIdx(null)}
              className="za-row-between"
              style={{
                padding: "8px 12px",
                borderRadius: 8,
                border: "1px solid var(--glass-border)",
                marginBottom: 6,
                background:
                  dragIdx === idx ? "rgba(127,127,127,0.08)" : undefined,
                opacity: dragIdx === idx ? 0.6 : 1,
                cursor: "grab",
              }}
            >
              <div className="za-row" style={{ gap: 10, alignItems: "center" }}>
                <span
                  className="za-faint"
                  style={{
                    cursor: "grab",
                    fontSize: "var(--fs-sm)",
                    lineHeight: 1,
                    userSelect: "none",
                  }}
                  title="拖动调整优先级"
                >
                  ⠿
                </span>
                <span
                  className="za-badge za-badge-neutral"
                  title="优先级（越小越先触发）"
                >
                  {idx + 1}
                </span>
                <div>
                  <div style={{ fontWeight: 500 }}>
                    {r.name}{" "}
                    <span className="za-badge za-badge-neutral">
                      {kindBadge(r.kind)}
                    </span>
                    {r.projectDir && (
                      <span
                        className="za-badge za-badge-neutral"
                        title={`仅项目：${r.projectDir}`}
                        style={{ marginLeft: 4 }}
                      >
                        {baseName(r.projectDir)}
                      </span>
                    )}
                    {r.switchPrimary && (
                      <span
                        className="za-badge za-badge-neutral"
                        title="切换时同步「模型管理」的主供应商标记到目标供应商"
                        style={{ marginLeft: 4 }}
                      >
                        同步主供应
                      </span>
                    )}
                  </div>
                  <div
                    className="za-faint za-mono"
                    style={{ fontSize: "var(--fs-xs)" }}
                  >
                    {r.kind === "cron"
                      ? `${weekdaySummary(r.weekdays || "")} ${r.timeStart || ""}${
                          r.fromProvider
                            ? "（源 " +
                              providerLabel(r.fromProvider) +
                              (r.fromModel ? "/" + r.fromModel : "") +
                              "）"
                            : ""
                        } → ${providerLabel(r.toProvider)}${r.toModel ? "/" + r.toModel : ""}`
                      : r.kind === "drain"
                        ? `剩余≤${r.threshold ?? 0} → ${providerLabel(r.toProvider)}${r.toModel ? "/" + r.toModel : ""}`
                        : `应用启动 → ${providerLabel(r.toProvider)}${r.toModel ? "/" + r.toModel : ""}`}
                  </div>
                </div>
              </div>
              <div className="za-row">
                <Switch
                  on={r.enabled}
                  onChange={() => toggle(r)}
                  title={r.enabled ? "已启用" : "已禁用"}
                />
                <button
                  className="za-btn za-btn-sm"
                  disabled={testing === r.id}
                  title="跳过触发条件，立即在 ZCode 界面执行一次切换（免重启）"
                  onClick={() => void test(r)}
                >
                  {testing === r.id ? "测试中…" : "测试"}
                </button>
                <button
                  className="za-btn za-btn-sm"
                  onClick={() => editRule(r)}
                >
                  编辑
                </button>
                <button
                  className="za-icon-btn"
                  style={{ width: 26, height: 26 }}
                  onClick={() => del(r.id)}
                >
                  <IconTrash width={13} height={13} />
                </button>
              </div>
            </div>
          ))
        )}
      </div>

      {editing && (
        <div className="za-panel za-card-pad">
          <div className="za-section-title">
            <h3>{editing.id ? "编辑规则" : "新建规则"}</h3>
          </div>
          <div className="za-grid za-grid-2" style={{ gap: 10 }}>
            <label style={field}>
              规则名称
              <input
                className="za-input"
                value={editing.name}
                onChange={(e) => setEditing({ ...editing, name: e.target.value })}
              />
            </label>
            <label style={field}>
              类型
              <select
                className="za-select"
                value={editing.kind}
                onChange={(e) =>
                  setEditing({
                    ...editing,
                    kind: e.target.value as AutoSwitchRule["kind"],
                  })
                }
              >
                <option value="cron">定时切换</option>
                <option value="drain">配额耗尽</option>
                <option value="appstart">应用启动</option>
              </select>
            </label>

            {/* 项目限定（可选，默认全部项目）：仅当最近对话发生在所选项目时触发 */}
            <label style={field}>
              项目（可选，留空=全部项目）
              <select
                className="za-select"
                value={editing.projectDir || ""}
                onChange={(e) =>
                  setEditing({ ...editing, projectDir: e.target.value })
                }
              >
                <option value="">（全部项目）</option>
                {projectOptions.map((p) => (
                  <option key={p.dir} value={p.dir} title={p.dir}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>

            {/* 源：供应商 → 模型（可不选模型 = 匹配该供应商任意模型；供应商留空=任意） */}
            <label style={field}>
              源供应商（可选，留空=任意）
              <select
                className="za-select"
                value={editing.fromProvider || ""}
                onChange={(e) => pickFromProvider(e.target.value)}
              >
                <option value="">（任意供应商）</option>
                {providers.map(([key, p]) => (
                  <option key={key} value={key}>
                    {p.name || key}
                  </option>
                ))}
              </select>
            </label>
            <label style={field}>
              源模型（可选，留空=该供应商任意模型）
              <select
                className="za-select"
                value={editing.fromModel || ""}
                onChange={(e) =>
                  setEditing({ ...editing, fromModel: e.target.value })
                }
                disabled={!editing.fromProvider}
              >
                <option value="">（任意模型）</option>
                {modelsOf(editing.fromProvider).map((m) => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </select>
            </label>

            {/* 目标：供应商 → 模型（必须选模型，切换供应商默认选第一个） */}
            <label style={field}>
              目标供应商
              <select
                className="za-select"
                value={editing.toProvider}
                onChange={(e) => pickToProvider(e.target.value)}
              >
                <option value="">请选择</option>
                {providers.map(([key, p]) => (
                  <option key={key} value={key}>
                    {p.name || key}
                  </option>
                ))}
              </select>
            </label>
            <label style={field}>
              目标模型（必选）
              <select
                className="za-select"
                value={editing.toModel || ""}
                onChange={(e) =>
                  setEditing({ ...editing, toModel: e.target.value })
                }
                disabled={!editing.toProvider}
              >
                <option value="">请选择</option>
                {modelsOf(editing.toProvider).map((m) => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </select>
            </label>

            {/* 主供应商联动（默认关）：切换时同步 zcode-assistant 的 ⭐ 主供应商标记到目标供应商 */}
            <div
              className="za-row"
              style={{
                gridColumn: "1 / -1",
                gap: 8,
                alignItems: "center",
                fontSize: "var(--fs-sm)",
                color: "var(--text-secondary)",
              }}
            >
              <Switch
                on={!!editing.switchPrimary}
                onChange={(v) => setEditing({ ...editing, switchPrimary: v })}
                title="开启后，规则切换时把模型管理里的主供应商也设为目标供应商"
              />
              <span>
                同时切换主供应商
                <span className="za-faint" style={{ marginLeft: 6 }}>
                  （同步「模型管理」的 ⭐ 主供应商标记到目标供应商，总览 / 悬浮球 /
                  托盘的配额展示跟随切换）
                </span>
              </span>
            </div>

            {editing.kind === "cron" ? (
              <>
                <label style={field}>
                  执行时间
                  <input
                    type="time"
                    className="za-input"
                    value={editing.timeStart || ""}
                    onChange={(e) =>
                      setEditing({ ...editing, timeStart: e.target.value })
                    }
                  />
                </label>
                <label style={field}>
                  星期（多选）
                  <WeekdayPicker
                    value={editing.weekdays || ""}
                    onChange={(v) => setEditing({ ...editing, weekdays: v })}
                  />
                </label>
              </>
            ) : editing.kind === "drain" ? (
              <label style={field}>
                剩余阈值（token）
                <input
                  className="za-input"
                  type="number"
                  value={editing.threshold ?? 0}
                  onChange={(e) =>
                    setEditing({
                      ...editing,
                      threshold: Number(e.target.value),
                    })
                  }
                />
              </label>
            ) : (
              <div
                className="za-faint"
                style={{
                  fontSize: "var(--fs-xs)",
                  alignSelf: "end",
                  paddingBottom: 8,
                }}
              >
                本应用每次启动后自动执行一次切换（免重启，下一轮对话生效）
              </div>
            )}
          </div>
          <div
            className="za-row"
            style={{ gap: 8, marginTop: 12, justifyContent: "flex-end" }}
          >
            <button className="za-btn za-btn-sm" onClick={() => setEditing(null)}>
              取消
            </button>
            <button className="za-btn za-btn-sm za-btn-primary" onClick={save}>
              保存
            </button>
          </div>
        </div>
      )}

      {showLogs && <LogsDialog onClose={() => setShowLogs(false)} />}
    </>
  );
}
