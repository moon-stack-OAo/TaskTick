import { Fragment, useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { exportFilteredLogs, writeTextFile } from "../api/tasks";
import type { ExecutionLog, LogFilter, Task } from "../types/task";
import { stepStatusLabel } from "../utils/labels";
import { Icon } from "../components/Icons";

type StatusFilter = "all" | "success" | "failed" | "running" | "skipped";

interface LogsPageProps {
  logs: ExecutionLog[];
  tasks: Task[];
  query: string;
  busy?: boolean;
  onToast: (message: string) => void;
}

function toFilter(
  status: StatusFilter,
  taskId: string,
  query: string,
  timeFrom: string,
  timeTo: string,
): LogFilter {
  return {
    status: status === "all" ? null : status,
    taskId: taskId || null,
    keyword: query.trim() || null,
    timeFrom: timeFrom.trim() || null,
    timeTo: timeTo.trim() || null,
    limit: 500,
  };
}

export function LogsPage({ logs, tasks, query, busy, onToast }: LogsPageProps) {
  const [status, setStatus] = useState<StatusFilter>("all");
  const [taskId, setTaskId] = useState("");
  const [timeFrom, setTimeFrom] = useState("");
  const [timeTo, setTimeTo] = useState("");
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [exporting, setExporting] = useState(false);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return logs.filter((l) => {
      if (status !== "all" && l.status !== status) return false;
      if (taskId && l.taskId !== taskId) return false;
      if (timeFrom.trim() && l.time < timeFrom.trim()) return false;
      if (timeTo.trim() && l.time > timeTo.trim()) return false;
      if (!q) return true;
      return (
        l.taskName.toLowerCase().includes(q) ||
        (l.detail || "").toLowerCase().includes(q) ||
        l.time.includes(q) ||
        l.taskId.toLowerCase().includes(q)
      );
    });
  }, [logs, status, taskId, timeFrom, timeTo, query]);

  const successCount = logs.filter((l) => l.status === "success").length;
  const failedCount = logs.filter((l) => l.status === "failed").length;

  async function handleExport(format: "json" | "txt") {
    if (exporting) return;
    setExporting(true);
    try {
      const filter = toFilter(status, taskId, query, timeFrom, timeTo);
      const content = await exportFilteredLogs(filter, format);
      if (!content.trim() || content === "[]") {
        onToast("当前筛选无日志可导出");
        return;
      }
      const stamp = new Date()
        .toISOString()
        .replace(/[:.]/g, "-")
        .slice(0, 19);
      const path = await save({
        title: "导出筛选日志",
        defaultPath: `tasktick-logs-${stamp}.${format}`,
        filters: [
          format === "json"
            ? { name: "JSON", extensions: ["json"] }
            : { name: "文本", extensions: ["txt"] },
        ],
      });
      if (!path) return;
      await writeTextFile(path, content);
      onToast(`已导出 ${filtered.length} 条日志`);
    } catch (e) {
      onToast(e instanceof Error ? e.message : String(e));
    } finally {
      setExporting(false);
    }
  }

  function resetFilters() {
    setStatus("all");
    setTaskId("");
    setTimeFrom("");
    setTimeTo("");
  }

  return (
    <div className="content">
      <div className="subbar" style={{ flexWrap: "wrap", gap: 10 }}>
        <div className="filter-group">
          {(
            [
              { id: "all" as const, label: `全部 ${logs.length}` },
              { id: "success" as const, label: `成功 ${successCount}` },
              { id: "failed" as const, label: `失败 ${failedCount}` },
              { id: "skipped" as const, label: "跳过" },
              { id: "running" as const, label: "执行中" },
            ] as const
          ).map((f) => (
            <button
              key={f.id}
              type="button"
              className={`filter-chip${status === f.id ? " active" : ""}`}
              onClick={() => setStatus(f.id)}
            >
              {f.label}
            </button>
          ))}
        </div>
        <div className="muted" style={{ fontSize: 12.5 }}>
          当前 {filtered.length} 条 · 展开可查看步骤
        </div>
      </div>

      <div className="card" style={{ marginBottom: 12, padding: "12px 14px" }}>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "minmax(140px, 1.2fr) repeat(2, minmax(150px, 1fr)) auto",
            gap: 10,
            alignItems: "end",
          }}
        >
          <div className="field" style={{ margin: 0 }}>
            <label htmlFor="log-task">任务</label>
            <select
              id="log-task"
              className="select"
              value={taskId}
              onChange={(e) => setTaskId(e.target.value)}
            >
              <option value="">全部任务</option>
              {tasks.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name}
                </option>
              ))}
            </select>
          </div>
          <div className="field" style={{ margin: 0 }}>
            <label htmlFor="log-from">开始时间</label>
            <input
              id="log-from"
              className="input"
              type="text"
              placeholder="YYYY-MM-DD 或完整时间"
              value={timeFrom}
              onChange={(e) => setTimeFrom(e.target.value)}
            />
          </div>
          <div className="field" style={{ margin: 0 }}>
            <label htmlFor="log-to">结束时间</label>
            <input
              id="log-to"
              className="input"
              type="text"
              placeholder="YYYY-MM-DD HH:MM:SS"
              value={timeTo}
              onChange={(e) => setTimeTo(e.target.value)}
            />
          </div>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <button
              className="btn btn-secondary btn-sm"
              type="button"
              onClick={resetFilters}
            >
              重置筛选
            </button>
            <button
              className="btn btn-secondary btn-sm"
              type="button"
              disabled={busy || exporting}
              onClick={() => void handleExport("json")}
            >
              导出 JSON
            </button>
            <button
              className="btn btn-secondary btn-sm"
              type="button"
              disabled={busy || exporting}
              onClick={() => void handleExport("txt")}
            >
              导出 TXT
            </button>
          </div>
        </div>
        <p className="hint" style={{ margin: "8px 0 0", fontSize: 12, color: "var(--muted)" }}>
          时间按字符串比较，建议格式与日志一致（本地 <span className="mono">YYYY-MM-DD HH:MM:SS</span>
          ）。顶栏关键词与本区条件同时生效；导出为「当前筛选」结果。
        </p>
      </div>

      <div className="card">
        {filtered.length === 0 ? (
          <div className="empty">
            <div className="empty-icon">{Icon.logs}</div>
            <h3>{logs.length === 0 ? "暂无执行记录" : "没有匹配的日志"}</h3>
            <p>
              {logs.length === 0
                ? "点击任务列表的「执行」，或等待调度触发后，结果会出现在这里。"
                : "试试调整筛选条件或清空搜索。"}
            </p>
          </div>
        ) : (
          <table className="task-table">
            <thead>
              <tr>
                <th style={{ width: 36 }} />
                <th style={{ width: 168 }}>时间</th>
                <th>任务</th>
                <th style={{ width: 110 }}>结果</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((log) => {
                const open = !!expanded[log.id];
                return (
                  <Fragment key={log.id}>
                    <tr
                      className="log-row"
                      onClick={() =>
                        setExpanded((s) => ({ ...s, [log.id]: !open }))
                      }
                    >
                      <td style={{ color: "var(--muted)" }}>
                        <span
                          style={{
                            display: "inline-block",
                            transform: open ? "rotate(180deg)" : "none",
                            transition: "transform .15s",
                          }}
                        >
                          {Icon.chevron}
                        </span>
                      </td>
                      <td className="mono">{log.time}</td>
                      <td>{log.taskName}</td>
                      <td>
                        <span
                          className={`pill ${
                            log.status === "success"
                              ? "pill-ok"
                              : log.status === "failed"
                                ? "pill-fail"
                                : "pill-info"
                          }`}
                        >
                          <span className="pill-dot" />
                          {log.status === "success"
                            ? "成功"
                            : log.status === "failed"
                              ? "失败"
                              : log.status}
                        </span>
                      </td>
                    </tr>
                    {open ? (
                      <tr>
                        <td colSpan={4} style={{ padding: 0 }}>
                          <div className="log-detail">
                            {Array.isArray(log.steps) && log.steps.length > 0 ? (
                              <ul className="timeline">
                                {log.steps.map((step, i) => (
                                  <li className="timeline-item" key={`${log.id}-step-${i}`}>
                                    <span className={`tl-dot ${step.status}`} />
                                    <div className="tl-body">
                                      <div className="tl-title">
                                        {step.title}
                                        <span
                                          className={`pill ${
                                            step.status === "ok"
                                              ? "pill-ok"
                                              : step.status === "fail"
                                                ? "pill-fail"
                                                : "pill-off"
                                          }`}
                                        >
                                          {stepStatusLabel(step.status)}
                                        </span>
                                      </div>
                                      <div className="tl-meta">
                                        {step.time || "—"} · {step.note || ""}
                                      </div>
                                    </div>
                                  </li>
                                ))}
                              </ul>
                            ) : null}
                            {log.detail ? (
                              <div className="log-detail-raw">{log.detail}</div>
                            ) : (
                              <div className="muted">无详情</div>
                            )}
                          </div>
                        </td>
                      </tr>
                    ) : null}
                  </Fragment>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
