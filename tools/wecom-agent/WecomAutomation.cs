using System.Diagnostics;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Text;
using FlaUI.Core.AutomationElements;
using FlaUI.Core.Definitions;
using FlaUI.Core.Input;
using FlaUI.Core.WindowsAPI;
using FlaUI.UIA3;

namespace WecomAgent;

/// <summary>
/// 企微 FlaUI 发送：前台 + 真实键鼠（与 wecom.rs 一致）。
/// 流程：Ctrl+F → 粘贴联系人 → Enter → 点击底部输入区 → 粘贴消息 → Enter。
/// </summary>
public static class WecomAutomation
{
    private static readonly string[] ExeCandidates =
    {
        @"C:\Program Files (x86)\WXWork\WXWork.exe",
        @"C:\Program Files\WXWork\WXWork.exe",
        @"D:\Program Files (x86)\WXWork\WXWork.exe",
        @"D:\Program Files\WXWork\WXWork.exe",
    };

    private static readonly string[] MessageHints =
    {
        "输入", "说点什么", "请输入", "发送消息",
    };

    public static AgentResponse Probe(AgentRequest req)
    {
        var steps = new List<AgentStep>();
        try
        {
            if (IsSessionLocked())
            {
                const string note = "系统已锁屏，已跳过企微自动化";
                steps.Add(Fail("锁屏检查", note));
                return FailResp(req.Id, note, steps);
            }
            using var automation = new UIA3Automation();
            var win = EnsureWindow(automation, req.LaunchWecom, req.TimeoutSec, steps);
            EnsureForeground(win, steps);
            return OkResp(req.Id, "FlaUI 预检：已定位并前置企微主窗口", steps);
        }
        catch (Exception ex)
        {
            steps.Add(Fail("预检", ex.Message));
            return FailResp(req.Id, ex.Message, steps);
        }
    }

    public static AgentResponse Send(AgentRequest req)
    {
        var steps = new List<AgentStep>();
        var contact = (req.Contact ?? "").Trim();
        var message = (req.Message ?? "").Trim();
        if (string.IsNullOrEmpty(contact) || string.IsNullOrEmpty(message))
        {
            steps.Add(Fail("参数校验", "联系人或消息为空"));
            return FailResp(req.Id, "联系人或消息为空", steps);
        }

        var attempts = Math.Max(1, req.RetryCount + 1);
        Exception? last = null;
        try
        {
            if (IsSessionLocked())
            {
                const string note = "系统已锁屏，已跳过企微自动化";
                steps.Add(Fail("锁屏检查", note));
                return FailResp(req.Id, note, steps);
            }

            for (var i = 1; i <= attempts; i++)
            {
                if (attempts > 1)
                    steps.Add(Ok("重试", $"第 {i}/{attempts} 次"));
                try
                {
                    if (IsSessionLocked())
                        throw new InvalidOperationException("系统已锁屏，已跳过企微自动化");

                    using var automation = new UIA3Automation();
                    var win = EnsureWindow(automation, req.LaunchWecom, req.TimeoutSec, steps);
                    var hwnd = RequireHwnd(win);
                    EnsureForeground(win, hwnd, steps);

                    // 1) Ctrl+F 打开搜索（与键鼠回退一致，不依赖 UIA 碰巧找到搜索框）
                    Keyboard.TypeSimultaneously(VirtualKeyShort.CONTROL, VirtualKeyShort.KEY_F);
                    Thread.Sleep(300);
                    steps.Add(Ok("打开搜索", "Ctrl+F（真实键盘）"));

                    // 2) 粘贴联系人
                    EnsureStillForeground(hwnd);
                    PasteText(contact);
                    Thread.Sleep(380);
                    steps.Add(Ok("粘贴联系人", $"已粘贴「{contact}」"));

                    // 3) Enter 打开首个搜索结果
                    EnsureStillForeground(hwnd);
                    Keyboard.Type(VirtualKeyShort.ENTER);
                    Thread.Sleep(480);
                    steps.Add(Ok("打开私聊", "Enter 选中首个结果（重名可能选错）"));

                    // 4) 点击主窗口下部输入区（比纯 UIA Focus 更稳）
                    EnsureStillForeground(hwnd);
                    ClickMessageInputArea(hwnd);
                    Thread.Sleep(140);
                    steps.Add(Ok("聚焦输入框", "已点击主窗口下部输入区域"));

                    // 可选：再点一次 UIA 找到的消息框，提高命中率
                    var input = FindMessageInput(win);
                    if (input != null)
                        TryClickElement(input);

                    // 5) 粘贴消息
                    EnsureStillForeground(hwnd);
                    PasteText(message);
                    Thread.Sleep(140);
                    steps.Add(Ok("粘贴消息", $"已粘贴 {message.Length} 字"));

                    // 6) Enter 发送
                    EnsureStillForeground(hwnd);
                    Keyboard.Type(VirtualKeyShort.ENTER);
                    Thread.Sleep(200);
                    steps.Add(Ok("发送", "Enter（请人工确认会话是否出现该消息）"));

                    if (req.CloseAfterSend)
                    {
                        Thread.Sleep(300);
                        try
                        {
                            win.Close();
                            steps.Add(Ok("关闭窗口", "已关闭企微主窗口（可能仍在托盘）"));
                        }
                        catch (Exception closeEx)
                        {
                            try
                            {
                                win.Patterns.Window.Pattern.SetWindowVisualState(
                                    WindowVisualState.Minimized);
                                steps.Add(Ok("关闭窗口", $"Close 失败已最小化 · {closeEx.Message}"));
                            }
                            catch (Exception minEx)
                            {
                                steps.Add(Fail("关闭窗口", $"{closeEx.Message}; 最小化也失败: {minEx.Message}"));
                            }
                        }
                    }

                    return OkResp(req.Id, $"FlaUI 已向「{contact}」发送", steps);
                }
                catch (Exception ex)
                {
                    last = ex;
                    steps.Add(Fail($"尝试{i}", ex.Message));
                    ForceReleaseModifiers();
                    Thread.Sleep(350);
                }
            }

            return FailResp(req.Id, last?.Message ?? "发送失败", steps);
        }
        finally
        {
            ForceReleaseModifiers();
        }
    }

