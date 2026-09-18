# 设计稿同步

## 为什么要迁移 OD 稿？

| 位置                                                      | 作用                   |
|---------------------------------------------------------|----------------------|
| OpenDesign 项目 `auto-task-5f73`（显示名「时序 · TaskTick 桌面应用」） | 继续改原型、出预览            |
| 本仓库 `design/`                                           | 开发对照、版本可追溯、不依赖 OD 在线 |

**结论：要迁一份到项目里；OD 原项目建议保留。**

## 同步命令（PowerShell）

```powershell
Copy-Item `
  "D:\Moon\OD\data\namespaces\release-stable-win\data\projects\auto-task-5f73\auto-task-desktop-prototype.html" `
  "D:\Moon\tools\TaskTick\design\tasktick-desktop-prototype.html" `
  -Force
```

## 注意

- 只把确认过的原型覆盖进 `design/`
- 前端实现以 `src/` 为准，`design/` 仅作视觉与信息架构参考
