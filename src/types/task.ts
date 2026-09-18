export type RunStatus = "success" | "failed" | "running" | "skipped";

export type OnActionFail = "stop" | "continue";

export type WaitMode = "fire_and_forget" | "wait_exit";

export type Trigger =
  | { type: "once"; datetime: string }
  | { type: "daily"; time: string; weekdays: number[] }
  | { type: "cron"; expr: string; note?: string }
  | { type: "interval"; every: number; unit: string };

export type Action =
  | {
      type: "open_app";
      id: string;
      path: string;
      args?: string;
      waitMode?: WaitMode;
      /** wait_exit 超时秒数；0=不限时；默认 60 */
      timeoutSec?: number;
    }
  | { type: "open_url"; id: string; url: string }
  | {
      type: "run_script";
      id: string;
      runtime: string;
      path: string;
      /** 工作目录；空则脚本所在目录 */
      workingDir?: string;
      /** 超时秒数；0=不限时；默认 60 */
      timeoutSec?: number;
    }
  | {
      type: "wecom_ui_dm";
      id: string;
      contact: string;
      message: string;
      launchWecom?: boolean;
      timeoutSec?: number;
      retryCount?: number;
      notifyOnFail?: boolean;
      /** 发送成功后关闭企微主窗口（默认 true） */
      closeAfterSend?: boolean;
    };

/** 错过触发补跑策略 */
export type MissedJobPolicy = "skip" | "run_once";

export interface Task {
  id: string;
  name: string;
  enabled: boolean;
  trigger: Trigger;
  actions: Action[];
  /** 动作失败策略，默认 stop */
  onActionFail?: OnActionFail;
  lastRunAt?: string | null;
  lastStatus?: RunStatus | null;
  updatedAt?: string | null;
  /** 下次预计触发（本地时区）；计算字段，禁用/不再执行时为 null */
  nextRunAt?: string | null;
}

export interface LogStep {
  title: string;
  status: string;
  time?: string;
  note?: string;
}

export interface ExecutionLog {
  id: string;
  taskId: string;
  taskName: string;
  time: string;
  status: RunStatus;
  detail?: string;
  steps?: LogStep[];
}

export interface StoreInfo {
  dataDir: string;
  storeFile: string;
  taskCount: number;
  logCount: number;
}

export interface SchedulerStatus {
  running: boolean;
  enabledTaskCount: number;
  runningTaskCount: number;
  lastTickAt?: string | null;
  nextCheckAt?: string | null;
  tickIntervalSecs: number;
  paused?: boolean;
}

/** 企微空闲发送偏好 */
export interface WecomIdleSendSettings {
  enabled: boolean;
  idleSeconds: number;
  countdownSeconds: number;
}

/** C# FlaUI Agent 偏好 */
export interface WecomAgentSettings {
  enabled: boolean;
  /** wecom-agent.exe 路径；空则自动查找 */
  path: string;
  /** Agent 失败时回退键鼠 */
  fallbackToInput: boolean;
}

export type IdleSendPhase = "idle" | "waiting_idle" | "countdown" | "running";

export interface PendingWecomItem {
  id: string;
  taskId: string;
  taskName: string;
  /** schedule | manual | catchup */
  source: string;
  enqueuedAt: string;
}

export interface IdleSendStatus {
  enabled: boolean;
  phase: IdleSendPhase;
  pendingCount: number;
  idleSecondsConfig: number;
  countdownSecondsConfig: number;
  currentIdleSeconds: number;
  countdownRemainingSecs: number;
  activePendingId?: string | null;
  activeTaskName?: string | null;
  schedulerPaused: boolean;
}

export interface AppSettings {
  autostart: boolean;
  minimizeToTrayOnClose: boolean;
  notifyOnTaskFail: boolean;
  schedulerPaused: boolean;
  /** 错过触发补跑：skip | run_once */
  missedJobPolicy: MissedJobPolicy;
  wecomIdleSend: WecomIdleSendSettings;
  wecomAgent: WecomAgentSettings;
  notificationPermission?: string;
  /** 只读：当前解析到的 Agent 路径 */
  wecomAgentResolvedPath?: string | null;
}

export interface WecomProbeResult {
  ok: boolean;
  note: string;
  steps: LogStep[];
}

export interface WecomTrySendResult {
  ok: boolean;
  note: string;
  steps: LogStep[];
  notified: boolean;
}

export type TaskDraft = Omit<Task, "id"> & { id?: string };

export type ImportMode = "merge" | "replace";

export interface ExportPayload {
  version: number;
  exportedAt: string;
  tasks: Task[];
  logs?: ExecutionLog[];
}

export interface ImportResult {
  imported: number;
  updated: number;
  skipped: number;
  errors: string[];
  taskCount: number;
}

export interface LogFilter {
  taskId?: string | null;
  status?: string | null;
  keyword?: string | null;
  timeFrom?: string | null;
  timeTo?: string | null;
  limit?: number | null;
}
