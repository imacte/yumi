[阅读中文文档](README.md)

# yumi - Intelligent CPU Scheduling Controller

<div align="center">

[![Android](https://img.shields.io/badge/platform-Android-3DDC84.svg?style=for-the-badge&logo=android)](https://developer.android.com/)
[![Rust](https://img.shields.io/badge/core-Rust-%23dea584.svg?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![eBPF](https://img.shields.io/badge/probe-eBPF-FF6B6B.svg?style=for-the-badge)](https://ebpf.io/)
[![WebUI](https://img.shields.io/badge/UI-WebUI-4FC08D.svg?style=for-the-badge&logo=html5)](https://developer.mozilla.org/en-US/docs/Web/HTML)
[![AArch64](https://img.shields.io/badge/arch-AArch64-FF6B6B.svg?style=for-the-badge)](https://en.wikipedia.org/wiki/AArch64)
[![Root Required](https://img.shields.io/badge/Root-Required-FF5722.svg?style=for-the-badge)](https://magiskmanager.com/)

**🚀 Intelligent CPU Scheduling System — eBPF Kernel-Level Monitoring + High-Performance Rust Daemon + PID-Controlled FAS Frame-Aware Scheduling + CPU Load Governor**

</div>

-----

## 📋 About The Project

**yumi** is a powerful Android CPU scheduling control system, consisting of a lightweight **WebUI** management interface and a high-performance **Rust daemon (yumi)**. The system uses **eBPF** kernel probes to collect CPU scheduling events and rendering frame data in real time. Combined with an advanced **PID controller** and **CPU Load Governor (CLG)**, it dynamically adjusts CPU frequency for different usage scenarios to achieve the optimal balance between performance and power efficiency. The built-in **FAS (Frame Aware Scheduling)** engine analyzes game frame times in real time to dynamically adjust CPU frequency on a per-frame basis, maximizing power savings while maintaining smooth gameplay.

### ✨ Key Features

  * 🔄 **Smart Dynamic Mode Switching** — Automatically adjusts performance modes based on the current foreground application.
  * 🎯 **FAS Frame-Aware Scheduling** — Built-in PID controller + frame time analysis engine for per-frame dynamic frequency scaling, balancing smoothness and power in gaming scenarios.
  * 📊 **eBPF Kernel-Level Monitoring** — Zero-overhead CPU utilization and frame rate collection via `sched_switch` tracepoint and `queueBuffer` uprobe.
  * ⚡ **CPU Load Governor (CLG)** — Adaptive frequency scaling based on real-time eBPF load data, replacing traditional kernel governors for everyday non-gaming scenarios.
  * 🌡️ **Temperature-Aware Throttling** — Reads CPU temperature sensors in real time, automatically throttling when thresholds are exceeded.
  * 🌙 **Smart Screen-Off Power Saving** — Automatically enters Doze mode when the screen is off, enforcing a low performance ceiling for maximum power savings.
  * 🌐 **Lightweight WebUI** — No extra app required; manage all scheduling settings directly from your browser.
  * 📱 **App Rule Management** — Set dedicated performance strategies for different applications, with per-app frame rate gear and margin support.
  * 🔧 **Highly Configurable** — YAML configuration files support deep customization with hot-reload — no restart needed.
  * 🌍 **Multi-Language Support** — Fluent-based i18n internationalization system, supporting Chinese and English log output.

## 🔧 System Requirements

  * **Android Version**: Android 8.0 (API 26) and above.
  * **Architecture Support**: ARM64 (AArch64).
  * **Permissions Required**: Root access.
  * **Kernel Requirements**: eBPF support (`CONFIG_BPF`, `CONFIG_BPF_SYSCALL`, etc.).

## 🏗️ System Architecture

yumi uses a **Monitor + Scheduler** dual-thread architecture, decoupling data collection from scheduling decisions via an `mpsc` event channel:

```
┌─────────────────────────────────────────────────────┐
│                  Monitor Thread Group                 │
│                                                      │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────┐ │
│  │ app_detect   │  │ fps_monitor  │  │ cpu_monitor │ │
│  │ (Cgroup)     │  │ (eBPF uprobe)│  │(eBPF trace)│ │
│  └──────┬───────┘  └──────┬───────┘  └─────┬──────┘ │
│         │                 │                 │        │
│         │     ┌───────────┴─────────────────┘        │
│         │     │   DaemonEvent (mpsc channel)         │
└─────────┼─────┼──────────────────────────────────────┘
          │     │
          ▼     ▼
┌─────────────────────────────────────────────────────┐
│              Scheduler IPC Thread                     │
│                                                      │
│  ┌──────────────┐  ┌──────────────┐                  │
│  │ FAS Controller│  │ CLG Governor │                  │
│  │ (Gaming)      │  │ (Daily use)  │                  │
│  └──────────────┘  └──────────────┘                  │
│               ▼                                      │
│     sysfs Frequency Write (FastWriter)               │
└─────────────────────────────────────────────────────┘
```

### Event Bus

The system uses five core events to drive scheduling decisions:

| Event | Source | Description |
| :--- | :--- | :--- |
| `ModeChange` | app_detect | Mode change triggered by foreground app switch; carries package name, PID, mode, and temperature. |
| `FrameUpdate` | fps_monitor (eBPF) | Triggered on each frame render completion; carries frame interval (nanosecond precision). |
| `SystemLoadUpdate` | cpu_monitor (eBPF) | Triggered every 200ms; carries per-core utilization and foreground app's heaviest thread utilization. |
| `ScreenStateChange` | screen_detect | Screen on/off events. |
| `ConfigReload` | config_watcher | Configuration file change events. |

## 🎯 Performance Modes

yumi offers five performance modes:

| Mode | Icon | Characteristics | Use Case |
| :--- | :--- | :--- | :--- |
| **Powersave** | 🔋 | Maximizes battery life; CLG with low performance ceiling. | Standby, light use, reading. |
| **Balance** | ⚖️ | CLG adaptive frequency scaling; optimal balance between performance and power. | Daily use, social apps. |
| **Performance** | ⚡ | CLG high-responsiveness configuration; prioritizes performance. | Large applications, light gaming. |
| **Fast** | 🚀 | CLG maximum performance output. | Heavy tasks, performance testing. |
| **FAS (Frame-Aware Scheduling)** | 🎯 | PID controller analyzes frame times in real time; per-frame dynamic frequency scaling with automatic gear switching. | Gaming — balances smoothness and power saving. |

### CLG (CPU Load Governor) Overview

In the four non-FAS modes, yumi uses the **CPU Load Governor (CLG)** to replace the kernel's native governor. CLG performs adaptive frequency scaling based on real CPU utilization data collected via eBPF, using EMA smoothing, up/down rate limiting, and a headroom factor. Each mode can independently configure CLG parameters (up/down thresholds, smoothing coefficients, performance floor/ceiling, etc.).

When the screen is off, the system automatically enters **Doze mode**, where CLG is reconfigured with extreme power-saving parameters (performance ceiling locked to 40%, extremely sluggish frequency ramp-up, instant ramp-down), dramatically reducing standby power consumption.

## 🌐 WebUI Management Interface

yumi includes a lightweight built-in WebUI. All management operations can be performed through a browser — no extra app installation needed.

  * **Mode Switching** — Switch performance modes in real time.
  * **App Rule Management** — Configure dedicated performance strategies for different apps.
  * **Configuration Editing** — Edit YAML configuration files online.
  * **Log Viewer** — View yumi daemon logs in real time.

-----

### 🛠️ Scheduling Core (yumi)

The core of yumi is driven by a Rust daemon, **yumi**. It uses eBPF kernel probes for zero-overhead data collection, achieving efficient performance control with extremely low resource consumption.

#### Core Features

  * **High-Performance Rust Implementation**: Extremely low system resource usage and minimal power consumption.
  * **eBPF Kernel-Level Monitoring**: Precisely collects per-core CPU utilization and thread runtime via `sched_switch` tracepoint; captures rendering frame intervals with zero overhead via `queueBuffer` uprobe.
  * **Real-time Configuration Monitoring**: Supports hot-reloading for `config.yaml` and `rules.yaml`, allowing mode switches without a reboot.
  * **Built-in FAS Engine**: PID controller-driven frame-aware scheduling with automatic capacity weight detection, per-app configuration, and CPU utilization-assisted frequency scaling.
  * **CLG Load Governor**: Adaptive frequency scaling based on real-time eBPF load data, replacing the kernel's native governor.
  * **Multi-Language Internationalization**: Fluent-based i18n system supporting Chinese and English log output.

#### Feature Modules

| Feature Module | Description |
| :--- | :--- |
| **eBPF CPU Monitor** | Collects per-core idle/busy time and thread runtime via `sched_switch` tracepoint, with real-time state compensation (compensating for tasks currently executing but not yet triggering a context switch). |
| **eBPF FPS Monitor** | Captures frame submission events via `queueBuffer` uprobe, with kernel-side PID filtering and zero-copy perf event transport to userspace. |
| **FAS Frame-Aware Scheduling** | PID controller-driven; analyzes frame intervals in real time and maps them to CPU frequency via perf_index (0.0–1.0) for per-frame dynamic frequency scaling. |
| **CLG Load Governor** | Adaptive frequency scaling based on real-time eBPF load data, with independent per-cluster control and up/down rate limiting. |
| **CPU Frequency Control** | FastWriter high-performance sysfs writer with deduplication, unmount, and frequency verification; locked-frequency writes to each core cluster. |
| **Auto Capacity Weight** | Runtime detection of each core cluster's `cpu_capacity` to automatically compute capacity_weight — no manual core architecture configuration needed. |
| **Smart Screen-Off Power Saving** | Auto-enters Doze mode when screen is off with CLG forced into extreme power-saving configuration; auto-restores on screen on. |
| **Temperature-Aware Throttling** | Real-time CPU temperature monitoring; limits FAS performance ceiling when threshold is exceeded. |
| **I/O Scheduler Optimization** | Iterates over all block devices with customizable I/O schedulers, read-ahead size, merge policy, and iostats parameters. |
| **Screen State Detection** | Monitors power/backlight events via Netlink uevent — zero-polling screen on/off detection. |
| **Foreground App Detection** | Reads `top-app` cgroup process list with non-blocking debounce; automatically filters IMEs and system processes. |

-----

### 🎯 FAS Frame-Aware Scheduling — In Depth

FAS (Frame Aware Scheduling) is yumi's built-in frame-aware dynamic frequency scaling engine, designed specifically for gaming scenarios. Unlike traditional static modes, FAS precisely controls CPU frequency through a PID controller that analyzes the rendering time of every frame in real time, minimizing power consumption while ensuring smoothness.

#### How It Works

The FAS engine maintains a **perf_index** (performance index, range 0.0–1.0) and adjusts it via a PID controller based on real-time frame time feedback:

  * **Frame time exceeds budget** → PID output is negative → perf_index rises → CPU frequency increases
  * **Frame time meets budget** → PID output is positive → perf_index slowly falls → CPU frequency decreases
  * **perf_index is mapped to actual frequency steps for each core cluster via capacity_weight**

The PID controller uses three configurable coefficients (kp, ki, kd) to control proportional, integral, and derivative responses respectively. The integral term features a leak mechanism to prevent saturation, and the derivative term uses low-pass filtering to suppress noise.

#### Core Mechanisms

  * **Automatic Frame Rate Gear Switching**: Supports multiple frame rate targets (e.g., 30/60/90/120/144 fps) with automatic up/downshift based on actual rendering capability. Before downshifting, the engine first attempts a boost to confirm whether a downshift is truly necessary, preventing false downshifts. Supports low-perf stable-framerate upgrades and extreme framerate native gear detection.
  * **CPU Load-Assisted Frequency Scaling**: Uses the foreground heaviest thread utilization collected via eBPF (EMA-smoothed) to apply a util_cap soft ceiling on perf_index, preventing frequency from running high with no actual load.
  * **Auto Capacity Weight Detection**: Reads each core cluster's `cpu_capacity` at runtime and automatically computes capacity_weight (higher weight for big cores) — no manual core architecture configuration needed.
  * **Per-App Configuration**: Supports individual frame rate gear lists and frame rate margins for each game, with runtime dynamic matching to the nearest gear.
  * **Loading Scene Detection**: Automatically identifies game loading screens (sustained heavy frames). Upon entering loading state, it locks to mid-to-high frequencies and resumes normal scheduling with protection after loading ends.
  * **Frequency Hysteresis**: Hysteresis bands are set between adjacent frequency steps to prevent rapid toggling at boundaries.
  * **Jank Cooldown**: After a severe frame drop, the engine enters a cooldown period during which it maintains a higher frequency to avoid triggering a chain of stutters. Jank response intensity increases exponentially with consecutive drops.
  * **Frequency Verification & Recovery**: Periodically reads back actual frequency to detect external overrides (e.g., thermal throttling), automatically unmounts and rewrites.
  * **Windowed Mode Support**: FAS state supports suspend/resume (5-second grace period). After a brief interruption, scheduling can resume quickly without re-initialization.
  * **perf_floor Deadlock Rescue**: Detects situations where perf_index is stuck at the floor for extended periods while frame rate is severely insufficient, automatically resetting to cold-boot performance level.

#### FAS Configuration (`rules.yaml`)

FAS parameters are configured in the `fas_rules` section of `rules.yaml`:

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

**Key Parameter Reference:**

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `fps_gears` | float[] | [30,60,90,120,144] | Supported frame rate gear list; FAS automatically switches between these levels. |
| `fps_margin` | string | "3.0" | Frame rate margin (fps). EMA budget = 1000 / (target − margin), providing a tolerance buffer. |
| `pid.kp / ki / kd` | float | 0.050/0.010/0.006 | PID controller proportional, integral, and derivative coefficients. |
| `auto_capacity_weight` | bool | true | Whether to auto-detect core cluster capacity weights. When disabled, uses manually configured `cluster_profiles`. |
| `perf_floor` | float | 0.22 | Minimum perf_index (dynamically raised for high-refresh games). |
| `perf_ceil` | float | 1.0 | Maximum perf_index. |
| `perf_init` | float | 0.45 | Initial perf_index on normal startup. |
| `perf_cold_boot` | float | 0.85 | perf_index during cold boot period (first 3.5 seconds). |
| `freq_hysteresis` | float | 0.015 | Frequency hysteresis coefficient, preventing frequent toggling between adjacent steps. |
| `heavy_frame_threshold_ms` | float | 150.0 | Heavy frame threshold (ms). Frames exceeding this value are treated as loading frames. |
| `loading_cumulative_ms` | float | 2500.0 | Enters loading state when cumulative heavy frame duration exceeds this value. |
| `post_loading_perf` | float | 0.65 | perf_index after loading ends. |
| `core_temp_threshold` | float | 0.0 | Temperature throttling threshold (°C). 0 = disabled. |
| `core_temp_throttle_perf` | float | 0.70 | Performance ceiling during temperature throttling. |
| `util_cap_divisor` | float | 0.45 | Divisor for CPU load-assisted frequency scaling (smaller = more aggressive throttling). |
| `per_app_profiles` | map | {} | Per-game configuration, supporting `target_fps` and `fps_margin`. |

-----

### ⚙️ Advanced Configuration (`config.yaml` Explained)

yumi uses a YAML-formatted configuration file, allowing for deep customization.

#### 1️⃣ Metadata (`meta`)

This section defines the basic behavior of the daemon.

```yaml
meta:
  loglevel: "INFO"
  language: "en"
```

| Field | Type | Description |
| :--- | :--- | :--- |
| `loglevel` | string | Log level detail. Options: `DEBUG`, `INFO`, `WARN`, `ERROR`. Supports runtime hot-update. |
| `language` | string | Daemon log language. Currently supports `en` (English) and `zh` (Chinese). Supports runtime hot-switch. |

#### 2️⃣ Function Toggles (`function`)

```yaml
function:
  CpuIdleScalingGovernor: false
  IOOptimization: true
```

| Function | Description |
| :--- | :--- |
| `CpuIdleScalingGovernor`| Whether to allow custom CPU Idle governors (see `CpuIdle` section). |
| `IOOptimization` | Enables I/O optimization, iterating over all block devices to apply scheduler and parameter settings (see `IO_Settings` section). |

#### 3️⃣ I/O Settings (`IO_Settings`)

Requires `function.IOOptimization` to be `true`. When enabled, iterates over all block devices under `/sys/block/*` and applies the following parameters.

```yaml
IO_Settings:
  Scheduler: "none"
  read_ahead_kb: "128"
  nomerges: "2"
  iostats: "0"
```

| Field | Type | Description |
| :--- | :--- | :--- |
| `Scheduler` | string | I/O scheduler, e.g., `"none"`, `"mq-deadline"`, `"bfq"`, `"kyber"`. Leave empty to keep the system default. |
| `read_ahead_kb` | string | Read-ahead size (KB). |
| `nomerges` | string | Merge policy. `0` = allow merges, `1` = simple merges only, `2` = disable merges. |
| `iostats` | string | I/O statistics. `0` = disable (recommended, reduces overhead), `1` = enable. |

#### 4️⃣ CPU Idle (`CpuIdle`)

Requires `function.CpuIdleScalingGovernor` to be `true`.

```yaml
CpuIdle:
  current_governor: "ladder"
```

  * `current_governor`: Sets the CPU Idle governor.

#### 5️⃣ Performance Mode Configuration

Each performance mode can independently configure CPU Load Governor (CLG) parameters:

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

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `up_threshold` | float | 0.80 | Ramps up frequency quickly when load exceeds this threshold. |
| `down_threshold` | float | 0.50 | Allows frequency ramp-down when load drops below this threshold. |
| `smoothing_up` | float | 0.60 | Ramp-up smoothing coefficient (larger = faster). |
| `smoothing_down` | float | 0.30 | Ramp-down smoothing coefficient (larger = faster). |
| `down_rate_limit_ticks` | int | 3 | Ramp-down rate limit (in ticks, each tick = 200ms). |
| `headroom_factor` | float | 1.25 | Target perf = actual load × headroom, providing frequency margin. |
| `perf_floor` | float | 0.15 | Performance floor. |
| `perf_ceil` | float | 1.0 | Performance ceiling. |
| `perf_init` | float | 0.50 | Initial performance value. |

-----

### 📊 eBPF Probes — In Depth

yumi uses two eBPF probes for kernel-level data collection:

#### CPU Probe (`cpu_probe.c`)

Attached to `tracepoint/sched/sched_switch`, triggered on every context switch, recording:

  * **Per-core idle/busy time**: Accumulated via `PERCPU_ARRAY`; userspace reads and computes real utilization.
  * **Per-core current TID**: Used by userspace to compensate in real time for tasks that haven't yet triggered a sched_switch.
  * **Thread runtime**: Recorded per-thread cumulative CPU time via `HASH` map, used to compute the foreground app's heaviest thread utilization.

#### FPS Probe (`fps_probe.c`)

Attached to `libgui.so`'s `queueBuffer` function (uprobe), triggered on every frame submission:

  * **Kernel-side PID filtering**: Only sends perf events for the target process via the `target_pid` map, reducing overhead from unrelated processes.
  * **Frame interval calculation**: Records the timestamp delta between each `queueBuffer` call, transmitted to userspace via `PERF_EVENT_ARRAY` with zero-copy.
  * **Baseline preheating**: Records timestamps even for non-target processes, ensuring the first frame after a PID switch can compute a correct delta.

-----

## 📥 Installation Instructions

### Prerequisites

1.  **Obtain Root Access**
2.  **Ensure kernel supports eBPF** (most Android 10+ devices already support this)

### Installation Steps

1.  **Download the Module** — Download the latest release from the [Releases](https://github.com/imacte/yumi/releases) page.
2.  **Flash the Module** — Flash the yumi module via Magisk / KernelSU.
3.  **Access the WebUI** — Once the module starts, open the WebUI in your browser to manage and configure settings.
4.  **Configure Rules** — Set performance strategies for different apps as needed.

## 🚀 Performance Optimization Suggestions

### Daily Use

1.  **Use Balance Mode** — CLG adaptive frequency scaling provides the best performance/power balance for most apps.
2.  **Set App Rules** — Assign FAS mode for gaming apps.

### Gaming Optimization

1.  **Use FAS Mode** — Frame-aware scheduling automatically saves power while maintaining smoothness; recommended as the primary gaming mode.
2.  **Configure Per-App Parameters** — Set `target_fps` and `fps_margin` for specific games in `rules.yaml`.
3.  **Tune PID Coefficients** — If frequency response feels too fast or too slow, fine-tune `kp`/`ki`/`kd`.
4.  **Monitor Temperature** — Set `core_temp_threshold` to enable thermal protection during extended gaming sessions.

### Power Saving Optimization

1.  **Use Powersave Mode** — CLG with low performance ceiling maximizes battery life in low-load scenarios.
2.  **Auto Screen-Off Savings** — No manual action needed; Doze mode activates automatically when the screen is off.
3.  **Optimize I/O Scheduler** — Reduce power consumption from storage access.

## 🔍 Troubleshooting

### Frequently Asked Questions

**Q: The module can't get Root access?**

  * Ensure your device is properly rooted and Magisk / KernelSU is installed.
  * Check your Root manager settings to ensure the yumi Root request has been granted.
  * Try reflashing the module or restarting the device.

**Q: eBPF probes fail to load?**

  * Ensure kernel supports eBPF (`CONFIG_BPF=y`, `CONFIG_BPF_SYSCALL=y`).
  * Check the logs for eBPF error messages.
  * Some older kernels may not support the required BPF features.

**Q: Smart Dynamic Mode isn't working?**

  * Verify that app rules are configured correctly.
  * Verify that the yumi module is installed and running correctly.
  * Check whether `ignored_apps` list accidentally excludes the target app.

**Q: Performance modes aren't switching?**

  * Verify that the yumi module is installed and running correctly.
  * View the yumi module logs to identify specific error messages.
  * Verify the configuration file format is correct (YAML syntax sensitive).

**Q: Frame rate is unstable in FAS mode?**

  * Check that `fps_gears` in `rules.yaml` includes the target frame rate.
  * Increasing `fps_margin` provides more headroom and reduces boundary fluctuations.
  * Check the FAS heartbeat entries in the logs (output every 30 frames) to confirm the scheduling state is normal.
  * If the frequency is being overridden by thermal throttling, a "freq mismatch" message will appear in the logs — this is normal verification and recovery behavior.
  * Try configuring dedicated parameters for the game in `per_app_profiles`.

**Q: CPU utilization data seems inaccurate?**

  * The eBPF CPU probe requires kernel support for `tracepoint/sched/sched_switch`.
  * Userspace read cycle is 200ms, with real-time state compensation to improve accuracy.
  * Check the cpu_monitor initialization info in the logs to confirm the online core list is correct.

## 📊 Project Statistics

<div align="center">

[![Star History Chart](https://api.star-history.com/svg?repos=imacte/yumi&type=Date)](https://star-history.com/#imacte/yumi&Date)

</div>

## 📮 Contact Us

  * **GitHub Issues** — [For project issues and suggestions](https://github.com/imacte/yumi/issues)
  * **QQ Group** — 1036909137
  * **Telegram** — [Join TG Channel](https://t.me/+gp4adLJAsXYzMjc1)

-----

<div align="center">

<sub>📅 Document Updated: March 11, 2026</sub><br>
<sub>🚀 yumi — Giving every Android device the best performance experience</sub>

</div>