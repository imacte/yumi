[Read this document in English](README.en.md)

# yumi - 智能 CPU 调度控制器

<div align="center">

[![Android](https://img.shields.io/badge/platform-Android-3DDC84.svg?style=for-the-badge&logo=android)](https://developer.android.com/)
[![Rust](https://img.shields.io/badge/core-Rust-%23dea584.svg?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![eBPF](https://img.shields.io/badge/probe-eBPF-FF6B6B.svg?style=for-the-badge)](https://ebpf.io/)
[![WebUI](https://img.shields.io/badge/UI-WebUI-4FC08D.svg?style=for-the-badge&logo=html5)](https://developer.mozilla.org/en-US/docs/Web/HTML)
[![AArch64](https://img.shields.io/badge/arch-AArch64-FF6B6B.svg?style=for-the-badge)](https://en.wikipedia.org/wiki/AArch64)
[![Root Required](https://img.shields.io/badge/Root-Required-FF5722.svg?style=for-the-badge)](https://magiskmanager.com/)

**🚀 智能 CPU 调度系统 - eBPF 内核级监控 + 高性能 Rust 守护进程 + PID 控制 FAS 帧感知调度 + CPU 负载调速器**

</div>

-----

## 📋 项目介绍

**yumi** 是一个功能强大的 Android CPU 调度控制系统，由轻量级的 **WebUI** 管理界面和高性能的 **Rust 守护进程 (yumi)** 组成。系统通过 **eBPF** 内核探针实时采集 CPU 调度事件和渲染帧数据，结合先进的 **PID 控制器** 和 **CPU 负载调速器 (CLG)**，根据不同使用场景动态调整 CPU 频率，实现最佳的性能与能效平衡。内置的 **FAS (Frame Aware Scheduling)** 帧感知调度引擎可实时分析游戏帧时间，以逐帧精度动态调频，在保证流畅度的同时最大化省电。

### ✨ 主要特性

  * 🔄 **智能动态模式切换** - 根据当前前台应用自动调整性能模式。
  * 🎯 **FAS 帧感知调度** - 内置 PID 控制器 + 帧时间分析引擎，逐帧动态调频，游戏场景下兼顾流畅与功耗。
  * 📊 **eBPF 内核级监控** - 通过 `sched_switch` tracepoint 和 `queueBuffer` uprobe 实现零开销的 CPU 利用率和帧率采集。
  * ⚡ **CPU 负载调速器 (CLG)** - 基于 eBPF 实时负载数据的自适应调频，替代传统内核调速器，适用于日常非游戏场景。
  * 🌡️ **温度感知调控** - 实时读取 CPU 温度传感器，超过阈值时自动降频保护。
  * 🌙 **智能息屏节电** - 息屏时自动进入 Doze 模式，强制限制性能上限，极致省电。
  * 🌐 **轻量 WebUI** - 无需安装额外 App，通过浏览器即可管理调度配置。
  * 📱 **应用规则管理** - 为不同应用设置专属的性能策略，支持 per-app 帧率档位和余量配置。
  * 🔧 **高度可配置** - YAML 配置文件支持深度自定义，配置热重载无需重启。
  * 🌍 **多语言支持** - 基于 Fluent 的 i18n 国际化系统，支持中英文日志。

## 🔧 系统要求

  * **Android 版本**: Android 8.0 (API 26) 及以上。
  * **架构支持**: ARM64 (AArch64)。
  * **权限要求**: Root 权限。
  * **内核要求**: 支持 eBPF（需要 `CONFIG_BPF`、`CONFIG_BPF_SYSCALL` 等内核选项）。

## 🏗️ 系统架构

yumi 采用 **Monitor + Scheduler** 双线程架构，通过 `mpsc` 事件通道解耦数据采集与调度决策：

```
┌─────────────────────────────────────────────────────┐
│                   Monitor 线程组                      │
│                                                      │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────┐ │
│  │ app_detect   │  │ fps_monitor  │  │ cpu_monitor │ │
│  │ (Cgroup检测)  │  │ (eBPF uprobe)│  │(eBPF trace)│ │
│  └──────┬───────┘  └──────┬───────┘  └─────┬──────┘ │
│         │                 │                 │        │
│         │     ┌───────────┴─────────────────┘        │
│         │     │   DaemonEvent (mpsc channel)         │
└─────────┼─────┼──────────────────────────────────────┘
          │     │
          ▼     ▼
┌─────────────────────────────────────────────────────┐
│               Scheduler IPC 线程                      │
│                                                      │
│  ┌──────────────┐  ┌──────────────┐                  │
│  │ FAS 控制器    │  │ CLG 负载调速 │                  │
│  │ (游戏场景)    │  │ (日常场景)    │                  │
│  └──────────────┘  └──────────────┘                  │
│               ▼                                      │
│     sysfs 频率写入 (FastWriter)                       │
└─────────────────────────────────────────────────────┘
```

### 事件总线

系统使用五种核心事件驱动调度决策：

| 事件 | 来源 | 描述 |
| :--- | :--- | :--- |
| `ModeChange` | app_detect | 前台应用切换引起的模式变更，携带包名、PID、模式、温度。 |
| `FrameUpdate` | fps_monitor (eBPF) | 每一帧渲染完成时触发，携带帧间隔（纳秒级精度）。 |
| `SystemLoadUpdate` | cpu_monitor (eBPF) | 每 200ms 触发，携带各核心利用率和前台应用最重线程利用率。 |
| `ScreenStateChange` | screen_detect | 屏幕亮灭事件。 |
| `ConfigReload` | config_watcher | 配置文件变更事件。 |

## 🎯 性能模式

yumi 提供五种性能模式：

| 模式 | 图标 | 特点 | 适用场景 |
| :--- | :--- | :--- | :--- |
| **省电 (Powersave)** | 🔋 | 最大化续航，CLG 低性能上限。 | 待机、轻度使用、阅读。 |
| **均衡 (Balance)** | ⚖️ | CLG 自适应调频，性能与功耗的最佳平衡点。 | 日常使用、社交应用。 |
| **性能 (Performance)** | ⚡ | CLG 高响应配置，优先性能。 | 大型应用、轻度游戏。 |
| **极速 (Fast)** | 🚀 | CLG 最大性能释放。 | 重度任务、性能测试。 |
| **FAS (帧感知调度)** | 🎯 | PID 控制器实时分析帧时间，逐帧动态调频，自动档位切换。 | 游戏场景，兼顾流畅与省电。 |

### CLG (CPU Load Governor) 说明

在非 FAS 的四种模式下，yumi 使用 **CPU 负载调速器 (CLG)** 替代内核原生调速器。CLG 基于 eBPF 采集的真实 CPU 利用率数据，通过 EMA 平滑、升降频速率限制和 headroom 因子进行自适应调频。每种模式可独立配置 CLG 参数（升降频阈值、平滑系数、性能上下限等）。

息屏时，系统自动进入 **Doze 模式**，CLG 被重新配置为极致省电参数（性能上限锁死 40%、升频极迟钝、瞬间降频），大幅降低待机功耗。

## 🌐 WebUI 管理界面

yumi 内置轻量级 WebUI，通过浏览器即可完成所有管理操作，无需安装额外 App。

  * **模式切换** - 实时切换性能模式。
  * **应用规则管理** - 为不同应用配置专属性能策略。
  * **配置编辑** - 在线编辑 YAML 配置文件。
  * **日志查看** - 实时查看 yumi 守护进程日志。

-----

### 🛠️ 调度核心 (yumi)

yumi 的核心是由一个 Rust 守护进程 **yumi** 驱动的。它使用 eBPF 内核探针进行零开销数据采集，以极低的资源占用实现高效的性能控制。

#### 核心特性

  * **高性能 Rust 实现**: 极低的系统资源占用，运行功耗极低。
  * **eBPF 内核级监控**: 通过 `sched_switch` tracepoint 精确采集每核心 CPU 利用率和线程运行时间；通过 `queueBuffer` uprobe 零开销捕获渲染帧间隔。
  * **实时配置监听**: 支持配置文件（`config.yaml`）和规则文件（`rules.yaml`）热重载，切换模式无需重启。
  * **内置 FAS 引擎**: PID 控制器驱动的帧感知调度，支持自动容量权重探测、per-app 配置、CPU 利用率辅助调频。
  * **CLG 负载调速器**: 基于 eBPF 实时负载的自适应调频，替代内核原生调速器。
  * **多语言国际化**: 基于 Fluent 的 i18n 系统，支持中英文日志输出。

#### 功能模块

| 功能模块 | 描述 |
| :--- | :--- |
| **eBPF CPU 监控** | 通过 `sched_switch` tracepoint 采集每核心 idle/busy 时间和线程运行时间，带实时状态补偿（补偿当前正在执行但尚未触发上下文切换的任务）。 |
| **eBPF FPS 监控** | 通过 `queueBuffer` uprobe 捕获渲染帧提交事件，内核侧 PID 过滤，perf event 零拷贝传输到用户态。 |
| **FAS 帧感知调度** | PID 控制器驱动，实时分析帧间隔，通过 perf_index (0.0~1.0) 映射到 CPU 频率，逐帧动态调频。 |
| **CLG 负载调速器** | 基于 eBPF 实时负载数据的自适应调频，支持按 cluster 独立控制，带升降频速率限制。 |
| **CPU 频率控制** | FastWriter 高性能 sysfs 写入器，带去重、unmount 和频率校验，锁频写入各核心簇。 |
| **自动容量权重** | 运行时探测各核心簇的 `cpu_capacity`，自动计算 capacity_weight，无需手动配置核心架构。 |
| **智能息屏节电** | 息屏自动进入 Doze 模式，CLG 强制限制为极致省电配置；亮屏自动恢复。 |
| **温度感知调控** | 实时监控 CPU 温度，超过阈值时限制 FAS 性能上限。 |
| **I/O 调度优化** | 遍历所有块设备，可自定义 I/O 调度器、预读大小、合并策略及 iostats 等参数。 |
| **屏幕状态检测** | 通过 Netlink uevent 监听 power/backlight 事件，零轮询检测屏幕亮灭。 |
| **前台应用检测** | 读取 `top-app` cgroup 进程列表，无阻塞防抖，自动过滤输入法和系统进程。 |

-----

### 🎯 FAS 帧感知调度详解

FAS (Frame Aware Scheduling) 是 yumi 内置的帧感知动态调频引擎，专为游戏场景设计。与传统的静态模式不同，FAS 通过 PID 控制器实时分析每一帧的渲染时间来精确控制 CPU 频率，在保证流畅度的同时尽可能降低功耗。

#### 工作原理

FAS 引擎维护一个 **perf_index**（性能指数，范围 0.0~1.0），并通过 PID 控制器根据帧时间的实时反馈来调整它：

  * **帧时间超出预算** → PID 输出为负 → perf_index 上升 → CPU 频率提高
  * **帧时间满足预算** → PID 输出为正 → perf_index 缓慢下降 → CPU 频率降低
  * **perf_index 通过 capacity_weight 映射到各核心簇的实际频率档位**

PID 控制器使用三个可配置的系数（kp、ki、kd）分别控制比例、积分和微分响应，其中积分项带有泄漏机制防止饱和，微分项使用低通滤波抑制噪声。

#### 核心机制

  * **自动帧率档位切换**: 支持多档位帧率（如 30/60/90/120/144 fps），根据实际渲染能力自动升降档。降档前会先尝试 boost 提频确认是否真的需要降档，避免误降。支持低 perf 稳帧升档和极端帧率原生档位检测。
  * **CPU 负载辅助调频**: 利用 eBPF 采集的前台最重线程利用率（EMA 平滑），对 perf_index 进行 util_cap 软封顶，防止频率虚高空转。
  * **自动容量权重探测**: 运行时读取各核心簇的 `cpu_capacity`，自动计算 capacity_weight（大核权重更高），无需手动配置核心架构。
  * **per-app 配置**: 支持为每个游戏单独配置帧率档位列表和帧率余量，运行时动态匹配最近的档位。
  * **加载场景检测**: 自动识别游戏加载画面（持续重帧），进入加载状态后锁定中高频率，加载结束后带保护地恢复正常调度。
  * **频率迟滞防抖**: 相邻频率档位间设置迟滞带，防止在边界处频繁跳档。
  * **Jank 冷却机制**: 发生严重掉帧后进入冷却期，期间维持较高频率，避免掉帧后立即降频引发连锁卡顿。Jank 响应强度随连续掉帧次数指数递增。
  * **频率校验与恢复**: 定期读回实际频率，检测是否被外部因素（如温控）覆盖，自动 unmount 并重新写入。
  * **小窗模式支持**: FAS 状态支持挂起/恢复（5 秒宽限期），短暂切离后可快速恢复调度，无需重新初始化。
  * **perf_floor 死锁救援**: 检测 perf_index 长时间贴地板但帧率严重不足的情况，自动重置到冷启动性能值。

#### FAS 配置 (`rules.yaml`)

FAS 的参数通过 `rules.yaml` 中的 `fas_rules` 节进行配置：

```yaml
fas_rules:
  fps_gears: [30.0, 60.0, 90.0, 120.0, 144.0]
  fps_margin: "3.0"

  pid:
    kp: 0.050
    ki: 0.010
    kd: 0.006

  auto_capacity_weight: true

  perf_floor: 0.22
  perf_ceil: 1.0
  perf_init: 0.45
  perf_cold_boot: 0.85
  freq_hysteresis: 0.015

  heavy_frame_threshold_ms: 150.0
  loading_cumulative_ms: 2500.0
  post_loading_ignore_frames: 5
  post_loading_perf: 0.65

  core_temp_threshold: 0.0
  core_temp_throttle_perf: 0.70
  util_cap_divisor: 0.45

  per_app_profiles:
    "com.miHoYo.GenshinImpact":
      target_fps: [30, 60]
      fps_margin: 4.0
    "com.tencent.tmgp.sgame":
      target_fps: [60, 90, 120]
      fps_margin: 3.0
```

**关键参数说明：**

| 参数 | 类型 | 默认值 | 描述 |
| :--- | :--- | :--- | :--- |
| `fps_gears` | float[] | [30,60,90,120,144] | 支持的帧率档位列表，FAS 会在这些档位间自动切换。 |
| `fps_margin` | string | "3.0" | 帧率余量（fps），EMA 预算 = 1000/(target - margin)，提供容错空间。 |
| `pid.kp / ki / kd` | float | 0.050/0.010/0.006 | PID 控制器的比例、积分、微分系数。 |
| `auto_capacity_weight` | bool | true | 是否自动探测各核心簇容量权重。关闭时使用 `cluster_profiles` 手动配置。 |
| `perf_floor` | float | 0.22 | perf_index 下限（高刷游戏会动态抬高）。 |
| `perf_ceil` | float | 1.0 | perf_index 上限。 |
| `perf_init` | float | 0.45 | 正常启动时的初始 perf_index。 |
| `perf_cold_boot` | float | 0.85 | 冷启动期间的 perf_index（前 3.5 秒）。 |
| `freq_hysteresis` | float | 0.015 | 频率迟滞系数，防止相邻档位间频繁跳变。 |
| `heavy_frame_threshold_ms` | float | 150.0 | 重帧阈值（毫秒），超过此值的帧被视为加载帧。 |
| `loading_cumulative_ms` | float | 2500.0 | 累计重帧时长超过此值后进入加载状态。 |
| `post_loading_perf` | float | 0.65 | 加载结束后的 perf_index。 |
| `core_temp_threshold` | float | 0.0 | 温度降频阈值（℃），0 = 禁用。 |
| `core_temp_throttle_perf` | float | 0.70 | 温度降频时的 perf 上限。 |
| `util_cap_divisor` | float | 0.45 | CPU 负载辅助调频的除数（越小越激进地限频）。 |
| `per_app_profiles` | map | {} | 每个游戏的独立配置，支持 `target_fps` 和 `fps_margin`。 |

-----

### ⚙️ 高级配置 (`config.yaml` 详解)

yumi 使用 YAML 格式的配置文件，允许用户进行深度自定义。

#### 1️⃣ 元信息 (`meta`)

这部分定义了守护进程的基本行为。

```yaml
meta:
  loglevel: "INFO"
  language: "en"
```

| 字段 | 类型 | 描述 |
| :--- | :--- | :--- |
| `loglevel` | string | 日志记录详细程度。可选值：`DEBUG`, `INFO`, `WARN`, `ERROR`。支持运行时热更新。 |
| `language` | string | 守护进程日志的语言。目前支持 `en` (英语) 和 `zh` (中文)。支持运行时热切换。 |

#### 2️⃣ 功能开关 (`function`)

```yaml
function:
  CpuIdleScalingGovernor: false
  IOOptimization: true
```

| 功能 | 描述 |
| :--- | :--- |
| `CpuIdleScalingGovernor`| 是否允许自定义 CPU Idle 调速器（见 `CpuIdle` 部分）。 |
| `IOOptimization` | 启用 I/O 优化，遍历所有块设备应用调度器和参数设置（见 `IO_Settings` 部分）。 |

#### 3️⃣ I/O 设置 (`IO_Settings`)

需要 `function.IOOptimization` 为 `true`。启用后会遍历 `/sys/block/*` 下的所有块设备，逐一应用以下参数。

```yaml
IO_Settings:
  Scheduler: "none"
  read_ahead_kb: "128"
  nomerges: "2"
  iostats: "0"
```

| 字段 | 类型 | 描述 |
| :--- | :--- | :--- |
| `Scheduler` | string | I/O 调度器，如 `"none"`, `"mq-deadline"`, `"bfq"`, `"kyber"`。留空则不修改。 |
| `read_ahead_kb` | string | 预读大小（KB）。 |
| `nomerges` | string | 合并策略。`0`=允许合并，`1`=仅简单合并，`2`=禁止合并。 |
| `iostats` | string | I/O 统计信息。`0`=禁用（推荐，减少开销），`1`=启用。 |

#### 4️⃣ CPU Idle (`CpuIdle`)

需要 `function.CpuIdleScalingGovernor` 为 `true`。

```yaml
CpuIdle:
  current_governor: "ladder"
```

  * `current_governor`: 设置 CPU Idle 调速器。

#### 5️⃣ 性能模式配置

每种性能模式可独立配置 CPU 负载调速器 (CLG) 的参数：

```yaml
balance:
  CpuLoadGovernor:
    up_threshold: 0.80
    down_threshold: 0.50
    smoothing_up: 0.60
    smoothing_down: 0.30
    down_rate_limit_ticks: 3
    headroom_factor: 1.25
    perf_floor: 0.15
    perf_ceil: 1.0
    perf_init: 0.50
```

| 参数 | 类型 | 默认值 | 描述 |
| :--- | :--- | :--- | :--- |
| `up_threshold` | float | 0.80 | 负载超过此阈值时快速升频。 |
| `down_threshold` | float | 0.50 | 负载低于此阈值时允许降频。 |
| `smoothing_up` | float | 0.60 | 升频平滑系数（越大越快）。 |
| `smoothing_down` | float | 0.30 | 降频平滑系数（越大越快）。 |
| `down_rate_limit_ticks` | int | 3 | 降频速率限制（tick 数，每 tick 200ms）。 |
| `headroom_factor` | float | 1.25 | 目标性能 = 实际负载 × headroom，提供频率余量。 |
| `perf_floor` | float | 0.15 | 性能下限。 |
| `perf_ceil` | float | 1.0 | 性能上限。 |
| `perf_init` | float | 0.50 | 初始性能值。 |

-----

### 📊 eBPF 探针详解

yumi 使用两个 eBPF 探针进行内核级数据采集：

#### CPU 探针 (`cpu_probe.c`)

挂载到 `tracepoint/sched/sched_switch`，在每次上下文切换时触发，记录：

  * **每核心 idle/busy 时间**: 通过 `PERCPU_ARRAY` 累计，用户态读取后计算真实利用率。
  * **每核心当前 TID**: 用于用户态实时补偿尚未触发 sched_switch 的任务时间。
  * **线程运行时间**: 通过 `HASH` map 记录每个线程的累计 CPU 时间，用于计算前台应用最重线程利用率。

#### FPS 探针 (`fps_probe.c`)

挂载到 `libgui.so` 的 `queueBuffer` 函数（uprobe），在每帧渲染提交时触发：

  * **内核侧 PID 过滤**: 通过 `target_pid` map 只对目标进程发送 perf event，减少无关进程的开销。
  * **帧间隔计算**: 记录每次 `queueBuffer` 的时间戳差值，通过 `PERF_EVENT_ARRAY` 零拷贝传输到用户态。
  * **基线预热**: 即使非目标进程也会记录时间戳，确保 PID 切换后第一帧就能算出正确的 delta。

-----

## 📥 安装说明

### 前置要求

1.  **获取 Root 权限**
2.  **确保内核支持 eBPF**（大部分 Android 10+ 设备均已支持）

### 安装步骤

1.  **下载模块** - 从 [Releases](https://github.com/imacte/yumi/releases) 下载最新版本。
2.  **刷入模块** - 通过 Magisk / KernelSU 刷入 yumi 模块。
3.  **访问 WebUI** - 模块启动后，通过浏览器访问 WebUI 进行管理和配置。
4.  **配置规则** - 根据需要为不同应用设置性能策略。

## 🚀 性能优化建议

### 日常使用

1.  **使用均衡模式** - CLG 自适应调频，为大部分应用提供最佳的性能功耗平衡。
2.  **设置应用规则** - 为游戏应用设置 FAS 模式。

### 游戏优化

1.  **使用 FAS 模式** - 帧感知调度可在保证流畅度的同时自动省电，推荐作为游戏的首选模式。
2.  **配置 per-app 参数** - 在 `rules.yaml` 中为特定游戏设置 `target_fps` 和 `fps_margin`。
3.  **调整 PID 系数** - 如果觉得调频响应过快或过慢，可微调 `kp`/`ki`/`kd`。
4.  **监控温度** - 设置 `core_temp_threshold` 启用温度保护，长时间游戏时防止过热。

### 省电优化

1.  **使用省电模式** - CLG 低性能上限，在低负载场景下最大化续航。
2.  **息屏自动省电** - 无需手动操作，息屏时自动进入 Doze 模式。
3.  **优化 I/O 调度** - 减少存储访问的功耗开销。

## 🔍 故障排除

### 常见问题

**Q: 模块无法获取 Root 权限？**

  * 确保设备已正确 Root 并安装 Magisk / KernelSU。
  * 检查 Root 管理器中是否允许了 yumi 的 Root 请求。
  * 尝试重新刷入模块或重启设备。

**Q: eBPF 探针加载失败？**

  * 确保内核支持 eBPF（`CONFIG_BPF=y`、`CONFIG_BPF_SYSCALL=y`）。
  * 查看日志中的 eBPF 错误信息。
  * 部分旧内核可能不支持所需的 BPF 功能。

**Q: 智能动态模式不工作？**

  * 验证应用规则是否正确配置。
  * 验证 yumi 模块是否安装并正常运行。
  * 检查 `ignored_apps` 列表是否误将目标应用排除。

**Q: 性能模式切换无效？**

  * 验证 yumi 模块是否安装并正常运行。
  * 查看 yumi 模块日志以确定具体错误信息。
  * 验证配置文件格式是否正确（YAML 语法敏感）。

**Q: FAS 模式下帧率不稳定？**

  * 检查 `rules.yaml` 中的 `fps_gears` 是否包含目标帧率。
  * 适当增大 `fps_margin` 可提供更多余量，减少边界波动。
  * 查看日志中的 FAS 心跳信息（每 30 帧输出一次），确认调度状态是否正常。
  * 若频率被温控覆盖，日志中会出现 "freq mismatch" 提示，属正常校验恢复行为。
  * 尝试为该游戏配置 `per_app_profiles` 中的专属参数。

**Q: CPU 利用率数据不准确？**

  * eBPF CPU 探针需要内核支持 `tracepoint/sched/sched_switch`。
  * 用户态读取周期为 200ms，包含实时状态补偿以提高精度。
  * 检查日志中的 cpu_monitor 初始化信息，确认在线核心列表是否正确。

## 📊 项目统计

<div align="center">

[![Star History Chart](https://api.star-history.com/svg?repos=imacte/yumi&type=Date)](https://star-history.com/#imacte/yumi&Date)

</div>

## 📮 联系我们

  * **GitHub Issues** - [项目问题和建议](https://github.com/imacte/yumi/issues)
  * **QQ 群** - 1036909137
  * **Telegram** - [加入 TG 频道](https://t.me/+gp4adLJAsXYzMjc1)

-----

<div align="center">

<sub>📅 文档更新时间：2026年3月11日</sub><br>
<sub>🚀 yumi - 让每一台 Android 设备都拥有最佳的性能体验</sub>

</div>