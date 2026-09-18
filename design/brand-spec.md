# Auto Task — Brand Spec

基于 tech-utility，按产品要求将强调色改为蓝色。

## Tokens

```css
:root {
  --bg: oklch(98% 0.005 250);
  --surface: oklch(100% 0 0);
  --fg: oklch(22% 0.02 240);
  --muted: oklch(50% 0.018 240);
  --border: oklch(90% 0.008 240);
  --accent: oklch(55% 0.17 250);
}
```

## Fonts

- Display / Body: `-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif`
- Mono: `'JetBrains Mono', 'IBM Plex Mono', ui-monospace, Menlo, monospace`

## Rules

1. 桌面效率工具：信息密度适中，表格/列表优先，无营销大图。
2. 状态用轻量 tinted pill（成功/失败/禁用），不用彩色竖条 callout。
3. 单页主操作仅一个实心主按钮；次要操作为 ghost / text。
4. Hover 调整背景 L 通道，不把文字改成 muted。
5. 路径、cron、脚本名、版本号用等宽字体。
6. 应用内更新嵌在设置「关于」区：轻量 tint 提示 + 单一主按钮，不另起营销大图或独立大模态。
7. 设置页用分组 Tab（通用 / 调度 / 企微 / 数据 / 关于）降低滚动；选中态为蓝色 underline 或 tinted pill，不用大色块。

一句话：冷静的浅色 Windows 工具壳 + 蓝色操作强调，像本地调度器而非 SaaS 落地页。
