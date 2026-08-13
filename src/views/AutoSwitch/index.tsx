import { useEffect, useState, type CSSProperties } from "react";
import { autoswitch as sw } from "../../api";
import type { AutoSwitchRule } from "../../types";
import { Switch } from "../../components/Switch";
import { IconPlus, IconTrash } from "../../components/icons";

const field: CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 4,
  fontSize: "var(--fs-sm)",
  color: "var(--text-secondary)",
};

const blank = (): AutoSwitchRule => ({
  id: "",
  name: "",
  kind: "cron",
  enabled: true,
  timeStart: "09:00",
  timeEnd: "18:00",
  weekdays: "1,2,3,4,5",
  family: "bigmodel",
  fromProvider: "",
  toProvider: "",
  threshold: 100000,
  createdAt: "",
});

export default function AutoSwitch() {
  const [rules, setRules] = useState<AutoSwitchRule[]>([]);
  const [editing, setEditing] = useState<AutoSwitchRule | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  const reload = async () => {
    try {
      setRules(await sw.listRules());
    } catch (e: unknown) {
      setMsg(String(e));
    }
  };
  useEffect(() => {
    reload();
  }, []);

  const save = async () => {
    if (!editing) return;
    if (!editing.name || !editing.toProvider) {
      setMsg("请填写名称和目标 provider");
      return;
    }
    try {
      await sw.upsertRule(editing);
      setEditing(null);
      await reload();
      setMsg(null);
    } catch (e: unknown) {
      setMsg(String(e));
    }
  };
  const del = async (id: string) => {
    try {
      await sw.deleteRule(id);
      await reload();
    } catch (e: unknown) {
      setMsg(String(e));
    }
  };
  const toggle = async (r: AutoSwitchRule) => {
    try {
      await sw.upsertRule({ ...r, enabled: !r.enabled });
      await reload();
    } catch (e: unknown) {
      setMsg(String(e));
    }
  };

  return (
    <>
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>自动切换规则</h3>
          <button
            className="za-btn za-btn-sm za-btn-primary"
            onClick={() => setEditing(blank())}
          >
            <IconPlus width={13} height={13} /> 新建规则
          </button>
        </div>
        <p className="za-muted" style={{ margin: "0 0 12px" }}>
          ① 定时（指定时段/星期不用某 provider → 切到目标）② 配额耗尽（剩余 ≤
          阈值 → 切到目标）。触发后弹窗询问，确认即重启 zcode 生效。
        </p>
        {rules.length === 0 ? (
          <div className="za-empty">暂无规则</div>
        ) : (
          rules.map((r) => (
            <div
              key={r.id}
              className="za-row-between"
              style={{
                padding: "8px 12px",
                borderRadius: 8,
                border: "1px solid var(--glass-border)",
                marginBottom: 6,
              }}
            >
              <div className="za-row" style={{ gap: 10 }}>
                <Switch on={r.enabled} onChange={() => toggle(r)} />
                <div>
                  <div style={{ fontWeight: 500 }}>
                    {r.name}{" "}
                    <span className="za-badge za-badge-neutral">
                      {r.kind === "cron" ? "定时" : "配额耗尽"}
                    </span>
                  </div>
                  <div
                    className="za-faint za-mono"
                    style={{ fontSize: "var(--fs-xs)" }}
                  >
                    {r.kind === "cron"
                      ? `周${r.weekdays} ${r.timeStart}-${r.timeEnd}${
                          r.fromProvider ? " 禁用 " + r.fromProvider : ""
                        } → ${r.toProvider}`
                      : `剩余≤${r.threshold} → ${r.toProvider}`}
                  </div>
                </div>
              </div>
              <div className="za-row">
                <button
                  className="za-btn za-btn-sm"
                  onClick={() => setEditing(r)}
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
        {msg && (
          <div className="za-muted za-mono" style={{ fontSize: "var(--fs-xs)" }}>
            {msg}
          </div>
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
                  setEditing({ ...editing, kind: e.target.value as "cron" | "drain" })
                }
              >
                <option value="cron">定时切换</option>
                <option value="drain">配额耗尽</option>
              </select>
            </label>
            <label style={field}>
              family（provider 家族）
              <input
                className="za-input"
                value={editing.family || ""}
                onChange={(e) =>
                  setEditing({ ...editing, family: e.target.value })
                }
                placeholder="bigmodel"
              />
            </label>
            <label style={field}>
              源 provider key（可选，留空=任意）
              <input
                className="za-input"
                value={editing.fromProvider || ""}
                onChange={(e) =>
                  setEditing({ ...editing, fromProvider: e.target.value })
                }
              />
            </label>
            <label style={{ ...field, gridColumn: "1 / -1" }}>
              目标 provider key
              <input
                className="za-input"
                value={editing.toProvider}
                onChange={(e) =>
                  setEditing({ ...editing, toProvider: e.target.value })
                }
                placeholder="coding-plan:builtin:bigmodel-coding-plan"
              />
            </label>
            {editing.kind === "cron" ? (
              <>
                <label style={field}>
                  起始 HH:MM
                  <input
                    className="za-input"
                    value={editing.timeStart || ""}
                    onChange={(e) =>
                      setEditing({ ...editing, timeStart: e.target.value })
                    }
                  />
                </label>
                <label style={field}>
                  结束 HH:MM
                  <input
                    className="za-input"
                    value={editing.timeEnd || ""}
                    onChange={(e) =>
                      setEditing({ ...editing, timeEnd: e.target.value })
                    }
                  />
                </label>
                <label style={field}>
                  星期（1-7，逗号分隔）
                  <input
                    className="za-input"
                    value={editing.weekdays || ""}
                    onChange={(e) =>
                      setEditing({ ...editing, weekdays: e.target.value })
                    }
                    placeholder="1,2,3,4,5"
                  />
                </label>
              </>
            ) : (
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
            )}
          </div>
          <div
            className="za-row"
            style={{ gap: 8, marginTop: 12, justifyContent: "flex-end" }}
          >
            <button
              className="za-btn za-btn-sm"
              onClick={() => setEditing(null)}
            >
              取消
            </button>
            <button
              className="za-btn za-btn-sm za-btn-primary"
              onClick={save}
            >
              保存
            </button>
          </div>
        </div>
      )}
    </>
  );
}
