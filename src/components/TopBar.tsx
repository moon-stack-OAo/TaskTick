import type { IdleSendStatus } from "../types/task";
import type { AppPage } from "./Sidebar";
import { Icon } from "./Icons";

interface TopBarProps {
  page: AppPage;
  taskQuery: string;
  logQuery: string;
  busy: boolean;
  idleStatus?: IdleSendStatus | null;
  pendingCount?: number;
  onTaskQueryChange: (value: string) => void;
  onLogQueryChange: (value: string) => void;
  onCreate: () => void;
  onRefresh: () => void;
  onClearLogs: () => void;
  onCancelAllPending?: () => void;
}

const META: Record<AppPage, { title: string; desc: string }> = {
  tasks: { title: "任务", desc: "管理定时任务与动作，支持企微私聊插件" },
  logs: { title: "执行日志", desc: "按任务 / 状态 / 时间筛选，可导出当前结果" },
  settings: { title: "设置", desc: "通用 · 调度 · 企微 · 数据 · 关于" },
};

export function TopBar({
  page,
  taskQuery,
  logQuery,
  busy,
  idleStatus,
  pendingCount = 0,
  onTaskQueryChange,
  onLogQueryChange,
  onCreate,
  onRefresh,
  onClearLogs,
  onCancelAllPending,
}: TopBarProps) {
  const meta = META[page];
  const phaseHint =
    idleStatus?.phase === "countdown"
      ? `倒计时 ${idleStatus.countdownRemainingSecs}s`
      : idleStatus?.phase === "running"
        ? "发送中"
        : pendingCount > 0
          ? `待发送 ${pendingCount}`
          : null;

  return (
    <div className="topbar">
      <div className="topbar-left">
        <h1 className="topbar-title">{meta.title}</h1>
        <p className="topbar-desc">{meta.desc}</p>
      </div>
      <div className="topbar-actions">
        {phaseHint ? (
          <span className="idle-chip" title="企微空闲发送队列">
            {phaseHint}
            {pendingCount > 0 && onCancelAllPending ? (
              <button
                type="button"
                className="idle-chip-btn"
                onClick={onCancelAllPending}
              >
                清空
              </button>
            ) : null}
          </span>
        ) : null}

        {page === "tasks" ? (
          <>
            <div className="search-wrap">
              {Icon.search}
              <input
                className="search"
                placeholder="搜索任务名、触发器或动作"
                value={taskQuery}
                onChange={(e) => onTaskQueryChange(e.target.value)}
              />
            </div>
            <button
              className="btn btn-secondary btn-sm"
              type="button"
              disabled={busy}
              onClick={onRefresh}
            >
              刷新
            </button>
            <button className="btn btn-primary" type="button" disabled={busy} onClick={onCreate}>
              {Icon.plus} 新建任务
            </button>
          </>
        ) : null}

        {page === "logs" ? (
          <>
            <div className="search-wrap">
              {Icon.search}
              <input
                className="search"
                placeholder="搜索任务名或详情"
                value={logQuery}
                onChange={(e) => onLogQueryChange(e.target.value)}
              />
            </div>
            <button
              className="btn btn-secondary btn-sm"
              type="button"
              disabled={busy}
              onClick={onRefresh}
            >
              刷新
            </button>
            <button
              className="btn btn-danger-ghost btn-sm"
              type="button"
              disabled={busy}
              onClick={onClearLogs}
            >
              清空日志
            </button>
          </>
        ) : null}

        {page === "settings" ? (
          <button
            className="btn btn-secondary btn-sm"
            type="button"
            disabled={busy}
            onClick={onRefresh}
          >
            刷新
          </button>
        ) : null}
      </div>
    </div>
  );
}