    private static void ForceReleaseModifiers()
    {
        try
        {
            const byte KEYEVENTF_KEYUP = 0x02;
            foreach (byte vk in new byte[]
                     {
                         0x11, 0xA2, 0xA3,
                         0x10, 0xA0, 0xA1,
                         0x12, 0xA4, 0xA5,
                     })
            {
                keybd_event(vk, 0, KEYEVENTF_KEYUP, UIntPtr.Zero);
            }
        }
        catch
        {
            try
            {
                Keyboard.Release(VirtualKeyShort.CONTROL);
                Keyboard.Release(VirtualKeyShort.SHIFT);
                Keyboard.Release(VirtualKeyShort.ALT);
            }
            catch
            {
                // ignore
            }
        }
    }

    private static bool IsSessionLocked()
    {
        const uint DESKTOP_READOBJECTS = 0x0001;
        const uint DESKTOP_WRITEOBJECTS = 0x0080;
        var h = OpenInputDesktop(0, false, DESKTOP_READOBJECTS | DESKTOP_WRITEOBJECTS);
        if (h == IntPtr.Zero)
            return true;
        CloseDesktop(h);
        return false;
    }

    [DllImport("user32.dll")]
    private static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr OpenInputDesktop(uint dwFlags, bool fInherit, uint dwDesiredAccess);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool CloseDesktop(IntPtr hDesktop);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    private static extern IntPtr GetForegroundWindow();

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

