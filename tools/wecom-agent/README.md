# wecom-agent（FlaUI）

企业微信 UI 自动化独立进程，供「时序 · TaskTick」通过 JSON Lines 调用。

## 依赖

- [.NET SDK](https://dotnet.microsoft.com/download)（本机常见为 .NET 10；目标框架 `net10.0-windows`）
- Windows + 已登录的企业微信

若 `dotnet` 命令找不到，使用：`& "C:\Program Files\dotnet\dotnet.exe"`，并把该目录加入 PATH 后重开终端。

## 构建

```powershell
& "C:\Program Files\dotnet\dotnet.exe" restore
& "C:\Program Files\dotnet\dotnet.exe" build -c Release
```

产物：`bin\Release\net10.0-windows\wecom-agent.exe`

## 手工试跑

```powershell
# ping
'{"id":"1","cmd":"ping"}' | & .\bin\Release\net10.0-windows\wecom-agent.exe

# 仅定位窗口
'{"id":"2","cmd":"probe","launchWecom":true,"timeoutSec":30}' | & .\bin\Release\net10.0-windows\wecom-agent.exe
```

## 协议

见仓库 `docs/wecom-agent.md`。
