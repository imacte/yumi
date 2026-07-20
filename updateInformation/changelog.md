# 🚀 更新日志 | Changelog

## 🎯 新增 (New Features)
* **[FPS 监控]** eBPF FPS 探针重构为单实例多 PID attach 架构，PID 切换零延迟、零丢帧

## ⚡️ 优化与特性 (Optimizations & Features)
* **[CLG]** CPU 负载调速器升频阻尼优化：
  - 新增 `up_rate_limit_ticks` 升频速率限制（连续 N tick 高负载才升频，默认 2）
  - 跳变阈值从 0.20 提高到 0.35，减少瞬时毛刺响应
  - 小幅 creep 系数 0.05 → 0.02，低负载波动几乎不升频
  - headroom 仅在 util ≥ up_threshold 时生效，低负载不放大
