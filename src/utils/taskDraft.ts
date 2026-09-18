import type { Action, OnActionFail, Task, Trigger, WaitMode } from "../types/task";
import { newId } from "./labels";

/** 编辑器内部草稿（各触发器字段并集，切换类型时保留填写） */
export interface TriggerDraft {
  type: Trigger["type"];
  datetime: string;
  time: string;
  weekdays: number[];
  expr: string;
  note: string;
  every: number;
  unit: string;
}

export interface ActionDraft {
  id: string;
  type: Action["type"];
  path: string;
  args: string;
  url: string;
  runtime: string;
  workingDir: string;
  waitMode: WaitMode;
  contact: string;
  message: string;
  launchWecom: boolean;
  timeoutSec: number;
  retryCount: number;
  notifyOnFail: boolean;
  closeAfterSend: boolean;
}

export interface TaskFormDraft {
  id: string;
  name: string;
  enabled: boolean;
  onActionFail: OnActionFail;
  trigger: TriggerDraft;
  actions: ActionDraft[];
  lastRunAt?: string | null;
  lastStatus?: Task["lastStatus"];
  updatedAt?: string | null;
}

export function emptyAction(): ActionDraft {
  return {
    id: newId("a"),
    type: "open_url",
    path: "",
    args: "",
    url: "",
    runtime: "ps1",
    workingDir: "",
    waitMode: "fire_and_forget",
    contact: "",
    message: "",
    launchWecom: true,
    timeoutSec: 30,
    retryCount: 0,
    notifyOnFail: true,
    closeAfterSend: true,
  };
}

export function emptyTaskDraft(): TaskFormDraft {
  return {
    id: "",
    name: "",
    enabled: true,
    onActionFail: "stop",
    trigger: {
      type: "daily",
      datetime: "",
      time: "09:00",
      weekdays: [1, 2, 3, 4, 5],
      expr: "0 9 * * *",
      note: "",
      every: 30,
      unit: "分钟",
    },
    actions: [emptyAction()],
  };
}

export function taskToDraft(task: Task): TaskFormDraft {
  const t = task.trigger;
  return {
    id: task.id,
    name: task.name,
    enabled: task.enabled,
    onActionFail: task.onActionFail === "continue" ? "continue" : "stop",
    lastRunAt: task.lastRunAt,
    lastStatus: task.lastStatus,
    updatedAt: task.updatedAt,
    trigger: {
      type: t.type,
      datetime: t.type === "once" ? t.datetime : "",
      time: t.type === "daily" ? t.time : "09:00",
      weekdays: t.type === "daily" ? [...t.weekdays] : [1, 2, 3, 4, 5],
      expr: t.type === "cron" ? t.expr : "0 9 * * *",
      note: t.type === "cron" ? t.note || "" : "",
      every: t.type === "interval" ? t.every : 30,
      unit: t.type === "interval" ? t.unit : "分钟",
    },
    actions: task.actions.map(actionToDraft),
  };
}

function actionToDraft(a: Action): ActionDraft {
  const base = emptyAction();
  base.id = a.id;
  base.type = a.type;
  if (a.type === "open_app") {
    base.path = a.path;
    base.args = a.args || "";
    base.waitMode = a.waitMode === "wait_exit" ? "wait_exit" : "fire_and_forget";
    base.timeoutSec = a.timeoutSec ?? 60;
  } else if (a.type === "open_url") {
    base.url = a.url;
  } else if (a.type === "run_script") {
    base.runtime = a.runtime;
    base.path = a.path;
    base.workingDir = a.workingDir || "";
    base.timeoutSec = a.timeoutSec ?? 60;
  } else if (a.type === "wecom_ui_dm") {
    base.contact = a.contact;
    base.message = a.message;
    base.launchWecom = a.launchWecom !== false;
    base.timeoutSec = a.timeoutSec ?? 30;
    base.retryCount = a.retryCount ?? 0;
    base.notifyOnFail = a.notifyOnFail !== false;
    base.closeAfterSend = a.closeAfterSend !== false;
  }
  return base;
}

export function draftToTrigger(d: TriggerDraft): Trigger {
  if (d.type === "once") return { type: "once", datetime: d.datetime };
  if (d.type === "daily") {
    return { type: "daily", time: d.time || "09:00", weekdays: d.weekdays };
  }
  if (d.type === "cron") {
    return { type: "cron", expr: d.expr, note: d.note || undefined };
  }
  return { type: "interval", every: d.every || 1, unit: d.unit || "分钟" };
}

export function draftToAction(d: ActionDraft): Action {
  if (d.type === "open_app") {
    return {
      type: "open_app",
      id: d.id,
      path: d.path.trim(),
      args: d.args.trim() || undefined,
      waitMode: d.waitMode,
      timeoutSec: d.timeoutSec,
    };
  }
  if (d.type === "open_url") {
    return { type: "open_url", id: d.id, url: d.url.trim() };
  }
  if (d.type === "run_script") {
    return {
      type: "run_script",
      id: d.id,
      runtime: d.runtime || "ps1",
      path: d.path.trim(),
      workingDir: d.workingDir.trim() || undefined,
      timeoutSec: d.timeoutSec,
    };
  }
  return {
    type: "wecom_ui_dm",
    id: d.id,
    contact: d.contact.trim(),
    message: d.message.trim(),
    launchWecom: d.launchWecom,
    timeoutSec: d.timeoutSec,
    retryCount: d.retryCount,
    notifyOnFail: d.notifyOnFail,
    closeAfterSend: d.closeAfterSend,
  };
}

export function draftToTask(draft: TaskFormDraft): Task {
  return {
    id: draft.id,
    name: draft.name.trim(),
    enabled: draft.enabled,
    onActionFail: draft.onActionFail,
    trigger: draftToTrigger(draft.trigger),
    actions: draft.actions.map(draftToAction),
    lastRunAt: draft.lastRunAt,
    lastStatus: draft.lastStatus,
    updatedAt: draft.updatedAt,
  };
}

export function isActionValid(a: ActionDraft): boolean {
  if (a.type === "open_url") return !!a.url.trim();
  if (a.type === "wecom_ui_dm") {
    return !!a.contact.trim() && !!a.message.trim();
  }
  return !!a.path.trim();
}

export function canSaveDraft(draft: TaskFormDraft): boolean {
  if (!draft.name.trim()) return false;
  const t = draft.trigger;
  if (t.type === "once" && !t.datetime.trim()) return false;
  if (t.type === "daily" && !t.time.trim()) return false;
  if (t.type === "cron") {
    // 细粒度校验由 TaskEditor + validateCronExpr 负责；此处仅非空
    if (!t.expr.trim()) return false;
  }
  if (t.type === "interval" && !(t.every > 0)) return false;
  if (!draft.actions.length) return false;
  return draft.actions.every(isActionValid);
}
