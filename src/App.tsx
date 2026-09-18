import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  cancelPendingWecom,
  clearLogs,
  deleteTask,
  getIdleSendStatus,
  getSchedulerStatus,
  getStoreInfo,
  listLogs,
  listPendingWecom,
  listTasks,
  reloadScheduler,
  runTaskNow,
  saveTask,
  toggleTask,
} from "./api/tasks";
import { Sidebar, type AppPage } from "./components/Sidebar";
import { TaskEditor } from "./components/TaskEditor";
import { TopBar } from "./components/TopBar";
import { LogsPage } from "./pages/LogsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { TasksPage } from "./pages/TasksPage";
import type {
  ExecutionLog,
  IdleSendStatus,
  PendingWecomItem,
  SchedulerStatus,
  StoreInfo,
  Task,
} from "./types/task";
import "./styles/tokens.css";
import "./styles/app.css";

export default function App() {
  const [page, setPage] = useState<AppPage>("tasks");
  const [info, setInfo] = useState<StoreInfo | null>(null);
  const [scheduler, setScheduler] = useState<SchedulerStatus | null>(null);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [logs, setLogs] = useState<ExecutionLog[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [taskQuery, setTaskQuery] = useState("");
  const [logQuery, setLogQuery] = useState("");
  const [editing, setEditing] = useState<Task | null | undefined>(undefined);
  const [toast, setToast] = useState("");
  const toastTimer = useRef<number | null>(null);
  const [idleStatus, setIdleStatus] = useState<IdleSendStatus | null>(null);
  const [pendingWecom, setPendingWecom] = useState<PendingWecomItem[]>([]);
  const [settingsRefreshToken, setSettingsRefreshToken] = useState(0);

  const showToast = useCallback((message: string) => {
    setToast(message);
    if (toastTimer.current != null) window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(""), 2600);
  }, []);

  const refreshIdle = useCallback(async () => {
    try {
      const [status, pending] = await Promise.all([
        getIdleSendStatus(),
        listPendingWecom(),
      ]);
      setIdleStatus(status);
      setPendingWecom(pending);
    } catch {
      /* ignore */
    }
  }, []);

  const refresh = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      const [nextInfo, nextTasks, nextLogs, nextScheduler] = await Promise.all([
        getStoreInfo(),
        listTasks(),
        listLogs(500),
        getSchedulerStatus(),
      ]);
      setInfo(nextInfo);
      setTasks(nextTasks);
      setLogs(nextLogs);
      setScheduler(nextScheduler);
      await refreshIdle();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, [refreshIdle]);

  useEffect(() => {
    void refresh();
    return () => {
      if (toastTimer.current != null) window.clearTimeout(toastTimer.current);
    };
  }, [refresh]);

  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void import("./api/updater")
        .then(({ checkForUpdate }) => checkForUpdate())
        .then((update) => {
          if (cancelled || !update) return;
          showToast(`发现新版本 ${update.version}，可在「设置 → 关于」中更新`);
        })
        .catch(() => undefined);
    }, 8000);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [showToast]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      void getSchedulerStatus()
        .then(setScheduler)
        .catch(() => undefined);
      void refreshIdle();
    }, 5000);
    return () => window.clearInterval(timer);
  }, [refreshIdle]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<IdleSendStatus>("idle-send-changed", (event) => {
      setIdleStatus(event.payload);
      void listPendingWecom()
        .then(setPendingWecom)
        .catch(() => undefined);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  async function handleSave(task: Task) {
    setBusy(true);
    setError("");
    try {
      await saveTask(task);
      await reloadScheduler().catch(() => undefined);
      setEditing(undefined);
      showToast(task.id ? "任务已更新" : "任务已创建");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  }

  async function handleToggle(task: Task) {
    setBusy(true);
    setError("");
    try {
      await toggleTask(task.id, !task.enabled);
      await reloadScheduler().catch(() => undefined);
      showToast(task.enabled ? "已禁用任务" : "已启用任务");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  }

  async function handleDelete(task: Task) {
    if (!window.confirm(`确定删除任务「${task.name}」？此操作不可撤销。`)) return;
    setBusy(true);
    setError("");
    try {
      await deleteTask(task.id);
      await reloadScheduler().catch(() => undefined);
      showToast("任务已删除");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  }

  async function handleRun(task: Task, force = false) {
    setBusy(true);
    setError("");
    try {
      const log = await runTaskNow(task.id, force);
      if (log.status === "skipped" && log.detail?.includes("空闲发送队列")) {
        showToast(`「${task.name}」已排队，空闲后发送`);
      } else if (log.status === "success") {
        showToast(force ? `已强制执行「${task.name}」` : `已执行「${task.name}」`);
      } else {
        showToast(`「${task.name}」执行失败`);
      }
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  }

  async function handleCancelPending(id?: string) {
    try {
      const n = await cancelPendingWecom(id);
      showToast(id ? `已取消 1 项待发送` : `已取消 ${n} 项待发送`);
      await refreshIdle();
    } catch (e) {
      showToast(e instanceof Error ? e.message : String(e));
    }
  }

  function hasWecom(task: Task) {
    return task.actions.some((a) => a.type === "wecom_ui_dm");
  }

  async function handleClearLogs() {
    if (!window.confirm("确定清空全部执行日志？")) return;
    setBusy(true);
    setError("");
    try {
      const n = await clearLogs();
      showToast(`已清空 ${n} 条日志`);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  }

  function handleRefresh() {
    if (page === "settings") {
      setSettingsRefreshToken((token) => token + 1);
    }
    void refresh();
  }

  const failLogCount = logs.filter((l) => l.status === "failed").length;

  return (
    <div className="app-shell">
      <Sidebar
        page={page}
        onNavigate={setPage}
        taskCount={tasks.length}
        failLogCount={failLogCount}
        scheduler={scheduler}
        pendingWecomCount={idleStatus?.pendingCount ?? pendingWecom.length}
      />
      <div className="main">
        <TopBar
          page={page}
          taskQuery={taskQuery}
          logQuery={logQuery}
          busy={busy}
          idleStatus={idleStatus}
          pendingCount={idleStatus?.pendingCount ?? pendingWecom.length}
          onTaskQueryChange={setTaskQuery}
          onLogQueryChange={setLogQuery}
          onCreate={() => setEditing(null)}
          onRefresh={handleRefresh}
          onClearLogs={() => void handleClearLogs()}
          onCancelAllPending={() => void handleCancelPending()}
        />
        {error ? (
          <div className="content" style={{ paddingBottom: 0 }}>
            <div className="app-error">{error}</div>
          </div>
        ) : null}

        {idleStatus &&
        (idleStatus.pendingCount > 0 ||
          idleStatus.phase === "countdown" ||
          idleStatus.phase === "running") ? (
          <div className="content" style={{ paddingBottom: 0 }}>
            <div className="idle-banner">
              <div className="idle-banner-main">
                {idleStatus.phase === "countdown" ? (
                  <>
                    <strong>即将发送</strong>
                    {idleStatus.activeTaskName
                      ? `「${idleStatus.activeTaskName}」`
                      : "企微任务"}
                    · 倒计时 {idleStatus.countdownRemainingSecs}s（移动鼠标可取消本次）
                  </>
                ) : idleStatus.phase === "running" ? (
                  <>
                    <strong>正在发送</strong>
                    {idleStatus.activeTaskName
                      ? `「${idleStatus.activeTaskName}」`
                      : "企微任务"}
                  </>
                ) : (
                  <>
                    <strong>企微待发送 {idleStatus.pendingCount}</strong>
                    · 空闲 ≥ {idleStatus.idleSecondsConfig}s 后倒计时发送
                    {idleStatus.schedulerPaused ? "（调度已暂停，暂不自动发送）" : ""}
                  </>
                )}
              </div>
              {idleStatus.pendingCount > 0 ? (
                <button
                  className="btn btn-secondary btn-sm"
                  type="button"
                  onClick={() => void handleCancelPending()}
                >
                  清空队列
                </button>
              ) : null}
            </div>
            {pendingWecom.length > 0 ? (
              <div className="pending-list">
                {pendingWecom.map((p) => (
                  <div className="pending-row" key={p.id}>
                    <div>
                      <span className="pending-name">{p.taskName}</span>
                      <span className="muted mono" style={{ marginLeft: 8, fontSize: 12 }}>
                        {p.source} · {p.enqueuedAt}
                      </span>
                    </div>
                    <button
                      className="btn btn-danger-ghost btn-sm"
                      type="button"
                      onClick={() => void handleCancelPending(p.id)}
                    >
                      取消
                    </button>
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        ) : null}

        {page === "tasks" ? (
          <TasksPage
            tasks={tasks}
            query={taskQuery}
            busy={busy}
            schedulerPaused={scheduler?.paused}
            idleSendEnabled={idleStatus?.enabled !== false}
            onCreate={() => setEditing(null)}
            onEdit={(t) => setEditing(t)}
            onToggle={(t) => void handleToggle(t)}
            onDelete={(t) => void handleDelete(t)}
            onRun={(t) => void handleRun(t, false)}
            onForceRun={(t) => void handleRun(t, true)}
            hasWecom={hasWecom}
          />
        ) : null}

        {page === "logs" ? (
          <LogsPage
            logs={logs}
            tasks={tasks}
            query={logQuery}
            busy={busy}
            onToast={showToast}
          />
        ) : null}

        {page === "settings" ? (
          <SettingsPage
            info={info}
            scheduler={scheduler}
            refreshToken={settingsRefreshToken}
            onToast={showToast}
            onSchedulerChange={setScheduler}
            onDataChanged={() => void refresh()}
          />
        ) : null}
      </div>

      {editing !== undefined ? (
        <TaskEditor
          task={editing}
          busy={busy}
          onCancel={() => setEditing(undefined)}
          onSave={handleSave}
          onToast={showToast}
        />
      ) : null}

      {toast ? <div className="toast">{toast}</div> : null}
    </div>
  );
}
