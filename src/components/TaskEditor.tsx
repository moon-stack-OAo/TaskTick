import { useMemo, useState } from "react";
import { wecomProbeWindow, wecomTrySend } from "../api/tasks";
import type { OnActionFail, Task, WaitMode } from "../types/task";
import {
  ACTION_LABELS,
  ACTION_TYPES,
  TRIGGER_LABELS,
} from "../utils/labels";
import { CRON_EXAMPLES, validateCronExpr } from "../utils/cron";
import {
  canSaveDraft,
  draftToTask,
  emptyAction,
  emptyTaskDraft,
  taskToDraft,
  type ActionDraft,
  type TaskFormDraft,
  type TriggerDraft,
} from "../utils/taskDraft";
import { Icon } from "./Icons";

interface TaskEditorProps {
  task: Task | null;
  busy: boolean;
  onCancel: () => void;
  onSave: (task: Task) => Promise<void>;
  onToast: (message: string) => void;
}

export function TaskEditor({ task, busy, onCancel, onSave, onToast }: TaskEditorProps) {
  const isNew = !task;
  const [draft, setDraft] = useState<TaskFormDraft>(() =>
    task ? taskToDraft(task) : emptyTaskDraft(),
  );
  const [precheckBusy, setPrecheckBusy] = useState<"locate" | "send" | null>(null);

  const cronCheck = useMemo(() => {
    if (draft.trigger.type !== "cron") return null;
    return validateCronExpr(draft.trigger.expr);
  }, [draft.trigger.type, draft.trigger.expr]);

  const canSave = useMemo(() => {
    if (!canSaveDraft(draft) || busy) return false;
    if (draft.trigger.type === "cron" && cronCheck && !cronCheck.ok) return false;
    return true;
  }, [draft, busy, cronCheck]);

  function update(patch: Partial<TaskFormDraft>) {
    setDraft((d) => ({ ...d, ...patch }));
  }

  function updateTrigger(patch: Partial<TriggerDraft>) {
    setDraft((d) => ({ ...d, trigger: { ...d.trigger, ...patch } }));
  }

  function updateAction(idx: number, patch: Partial<ActionDraft>) {
    setDraft((d) => {
      const actions = d.actions.map((a, i) => {
        if (i !== idx) return a;
        const next = { ...a, ...patch };
        // 切换动作类型时给出合理默认超时
        if (patch.type && patch.type !== a.type) {
          if (patch.type === "run_script" || patch.type === "open_app") {
            next.timeoutSec = 60;
          } else if (patch.type === "wecom_ui_dm") {
            next.timeoutSec = 30;
          }
        }
        return next;
      });
      return { ...d, actions };
    });
  }

  function addAction() {
    setDraft((d) => ({ ...d, actions: [...d.actions, emptyAction()] }));
  }

  function removeAction(idx: number) {
    setDraft((d) => {
      if (d.actions.length <= 1) return d;
      return { ...d, actions: d.actions.filter((_, i) => i !== idx) };
    });
  }

  async function handleSave() {
    if (!canSaveDraft(draft)) return;
    await onSave(draftToTask(draft));
  }

  async function runWecomPrecheck(action: ActionDraft, kind: "locate" | "send") {
    if (precheckBusy) return;
    if (kind === "locate") {
      setPrecheckBusy("locate");
      try {
        const result = await wecomProbeWindow({
          launchWecom: !!action.launchWecom,
          timeoutSec: action.timeoutSec || 30,
        });
        onToast(result.ok ? `✓ ${result.note}` : `✗ ${result.note}`);
      } catch (e) {
        onToast(e instanceof Error ? e.message : String(e));
      } finally {
        setPrecheckBusy(null);
      }
      return;
    }

    const contact = action.contact.trim();
    const message = action.message.trim();
    if (!contact || !message) {
      onToast("请先填写联系人与消息内容，再试发送");
      return;
    }
    setPrecheckBusy("send");
    try {
      const result = await wecomTrySend({
        contact,
        message,
        launchWecom: !!action.launchWecom,
        timeoutSec: action.timeoutSec || 30,
        retryCount: action.retryCount || 0,
      });
      onToast(result.ok ? `✓ ${result.note}` : `✗ ${result.note}`);
    } catch (e) {
      onToast(e instanceof Error ? e.message : String(e));
    } finally {
      setPrecheckBusy(null);
    }
  }

  return (
    <div
      className="overlay"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div className="modal" role="dialog" aria-modal="true">
        <div className="modal-header">
          <h2>{isNew ? "新建任务" : "编辑任务"}</h2>
          <button className="btn-icon" type="button" onClick={onCancel} aria-label="关闭">
            {Icon.close}
          </button>
        </div>
        <div className="modal-body">
          <div className="form-grid">
            <div className="field">
              <label htmlFor="task-name">任务名称</label>
              <input
                id="task-name"
                className="input"
                value={draft.name}
                placeholder="例如：工作日备份脚本"
                onChange={(e) => update({ name: e.target.value })}
              />
            </div>

            <div className="field">
              <label>触发器</label>
              <div className="seg" style={{ marginBottom: 10 }}>
                {(["once", "daily", "cron", "interval"] as const).map((type) => (
                  <button
                    key={type}
                    type="button"
                    className={draft.trigger.type === type ? "active" : ""}
                    onClick={() => updateTrigger({ type })}
                  >
                    {TRIGGER_LABELS[type]}
                  </button>
                ))}
              </div>

              {draft.trigger.type === "once" ? (
                <input
                  className="input"
                  type="datetime-local"
                  value={draft.trigger.datetime}
                  onChange={(e) => updateTrigger({ datetime: e.target.value })}
                />
              ) : null}

              {draft.trigger.type === "daily" ? (
                <div className="row-2">
                  <input
                    className="input"
                    type="time"
                    value={draft.trigger.time || "09:00"}
                    onChange={(e) => updateTrigger({ time: e.target.value })}
                  />
                  <div className="hint" style={{ alignSelf: "center", margin: 0 }}>
                    每天固定时刻触发
                  </div>
                </div>
              ) : null}

              {draft.trigger.type === "cron" ? (
                <>
                  <input
                    className="input mono"
                    value={draft.trigger.expr}
                    placeholder="30 18 * * 1-5"
                    aria-invalid={cronCheck != null && !cronCheck.ok}
                    onChange={(e) => updateTrigger({ expr: e.target.value })}
                  />
                  {cronCheck && !cronCheck.ok ? (
                    <div className="field-error" role="alert">
                      {cronCheck.error}
                    </div>
                  ) : (
                    <div className="hint">
                      {cronCheck?.summary
                        ? `摘要：${cronCheck.summary} · `
                        : null}
                      标准 5 段 POSIX（分 时 日 月 周），本机本地时区。
                    </div>
                  )}
                  <div className="cron-chips" style={{ marginTop: 8 }}>
                    {CRON_EXAMPLES.map((ex) => (
                      <button
                        key={ex.expr}
                        type="button"
                        className="filter-chip"
                        title={ex.expr}
                        onClick={() =>
                          updateTrigger({
                            expr: ex.expr,
                            note: draft.trigger.note || ex.label,
                          })
                        }
                      >
                        {ex.label}
                      </button>
                    ))}
                  </div>
                  <input
                    className="input"
                    style={{ marginTop: 8 }}
                    value={draft.trigger.note}
                    placeholder="可读摘要（可选），如：工作日 18:30"
                    onChange={(e) => updateTrigger({ note: e.target.value })}
                  />
                </>
              ) : null}

              {draft.trigger.type === "interval" ? (
                <div className="row-2">
                  <input
                    className="input"
                    type="number"
                    min={1}
                    value={draft.trigger.every}
                    onChange={(e) => updateTrigger({ every: Number(e.target.value) })}
                  />
                  <select
                    className="select"
                    value={draft.trigger.unit || "分钟"}
                    onChange={(e) => updateTrigger({ unit: e.target.value })}
                  >
                    <option>分钟</option>
                    <option>小时</option>
                    <option>秒</option>
                  </select>
                </div>
              ) : null}
            </div>

            <div className="field">
              <label>动作失败策略</label>
              <div className="seg" style={{ marginBottom: 8 }}>
                {(
                  [
                    ["stop", "失败即停"],
                    ["continue", "失败继续"],
                  ] as [OnActionFail, string][]
                ).map(([value, label]) => (
                  <button
                    key={value}
                    type="button"
                    className={draft.onActionFail === value ? "active" : ""}
                    onClick={() => update({ onActionFail: value })}
                  >
                    {label}
                  </button>
                ))}
              </div>
              <div className="hint">
                {draft.onActionFail === "continue"
                  ? "某一动作失败后仍执行后续动作；任务总状态仍为失败。"
                  : "某一动作失败后中止后续动作（默认）。"}
              </div>
            </div>

            <div className="field">
              <div className="action-head" style={{ marginBottom: 8 }}>
                <label style={{ margin: 0 }}>动作列表</label>
                <button className="btn btn-secondary btn-sm" type="button" onClick={addAction}>
                  {Icon.plus} 添加动作
                </button>
              </div>

              <div style={{ display: "grid", gap: 10 }}>
                {draft.actions.map((action, idx) => (
                  <div className="action-block" key={action.id}>
                    <div className="action-head">
                      <div className="action-title">
                        <span className="pill pill-info">#{idx + 1}</span>
                        {ACTION_LABELS[action.type]}
                      </div>
                      <button
                        className="btn btn-danger-ghost btn-sm"
                        type="button"
                        disabled={draft.actions.length <= 1}
                        onClick={() => removeAction(idx)}
                      >
                        删除
                      </button>
                    </div>

                    <div>
                      <label
                        style={{
                          fontSize: 12,
                          color: "var(--muted)",
                          marginBottom: 6,
                          display: "block",
                        }}
                      >
                        动作类型
                      </label>
                      <select
                        className="select action-type-select"
                        value={action.type}
                        onChange={(e) =>
                          updateAction(idx, { type: e.target.value as ActionDraft["type"] })
                        }
                      >
                        {ACTION_TYPES.map((type) => (
                          <option key={type} value={type}>
                            {ACTION_LABELS[type]}
                          </option>
                        ))}
                      </select>
                    </div>

                    {action.type === "open_url" ? (
                      <input
                        className="input mono"
                        placeholder="https://news.example.com"
                        value={action.url}
                        onChange={(e) => updateAction(idx, { url: e.target.value })}
                      />
                    ) : null}

                    {action.type === "open_app" ? (
                      <>
                        <input
                          className="input mono"
                          placeholder="C:/Program Files/.../app.exe"
                          value={action.path}
                          onChange={(e) => updateAction(idx, { path: e.target.value })}
                        />
                        <input
                          className="input mono"
                          placeholder="启动参数（可选）"
                          value={action.args}
                          onChange={(e) => updateAction(idx, { args: e.target.value })}
                        />
                        <div className="row-2">
                          <div>
                            <label
                              style={{
                                fontSize: 12,
                                color: "var(--muted)",
                                marginBottom: 6,
                                display: "block",
                              }}
                            >
                              等待策略
                            </label>
                            <select
                              className="select"
                              value={action.waitMode}
                              onChange={(e) =>
                                updateAction(idx, {
                                  waitMode: e.target.value as WaitMode,
                                })
                              }
                            >
                              <option value="fire_and_forget">启动即继续</option>
                              <option value="wait_exit">等待退出</option>
                            </select>
                          </div>
                          {action.waitMode === "wait_exit" ? (
                            <div>
                              <label
                                style={{
                                  fontSize: 12,
                                  color: "var(--muted)",
                                  marginBottom: 6,
                                  display: "block",
                                }}
                              >
                                超时秒数（0=不限时）
                              </label>
                              <input
                                className="input mono"
                                type="number"
                                min={0}
                                max={86400}
                                value={action.timeoutSec}
                                onChange={(e) =>
                                  updateAction(idx, {
                                    timeoutSec: Math.max(0, Number(e.target.value) || 0),
                                  })
                                }
                              />
                            </div>
                          ) : (
                            <div className="hint" style={{ alignSelf: "end", margin: 0 }}>
                              不等待进程结束，立即执行下一动作
                            </div>
                          )}
                        </div>
                      </>
                    ) : null}

                    {action.type === "run_script" ? (
                      <>
                        <div className="row-2">
                          <select
                            className="select"
                            value={action.runtime || "ps1"}
                            onChange={(e) => updateAction(idx, { runtime: e.target.value })}
                          >
                            <option value="ps1">PowerShell (.ps1)</option>
                            <option value="bat">批处理 (.bat)</option>
                            <option value="python">Python (.py)</option>
                          </select>
                          <input
                            className="input mono"
                            placeholder="D:/scripts/backup.ps1"
                            value={action.path}
                            onChange={(e) => updateAction(idx, { path: e.target.value })}
                          />
                        </div>
                        <input
                          className="input mono"
                          placeholder="工作目录（可选，默认脚本所在目录）"
                          value={action.workingDir}
                          onChange={(e) => updateAction(idx, { workingDir: e.target.value })}
                        />
                        <div>
                          <label
                            style={{
                              fontSize: 12,
                              color: "var(--muted)",
                              marginBottom: 6,
                              display: "block",
                            }}
                          >
                            超时秒数（0=不限时，默认 60）
                          </label>
                          <input
                            className="input mono"
                            type="number"
                            min={0}
                            max={86400}
                            value={action.timeoutSec}
                            onChange={(e) =>
                              updateAction(idx, {
                                timeoutSec: Math.max(0, Number(e.target.value) || 0),
                              })
                            }
                          />
                        </div>
                        <div className="hint">stdout/stderr 会写入执行日志（各最多约 4KB，超长截断）。</div>
                      </>
                    ) : null}

                    {action.type === "wecom_ui_dm" ? (
                      <>
                        <div className="notice">
                          <svg
                            className="notice-icon"
                            viewBox="0 0 18 18"
                            fill="none"
                            aria-hidden="true"
                          >
                            <circle cx="9" cy="9" r="7.2" stroke="currentColor" strokeWidth="1.4" />
                            <path
                              d="M9 5.2v5.2M9 12.6h.01"
                              stroke="currentColor"
                              strokeWidth="1.5"
                              strokeLinecap="round"
                            />
                          </svg>
                          <div>
                            此动作为 UI 自动化，依赖窗口控件；企微改版、窗口未前台或联系人名称不一致可能导致失败。首版仅私聊。可用下方预检真实定位/试发送。
                          </div>
                        </div>
                        <div className="step-stack">
                          <div className="step-card">
                            <div className="step-head">
                              <span className="step-num">1</span>联系人
                            </div>
                            <p className="step-hint">
                              填写与企业微信通讯录完全一致的显示名，避免重名歧义。
                            </p>
                            <input
                              className="input"
                              placeholder="例如：张三"
                              value={action.contact}
                              onChange={(e) => updateAction(idx, { contact: e.target.value })}
                            />
                          </div>
                          <div className="step-card">
                            <div className="step-head">
                              <span className="step-num">2</span>消息内容
                            </div>
                            <p className="step-hint">将写入私聊输入框并发送。请避免敏感或批量骚扰内容。</p>
                            <textarea
                              className="textarea"
                              placeholder="输入要发送的私聊消息"
                              value={action.message}
                              onChange={(e) => updateAction(idx, { message: e.target.value })}
                            />
                          </div>
                          <div className="step-card">
                            <div className="step-head">
                              <span className="step-num">3</span>先启动企业微信
                            </div>
                            <label className="check-row">
                              <input
                                type="checkbox"
                                checked={!!action.launchWecom}
                                onChange={(e) =>
                                  updateAction(idx, { launchWecom: e.target.checked })
                                }
                              />
                              执行前自动启动企业微信
                            </label>
                            <label className="check-row" style={{ marginTop: 8 }}>
                              <input
                                type="checkbox"
                                checked={action.closeAfterSend !== false}
                                onChange={(e) =>
                                  updateAction(idx, { closeAfterSend: e.target.checked })
                                }
                              />
                              发送成功后关闭企微窗口
                            </label>
                            <p className="step-hint" style={{ marginTop: 6 }}>
                              关闭通常进托盘，不会强制结束企微进程。
                            </p>
                          </div>
                          <div className="step-card">
                            <div className="step-head">
                              <span className="step-num">4</span>超时秒数
                            </div>
                            <input
                              className="input mono"
                              type="number"
                              min={5}
                              max={180}
                              value={action.timeoutSec}
                              onChange={(e) =>
                                updateAction(idx, {
                                  timeoutSec: Number(e.target.value) || 30,
                                })
                              }
                            />
                          </div>
                        </div>
                        <div className="precheck-row">
                          <button
                            className="btn btn-secondary btn-sm"
                            type="button"
                            disabled={!!precheckBusy}
                            onClick={() => void runWecomPrecheck(action, "locate")}
                          >
                            {precheckBusy === "locate" ? "定位中…" : "测试定位窗口"}
                          </button>
                          <button
                            className="btn btn-secondary btn-sm"
                            type="button"
                            disabled={!!precheckBusy}
                            onClick={() => void runWecomPrecheck(action, "send")}
                          >
                            {precheckBusy === "send" ? "发送中…" : "试发送"}
                          </button>
                          <span className="muted" style={{ fontSize: 12 }}>
                            预检会暂时隐藏本窗口以免抢焦点；试发送会真实发消息，但不写正式任务日志。请保持企微已登录、勿锁屏。
                          </span>
                        </div>
                        <div className="fallback-box">
                          <p className="fallback-title">失败兜底</p>
                          <div className="row-2">
                            <div>
                              <label
                                style={{
                                  fontSize: 12.5,
                                  fontWeight: 600,
                                  marginBottom: 6,
                                  display: "block",
                                }}
                              >
                                失败重试次数
                              </label>
                              <input
                                className="input mono"
                                type="number"
                                min={0}
                                max={5}
                                value={action.retryCount}
                                onChange={(e) =>
                                  updateAction(idx, {
                                    retryCount: Math.max(0, Number(e.target.value) || 0),
                                  })
                                }
                              />
                            </div>
                            <label className="check-row" style={{ alignSelf: "end" }}>
                              <input
                                type="checkbox"
                                checked={action.notifyOnFail !== false}
                                onChange={(e) =>
                                  updateAction(idx, { notifyOnFail: e.target.checked })
                                }
                              />
                              失败后系统通知
                            </label>
                          </div>
                          <div className="hint" style={{ margin: 0 }}>
                            仍受设置页「任务失败时通知」总开关约束；总开关关闭时不弹通知。
                          </div>
                        </div>
                      </>
                    ) : null}
                  </div>
                ))}
              </div>
              <div className="hint">
                内置动作：打开应用 / 网址 / 脚本；可插拔扩展：企业微信 · 发私聊（UI 自动化，仅私聊）。多动作按顺序串行执行。
              </div>
            </div>

            <div className="form-footer">
              <button className="btn btn-secondary" type="button" onClick={onCancel}>
                取消
              </button>
              <button
                className="btn btn-primary"
                type="button"
                disabled={!canSave}
                onClick={() => void handleSave()}
                style={{ opacity: canSave ? 1 : 0.55 }}
              >
                保存
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
