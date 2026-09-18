import { useCallback, useEffect, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  exportTasks,
  getSettings,
  importTasks,
  openDataDir,
  readTextFile,
  reloadScheduler,
  requestNotificationPermission,
  resetSettings,
  setSchedulerPaused,
  updateSettings,
  wecomAgentPing,
  writeTextFile,
} from "../api/tasks";
import {
  appVersion,
  checkForUpdate,
  downloadAndInstallUpdate,
  relaunchApp,
} from "../api/updater";
import type {
  AppSettings,
  ImportMode,
  MissedJobPolicy,
  SchedulerStatus,
  StoreInfo,
} from "../types/task";

interface SettingsPageProps {
  info: StoreInfo | null;
  scheduler: SchedulerStatus | null;
  refreshToken: number;
  onToast: (message: string) => void;
  onSchedulerChange?: (status: SchedulerStatus) => void;
  onDataChanged?: () => void;
}

type SettingKey =
  | "autostart"
  | "minimizeToTrayOnClose"
  | "notifyOnTaskFail";

const SETTINGS_TABS = [
  { id: "general", label: "通用" },
  { id: "schedule", label: "调度" },
  { id: "wecom", label: "企微" },
  { id: "data", label: "数据" },
  { id: "about", label: "关于" },
] as const;

type SettingsTab = (typeof SETTINGS_TABS)[number]["id"];

function notificationPermissionLabel(value?: string) {
  const permission = value?.toLowerCase() ?? "unknown";
  if (permission.includes("granted")) return "已授权";
  if (permission.includes("denied")) return "已拒绝";
  if (permission.includes("prompt") || permission.includes("default")) return "待授权";
  return "未知";
}

