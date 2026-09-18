/** 前端 Cron（5 字段 POSIX）校验与简易摘要 */

const FIELD_COUNT = 5;

export interface CronValidation {
  ok: boolean;
  error?: string;
  /** 粗粒度中文摘要 */
  summary?: string;
}

const CRON_EXAMPLES: { label: string; expr: string }[] = [
  { label: "每天 0 点", expr: "0 0 * * *" },
  { label: "工作日 9:05", expr: "5 9 * * 1-5" },
  { label: "每小时", expr: "0 * * * *" },
  { label: "每 15 分钟", expr: "*/15 * * * *" },
  { label: "每天 18:30", expr: "30 18 * * *" },
];

export { CRON_EXAMPLES };

/** 校验 5 字段 POSIX cron（与后端 croner 基本对齐的轻量检查）。 */
export function validateCronExpr(raw: string): CronValidation {
  const expr = raw.trim();
  if (!expr) {
    return { ok: false, error: "Cron 表达式不能为空" };
  }
  const fields = expr.split(/\s+/);
  if (fields.length !== FIELD_COUNT) {
    return {
      ok: false,
      error: `须为 5 字段（分 时 日 月 周），当前 ${fields.length} 段`,
    };
  }

  const [min, hour, dom, mon, dow] = fields;
  const checks: [string, string, (s: string) => boolean][] = [
    ["分钟", min, (s) => isCronField(s, 0, 59)],
    ["小时", hour, (s) => isCronField(s, 0, 23)],
    ["日", dom, (s) => isCronField(s, 1, 31, true)],
    ["月", mon, (s) => isCronField(s, 1, 12, true)],
    ["周", dow, (s) => isCronField(s, 0, 7, true)],
  ];

  for (const [name, value, ok] of checks) {
    if (!ok(value)) {
      return { ok: false, error: `${name}字段无效：${value}` };
    }
  }

  return { ok: true, summary: describeCron(expr) };
}

function isCronField(
  field: string,
  min: number,
  max: number,
  allowNames = false,
): boolean {
  if (!field) return false;
  // 拆列表
  const parts = field.split(",");
  for (const part of parts) {
    if (!isCronPart(part, min, max, allowNames)) return false;
  }
  return true;
}

function isCronPart(
  part: string,
  min: number,
  max: number,
  allowNames: boolean,
): boolean {
  const p = part.trim().toUpperCase();
  if (!p) return false;
  if (p === "*" || p === "?") return true;

  // */n 或 a-b/n
  const stepMatch = /^(.+)\/(\d+)$/.exec(p);
  if (stepMatch) {
    const base = stepMatch[1];
    const step = Number(stepMatch[2]);
    if (!Number.isFinite(step) || step <= 0) return false;
    if (base === "*") return true;
    return isCronRangeOrValue(base, min, max, allowNames);
  }

  return isCronRangeOrValue(p, min, max, allowNames);
}

function isCronRangeOrValue(
  p: string,
  min: number,
  max: number,
  allowNames: boolean,
): boolean {
  if (p.includes("-")) {
    const [a, b] = p.split("-");
    const va = parseCronAtom(a, allowNames);
    const vb = parseCronAtom(b, allowNames);
    if (va == null || vb == null) return false;
    return va >= min && vb <= max && va <= vb;
  }
  const v = parseCronAtom(p, allowNames);
  if (v == null) return false;
  return v >= min && v <= max;
}

const MONTH_NAMES: Record<string, number> = {
  JAN: 1,
  FEB: 2,
  MAR: 3,
  APR: 4,
  MAY: 5,
  JUN: 6,
  JUL: 7,
  AUG: 8,
  SEP: 9,
  OCT: 10,
  NOV: 11,
  DEC: 12,
};

const DOW_NAMES: Record<string, number> = {
  SUN: 0,
  MON: 1,
  TUE: 2,
  WED: 3,
  THU: 4,
  FRI: 5,
  SAT: 6,
};

function parseCronAtom(raw: string, allowNames: boolean): number | null {
  const s = raw.trim().toUpperCase();
  if (/^\d+$/.test(s)) return Number(s);
  if (!allowNames) return null;
  if (MONTH_NAMES[s] != null) return MONTH_NAMES[s];
  if (DOW_NAMES[s] != null) return DOW_NAMES[s];
  return null;
}

/** 简易人类可读摘要（非完整 croner describe）。 */
export function describeCron(expr: string): string {
  const fields = expr.trim().split(/\s+/);
  if (fields.length !== 5) return expr;
  const [min, hour, dom, mon, dow] = fields;

  const known = CRON_EXAMPLES.find((e) => e.expr === expr.trim());
  if (known) return known.label;

  if (min.startsWith("*/") && hour === "*" && dom === "*" && mon === "*" && dow === "*") {
    return `每 ${min.slice(2)} 分钟`;
  }
  if (min === "0" && hour === "*" && dom === "*" && mon === "*" && dow === "*") {
    return "每小时整点";
  }
  if (dom === "*" && mon === "*" && (dow === "*" || dow === "1-5" || dow === "0-6")) {
    const time =
      /^\d+$/.test(hour) && /^\d+$/.test(min)
        ? `${hour.padStart(2, "0")}:${min.padStart(2, "0")}`
        : `${hour}:${min}`;
    if (dow === "1-5") return `工作日 ${time}`;
    if (dow === "*") return `每天 ${time}`;
  }

  return `Cron · ${expr}`;
}
