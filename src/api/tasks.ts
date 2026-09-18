import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  ExecutionLog,
  ExportPayload,
  IdleSendStatus,
  ImportMode,
  ImportResult,
  LogFilter,
  MissedJobPolicy,
  PendingWecomItem,
  SchedulerStatus,
  StoreInfo,
  Task,
  TaskDraft,
  WecomAgentSettings,
  WecomIdleSendSettings,
  WecomProbeResult,
  WecomTrySendResult,
} from "../types/task";

export function getStoreInfo(): Promise<StoreInfo> {
  return invoke("get_store_info");
}

export function listTasks(): Promise<Task[]> {
  return invoke("list_tasks");
}

export function getTask(id: string): Promise<Task | null> {
  return invoke("get_task", { id });
}

export function saveTask(task: Task | TaskDraft): Promise<Task> {
  return invoke("save_task", { task });
}

export function deleteTask(id: string): Promise<boolean> {
  return invoke("delete_task", { id });
}

export function toggleTask(id: string, enabled: boolean): Promise<Task> {
  return invoke("toggle_task", { id, enabled });
}

export function listLogs(limit?: number): Promise<ExecutionLog[]> {
  return invoke("list_logs", { limit: limit ?? null });
}

export function appendLog(log: ExecutionLog): Promise<ExecutionLog> {
  return invoke("append_log", { log });
}

export function clearLogs(): Promise<number> {
  return invoke("clear_logs");
}

export function runTaskNow(id: string, force = false): Promise<ExecutionLog> {
  return invoke("run_task_now", { id, force });
}

export function getSchedulerStatus(): Promise<SchedulerStatus> {
  return invoke("get_scheduler_status");
}

export function reloadScheduler(): Promise<SchedulerStatus> {
  return invoke("reload_scheduler");
}

export function getSettings(): Promise<AppSettings> {
  return invoke("get_settings");
}

export function updateSettings(patch: {
  autostart?: boolean;
  minimizeToTrayOnClose?: boolean;
  notifyOnTaskFail?: boolean;
  schedulerPaused?: boolean;
  missedJobPolicy?: MissedJobPolicy;
  wecomIdleSend?: Partial<WecomIdleSendSettings>;
  wecomAgent?: Partial<WecomAgentSettings>;
}): Promise<AppSettings> {
  return invoke("update_settings", { patch });
}

export function wecomAgentPing(): Promise<{
  ok: boolean;
  note: string;
  steps: import("../types/task").LogStep[];
  usedAgent: boolean;
}> {
  return invoke("wecom_agent_ping");
}

export function listPendingWecom(): Promise<PendingWecomItem[]> {
  return invoke("list_pending_wecom");
}

export function cancelPendingWecom(id?: string | null): Promise<number> {
  return invoke("cancel_pending_wecom", {
    args: { id: id ?? null },
  });
}

export function getIdleSendStatus(): Promise<IdleSendStatus> {
  return invoke("get_idle_send_status");
}

export function setSchedulerPaused(paused: boolean): Promise<SchedulerStatus> {
  return invoke("set_scheduler_paused", { paused });
}

export function wecomProbeWindow(args?: {
  launchWecom?: boolean;
  timeoutSec?: number;
}): Promise<WecomProbeResult> {
  return invoke("wecom_probe_window", {
    args: {
      launchWecom: args?.launchWecom ?? true,
      timeoutSec: args?.timeoutSec ?? 30,
    },
  });
}

export function wecomTrySend(args: {
  contact: string;
  message: string;
  launchWecom?: boolean;
  timeoutSec?: number;
  retryCount?: number;
}): Promise<WecomTrySendResult> {
  return invoke("wecom_try_send", {
    args: {
      contact: args.contact,
      message: args.message,
      launchWecom: args.launchWecom ?? true,
      timeoutSec: args.timeoutSec ?? 30,
      retryCount: args.retryCount ?? 0,
    },
  });
}

export function requestNotificationPermission(): Promise<string> {
  return invoke("request_notification_permission");
}

export function openDataDir(): Promise<void> {
  return invoke("open_data_dir");
}

export function resetSettings(): Promise<AppSettings> {
  return invoke("reset_settings");
}

export function exportTasks(includeLogs = false): Promise<ExportPayload> {
  return invoke("export_tasks", { args: { includeLogs } });
}

export function importTasks(json: string, mode: ImportMode): Promise<ImportResult> {
  return invoke("import_tasks", { args: { json, mode } });
}

export function filterLogs(filter: LogFilter): Promise<ExecutionLog[]> {
  return invoke("filter_logs", { filter });
}

export function exportFilteredLogs(
  filter: LogFilter,
  format: "json" | "txt" = "json",
): Promise<string> {
  return invoke("export_filtered_logs", { args: { filter, format } });
}

export function writeTextFile(path: string, content: string): Promise<void> {
  return invoke("write_text_file", { path, content });
}

export function readTextFile(path: string): Promise<string> {
  return invoke("read_text_file", { path });
}
