/**
 * 项目管理 —— 数据源为 zcode cli db 的 session 等表（读写）。
 * - 项目列表：会话数（活跃 / 归档）/ 对话次数 / token 消耗 / 创建与最近活跃时间；
 * - 展开项目查看会话明细（消耗含子代理后代），支持行内改名（title_source=custom）；
 * - 归档会话默认隐藏，「查看历史」开关两级控制（全局：全归档项目；项目内：归档会话），
 *   归档会话可一键恢复（清 time_archived），回 zcode 会话列表继续对话；
 * - 项目与会话均支持勾选批量归档（不删除，可在「查看历史」恢复）与批量删除（级联清掉消息 / 用量）。
 * - 排序：项目与会话均按最后活跃时间倒序。
 */
import { useCallback, useEffect, useState } from "react";
import { projects as projectsApi, formatUnits } from "../../api";
import type { ZcCacheStats, ZcProject, ZcSession } from "../../types";
import {
  IconRefresh,
  IconTrash,
  IconCheck,
  IconEdit,
  IconClose,
  IconFolder,
} from "../../components/icons";
import { toast } from "../../components/Toast";
import { RestartBar } from "../../components/RestartBar";

/** 会话名称/归档恢复相关操作需重启 zcode 才可生效的提示文案 */
const SESSION_RESTART_HINT = "会话名称修改及恢复，需要重启zcode生效";

