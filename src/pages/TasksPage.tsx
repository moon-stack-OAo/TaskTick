import { useMemo, useState } from "react";
import type { Task } from "../types/task";
import {
  ACTION_LABELS,
  TRIGGER_LABELS,
  summarizeActions,
  summarizeTrigger,
} from "../utils/labels";
import { Icon } from "../components/Icons";

type Filter = "all" | "enabled" | "disabled";

interface TasksPageProps {
  tasks: Task[];
  query: string;
  busy: boolean;
  /** 调度是否暂停；暂停时仍显示「若恢复后」的下次时间并标注 */
  schedulerPaused?: boolean;
  /** 空闲发送是否开启；开启时含企微任务的「执行」会入队 */
  idleSendEnabled?: boolean;
  onCreate: () => void;
  onEdit: (task: Task) => void;
  onToggle: (task: Task) => void;
  onDelete: (task: Task) => void;
  onRun: (task: Task) => void;
  onForceRun?: (task: Task) => void;
  hasWecom?: (task: Task) => boolean;
}

function nextRunLabel(
  task: Task,
  schedulerPaused?: boolean,
): { text: string; muted: boolean } {
  if (!task.enabled) {
    return { text: "不再执行", muted: true };
  }
  if (task.trigger.type === "once" && !task.nextRunAt) {
    return { text: "不再执行", muted: true };
  }
  if (!task.nextRunAt) {
    return { text: "—", muted: true };
  }
  if (schedulerPaused) {
    return { text: `若恢复 · ${task.nextRunAt}`, muted: false };
  }
  return { text: task.nextRunAt, muted: false };
}

