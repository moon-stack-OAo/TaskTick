using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using FlaUI.Core.AutomationElements;
using FlaUI.Core.Definitions;
using FlaUI.Core.Input;
using FlaUI.Core.Patterns;
using FlaUI.Core.WindowsAPI;
using FlaUI.UIA3;

namespace WecomAgent;

/// <summary>
/// 企微 FlaUI 最小实现：定位主窗口 + 搜索/输入。
/// 控件树随版本变化，需用 Accessibility Insights 校准。
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
            win.Focus();
            steps.Add(Ok("窗口前置", $"Name={win.Name}"));
            return OkResp(req.Id, "FlaUI 预检：已定位企微主窗口", steps);
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
                    win.SetForeground();
                    win.Focus();
                    Thread.Sleep(180);
                    steps.Add(Ok("窗口前置", $"Name={win.Name}"));

                    Keyboard.TypeSimultaneously(VirtualKeyShort.CONTROL, VirtualKeyShort.KEY_F);
                    Thread.Sleep(300);
                    steps.Add(Ok("打开搜索", "Ctrl+F"));

                    var search = FindEditByHint(win, "搜索") ?? FirstEdit(win);
                    if (search == null)
                        throw new InvalidOperationException("未找到搜索输入框。请用 Accessibility Insights 校准。");

                    search.Focus();
                    PasteText(contact);
                    Thread.Sleep(350);
                    steps.Add(Ok("输入联系人", contact));

                    Keyboard.Type(VirtualKeyShort.ENTER);
                    Thread.Sleep(450);
                    steps.Add(Ok("打开私聊", "Enter 选中首个结果"));

                    var input = FindEditByHint(win, "输入")
                        ?? FindDocument(win)
                        ?? BottomMostEdit(win);
                    if (input == null)
                        throw new InvalidOperationException("未找到消息输入框。请校准控件树。");

                    input.Focus();
                    Thread.Sleep(80);
                    PasteText(message);
                    Thread.Sleep(120);
                    steps.Add(Ok("输入消息", $"{message.Length} 字"));

                    Keyboard.Type(VirtualKeyShort.ENTER);
                    Thread.Sleep(200);
                    steps.Add(Ok("发送", "Enter"));

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
                    // 单次失败也抬起修饰键，避免粘键影响后续重试
                    ForceReleaseModifiers();
                    Thread.Sleep(350);
                }
            }

            return FailResp(req.Id, last?.Message ?? "发送失败", steps);
        }
        finally
        {
            // Send 流程（含 catch/重试）结束时强制抬起 Ctrl/Shift/Alt
            ForceReleaseModifiers();
        }
    }

    /// <summary>
    /// 强制抬起修饰键，避免锁屏/中断导致 Ctrl 等 KEYUP 丢失后「粘键」。
    /// </summary>
    private static void ForceReleaseModifiers()
    {
        // 优先 Win32 KEYUP（不依赖 FlaUI 枚举是否齐全）
        try
        {
            const byte KEYEVENTF_KEYUP = 0x02;
            foreach (byte vk in new byte[]
                     {
                         0x11, // VK_CONTROL
                         0xA2, // VK_LCONTROL
                         0xA3, // VK_RCONTROL
                         0x10, // VK_SHIFT
                         0xA0, // VK_LSHIFT
                         0xA1, // VK_RSHIFT
                         0x12, // VK_MENU (Alt)
                         0xA4, // VK_LMENU
                         0xA5, // VK_RMENU
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
                // 忽略复位失败
            }
        }
    }

    /// <summary>
    /// OpenInputDesktop 失败则视为会话已锁屏。
    /// </summary>
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

    private static AutomationElement? FindDocument(Window win)
    {
        return win.FindFirstDescendant(cf => cf.ByControlType(ControlType.Document));
    }

    private static AutomationElement? FirstEdit(Window win)
    {
        return win.FindFirstDescendant(cf => cf.ByControlType(ControlType.Edit));
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

    private static void PasteText(string text)
    {
        SetClipboardText(text);
        Keyboard.TypeSimultaneously(VirtualKeyShort.CONTROL, VirtualKeyShort.KEY_A);
        Thread.Sleep(20);
        Keyboard.TypeSimultaneously(VirtualKeyShort.CONTROL, VirtualKeyShort.KEY_V);
    }

    private static void SetClipboardText(string text)
    {
        if (!OpenClipboard(IntPtr.Zero))
            throw new InvalidOperationException("OpenClipboard 失败");
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
                    throw new InvalidOperationException("SetClipboardData 失败");
                }
                // 系统接管内存，勿 Free
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