/** 毫秒时间戳 → YYYY-MM-DD HH:mm */
function fmtTime(ms?: number | null): string {
  if (ms == null || !Number.isFinite(ms)) return "—";
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "—";
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(
    d.getHours()
  )}:${p(d.getMinutes())}`;
}

/** 目录路径取末段作为项目名 */
function baseName(dir: string): string {
  if (!dir) return "(未知)";
  const parts = dir.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || dir;
}

/** 字节数 → 人类可读（B/KB/MB/GB/TB） */
function fmtBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const s = i === 0 ? String(Math.round(v)) : v >= 100 ? v.toFixed(0) : v.toFixed(1);
  return `${s} ${units[i]}`;
}

/** 清理缓存的时间范围选项（删除最后活跃早于该时间点的会话及其全部数据） */
const CACHE_OPTIONS = [
  { days: 3, label: "3 天前" },
  { days: 5, label: "5 天前" },
  { days: 7, label: "7 天前" },
  { days: 15, label: "半个月前" },
  { days: 30, label: "1 个月前" },
];

export default function Projects() {
  const [loading, setLoading] = useState(true);
  const [list, setList] = useState<ZcProject[]>([]);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  // 项目 → 会话列表缓存（删除 / 改名后按需失效）
  const [sessionsMap, setSessionsMap] = useState<Record<string, ZcSession[]>>({});
  const [sessionsLoading, setSessionsLoading] = useState<Record<string, boolean>>({});
  const [selectedProjects, setSelectedProjects] = useState<Set<string>>(new Set());
  const [selectedSessions, setSelectedSessions] = useState<Set<string>>(new Set());
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [deleting, setDeleting] = useState(false);
  const [archiving, setArchiving] = useState(false);
  // 查看历史开关：全局（默认隐藏全部会话均已归档的项目）
  const [showHistory, setShowHistory] = useState(false);
  // 项目内开关（默认隐藏归档会话），按项目 id 记忆
  const [projectHistory, setProjectHistory] = useState<Record<string, boolean>>({});
  // 清理缓存弹窗：开关 / 选定时间范围 / 预览统计 / 执行中标记
  const [cacheOpen, setCacheOpen] = useState(false);
  const [cacheDays, setCacheDays] = useState(7);
  const [cacheStats, setCacheStats] = useState<ZcCacheStats | null>(null);
  const [cacheStatsLoading, setCacheStatsLoading] = useState(false);
  const [cleaning, setCleaning] = useState(false);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      setList(await projectsApi.list());
    } catch (e: unknown) {
      toast.error(`加载失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  // 打开清理弹窗或切换时间范围时拉取预览统计
  useEffect(() => {
    if (!cacheOpen) return;
    let alive = true;
    setCacheStats(null);
    setCacheStatsLoading(true);
    projectsApi
      .cacheStats(cacheDays)
      .then((s) => {
        if (alive) setCacheStats(s);
      })
      .catch((e: unknown) => {
        if (alive) toast.error(`统计失败：${e instanceof Error ? e.message : String(e)}`);
      })
      .finally(() => {
        if (alive) setCacheStatsLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [cacheOpen, cacheDays]);

  const loadSessions = useCallback(async (projectId: string, force = false) => {
    if (!force && sessionsMap[projectId]) return;
    setSessionsLoading((m) => ({ ...m, [projectId]: true }));
    try {
      const rows = await projectsApi.sessions(projectId);
      setSessionsMap((m) => ({ ...m, [projectId]: rows }));
    } catch (e: unknown) {
      toast.error(`加载会话失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setSessionsLoading((m) => ({ ...m, [projectId]: false }));
    }
  }, [sessionsMap]);

  const toggleExpand = (p: ZcProject) => {
    if (expandedId === p.id) {
      setExpandedId(null);
      return;
    }
    setExpandedId(p.id);
    loadSessions(p.id);
  };

  // ============ 勾选 ============
  const toggleProject = (id: string) => {
    setSelectedProjects((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  };
  const toggleSession = (id: string) => {
    setSelectedSessions((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  };
  const clearSelection = () => {
    setSelectedProjects(new Set());
    setSelectedSessions(new Set());
  };

  // ============ 改名 ============
  const startEdit = (s: ZcSession) => {
    setEditingId(s.id);
    setEditTitle(s.title);
  };
  const cancelEdit = () => {
    setEditingId(null);
    setEditTitle("");
  };
  const commitEdit = async () => {
    if (!editingId) return;
    const v = editTitle.trim();
    if (!v) {
      cancelEdit();
      return;
    }
    try {
      await projectsApi.renameSession(editingId, v);
      const pid = expandedId;
      if (pid) {
        setSessionsMap((m) => ({
          ...m,
          [pid]: (m[pid] ?? []).map((s) =>
            s.id === editingId ? { ...s, title: v, titleSource: "custom" } : s
          ),
        }));
      }
      cancelEdit();
      toast.success("已更新会话名称");
      toast.warning(SESSION_RESTART_HINT);
    } catch (e: unknown) {
      toast.error(`改名失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  // ============ 归档 / 恢复（会话级 + 项目级，对称于 zcode 的归档语义） ============
  const doArchiveSession = async (s: ZcSession) => {
    try {
      await projectsApi.archiveSession(s.id);
      setSessionsMap((m) => ({
        ...m,
        [s.projectId]: (m[s.projectId] ?? []).map((x) =>
          x.id === s.id ? { ...x, archived: true, timeArchivedMs: Date.now() } : x
        ),
      }));
      await reload();
      toast.success("已归档会话，可在「查看历史会话」中恢复");
    } catch (e: unknown) {
      toast.error(`归档失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const doArchiveProject = async (p: ZcProject) => {
    try {
      const n = await projectsApi.archiveProject(p.id);
      await Promise.all([loadSessions(p.id, true), reload()]);
      toast.success(`已归档该项目 ${n} 个会话，可在「查看历史会话」恢复`);
    } catch (e: unknown) {
      toast.error(`归档失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  // ============ 批量归档（不删除，可在「查看历史」恢复） ============
  const doArchiveSelected = async () => {
    const sids = [...selectedSessions];
    if (sids.length === 0) return;
    if (
      !confirm(
        `将归档所选 ${sids.length} 个会话（已归档的自动跳过），在 zcode 会话列表隐藏，可随时在「查看历史会话」中恢复。继续？`
      )
    ) {
      return;
    }
    setArchiving(true);
    try {
      const n = await projectsApi.archiveSessions(sids);
      clearSelection();
      if (expandedId) await loadSessions(expandedId, true);
      await reload();
      toast.success(`已归档 ${n} 个会话，可在「查看历史会话」中恢复`);
    } catch (e: unknown) {
      toast.error(`归档失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setArchiving(false);
    }
  };

  /** 批量归档所选项目：逐个项目归档其全部活跃会话（不删除，可随时恢复） */
  const doArchiveSelectedProjects = async () => {
    const pids = [...selectedProjects];
    if (pids.length === 0) return;
    if (
      !confirm(
        `将归档所选 ${pids.length} 个项目的全部活跃会话（已归档的自动跳过），在 zcode 会话列表隐藏，可随时恢复。继续？`
      )
    ) {
      return;
    }
    setArchiving(true);
    try {
      let n = 0;
      for (const pid of pids) {
        n += await projectsApi.archiveProject(pid);
      }
      clearSelection();
      if (expandedId) await loadSessions(expandedId, true);
      await reload();
      toast.success(`已归档 ${pids.length} 个项目共 ${n} 个会话，可在「查看历史会话」中恢复`);
    } catch (e: unknown) {
      toast.error(`归档失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setArchiving(false);
    }
  };

  // ============ 恢复项目 ============
  const doRestoreProject = async (p: ZcProject) => {
    if (p.archivedSessions === 0) return;
    try {
      const n = await projectsApi.restoreProject(p.id);
      await Promise.all([loadSessions(p.id, true), reload()]);
      toast.success(`已恢复该项目 ${n} 个会话，可在 zcode 中继续对话`);
      toast.warning(SESSION_RESTART_HINT);
    } catch (e: unknown) {
      toast.error(`恢复失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  // ============ 恢复归档 ============
  const doRestore = async (s: ZcSession) => {
    try {
      await projectsApi.restoreSession(s.id);
      setSessionsMap((m) => ({
        ...m,
        [s.projectId]: (m[s.projectId] ?? []).map((x) =>
          x.id === s.id ? { ...x, archived: false, timeArchivedMs: null } : x
        ),
      }));
      await reload();
      toast.success("已恢复归档，可在 zcode 该项目的会话列表中继续对话");
      toast.warning(SESSION_RESTART_HINT);
    } catch (e: unknown) {
      toast.error(`恢复失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  // ============ 批量删除 ============
  /** 不传参时使用当前勾选项；单行删除直接传目标 id */
  const doDelete = async (targetSids?: string[], targetPids?: string[]) => {
    const pids = targetPids ?? [...selectedProjects];
    const sids = targetSids ?? [...selectedSessions];
    if (pids.length === 0 && sids.length === 0) return;
    const nameOf = (id: string) => {
      const p = list.find((x) => x.id === id);
      return p ? `「${baseName(p.directory)}」` : "";
    };
    const msg =
      pids.length > 0
        ? `将删除 ${pids.length} 个项目（${pids.slice(0, 3).map(nameOf).join("、")}${
            pids.length > 3 ? " 等" : ""
          }）及其全部会话、消息与用量记录`
        : `将删除 ${sids.length} 个会话（含子代理）及其消息与用量记录`;
    if (!confirm(`${msg}，操作不可恢复。若相关会话正在 zcode 中打开，建议先关闭对应窗口。继续？`)) {
      return;
    }
    if (!confirm("二次确认：删除后数据不可恢复、无法找回。确定删除吗？")) {
      return;
    }
    setDeleting(true);
    try {
      const res = await projectsApi.delete(sids, pids);
      clearSelection();
      cancelEdit();
      // 失效受影响缓存：删掉的项目的会话缓存，以及各项目中已删除的会话
      setSessionsMap((m) => {
        const n: Record<string, ZcSession[]> = {};
        for (const k of Object.keys(m)) {
          if (!pids.includes(k)) n[k] = m[k].filter((s) => !sids.includes(s.id));
        }
        return n;
      });
      if (expandedId && pids.includes(expandedId)) setExpandedId(null);
      await reload();
      toast.success(
        `已删除 ${res.deletedSessions} 个会话${
          res.deletedProjects > 0 ? `（含 ${res.deletedProjects} 个项目）` : ""
        }`
      );
    } catch (e: unknown) {
      toast.error(`删除失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setDeleting(false);
    }
  };

  // ============ 清理缓存（按时间批量删除久未活跃会话的全部数据） ============
  const doCacheCleanup = async () => {
    if (!cacheStats || cacheStats.sessions === 0) return;
    const opt = CACHE_OPTIONS.find((o) => o.days === cacheDays);
    const label = opt ? opt.label : `${cacheDays} 天前`;
    const sizeText =
      cacheStats.freedBytes > 0 ? `，可释放约 ${fmtBytes(cacheStats.freedBytes)} 的缓存文件` : "";
    if (
      !confirm(
        `将删除${label}至今未活跃的 ${cacheStats.sessions} 个会话（含子代理共 ${cacheStats.totalSessions} 个）及其消息、子代理记录、工具缓存与用量记录${sizeText}。若相关会话正在 zcode 中打开，建议先关闭对应窗口。继续？`
      )
    ) {
      return;
    }
    if (!confirm("二次确认：清理后数据不可恢复、无法找回。确定清理吗？")) {
      return;
    }
    setCleaning(true);
    try {
      const res = await projectsApi.cacheCleanup(cacheDays);
      setCacheOpen(false);
      setSessionsMap({});
      clearSelection();
      setExpandedId(null);
      await reload();
      toast.success(
        `已清理 ${res.deletedSessions} 个会话${
          res.freedBytes > 0 ? `，释放约 ${fmtBytes(res.freedBytes)}` : ""
        }${res.dbVacuumed ? "，会话库已压缩" : ""}`
      );
    } catch (e: unknown) {
      toast.error(`清理失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setCleaning(false);
    }
  };

  const totalSelected = selectedProjects.size + selectedSessions.size;
  // 全局「查看历史」关闭时隐藏全部会话均已归档的项目
  const visibleProjects = showHistory
    ? list
    : list.filter((p) => p.archivedSessions < p.sessions);
  const expanded = visibleProjects.find((p) => p.id === expandedId) ?? null;
  const sessions = expanded ? sessionsMap[expanded.id] ?? [] : [];
  // 项目内「查看历史」关闭时隐藏归档会话
  const showProjHistory = expanded ? projectHistory[expanded.id] === true : false;
  const visibleSessions = showProjHistory
    ? sessions
    : sessions.filter((s) => !s.archived);
  const archivedCount = sessions.filter((s) => s.archived).length;
  // 会话表头全选：作用于当前展开项目的可见会话（半选 = 只选中了其中一部分）
  const allSessionsSelected =
    visibleSessions.length > 0 &&
    visibleSessions.every((s) => selectedSessions.has(s.id));
  const someSessionsSelected = visibleSessions.some((s) =>
    selectedSessions.has(s.id)
  );
  const toggleSelectAllSessions = () => {
    setSelectedSessions((prev) => {
      const n = new Set(prev);
      if (allSessionsSelected) {
        visibleSessions.forEach((s) => n.delete(s.id));
      } else {
        visibleSessions.forEach((s) => n.add(s.id));
      }
      return n;
    });
  };
  /** 按钮激活态样式（同用量页预设按钮） */
  const activeBtnStyle = {
    borderColor: "var(--accent)",
    color: "var(--accent)",
    background: "var(--accent-subtle)",
  } as const;

  return (
    <>
      <RestartBar hint={SESSION_RESTART_HINT} />

      {/* 工具条 */}
      <div className="za-panel za-card-pad">
        <div className="za-section-title">
          <h3>项目管理</h3>
          <div className="za-row">
            <button
              className="za-btn za-btn-sm"
              onClick={() => setCacheOpen(true)}
              title="按时间批量清理久未活跃的会话及其缓存数据，释放磁盘空间"
            >
              <IconTrash width={14} height={14} />
              清理缓存
            </button>
            <button
              className="za-btn za-btn-sm"
              data-active={showHistory}
              style={showHistory ? activeBtnStyle : undefined}
              onClick={() => setShowHistory((v) => !v)}
              title="开启后显示全部会话均已归档的项目"
            >
              查看历史项目
            </button>
            <button
              className="za-btn za-btn-sm za-btn-ghost"
              onClick={() => reload()}
              disabled={loading}
            >
              <IconRefresh width={14} height={14} />
              刷新
            </button>
          </div>
        </div>
        <p className="za-muted" style={{ margin: "0 0 12px" }}>
          管理 zcode 的项目与会话：查看各项目 / 会话的 token 消耗、对话次数与创建时间，支持会话改名、
          归档项目 / 归档会话 / 恢复项目 / 恢复会话与批量归档（只归档不删除，可在「查看历史」恢复）、
          批量删除。默认只显示活跃会话，开启「查看历史项目 / 查看历史会话」查看归档。
          「清理缓存」可按时间批量删除久未活跃的会话及其全部数据；删除 / 清理会同时清理消息、用量记录、
          子代理记录与缓存文件，不可恢复。
        </p>

        {totalSelected > 0 && (
          <div
            className="za-row-between"
            style={{
              padding: "8px 12px",
              marginBottom: 12,
              borderRadius: 8,
              border: "1px solid #e5484d55",
              background: "rgba(229,72,77,0.08)",
            }}
          >
            <span className="za-faint" style={{ fontSize: "var(--fs-sm)" }}>
              已选 {selectedProjects.size} 个项目、{selectedSessions.size} 个会话
            </span>
            <div className="za-row">
              <button className="za-btn za-btn-sm" onClick={clearSelection} disabled={deleting}>
                取消选择
              </button>
              {selectedSessions.size > 0 && (
                <button
                  className="za-btn za-btn-sm"
                  onClick={doArchiveSelected}
                  disabled={archiving || deleting}
                  title="批量归档所选会话（不删除：zcode 会话列表隐藏，可随时恢复）"
                >
                  {archiving ? "归档中…" : "归档所选会话"}
                </button>
              )}
              {selectedProjects.size > 0 && (
                <button
                  className="za-btn za-btn-sm"
                  onClick={doArchiveSelectedProjects}
                  disabled={archiving || deleting}
                  title="批量归档所选项目的全部活跃会话（不删除：可随时恢复）"
                >
                  {archiving ? "归档中…" : "归档所选项目"}
                </button>
              )}
              <button
                className="za-btn za-btn-sm"
                style={{ color: "#e5484d", borderColor: "#e5484d" }}
                onClick={() => doDelete()}
                disabled={deleting}
              >
                <IconTrash width={13} height={13} />
                {deleting ? "删除中…" : "删除所选"}
              </button>
            </div>
          </div>
        )}

        {loading ? (
          <div className="za-empty">加载中…</div>
        ) : visibleProjects.length === 0 ? (
          <div className="za-empty">
            {list.length === 0
              ? "暂无项目（未找到 zcode 会话数据）"
              : "活跃项目为空，开启「查看历史」查看已归档项目"}
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {visibleProjects.map((p) => {
              const isOpen = expandedId === p.id;
              const sessLoading = sessionsLoading[p.id] === true;
              const activeCount = p.sessions - p.archivedSessions;
              const fullyArchived = p.archivedSessions >= p.sessions;
              return (
                <div
                  key={p.id}
                  style={{
                    borderRadius: 8,
                    border: `1px solid ${isOpen ? "var(--accent)" : "var(--glass-border)"}`,
                    overflow: "hidden",
                    opacity: fullyArchived ? 0.72 : 1,
                  }}
                >
                  {/* 项目行 */}
                  <div
                    className="za-row-between"
                    style={{ padding: "8px 12px", cursor: "pointer" }}
                    onClick={() => toggleExpand(p)}
                  >
                    <div className="za-row" style={{ gap: 10, minWidth: 0, flex: 1 }}>
                      <input
                        type="checkbox"
                        checked={selectedProjects.has(p.id)}
                        onClick={(e) => e.stopPropagation()}
                        onChange={() => toggleProject(p.id)}
                        style={{ accentColor: "var(--accent)", flexShrink: 0 }}
                      />
                      <IconFolder
                        width={16}
                        height={16}
                        style={{ color: "var(--accent)", flexShrink: 0 }}
                      />
                      <div style={{ minWidth: 0 }}>
                        <div
                          style={{
                            fontWeight: 500,
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                          }}
                        >
                          {baseName(p.directory)}
                          <span
                            className="za-badge"
                            style={{
                              marginLeft: 8,
                              background: fullyArchived
                                ? "rgba(148,163,184,0.12)"
                                : "rgba(34,197,94,0.15)",
                              color: fullyArchived
                                ? "var(--text-tertiary)"
                                : "#22C55E",
                            }}
                            title={
                              fullyArchived
                                ? "全部会话均已归档"
                                : "存在活跃会话"
                            }
                          >
                            {fullyArchived ? "归档" : "活跃"}
                          </span>
                          <span className="za-badge za-badge-neutral" style={{ marginLeft: 8 }}>
                            {p.sessions} 会话 · {activeCount} 活跃 · {p.archivedSessions} 归档
                          </span>
                        </div>
                        <div
                          className="za-faint za-mono"
                          style={{ fontSize: "var(--fs-xs)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
                          title={p.directory}
                        >
                          {p.directory}
                        </div>
                      </div>
                    </div>
                    <div className="za-row za-faint za-mono" style={{ gap: 14, fontSize: "var(--fs-xs)", flexShrink: 0 }}>
                      {activeCount > 0 && (
                        <button
                          className="za-btn za-btn-sm"
                          style={{ height: 24, padding: "0 10px", flexShrink: 0 }}
                          onClick={(e) => {
                            e.stopPropagation();
                            doArchiveProject(p);
                          }}
                          title="归档项目：批量归档该项目全部活跃会话（对称于 zcode 的「归档项目」），可随时恢复"
                        >
                          归档项目
                        </button>
                      )}
                      {p.archivedSessions > 0 && (
                        <button
                          className="za-btn za-btn-sm"
                          style={{ height: 24, padding: "0 10px", flexShrink: 0 }}
                          onClick={(e) => {
                            e.stopPropagation();
                            doRestoreProject(p);
                          }}
                          title="恢复项目：恢复该项目全部已归档会话，可在 zcode 中继续对话"
                        >
                          恢复项目
                        </button>
                      )}
                      <span title="对话次数（含子代理）">对话 {p.turns.toLocaleString()}</span>
                      <span title="token 总消耗（与用量查询同口径）">
                        {formatUnits(p.totalTokens)} tok
                      </span>
                      <span title="最近活跃 / 创建时间">
                        {fmtTime(p.timeUpdatedMs)} · {fmtTime(p.timeCreatedMs)}
                      </span>
                      <span>{isOpen ? "▲" : "▼"}</span>
                    </div>
                  </div>

                  {/* 会话明细 */}
                  {isOpen && (
                    <div style={{ borderTop: "1px solid var(--glass-border)", padding: "8px 12px 12px" }}>
                      <div className="za-row-between" style={{ marginBottom: 8 }}>
                        <span className="za-faint za-mono" style={{ fontSize: "var(--fs-xs)" }}>
                          共 {sessions.length} 会话 · {sessions.length - archivedCount} 活跃 · {archivedCount} 归档（按最后活跃排序）
                        </span>
                        <button
                          className="za-btn za-btn-sm"
                          data-active={showProjHistory}
                          style={showProjHistory ? activeBtnStyle : undefined}
                          onClick={() =>
                            setProjectHistory((m) => ({
                              ...m,
                              [p.id]: !(m[p.id] === true),
                            }))
                          }
                          title="开启后显示该项目已归档的会话"
                        >
                          查看历史会话
                        </button>
                      </div>
                      {sessLoading ? (
                        <div className="za-empty">加载会话…</div>
                      ) : visibleSessions.length === 0 ? (
                        <div className="za-empty">
                          {archivedCount > 0
                            ? "无活跃会话，开启「查看历史」查看归档会话"
                            : "该项目暂无会话"}
                        </div>
                      ) : (
                        <div className="za-usage-scroll">
                          <table className="za-usage-table">
                            <thead>
                              <tr>
                                <th style={{ width: 28 }}>
                                  <input
                                    type="checkbox"
                                    checked={allSessionsSelected}
                                    ref={(el) => {
                                      if (el)
                                        el.indeterminate =
                                          someSessionsSelected &&
                                          !allSessionsSelected;
                                    }}
                                    onChange={toggleSelectAllSessions}
                                    title="全选 / 取消全选当前显示的会话"
                                    style={{
                                      accentColor: "var(--accent)",
                                      verticalAlign: "middle",
                                    }}
                                  />
                                </th>
                                <th className="za-ut-l">会话</th>
                                <th>对话</th>
                                <th>调用</th>
                                <th>输入</th>
                                <th>输出</th>
                                <th>总量</th>
                                <th>创建时间</th>
                                <th>最近活跃</th>
                                <th style={{ width: 158 }}></th>
                              </tr>
                            </thead>
                            <tbody>
                              {visibleSessions.map((s) => {
                                const editing = editingId === s.id;
                                const archived = s.archived;
                                return (
                                  <tr key={s.id} style={{ opacity: archived ? 0.62 : undefined }}>
                                    <td>
                                      <input
                                        type="checkbox"
                                        checked={selectedSessions.has(s.id)}
                                        onChange={() => toggleSession(s.id)}
                                        style={{ accentColor: "var(--accent)" }}
                                      />
                                    </td>
                                    <td className="za-ut-l" title={s.title}>
                                      {editing ? (
                                        <div className="za-row" style={{ gap: 6 }}>
                                          <input
                                            className="za-input za-btn-sm"
                                            style={{ height: 30, minWidth: 200 }}
                                            value={editTitle}
                                            autoFocus
                                            placeholder="输入会话名称"
                                            onChange={(e) => setEditTitle(e.target.value)}
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
                                        <span style={{ display: "flex", alignItems: "center", gap: 6, minWidth: 0 }}>
                                          <span
                                            style={{
                                              flex: 1,
                                              minWidth: 0,
                                              overflow: "hidden",
                                              textOverflow: "ellipsis",
                                              whiteSpace: "nowrap",
                                            }}
                                          >
                                            {s.title || "(无标题)"}
                                          </span>
                                          {s.titleSource === "custom" && (
                                            <span className="za-badge za-badge-neutral" style={{ flexShrink: 0 }}>
                                              自定义
                                            </span>
                                          )}
                                          <span
                                            className="za-badge"
                                            style={{
                                              flexShrink: 0,
                                              background: archived
                                                ? "rgba(148,163,184,0.12)"
                                                : "rgba(34,197,94,0.15)",
                                              color: archived
                                                ? "var(--text-tertiary)"
                                                : "#22C55E",
                                            }}
                                            title={
                                              archived
                                                ? s.timeArchivedMs
                                                  ? `归档于 ${fmtTime(s.timeArchivedMs)}`
                                                  : "已归档（zcode 任务索引）"
                                                : "活跃会话"
                                            }
                                          >
                                            {archived ? "归档" : "活跃"}
                                          </span>
                                        </span>
                                      )}
                                    </td>
                                    <td className="za-mono">{s.turns.toLocaleString()}</td>
                                    <td className="za-mono za-faint">{s.calls.toLocaleString()}</td>
                                    <td className="za-mono za-faint">{formatUnits(s.inputTokens)}</td>
                                    <td className="za-mono">{formatUnits(s.outputTokens)}</td>
                                    <td className="za-mono" style={{ fontWeight: 600 }}>
                                      {formatUnits(s.totalTokens)}
                                    </td>
                                    <td className="za-mono za-faint">{fmtTime(s.timeCreatedMs)}</td>
                                    <td className="za-mono za-faint">{fmtTime(s.timeUpdatedMs)}</td>
                                    <td>
                                      {!editing && (
                                        <div className="za-row" style={{ gap: 4 }}>
                                          {archived ? (
                                            <button
                                              className="za-btn za-btn-sm"
                                              style={{ height: 24, padding: "0 10px" }}
                                              onClick={() => doRestore(s)}
                                              title="恢复会话：清掉归档标记，回 zcode 会话列表继续对话"
                                            >
                                              恢复会话
                                            </button>
                                          ) : (
                                            <button
                                              className="za-btn za-btn-sm"
                                              style={{ height: 24, padding: "0 10px" }}
                                              onClick={() => doArchiveSession(s)}
                                              title="归档会话：zcode 会话列表隐藏，可随时恢复"
                                            >
                                              归档会话
                                            </button>
                                          )}
                                          <button
                                            className="za-icon-btn"
                                            style={{ width: 26, height: 26 }}
                                            onClick={() => startEdit(s)}
                                            title="修改名称"
                                          >
                                            <IconEdit width={13} height={13} />
                                          </button>
                                          <button
                                            className="za-icon-btn"
                                            style={{ width: 26, height: 26 }}
                                            onClick={() => doDelete([s.id], [])}
                                            title="删除会话"
                                          >
                                            <IconTrash width={13} height={13} />
                                          </button>
                                        </div>
                                      )}
                                    </td>
                                  </tr>
                                );
                              })}
                            </tbody>
                          </table>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* 清理缓存弹窗 */}
      {cacheOpen && (
        <div className="rd-overlay" onClick={() => !cleaning && setCacheOpen(false)}>
          <div
            className="za-glass-strong rd-card"
            style={{ width: 420 }}
            onClick={(e) => e.stopPropagation()}
          >
            <strong>清理缓存数据</strong>
            <p className="za-muted" style={{ fontSize: "var(--fs-sm)", margin: 0 }}>
              删除最后活跃时间早于所选时间点的会话及其全部数据：消息、子代理执行记录、工具结果缓存、
              执行日志与用量记录，并尝试压缩会话数据库进一步释放空间。
            </p>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              {CACHE_OPTIONS.map((o) => (
                <label
                  key={o.days}
                  className="za-row"
                  style={{ gap: 8, cursor: "pointer", fontSize: "var(--fs-sm)" }}
                >
                  <input
                    type="radio"
                    name="cache-days"
                    checked={cacheDays === o.days}
                    onChange={() => setCacheDays(o.days)}
                    style={{ accentColor: "var(--accent)" }}
                  />
                  {o.label}的会话
                </label>
              ))}
            </div>
            <div
              className="za-faint"
              style={{
                fontSize: "var(--fs-xs)",
                padding: "8px 10px",
                borderRadius: 8,
                border: "1px solid var(--glass-border)",
                lineHeight: 1.6,
              }}
            >
              {cacheStatsLoading
                ? "统计中…"
                : cacheStats
                ? cacheStats.sessions === 0
                  ? "该时间范围内没有可清理的会话"
                  : `将删除 ${cacheStats.sessions} 个会话（含子代理共 ${cacheStats.totalSessions} 个），预计释放约 ${fmtBytes(cacheStats.freedBytes)} 的缓存文件。`
                : "—"}
            </div>
            <div className="za-row" style={{ justifyContent: "flex-end", gap: 8 }}>
              <button
                className="za-btn za-btn-sm"
                onClick={() => setCacheOpen(false)}
                disabled={cleaning}
              >
                取消
              </button>
              <button
                className="za-btn za-btn-sm"
                style={{ color: "#e5484d", borderColor: "#e5484d" }}
                disabled={cleaning || !cacheStats || cacheStats.sessions === 0}
                onClick={doCacheCleanup}
              >
                <IconTrash width={13} height={13} />
                {cleaning ? "清理中…" : "确认清理"}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