export function TasksPage({
  tasks,
  query,
  busy,
  schedulerPaused,
  idleSendEnabled = true,
  onCreate,
  onEdit,
  onToggle,
  onDelete,
  onRun,
  onForceRun,
  hasWecom,
}: TasksPageProps) {
  const [filter, setFilter] = useState<Filter>("all");

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return tasks.filter((t) => {
      if (filter === "enabled" && !t.enabled) return false;
      if (filter === "disabled" && t.enabled) return false;
      if (!q) return true;
      const hay = [
        t.name,
        summarizeTrigger(t.trigger),
        summarizeActions(t.actions),
        TRIGGER_LABELS[t.trigger.type],
        t.actions.map((a) => ACTION_LABELS[a.type]).join(" "),
      ]
        .join(" ")
        .toLowerCase();
      return hay.includes(q);
    });
  }, [tasks, filter, query]);

  const enabledCount = tasks.filter((t) => t.enabled).length;
  const disabledCount = tasks.length - enabledCount;

  return (
    <div className="content">
      <div className="notice" style={{ marginBottom: 12 }}>
        <svg className="notice-icon" viewBox="0 0 18 18" fill="none" aria-hidden="true">
          <circle cx="9" cy="9" r="7.2" stroke="currentColor" strokeWidth="1.4" />
          <path
            d="M9 5.2v5.2M9 12.6h.01"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
          />
        </svg>
        <div>
          <strong>调度已接入</strong>
          ：可执行「打开应用 / 网址 / 脚本 / 企微私聊」。含企微的任务默认入空闲队列（不抢前台）；可用「强制立即」跳过空闲。详见设置页。
        </div>
      </div>

      <div className="subbar">
        <div className="filter-group">
          {(
            [
              { id: "all" as const, label: `全部 ${tasks.length}` },
              { id: "enabled" as const, label: `已启用 ${enabledCount}` },
              { id: "disabled" as const, label: `已禁用 ${disabledCount}` },
            ] as const
          ).map((f) => (
            <button
              key={f.id}
              type="button"
              className={`filter-chip${filter === f.id ? " active" : ""}`}
              onClick={() => setFilter(f.id)}
            >
              {f.label}
            </button>
          ))}
        </div>
        <div className="muted" style={{ fontSize: 12.5 }}>
          当前展示 {filtered.length} 条
        </div>
      </div>

      <div className="card">
        {filtered.length === 0 ? (
          <div className="empty">
            <div className="empty-icon">{Icon.tasks}</div>
            <h3>{tasks.length === 0 ? "还没有任务" : "没有匹配的任务"}</h3>
            <p>
              {tasks.length === 0
                ? "点击右上角「新建任务」创建第一条定时任务，例如每天打开早报、运行备份脚本或发送企微私聊。"
                : "试试切换「全部 / 已启用 / 已禁用」，或清空搜索关键词。"}
            </p>
            {tasks.length === 0 ? (
              <div style={{ marginTop: 14 }}>
                <button className="btn btn-primary" type="button" onClick={onCreate}>
                  {Icon.plus} 新建任务
                </button>
              </div>
            ) : null}
          </div>
        ) : (
          <div className="task-list">
            {filtered.map((task) => (
              <div className="task-row" key={task.id}>
                <div className="task-row-toggle">
                  <button
                    type="button"
                    className={`switch${task.enabled ? " on" : ""}`}
                    aria-pressed={task.enabled}
                    aria-label={task.enabled ? "禁用任务" : "启用任务"}
                    disabled={busy}
                    onClick={() => onToggle(task)}
                  />
                </div>
                <div className="task-row-main">
                  <div className="task-row-title">
                    <span className="name" title={task.name}>
                      {task.name}
                    </span>
                    <span className={`pill ${task.enabled ? "pill-on" : "pill-off"}`}>
                      <span className="pill-dot" />
                      {task.enabled ? "已启用" : "已禁用"}
                    </span>
                  </div>
                  <div className="task-row-summary">
                    <span className="pill pill-info">{TRIGGER_LABELS[task.trigger.type]}</span>
                    <span className="mono">{summarizeTrigger(task.trigger)}</span>
                    <span className="sep" aria-hidden="true" />
                    <span className="sum-text" title={summarizeActions(task.actions)}>
                      {ACTION_LABELS[task.actions[0]?.type] || "—"}
                      {" · "}
                      {summarizeActions(task.actions)}
                      {task.actions.length > 1 ? ` 等 ${task.actions.length} 项` : ""}
                    </span>
                  </div>
                </div>
                <div className="task-row-aside">
                  <div className="task-row-last">
                    {(() => {
                      const next = nextRunLabel(task, schedulerPaused);
                      return (
                        <div style={{ marginBottom: 6 }}>
                          <div className="muted" style={{ fontSize: 11, marginBottom: 2 }}>
                            下次执行
                          </div>
                          <div
                            className={`mono${next.muted ? " muted" : ""}`}
                            style={{ fontSize: 12.5 }}
                            title={
                              schedulerPaused && task.nextRunAt
                                ? "调度已暂停，显示为恢复后的预计时间"
                                : undefined
                            }
                          >
                            {next.text}
                          </div>
                        </div>
                      );
                    })()}
                    {task.lastRunAt ? (
                      <>
                        <span
                          className={`pill ${
                            task.lastStatus === "success"
                              ? "pill-ok"
                              : task.lastStatus === "failed"
                                ? "pill-fail"
                                : "pill-info"
                          }`}
                        >
                          <span className="pill-dot" />
                          {task.lastStatus === "success"
                            ? "最近成功"
                            : task.lastStatus === "failed"
                              ? "最近失败"
                              : "最近执行"}
                        </span>
                        <div className="task-meta mono">{task.lastRunAt}</div>
                      </>
                    ) : (
                      <span className="muted">尚未执行</span>
                    )}
                  </div>
                  <div className="row-actions">
                    <button
                      className="btn btn-primary btn-sm"
                      type="button"
                      title={
                        idleSendEnabled && hasWecom?.(task)
                          ? "加入空闲发送队列"
                          : "立即执行"
                      }
                      disabled={busy}
                      onClick={() => onRun(task)}
                    >
                      {Icon.play}{" "}
                      {idleSendEnabled && hasWecom?.(task) ? "排队" : "执行"}
                    </button>
                    {idleSendEnabled && hasWecom?.(task) && onForceRun ? (
                      <button
                        className="btn btn-secondary btn-sm"
                        type="button"
                        title="跳过空闲检测，立即抢前台发送"
                        disabled={busy}
                        onClick={() => onForceRun(task)}
                      >
                        强制立即
                      </button>
                    ) : null}
                    <button
                      className="btn btn-secondary btn-sm"
                      type="button"
                      disabled={busy}
                      onClick={() => onEdit(task)}
                    >
                      {Icon.edit} 编辑
                    </button>
                    <button
                      className="btn btn-danger-ghost btn-sm"
                      type="button"
                      disabled={busy}
                      onClick={() => onDelete(task)}
                    >
                      删除
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
