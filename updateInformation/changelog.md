# 🚀 更新日志 | Changelog

## 🎯 新增 (New Features)
* **[FPS 监控]** eBPF FPS 探针重构为单实例多 PID attach 架构，PID 切换零延迟、零丢帧
* **[WebUI]** 获取应用列表增加 `pm list packages` 备用方案，KernelSU bridge 不可用时自动降级
* **[构建]** 新增 `cargo xtask release` 命令，一键同步更新 Cargo.toml / module.prop / update.json 版本号

## ⚡️ 优化与特性 (Optimizations & Features)
* **[CLG]** CPU 负载调速器升频阻尼优化：
  - 新增 `up_rate_limit_ticks` 升频速率限制（连续 N tick 高负载才升频，默认 2）
  - 跳变阈值从 0.20 提高到 0.35，减少瞬时毛刺响应
  - 小幅 creep 系数 0.05 → 0.02，低负载波动几乎不升频
  - headroom 仅在 util ≥ up_threshold 时生效，低负载不放大
* **[内核]** 解除了内核 eBPF Map 的内存锁定限制

## 🐛 修复 (Bug Fixes)
* **[FPS 监控]** 修复 uprobe attach 参数顺序错误（aya 0.14 API），正确 hook 到 `Surface::queueBuffer`
* **[构建]** 修复 `vue-tsc` 与 TypeScript 7.0 不兼容问题——暂跳过 type-check，等上游适配

## 🧹 清理 (Code Cleanup)
* 删除死代码 85 行：`apply_freq_relaxed`、`set_ignore_policy`、`write_value`、`apply_all_settings` 等
* 删除 `FastWriter` 无效的 `last_value` / `invalidate` 去重逻辑
* 删除 `DaemonEvent::FrameUpdate` 未使用的 `fps` 字段
* 修复 `scheduler.rs` 错误 import `super::utils::SysPathExist`
* 删除 `CpuScheduler` 未使用的 `current_mode_name` 字段

## 📝 文档 (Documentation)
* README 全面同步：补全 FAS 全部 38 项参数、4 模式 CLG 配置示例、修正 eBPF 探针描述
* 修正 CLG 配置 key 名 `CpuLoadGovernor` → `cpu_load_governor`

## 📦 依赖 (Dependencies)
* TypeScript 5.9.3 → 7.0.2
* vue-tsc 3.2.4 → 3.3.7
