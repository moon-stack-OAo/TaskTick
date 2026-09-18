import type { SchedulerStatus } from "../types/task";
import { Icon } from "./Icons";

export type AppPage = "tasks" | "logs" | "settings";

interface SidebarProps {
  page: AppPage;
  onNavigate: (page: AppPage) => void;
  taskCount: number;
  failLogCount: number;
  scheduler: SchedulerStatus | null;
  pendingWecomCount?: number;
}

export function Sidebar({
  page,
  onNavigate,
  taskCount,
  failLogCount,
  scheduler,
  pendingWecomCount = 0,
}: SidebarProps) {
  const items: { id: AppPage; label: string; icon: typeof Icon.tasks; badge?: number | null }[] = [
    { id: "tasks", label: "任务", icon: Icon.tasks, badge: taskCount },
    {
      id: "logs",
      label: "执行日志",
      icon: Icon.logs,
      badge: failLogCount > 0 ? failLogCount : null,
    },
    { id: "settings", label: "设置", icon: Icon.settings },
  ];

  const paused = scheduler?.paused ?? false;
  const running = (scheduler?.running ?? false) && !paused;
  const enabled = scheduler?.enabledTaskCount ?? 0;
  const statusText = !scheduler
    ? "调度器加载中…"
    : paused
      ? `调度已暂停 · ${enabled} 个启用`
      : running
        ? `调度运行中 · ${enabled} 个启用`
        : "调度器已停止";
  const pendingText =
    pendingWecomCount > 0 ? `企微待发送 ${pendingWecomCount}` : null;

  return (
    <aside className="sidebar">
      <div className="nav-label">导航</div>
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          className={`nav-item${page === item.id ? " active" : ""}`}
          onClick={() => onNavigate(item.id)}
        >
          <span className="icon">{item.icon}</span>
          {item.label}
          {item.badge != null ? <span className="badge">{item.badge}</span> : null}
        </button>
      ))}
      <div className="sidebar-foot">
        <div className="status-chip" title={scheduler?.lastTickAt || undefined}>
          <span className={`dot${running ? " ok" : ""}`} />
          {statusText}
        </div>
        {pendingText ? (
          <div className="status-chip pending-chip" style={{ marginTop: 6 }}>
            <span className="dot warn" />
            {pendingText}
          </div>
        ) : null}
      </div>
    </aside>
  );
}
