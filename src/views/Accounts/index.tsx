import { useEffect, useState } from "react";
import { accounts as acc, events } from "../../api";
import type { AccountMeta } from "../../types";
import {
  IconPlus,
  IconTrash,
  IconCheck,
  IconEdit,
  IconClose,
} from "../../components/icons";
import { RestartBar } from "../../components/RestartBar";

export default function Accounts() {
  const [list, setList] = useState<AccountMeta[]>([]);
  const [current, setCurrent] = useState<AccountMeta | null>(null);
  const [label, setLabel] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editLabel, setEditLabel] = useState("");
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const reload = async () => {
    try {
      setList(await acc.list());
      setCurrent(await acc.current());
    } catch (e: unknown) {
      setMsg(String(e));
    }
  };
  useEffect(() => {
    reload();
  }, []);

  const capture = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const cur = await acc.current();
      if (cur) {
        setMsg(`该账号已存在（${cur.label}），无需重复捕获`);
        return;
      }
      await acc.capture(label || "账号");
      setLabel("");
      await reload();
      setMsg("已捕获当前账号快照");
    } catch (e: unknown) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  };
  const use = async (id: string) => {
    if (!confirm("切换账号会关闭并重启 zcode，继续？")) return;
    setBusy(true);
    try {
      await acc.use(id);
      await reload();
      // 通知 Dashboard 立即刷新「当前账号 / 套餐」（不必等下一次 5s 轮询）
      events.emitRefreshRequested();
      setMsg("已切换");
    } catch (e: unknown) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  };
  const doDelete = async (id: string) => {
    try {
      await acc.remove(id);
      setDeletingId(null);
      await reload();
      setMsg("已删除账号");
    } catch (e: unknown) {
      setMsg(String(e));
    }
  };
  const startEdit = (a: AccountMeta) => {
    setDeletingId(null);
    setEditingId(a.id);
    setEditLabel(a.label);
  };
  const cancelEdit = () => {
    setEditingId(null);
    setEditLabel("");
  };
  const commitEdit = async () => {
    if (!editingId) return;
    const v = editLabel.trim();
    if (!v) {
      cancelEdit();
      return;
    }
    try {
      await acc.rename(editingId, v);
      cancelEdit();
      await reload();
      setMsg("已更新别名");
    } catch (e: unknown) {
      setMsg(String(e));
    }
  };

  return (
    <>
      <RestartBar hint="账号 / 配置变更后需重启 zcode 生效" />

      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>智谱账号</h3>
          <div className="za-row">
            <input
              className="za-input za-btn-sm"
              style={{ height: 30, width: 140 }}
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder="账号备注名"
            />
            <button
              className="za-btn za-btn-sm za-btn-primary"
              disabled={busy}
              onClick={capture}
            >
              <IconPlus width={13} height={13} /> 捕获当前
            </button>
          </div>
        </div>
        <p className="za-muted" style={{ margin: "0 0 12px" }}>
        捕获当前 zcode 登录态为快照（机器绑定，不可跨机器），可在多个智谱账号间一键切换。切换会
        kill 并重启 zcode。
      </p>
      {list.length === 0 ? (
        <div className="za-empty">暂无账号，点击「捕获当前」添加</div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {list.map((a) => {
            const editing = editingId === a.id;
            return (
              <div
                key={a.id}
                className="za-row-between"
                style={{
                  padding: "8px 12px",
                  borderRadius: 8,
                  border: "1px solid var(--glass-border)",
                }}
              >
                <div className="za-row" style={{ gap: 8, minWidth: 0, flex: 1 }}>
                  {!editing && current?.id === a.id && (
                    <IconCheck
                      width={14}
                      height={14}
                      style={{ color: "var(--accent)" }}
                    />
                  )}
                  {editing ? (
                    <div className="za-row" style={{ gap: 6, flex: 1 }}>
                      <input
                        className="za-input za-btn-sm"
                        style={{ height: 30, flex: 1, minWidth: 120 }}
                        value={editLabel}
                        autoFocus
                        placeholder="输入账号别名"
                        onChange={(e) => setEditLabel(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") commitEdit();
                          if (e.key === "Escape") cancelEdit();
                        }}
                      />
                      <button
                        className="za-icon-btn"
                        style={{ width: 26, height: 26, color: "var(--accent)" }}
                        onClick={commitEdit}
                        title="确认"
                      >
                        <IconCheck width={13} height={13} />
                      </button>
                      <button
                        className="za-icon-btn"
                        style={{ width: 26, height: 26 }}
                        onClick={cancelEdit}
                        title="取消"
                      >
                        <IconClose width={13} height={13} />
                      </button>
                    </div>
                  ) : (
                    <div style={{ minWidth: 0 }}>
                      <div
                        style={{
                          fontWeight: 500,
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                        }}
                      >
                        {a.label}
                        {current?.id === a.id && (
                          <span className="za-badge" style={{ marginLeft: 8 }}>
                            当前
                          </span>
                        )}
                      </div>
                      <div
                        className="za-faint za-mono"
                        style={{ fontSize: "var(--fs-xs)" }}
                      >
                        {a.email || a.shortId || a.userId} ·{" "}
                        {new Date(a.capturedAt).toLocaleString()}
                      </div>
                    </div>
                  )}
                </div>
                {!editing && (
                  <div className="za-row">
                    {current?.id !== a.id && deletingId !== a.id && (
                      <button
                        className="za-btn za-btn-sm"
                        disabled={busy}
                        onClick={() => use(a.id)}
                      >
                        切换
                      </button>
                    )}
                    {deletingId !== a.id && (
                      <button
                        className="za-icon-btn"
                        style={{ width: 26, height: 26 }}
                        onClick={() => startEdit(a)}
                        title="设置别名"
                      >
                        <IconEdit width={13} height={13} />
                      </button>
                    )}
                    {deletingId === a.id ? (
                      <div className="za-row" style={{ gap: 4 }}>
                        <button
                          className="za-btn za-btn-sm"
                          style={{ color: "#e5484d", borderColor: "#e5484d" }}
                          onClick={() => doDelete(a.id)}
                        >
                          确认删除
                        </button>
                        <button
                          className="za-icon-btn"
                          style={{ width: 26, height: 26 }}
                          onClick={() => setDeletingId(null)}
                          title="取消"
                        >
                          <IconClose width={13} height={13} />
                        </button>
                      </div>
                    ) : (
                      <button
                        className="za-icon-btn"
                        style={{ width: 26, height: 26 }}
                        onClick={() => setDeletingId(a.id)}
                        title="删除"
                      >
                        <IconTrash width={13} height={13} />
                      </button>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
      {msg && (
        <div
          className="za-muted za-mono"
          style={{ fontSize: "var(--fs-xs)", marginTop: 10 }}
        >
          {msg}
        </div>
      )}
      </div>
    </>
  );
}