    [DllImport("user32.dll")]
    private static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);

    [DllImport("kernel32.dll")]
    private static extern uint GetCurrentThreadId();

    [DllImport("user32.dll")]
    private static extern bool BringWindowToTop(IntPtr hWnd);

    [DllImport("user32.dll")]
    private static extern bool GetClientRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    private static extern bool ClientToScreen(IntPtr hWnd, ref POINT lpPoint);

    [DllImport("user32.dll")]
    private static extern bool SetCursorPos(int x, int y);

    [DllImport("user32.dll")]
    private static extern int GetSystemMetrics(int nIndex);

    [StructLayout(LayoutKind.Sequential)]
    private struct RECT
    {
        public int Left, Top, Right, Bottom;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT
    {
        public int X, Y;
    }

    private static Window EnsureWindow(
        UIA3Automation automation,
        bool launch,
        int timeoutSec,
        List<AgentStep> steps)
    {
        var existing = FindWecomWindow(automation);
        if (existing != null)
        {
            steps.Add(Ok("定位主窗口", $"已存在 · {existing.Name}"));
            return existing;
        }

        if (launch)
        {
            var path = ResolveExe()
                ?? throw new InvalidOperationException("未找到 WXWork.exe，请手动启动企业微信");
            Process.Start(new ProcessStartInfo(path) { UseShellExecute = true });
            steps.Add(Ok("启动企微", path));
        }

        var deadline = DateTime.UtcNow.AddSeconds(Math.Max(5, timeoutSec));
        while (DateTime.UtcNow < deadline)
        {
            var w = FindWecomWindow(automation);
            if (w != null)
            {
                steps.Add(Ok("定位主窗口", w.Name));
                return w;
            }
            Thread.Sleep(200);
        }

        throw new TimeoutException("超时未找到企业微信主窗口（标题含「企业微信」或 WXWork）");
    }

    private static Window? FindWecomWindow(UIA3Automation automation)
    {
        var desktop = automation.GetDesktop();
        var windows = desktop.FindAllChildren(cf => cf.ByControlType(ControlType.Window));
        foreach (var el in windows)
        {
            var name = el.Name ?? "";
            if (name.Contains("企业微信", StringComparison.Ordinal)
                || name.Contains("WXWork", StringComparison.OrdinalIgnoreCase))
            {
                return el.AsWindow();
            }
        }
        return null;
    }

    private static string? ResolveExe()
    {
        foreach (var c in ExeCandidates)
        {
            if (File.Exists(c)) return c;
        }
        var local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        var p = Path.Combine(local, "WXWork", "WXWork.exe");
        return File.Exists(p) ? p : null;
    }

    private static AutomationElement? FindEditByHint(Window win, string hint)
    {
        var edits = win.FindAllDescendants(cf => cf.ByControlType(ControlType.Edit));
        foreach (var e in edits)
        {
            var name = e.Name ?? "";
            if (name.Contains(hint, StringComparison.OrdinalIgnoreCase))
                return e;
        }
        return null;
    }

    private static AutomationElement? FindMessageInput(Window win)
    {
        foreach (var hint in MessageHints)
        {
            var byHint = FindEditByHint(win, hint);
            if (byHint != null)
                return byHint;
        }

        var doc = win.FindFirstDescendant(cf => cf.ByControlType(ControlType.Document));
        if (doc != null)
            return doc;

        return BottomMostEdit(win);
    }

    private static AutomationElement? BottomMostEdit(Window win)
    {
        var edits = win.FindAllDescendants(cf => cf.ByControlType(ControlType.Edit));
        AutomationElement? best = null;
        double bestY = -1;
        foreach (var e in edits)
        {
            try
            {
                var r = e.BoundingRectangle;
                if (r.IsEmpty) continue;
                var name = e.Name ?? "";
                if (name.Contains("搜索", StringComparison.OrdinalIgnoreCase))
                    continue;
                if (r.Top > bestY)
                {
                    bestY = r.Top;
                    best = e;
                }
            }
            catch
            {
                // ignore
            }
        }
        return best;
    }

    private static IntPtr RequireHwnd(Window win)
    {
        var hwnd = win.Properties.NativeWindowHandle.ValueOrDefault;
        if (hwnd == IntPtr.Zero)
        {
            try { hwnd = win.Properties.NativeWindowHandle.Value; }
            catch { hwnd = IntPtr.Zero; }
        }
        if (hwnd == IntPtr.Zero)
            throw new InvalidOperationException("企微窗口没有句柄，无法前置/点击");
        return hwnd;
    }

    private static void EnsureForeground(Window win, List<AgentStep> steps)
    {
        EnsureForeground(win, RequireHwnd(win), steps);
    }

    private static void EnsureForeground(Window win, IntPtr hwnd, List<AgentStep> steps)
    {
        ShowWindow(hwnd, 9); // SW_RESTORE
        ForceSetForeground(hwnd);
        try { win.Focus(); } catch { /* ignore */ }

        var deadline = DateTime.UtcNow.AddMilliseconds(1000);
        while (DateTime.UtcNow < deadline)
        {
            if (GetForegroundWindow() == hwnd)
            {
                steps.Add(Ok("窗口前置", "已激活企微前台（后续键鼠依赖前台）"));
                Thread.Sleep(120);
                return;
            }
            ForceSetForeground(hwnd);
            Thread.Sleep(40);
        }

        throw new InvalidOperationException(
            "企微未能置于前台，无法可靠输入（后台窗口不支持）。请先打开企微主窗口，勿锁屏/最小化到托盘后重试。");
    }

    private static void ForceSetForeground(IntPtr hwnd)
    {
        var fg = GetForegroundWindow();
        var curTid = GetCurrentThreadId();
        var fgTid = GetWindowThreadProcessId(fg, out _);
        var targetTid = GetWindowThreadProcessId(hwnd, out _);

        try
        {
            // 与 wecom.rs 一致：AttachThreadInput(前台线程, 当前线程)
            if (fgTid != 0 && fgTid != curTid)
                AttachThreadInput(fgTid, curTid, true);
            if (targetTid != 0 && targetTid != curTid)
                AttachThreadInput(targetTid, curTid, true);

            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
        }
        finally
        {
            if (fgTid != 0 && fgTid != curTid)
                AttachThreadInput(fgTid, curTid, false);
            if (targetTid != 0 && targetTid != curTid)
                AttachThreadInput(targetTid, curTid, false);
        }
    }

    private static void EnsureStillForeground(IntPtr hwnd)
    {
        if (GetForegroundWindow() == hwnd) return;
        ForceSetForeground(hwnd);
        Thread.Sleep(50);
        if (GetForegroundWindow() != hwnd)
        {
            throw new InvalidOperationException(
                "企微失去前台焦点，写入中断。请保持企微在前台，勿切换到其他窗口。");
        }
    }

    /// <summary>
    /// 点击主窗口客户区偏下位置（水平约 62%，垂直约 92%），对齐 wecom.rs。
    /// </summary>
    private static void ClickMessageInputArea(IntPtr hwnd)
    {
        if (!GetClientRect(hwnd, out var rect))
            throw new InvalidOperationException("GetClientRect 失败");

        var width = rect.Right - rect.Left;
        var height = rect.Bottom - rect.Top;
        if (width <= 0 || height <= 0)
            throw new InvalidOperationException("企微窗口客户区无效");

        var pt = new POINT
        {
            X = rect.Left + width * 62 / 100,
            Y = rect.Top + height * 92 / 100,
        };
        if (!ClientToScreen(hwnd, ref pt))
            throw new InvalidOperationException("ClientToScreen 失败");

        SetCursorPos(pt.X, pt.Y);
        Thread.Sleep(40);
        Mouse.Click(new Point(pt.X, pt.Y));
        Thread.Sleep(60);
    }

    private static void TryClickElement(AutomationElement element)
    {
        try
        {
            var r = element.BoundingRectangle;
            if (r.IsEmpty || r.Width < 2 || r.Height < 2) return;
            // 排除顶部搜索框，避免又点回去
            var name = element.Name ?? "";
            if (name.Contains("搜索", StringComparison.OrdinalIgnoreCase))
                return;
            Mouse.Click(new Point((int)(r.Left + r.Width / 2), (int)(r.Top + r.Height / 2)));
            Thread.Sleep(50);
        }
        catch
        {
            // ignore
        }
    }

    private static void PasteText(string text)
    {
        SetClipboardText(text);
        Keyboard.TypeSimultaneously(VirtualKeyShort.CONTROL, VirtualKeyShort.KEY_A);
        Thread.Sleep(30);
        Keyboard.TypeSimultaneously(VirtualKeyShort.CONTROL, VirtualKeyShort.KEY_V);
        Thread.Sleep(40);
        ForceReleaseModifiers();
    }

    private static void SetClipboardText(string text)
    {
        Exception? last = null;
        for (var i = 0; i < 3; i++)
        {
            try
            {
                if (!OpenClipboard(IntPtr.Zero))
                    throw new InvalidOperationException($"OpenClipboard 失败（err={Marshal.GetLastWin32Error()}）");
                try
                {
                    EmptyClipboard();
                    var bytes = Encoding.Unicode.GetBytes(text + "\0");
                    var hGlobal = Marshal.AllocHGlobal(bytes.Length);
                    try
                    {
                        Marshal.Copy(bytes, 0, hGlobal, bytes.Length);
                        if (SetClipboardData(13 /* CF_UNICODETEXT */, hGlobal) == IntPtr.Zero)
                        {
                            Marshal.FreeHGlobal(hGlobal);
                            throw new InvalidOperationException($"SetClipboardData 失败（err={Marshal.GetLastWin32Error()}）");
                        }
                        hGlobal = IntPtr.Zero;
                    }
                    finally
                    {
                        if (hGlobal != IntPtr.Zero)
                            Marshal.FreeHGlobal(hGlobal);
                    }
                }
                finally
                {
                    CloseClipboard();
                }
                return;
            }
            catch (Exception ex)
            {
                last = ex;
                Thread.Sleep(50);
            }
        }
        throw new InvalidOperationException(
            $"写入剪贴板失败：{last?.Message}。请关闭占用剪贴板的工具后重试。");
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool OpenClipboard(IntPtr hWndNewOwner);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool CloseClipboard();

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool EmptyClipboard();

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr SetClipboardData(uint uFormat, IntPtr hMem);

    private static AgentStep Ok(string title, string note) => new()
    {
        Title = title,
        Status = "ok",
        Time = DateTime.Now.ToString("HH:mm:ss"),
        Note = note,
    };

    private static AgentStep Fail(string title, string note) => new()
    {
        Title = title,
        Status = "fail",
        Time = DateTime.Now.ToString("HH:mm:ss"),
        Note = note,
    };

    private static AgentResponse OkResp(string id, string note, List<AgentStep> steps) => new()
    {
        Id = id,
        Ok = true,
        Note = note,
        Steps = steps,
    };

    private static AgentResponse FailResp(string id, string note, List<AgentStep> steps) => new()
    {
        Id = id,
        Ok = false,
        Note = note,
        Steps = steps,
    };
}