export function SettingsPage({
  info,
  scheduler,
  refreshToken,
  onToast,
  onSchedulerChange,
  onDataChanged,
}: SettingsPageProps) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [includeLogs, setIncludeLogs] = useState(false);
  const [importMode, setImportMode] = useState<ImportMode>("merge");
  const [version, setVersion] = useState<string>("…");
  const [updateHint, setUpdateHint] = useState<string>("");
  const tabRefs = useRef<Record<SettingsTab, HTMLButtonElement | null>>({
    general: null,
    schedule: null,
    wecom: null,
    data: null,
    about: null,
  });

  const loadSettings = useCallback(async () => {
    try {
      const next = await getSettings();
      setSettings(next);
    } catch (e) {
      onToast(e instanceof Error ? e.message : String(e));
    }
  }, [onToast]);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings, refreshToken]);

  useEffect(() => {
    void appVersion()
      .then(setVersion)
      .catch(() => setVersion("未知"));
  }, []);

  useEffect(() => {
    if (scheduler == null) return;
    const paused = !!scheduler.paused;
    setSettings((s) => {
      if (!s || s.schedulerPaused === paused) return s;
      return { ...s, schedulerPaused: paused };
    });
  }, [scheduler?.paused]);

  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      onToast("已复制路径");
    } catch {
      onToast("复制失败，请手动选择文本");
    }
  }

  function selectTab(tab: SettingsTab, focus = false) {
    setActiveTab(tab);
    if (focus) tabRefs.current[tab]?.focus();
  }

  function handleTabKeyDown(event: React.KeyboardEvent<HTMLButtonElement>) {
    const currentIndex = SETTINGS_TABS.findIndex((tab) => tab.id === activeTab);
    let nextIndex = currentIndex;
    if (event.key === "ArrowRight") {
      nextIndex = (currentIndex + 1) % SETTINGS_TABS.length;
    } else if (event.key === "ArrowLeft") {
      nextIndex = (currentIndex - 1 + SETTINGS_TABS.length) % SETTINGS_TABS.length;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = SETTINGS_TABS.length - 1;
    } else {
      return;
    }
    event.preventDefault();
    selectTab(SETTINGS_TABS[nextIndex].id, true);
  }

  async function patchSetting(key: SettingKey, value: boolean) {
    if (!settings || busyKey) return;
    setBusyKey(key);
    const prev = settings;
    setSettings({ ...settings, [key]: value });
    try {
      const next = await updateSettings({ [key]: value });
      setSettings(next);
      const messages: Record<SettingKey, [string, string]> = {
        autostart: ["已开启开机自启", "已关闭开机自启"],
        minimizeToTrayOnClose: ["关闭窗口将最小化到托盘", "关闭窗口将退出应用"],
        notifyOnTaskFail: [
          "任务失败时将尝试系统通知",
          "已关闭失败通知总开关（即使动作要求也不弹）",
        ],
      };
      onToast(value ? messages[key][0] : messages[key][1]);
    } catch (e) {
      setSettings(prev);
      onToast(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  }

  async function handlePauseToggle() {
    if (!settings || busyKey) return;
    const nextPaused = !settings.schedulerPaused;
    setBusyKey("schedulerPaused");
    const prev = settings;
    setSettings({ ...settings, schedulerPaused: nextPaused });
    try {
      const status = await setSchedulerPaused(nextPaused);
      onSchedulerChange?.(status);
      const refreshed = await getSettings();
      setSettings(refreshed);
      onToast(nextPaused ? "已暂停调度（状态已保存）" : "已恢复调度");
    } catch (e) {
      setSettings(prev);
      onToast(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  }

  async function handleMissedPolicy(policy: MissedJobPolicy) {
    if (!settings || busyKey || settings.missedJobPolicy === policy) return;
    setBusyKey("missedJobPolicy");
    const prev = settings;
    setSettings({ ...settings, missedJobPolicy: policy });
    try {
      const next = await updateSettings({ missedJobPolicy: policy });
      setSettings(next);
      onToast(
        policy === "run_once"
          ? "错过触发将补跑一次（24h 内最近应触发点）"
          : "错过触发不补跑",
      );
    } catch (e) {
      setSettings(prev);
      onToast(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  }

  async function patchWecomIdle(patch: {
    enabled?: boolean;
    idleSeconds?: number;
    countdownSeconds?: number;
  }) {
    if (!settings || busyKey) return;
    setBusyKey("wecomIdleSend");
    const prev = settings;
    const merged = {
      ...settings.wecomIdleSend,
      ...patch,
    };
    setSettings({ ...settings, wecomIdleSend: merged });
    try {
      const next = await updateSettings({ wecomIdleSend: patch });
      setSettings(next);
      if (patch.enabled != null) {
        onToast(
          patch.enabled
            ? "已开启企微空闲发送：含企微任务将排队，空闲后发送"
            : "已关闭空闲发送：含企微任务将立即抢前台发送",
        );
      } else {
        onToast("企微空闲发送参数已更新");
      }
    } catch (e) {
      setSettings(prev);
      onToast(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  }

  async function handleRequestPermission() {
    if (busyKey) return;
    setBusyKey("notifyPerm");
    try {
      const state = await requestNotificationPermission();
      await loadSettings();
      if (state.includes("granted")) {
        onToast("通知权限已授予");
      } else if (state.includes("denied")) {
        onToast("通知权限被拒绝，请在系统设置中允许「时序 · TaskTick」");
      } else {
        onToast(`通知权限状态：${notificationPermissionLabel(state)}`);
      }
    } catch (e) {
      onToast(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  }

  async function handleCheckUpdate() {
    if (busyKey) return;
    setBusyKey("checkUpdate");
    setUpdateHint("");
    try {
      const update = await checkForUpdate();
      if (!update) {
        setUpdateHint("当前已是最新版本");
        onToast("当前已是最新版本");
        return;
      }
      const notes = (update.body ?? "").trim();
      const confirmMsg = notes
        ? `发现新版本 ${update.version}（当前 ${version}）\n\n${notes}\n\n下载并安装？安装过程中应用将退出。`
        : `发现新版本 ${update.version}（当前 ${version}）\n\n下载并安装？安装过程中应用将退出。`;
      if (!window.confirm(confirmMsg)) {
        setUpdateHint(`有可用更新：${update.version}`);
        return;
      }
      setUpdateHint(`正在下载 ${update.version}…`);
      await downloadAndInstallUpdate(update, ({ downloaded, contentLength }) => {
        if (contentLength && contentLength > 0) {
          const pct = Math.min(100, Math.round((downloaded / contentLength) * 100));
          setUpdateHint(`正在下载 ${update.version}… ${pct}%`);
        } else {
          setUpdateHint(`正在下载 ${update.version}…`);
        }
      });
      setUpdateHint("更新已安装，正在重启…");
      onToast("更新已安装");
      try {
        await relaunchApp();
      } catch {
        /* Windows 安装器通常已退出进程 */
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setUpdateHint(`检查更新失败：${msg}`);
      onToast(`检查更新失败：${msg}`);
    } finally {
      setBusyKey(null);
    }
  }

  async function handleOpenDataDir() {
    if (busyKey) return;
    setBusyKey("openDataDir");
    try {
      await openDataDir();
      onToast("已打开数据目录");
    } catch (e) {
      onToast(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  }

  async function handleResetSettings() {
    if (busyKey) return;
    if (
      !window.confirm(
        "确定将设置恢复为默认？\n（不影响任务与日志；会关闭开机自启并恢复调度默认策略）",
      )
    ) {
      return;
    }
    setBusyKey("resetSettings");
    try {
      const next = await resetSettings();
      setSettings(next);
      const status = await reloadScheduler().catch(() => null);
      if (status) onSchedulerChange?.(status);
      onToast("设置已恢复默认");
    } catch (e) {
      onToast(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  }

  async function handleExportTasks() {
    if (busyKey) return;
    setBusyKey("exportTasks");
    try {
      const payload = await exportTasks(includeLogs);
      const stamp = new Date()
        .toISOString()
        .replace(/[:.]/g, "-")
        .slice(0, 19);
      const path = await save({
        title: "导出任务",
        defaultPath: `tasktick-export-${stamp}.json`,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      await writeTextFile(path, JSON.stringify(payload, null, 2));
      const logHint = includeLogs
        ? `，含 ${payload.logs?.length ?? 0} 条日志`
        : "（不含日志）";
      onToast(`已导出 ${payload.tasks.length} 个任务${logHint}`);
    } catch (e) {
      onToast(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  }

  async function handleImportTasks() {
    if (busyKey) return;
    if (importMode === "replace") {
      if (
        !window.confirm(
          "替换模式将用导入文件中的合法任务覆盖当前全部任务（不会清空日志）。确定继续？",
        )
      ) {
        return;
      }
    }
    setBusyKey("importTasks");
    try {
      const selected = await open({
        title: "导入任务",
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!selected || Array.isArray(selected)) {
        return;
      }
      const raw = await readTextFile(selected);
      const result = await importTasks(raw, importMode);
      await reloadScheduler().catch(() => undefined);
      onDataChanged?.();
      const errHint =
        result.skipped > 0
          ? `，跳过 ${result.skipped}${
              result.errors[0] ? `（如：${result.errors[0]}）` : ""
            }`
          : "";
      onToast(
        importMode === "merge"
          ? `合并完成：新增 ${result.imported}，更新 ${result.updated}${errHint}；现有 ${result.taskCount} 个任务`
          : `替换完成：写入 ${result.imported} 个任务${errHint}`,
      );
    } catch (e) {
      onToast(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyKey(null);
    }
  }

  const paused = settings?.schedulerPaused ?? scheduler?.paused ?? false;

  const schedulerDesc = (() => {
    if (!scheduler) return "状态未知或未启动";
    if (paused) {
      return `已暂停 · 每 ${scheduler.tickIntervalSecs}s 检查（不触发） · 已启用 ${scheduler.enabledTaskCount} 个任务`;
    }
    if (scheduler.running) {
      return `运行中 · 每 ${scheduler.tickIntervalSecs}s 检查 · 已启用 ${scheduler.enabledTaskCount} 个任务`;
    }
    return "状态未知或未启动";
  })();

  const permLabel = notificationPermissionLabel(settings?.notificationPermission);
  const policy = settings?.missedJobPolicy ?? "skip";
  const wecomIdle = settings?.wecomIdleSend ?? {
    enabled: true,
    idleSeconds: 30,
    countdownSeconds: 5,
  };

  return (
    <div className="content">
      <div className="settings-wrap">
        <div className="settings-tabs" role="tablist" aria-label="设置分组">
          {SETTINGS_TABS.map((tab) => (
            <button
              key={tab.id}
              ref={(element) => {
                tabRefs.current[tab.id] = element;
              }}
              id={`settings-tab-${tab.id}`}
              type="button"
              role="tab"
              aria-selected={activeTab === tab.id}
              aria-controls={`settings-panel-${tab.id}`}
              tabIndex={activeTab === tab.id ? 0 : -1}
              className={`settings-tab${activeTab === tab.id ? " active" : ""}`}
              onClick={() => selectTab(tab.id)}
              onKeyDown={handleTabKeyDown}
            >
              {tab.label}
            </button>
          ))}
        </div>

        <div
          id={`settings-panel-${activeTab}`}
          className="settings-stack"
          role="tabpanel"
          aria-labelledby={`settings-tab-${activeTab}`}
          tabIndex={0}
        >
        {activeTab === "data" ? (
          <>
        <div className="card">
          <div className="setting-row" style={{ flexDirection: "column", alignItems: "stretch" }}>
            <div>
              <p className="setting-title">数据目录</p>
              <p className="setting-desc">任务定义、执行日志与扩展配置存放于此。</p>
            </div>
            <div className="path-box">
              <span>{info?.dataDir || "加载中…"}</span>
              <div className="setting-inline-actions">
                {info?.dataDir ? (
                  <>
                    <button
                      className="btn btn-secondary btn-sm"
                      type="button"
                      disabled={busyKey === "openDataDir"}
                      onClick={() => void handleOpenDataDir()}
                    >
                      打开
                    </button>
                    <button
                      className="btn btn-secondary btn-sm"
                      type="button"
                      onClick={() => void copyPath(info.dataDir)}
                    >
                      复制
                    </button>
                  </>
                ) : null}
              </div>
            </div>
          </div>
          <div className="setting-row" style={{ flexDirection: "column", alignItems: "stretch" }}>
            <div>
              <p className="setting-title">存储文件</p>
              <p className="setting-desc">当前使用的本地存储文件完整路径。</p>
            </div>
            <div className="path-box">
              <span>{info?.storeFile || "加载中…"}</span>
              {info?.storeFile ? (
                <button
                  className="btn btn-secondary btn-sm"
                  type="button"
                  onClick={() => void copyPath(info.storeFile)}
                >
                  复制
                </button>
              ) : null}
            </div>
          </div>
          <div className="setting-row">
            <div>
              <p className="setting-title">计数</p>
              <p className="setting-desc">本地已持久化的任务与日志条数。</p>
            </div>
            <div className="mono" style={{ fontSize: 13 }}>
              任务 {info?.taskCount ?? "—"} / 日志 {info?.logCount ?? "—"}
            </div>
          </div>
        </div>

        <div className="card">
          <div
            className="setting-row"
            style={{ flexDirection: "column", alignItems: "stretch", gap: 12 }}
          >
            <div>
              <p className="setting-title">任务导入 / 导出</p>
              <p className="setting-desc">
                导出为 JSON（默认不含日志）。导入支持本应用导出包、含 tasks 的 store.json
                片段，或任务数组。合并按 id 更新/新增；替换覆盖全部任务（日志与设置不动）。非法项跳过并汇总。
              </p>
            </div>
            <label
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                fontSize: 13,
                color: "var(--fg)",
              }}
            >
              <input
                type="checkbox"
                checked={includeLogs}
                onChange={(e) => setIncludeLogs(e.target.checked)}
              />
              导出时包含执行日志
            </label>
            <div className="seg" style={{ alignSelf: "flex-start" }}>
              <button
                type="button"
                className={importMode === "merge" ? "active" : ""}
                onClick={() => setImportMode("merge")}
              >
                合并导入
              </button>
              <button
                type="button"
                className={importMode === "replace" ? "active" : ""}
                onClick={() => setImportMode("replace")}
              >
                替换导入
              </button>
            </div>
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
              <button
                className="btn btn-secondary btn-sm"
                type="button"
                disabled={!!busyKey}
                onClick={() => void handleExportTasks()}
              >
                {busyKey === "exportTasks" ? "导出中…" : "导出任务"}
              </button>
              <button
                className="btn btn-primary btn-sm"
                type="button"
                disabled={!!busyKey}
                onClick={() => void handleImportTasks()}
              >
                {busyKey === "importTasks" ? "导入中…" : "导入任务"}
              </button>
            </div>
          </div>
        </div>

          </>
        ) : null}

        {activeTab === "schedule" ? (
        <div className="card">
          <div className="setting-row">
            <div>
              <p className="setting-title">调度器</p>
              <p className="setting-desc">{schedulerDesc}</p>
            </div>
            <div className="mono" style={{ fontSize: 12.5, textAlign: "right" }}>
              <div>{scheduler?.lastTickAt ? `上次检查 ${scheduler.lastTickAt}` : "—"}</div>
              <div className="muted">
                {scheduler?.runningTaskCount
                  ? `执行中 ${scheduler.runningTaskCount}`
                  : paused
                    ? "已暂停"
                    : "空闲"}
              </div>
            </div>
          </div>
          <div className="setting-row">
            <div>
              <p className="setting-title">暂停调度</p>
              <p className="setting-desc">
                与托盘「暂停/恢复调度」同步，下次启动会保持当前状态。暂停期间不触发任务，手动执行仍可用。
              </p>
            </div>
            <button
              type="button"
              className={`switch${paused ? " on" : ""}`}
              aria-label="暂停调度"
              aria-pressed={paused}
              disabled={!settings || busyKey === "schedulerPaused"}
              onClick={() => void handlePauseToggle()}
            />
          </div>
          <div
            className="setting-row"
            style={{ flexDirection: "column", alignItems: "stretch", gap: 10 }}
          >
            <div>
              <p className="setting-title">错过补跑策略</p>
              <p className="setting-desc">
                应用启动或从暂停恢复后，可补跑过去 24 小时内最近一次错过的任务。每个任务最多补跑一次；单次任务处理后会自动禁用。
              </p>
            </div>
            <div className="seg" style={{ alignSelf: "flex-start" }}>
              <button
                type="button"
                className={policy === "skip" ? "active" : ""}
                disabled={!settings || busyKey === "missedJobPolicy"}
                onClick={() => void handleMissedPolicy("skip")}
              >
                不补跑
              </button>
              <button
                type="button"
                className={policy === "run_once" ? "active" : ""}
                disabled={!settings || busyKey === "missedJobPolicy"}
                onClick={() => void handleMissedPolicy("run_once")}
              >
                补跑一次
              </button>
            </div>
          </div>
          <div className="setting-row" style={{ flexDirection: "column", alignItems: "stretch" }}>
            <div>
              <p className="setting-title">时区</p>
              <p className="setting-desc">
                <strong>所有触发时间均以本机本地时区为准</strong>
                ，不进行多时区转换。once / daily / cron / interval 的预览与调度均使用本地时钟。
              </p>
            </div>
          </div>
        </div>
        ) : null}

        {activeTab === "general" ? (
        <div className="card">
          <div className="setting-row">
            <div>
              <p className="setting-title">开机自启</p>
              <p className="setting-desc">
                登录 Windows 后自动启动（写入系统启动项；重启后生效）。
              </p>
            </div>
            <button
              type="button"
              className={`switch${settings?.autostart ? " on" : ""}`}
              aria-label="开机自启"
              aria-pressed={settings?.autostart ?? false}
              disabled={!settings || busyKey === "autostart"}
              onClick={() =>
                void patchSetting("autostart", !(settings?.autostart ?? false))
              }
            />
          </div>
          <div className="setting-row">
            <div>
              <p className="setting-title">关闭窗口时最小化到托盘</p>
              <p className="setting-desc">
                开启后点击关闭仅隐藏主窗口，进程与调度继续运行；托盘可退出。
              </p>
            </div>
            <button
              type="button"
              className={`switch${settings?.minimizeToTrayOnClose ? " on" : ""}`}
              aria-label="关闭窗口时最小化到托盘"
              aria-pressed={settings?.minimizeToTrayOnClose ?? false}
              disabled={!settings || busyKey === "minimizeToTrayOnClose"}
              onClick={() =>
                void patchSetting(
                  "minimizeToTrayOnClose",
                  !(settings?.minimizeToTrayOnClose ?? false),
                )
              }
            />
          </div>
          <div className="setting-row">
            <div>
              <p className="setting-title">任务失败时通知</p>
              <p className="setting-desc">
                总开关：关闭后即使任务动作启用了失败通知，也不再弹出系统通知。
              </p>
            </div>
            <button
              type="button"
              className={`switch${settings?.notifyOnTaskFail ? " on" : ""}`}
              aria-label="任务失败时通知"
              aria-pressed={settings?.notifyOnTaskFail ?? false}
              disabled={!settings || busyKey === "notifyOnTaskFail"}
              onClick={() =>
                void patchSetting(
                  "notifyOnTaskFail",
                  !(settings?.notifyOnTaskFail ?? false),
                )
              }
            />
          </div>
          <div className="setting-row">
            <div>
              <p className="setting-title">通知权限</p>
              <p className="setting-desc">
                当前状态：{permLabel}。若发送失败，请申请权限或在 Windows
                「通知与操作」中允许本应用。
              </p>
            </div>
            <button
              className="btn btn-secondary btn-sm"
              type="button"
              disabled={busyKey === "notifyPerm"}
              onClick={() => void handleRequestPermission()}
            >
              {busyKey === "notifyPerm" ? "申请中…" : "申请权限"}
            </button>
          </div>
          <div className="setting-row">
            <div>
              <p className="setting-title">重置为默认</p>
              <p className="setting-desc">
                恢复所有设置的默认值，不修改任务与日志。
              </p>
            </div>
            <button
              className="btn btn-danger-ghost btn-sm"
              type="button"
              disabled={!!busyKey}
              onClick={() => void handleResetSettings()}
            >
              重置设置
            </button>
          </div>
        </div>

        ) : null}

        {activeTab === "wecom" ? (
        <div className="card">
          <div
            className="setting-row"
            style={{ flexDirection: "column", alignItems: "stretch", gap: 10 }}
          >
            <div>
              <p className="setting-title">UI 自动化插件</p>
              <p className="setting-desc">
                「企业微信 · 发私聊」默认使用键鼠操作，也可优先使用 FlaUI Agent。
              </p>
            </div>
            <div className="setting-row" style={{ paddingLeft: 0 }}>
              <div>
                <p className="setting-title">启用 FlaUI Agent</p>
                <p className="setting-desc">
                  优先调用 wecom-agent.exe；失败时可回退键鼠。
                  {settings?.wecomAgentResolvedPath
                    ? ` 当前：${settings.wecomAgentResolvedPath}`
                    : " 尚未解析到 Agent 可执行文件。"}
                </p>
              </div>
              <button
                type="button"
                className={`switch${settings?.wecomAgent?.enabled ? " on" : ""}`}
                aria-label="启用 FlaUI Agent"
                aria-pressed={settings?.wecomAgent?.enabled ?? false}
                disabled={!settings || busyKey === "wecomAgent"}
                onClick={() =>
                  void (async () => {
                    if (!settings) return;
                    setBusyKey("wecomAgent");
                    try {
                      const next = await updateSettings({
                        wecomAgent: { enabled: !settings.wecomAgent?.enabled },
                      });
                      setSettings(next);
                      onToast(
                        next.wecomAgent.enabled
                          ? "已启用 FlaUI Agent"
                          : "已关闭 FlaUI Agent（使用键鼠）",
                      );
                    } catch (e) {
                      onToast(e instanceof Error ? e.message : String(e));
                    } finally {
                      setBusyKey(null);
                    }
                  })()
                }
              />
            </div>
            <div className="setting-row" style={{ paddingLeft: 0 }}>
              <div>
                <p className="setting-title">Agent 失败回退键鼠</p>
                <p className="setting-desc">关闭后 Agent 失败即记失败，不再尝试内置键鼠。</p>
              </div>
              <button
                type="button"
                className={`switch${settings?.wecomAgent?.fallbackToInput !== false ? " on" : ""}`}
                aria-label="Agent 失败时回退键鼠操作"
                aria-pressed={settings?.wecomAgent?.fallbackToInput !== false}
                disabled={!settings || busyKey === "wecomAgentFb"}
                onClick={() =>
                  void (async () => {
                    if (!settings) return;
                    setBusyKey("wecomAgentFb");
                    try {
                      const next = await updateSettings({
                        wecomAgent: {
                          fallbackToInput: !(settings.wecomAgent?.fallbackToInput !== false),
                        },
                      });
                      setSettings(next);
                      onToast("已更新回退策略");
                    } catch (e) {
                      onToast(e instanceof Error ? e.message : String(e));
                    } finally {
                      setBusyKey(null);
                    }
                  })()
                }
              />
            </div>
            <div className="setting-row" style={{ paddingLeft: 0 }}>
              <div>
                <p className="setting-title">探测 Agent</p>
                <p className="setting-desc">确认 Agent 可执行文件能够正常启动并响应。</p>
              </div>
              <button
                className="btn btn-secondary btn-sm"
                type="button"
                disabled={busyKey === "agentPing"}
                onClick={() =>
                  void (async () => {
                    setBusyKey("agentPing");
                    try {
                      const r = await wecomAgentPing();
                      onToast(r.note);
                      await loadSettings();
                    } catch (e) {
                      onToast(e instanceof Error ? e.message : String(e));
                    } finally {
                      setBusyKey(null);
                    }
                  })()
                }
              >
                {busyKey === "agentPing" ? "探测中…" : "检测 Agent"}
              </button>
            </div>
            <div className="notice">
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
                <strong>风险提示</strong>
                ：依赖企业微信窗口结构与前台焦点。改版后需更新适配。请勿在无人值守锁屏场景依赖本动作。
              </div>
            </div>
          </div>
          <div className="setting-row">
            <div>
              <p className="setting-title">企微空闲后再发送</p>
              <p className="setting-desc">
                开启后，包含企微私聊动作的任务会等待键鼠空闲再执行。关闭后到点立即发送；「试发送」与「强制立即」始终立刻执行。
              </p>
            </div>
            <button
              type="button"
              className={`switch${wecomIdle.enabled ? " on" : ""}`}
              aria-label="企微空闲后再发送"
              aria-pressed={wecomIdle.enabled}
              disabled={!settings || busyKey === "wecomIdleSend"}
              onClick={() => void patchWecomIdle({ enabled: !wecomIdle.enabled })}
            />
          </div>
          <div className="setting-row">
            <div>
              <p className="setting-title">空闲阈值（秒）</p>
              <p className="setting-desc">
                使用 GetLastInputInfo 检测键鼠空闲；达到该秒数且调度未暂停时开始倒计时。范围 5–600，默认
                30。
              </p>
            </div>
            <input
              className="input mono"
              style={{ width: 88, textAlign: "right" }}
              type="number"
              aria-label="企微空闲阈值（秒）"
              min={5}
              max={600}
              disabled={!settings || busyKey === "wecomIdleSend"}
              value={wecomIdle.idleSeconds}
              onChange={(e) => {
                const v = Number(e.target.value);
                if (!settings || Number.isNaN(v)) return;
                setSettings({
                  ...settings,
                  wecomIdleSend: { ...wecomIdle, idleSeconds: v },
                });
              }}
              onBlur={() => {
                void patchWecomIdle({
                  idleSeconds: Math.min(600, Math.max(5, wecomIdle.idleSeconds)),
                });
              }}
            />
          </div>
          <div className="setting-row">
            <div>
              <p className="setting-title">发送前倒计时（秒）</p>
              <p className="setting-desc">
                倒计时期间若再次键鼠输入，取消本次尝试并继续等待空闲。范围 1–60，默认 5。
              </p>
            </div>
            <input
              className="input mono"
              style={{ width: 88, textAlign: "right" }}
              type="number"
              aria-label="企微发送前倒计时（秒）"
              min={1}
              max={60}
              disabled={!settings || busyKey === "wecomIdleSend"}
              value={wecomIdle.countdownSeconds}
              onChange={(e) => {
                const v = Number(e.target.value);
                if (!settings || Number.isNaN(v)) return;
                setSettings({
                  ...settings,
                  wecomIdleSend: { ...wecomIdle, countdownSeconds: v },
                });
              }}
              onBlur={() => {
                void patchWecomIdle({
                  countdownSeconds: Math.min(
                    60,
                    Math.max(1, wecomIdle.countdownSeconds),
                  ),
                });
              }}
            />
          </div>
        </div>

        ) : null}

        {activeTab === "about" ? (
        <div className="card">
          <div className="about-block">
            <p className="setting-title" style={{ marginBottom: 8 }}>
              关于
            </p>
            <div className="about-row">
              <span>应用</span>
              <span>时序 · TaskTick</span>
            </div>
            <div className="about-row">
              <span>版本</span>
              <span className="mono">{version}</span>
            </div>
            <div className="about-row about-update-row">
              <div>
                <span>应用更新</span>
                {updateHint ? (
                  <p className="setting-desc update-hint" role="status" aria-live="polite">
                    {updateHint}
                  </p>
                ) : null}
              </div>
              <button
                type="button"
                className="btn btn-secondary btn-sm"
                disabled={busyKey === "checkUpdate"}
                onClick={() => void handleCheckUpdate()}
              >
                {busyKey === "checkUpdate" ? "检查中…" : "检查更新"}
              </button>
            </div>
          </div>
        </div>
        ) : null}
        </div>
      </div>
    </div>
  );
}
