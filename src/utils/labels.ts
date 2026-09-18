import type { Action, RunStatus, Trigger } from "../types/task";

export const TRIGGER_LABELS: Record<Trigger["type"], string> = {
  once: "一次性",
  daily: "每天",
  cron: "Cron",
  interval: "间隔",
};

export const ACTION_LABELS: Record<Action["type"], string> = {
  open_app: "打开应用",
  open_url: "打开网址",
  run_script: "运行脚本",
  wecom_ui_dm: "企业微信 · 发私聊",
};

export const ACTION_TYPES: Action["type"][] = [
  "open_app",
  "open_url",
  "run_script",
  "wecom_ui_dm",
];

export function shortPath(p = ""): string {
  if (!p) return "未设置";
  const parts = p.replace(/\\/g, "/").split("/");
  return parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : p;
}

export function summarizeTrigger(trigger: Trigger): string {
  if (trigger.type === "once") return `一次性 · ${trigger.datetime || "未设置"}`;
  if (trigger.type === "daily") return `每天 ${trigger.time || "--:--"}`;
  if (trigger.type === "interval") {
    return `每 ${trigger.every || "?"} ${trigger.unit || "分钟"}`;
  }
  if (trigger.type === "cron") return trigger.note || trigger.expr || "Cron";
  return "—";
}

export function summarizeActions(actions: Action[] = []): string {
  if (!actions.length) return "无动作";
  return actions
    .map((a) => {
      switch (a.type) {
        case "open_url":
          return `打开网址 · ${a.url}`;
        case "open_app":
          return `打开应用 · ${shortPath(a.path)}`;
        case "run_script":
          return `运行脚本 · ${a.runtime?.toUpperCase() || "SCRIPT"} · ${shortPath(a.path)}`;
        case "wecom_ui_dm":
          return `企业微信私聊 · ${a.contact || "未指定联系人"}`;
        default:
          return "未知动作";
      }
    })
    .join("；");
}

export function statusLabel(status?: RunStatus | null): string {
  if (status === "success") return "成功";
  if (status === "failed") return "失败";
  if (status === "running") return "运行中";
  if (status === "skipped") return "跳过";
  return "—";
}

export function stepStatusLabel(status: string): string {
  if (status === "ok") return "成功";
  if (status === "fail") return "失败";
  return "跳过";
}

export function newId(prefix: string): string {
  return `${prefix}_${crypto.randomUUID().slice(0, 8)}`;
}
