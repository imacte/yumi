# --- Main & Monitor ---
yumi-module-starting = yumi-module Unified Starting...
scheduler-module-started = Scheduler module started.
scheduler-module-start-failed = Failed to start scheduler module: { $error }
monitor-module-crashed = Monitor module crashed: { $error }
monitor-module-started = Monitor module started.
monitor-starting = Starting yumi-monitor module...
monitor-boot-scripts-failed = [Main] Failed to run boot scripts: { $error }
monitor-initial-config-failed = [Main] Failed to read initial config: { $error }.
    Using default.
monitor-screen-watcher-failed = [Main] Screen state watcher thread crashed: { $error }
monitor-config-watcher-failed = [Main] Config watcher thread crashed: { $error }
monitor-fps-crashed = [Main] FPS Monitor crashed: { $error }
monitor-fps-tokio-failed = [Main] Failed to create Tokio runtime for FPS monitor
monitor-cpu-crashed = [Main] CPU Load Monitor crashed: { $error }
monitor-cpu-tokio-failed = [Main] Failed to create Tokio runtime for CPU monitor

# Boot
boot-scripts-running = [Boot] Running boot scripts...
boot-script-applying = [Boot] Applying script: { $path }
boot-script-success = [Boot] Script { $name } applied successfully.
boot-script-failed = [Boot] Script { $name } failed: { $error }
boot-script-exec-failed = [Boot] Failed to execute script { $name }: { $error }
boot-scripts-finished = [Boot] Boot scripts execution finished.

# Power
power-cpu-temp-found = [Power] Successfully found CPU temp sensor: { $path }
power-cpu-temp-not-found = [Power] Failed to find CPU temp path: { $error }.
    Using 0.0 as temperature.
power-loop-started = [Power] Power monitoring loop started.
power-screen-off-skip = [Power] Screen is off, skipping power poll.
power-charging-stopped = [Power] Charging stopped. Checking session limits...
power-trim-failed = [Power] Failed to trim old sessions: { $error }
power-new-session = [Power] Starting new session: { $id }
power-db-write-failed = [Power] Failed to write power log to DB: { $error }
power-read-failed = [Power] Failed to read voltage or current: { $error }
power-status-read-failed = [Power] Failed to read charging status: { $error }

# AppDetect
app-detect-config-watch = [AppDetect] Started watching config file: { $path }
app-detect-change-detected = [AppDetect] Change detected, debouncing (100ms)...
app-detect-reloading = [AppDetect] Debounce finished. Reloading config...
app-detect-load-failed = [AppDetect] Failed: { $error }. Using default.
app-detect-reload-success = [AppDetect] Config reloaded successfully.
app-detect-loop-started = [AppDetect] App detection loop started (3000ms poll).
app-detect-screen-changed = [AppDetect] Screen state changed: { $old } -> { $new }
app-detect-mode-change = [AppDetect] Mode change: { $old } -> { $new }
app-detect-mode-change-pkg = [AppDetect] Mode change: { $old } -> { $new } ({ $pkg })
app-detect-ime-auto = [AppDetect] Auto-detected IME: { $pkg }
app-detect-ime-fallback = [AppDetect] Failed to auto-detect IME, using fallback list.

# ScreenDetect
screen-state-change-detected = [Screen] State change detected via '{ $source }'.
screen-state-changed-value = [Screen] Screen state changed: { $state }
screen-netlink-started = [Screen] Started netlink-sys socket listener.

# --- Monitors ---
cpu-monitor-started = [CPU Monitor] eBPF System Load monitor started (Long-task blind spot fixed).
cpu-monitor-online-cpus-failed = [CPU Monitor] Failed to get online CPUs: { $error }
cpu-monitor-online-cpus = [CPU Monitor] Detected online CPU core IDs: { $cpus }
cpu-monitor-fg-pid-updated = [CPU Monitor] Foreground PID updated { $old } -> { $new }
cpu-monitor-tick-log = [CPU Monitor] cores=[{ $cores }] fg_pid={ $pid } fg_max_util={ $util }% threads_tracked={ $threads } delta={ $delta }ms
cpu-monitor-channel-closed = [CPU Monitor] Channel closed, exiting loop.
fps-monitor-init = [FPS Monitor] Initializing eBPF FPS monitor...
fps-monitor-attached = [FPS Monitor] Attached uprobe to symbol: { $sym }
fps-monitor-attach-failed = [FPS Monitor] Failed to attach any Uprobe symbols!
fps-monitor-pid-filter-updated = [FPS Monitor] Updated kernel PID filter: { $old } -> { $new }
fps-monitor-started = [FPS Monitor] eBPF FPS monitor started successfully (kernel PID filter: { $filter }).

# --- Scheduler ---
scheduler-ipc-started = [Scheduler] IPC Channel listener started.
scheduler-mode-change-request = [Scheduler] Mode change request: { $old } -> { $new } (Pkg: { $pkg }, Temp: { $temp })
scheduler-boost-active-ignore = [Scheduler] Boost active, ignoring mode apply.
scheduler-apply-failed = [Scheduler] Failed to apply settings: { $error }
scheduler-channel-closed = [Scheduler] Channel closed! Thread exiting.
scheduler-doze-enable = [Scheduler] Screen OFF: Enabling Extreme Doze mode (Restricting CPU max performance).
scheduler-doze-restore = [Scheduler] Screen ON: Restoring previous performance constraints.
scheduler-fas-suspend-clear = [Scheduler] FAS: clearing stale suspend state before applying static mode
scheduler-fas-suspended = [Scheduler] FAS: suspended (pkg={ $pkg }, grace={ $grace }s, in-memory state preserved)
scheduler-fas-resumed = [Scheduler] FAS: resumed from suspend (pkg={ $pkg }, pid={ $pid }, policies intact, sysfs reapplied)
scheduler-fas-takeover = [Scheduler] Entered FAS mode (pkg={ $pkg }, pid={ $pid }), FAS controller is now taking over CPU frequencies.
scheduler-clg-resync = [Scheduler] CLG: resync after app launch boost ended
scheduler-config-reload-event = [Scheduler] Received config reload event. Updating in-memory rules...
scheduler-fas-full-init = [Scheduler] FAS: full policy init triggered by config reload (was empty).
scheduler-fas-hot-reload = [Scheduler] FAS: rules hot-reloaded without resetting runtime state.
scheduler-fas-grace-expired = [Scheduler] FAS: suspend grace expired, clearing FAS in-memory state
scheduler-clg-init = [Scheduler] CPU Load Governor: initialized at startup (mode={ $mode })

# --- Scheduler: Config Watcher ---
config-reloading = [Config] Config file change detected, reloading...
config-reloaded-success = [Config] Config reloaded successfully.
config-reload-fail = [Config] Config reload failed: { $error }
config-watch-error = [Config] Failed to watch config directory: { $error }
config-apply-mode-failed = [Config] Failed to apply reloaded mode settings: { $error }
config-apply-tweaks-failed = [Config] Failed to apply reloaded system tweaks: { $error }

# --- CLG ---
clg-init = [CLG] P{ $pid } init | cores={ $cpus } | freqs={ $fmin }-{ $fmax } MHz | P={ $perf } -> { $freq } kHz
clg-activated = [CLG] CPU Load Governor activated, taking over { $count } cluster(s)
clg-no-clusters = [CLG] CPU Load Governor: no valid clusters found, staying inactive
clg-deactivated = [CLG] CPU Load Governor deactivated
clg-config-reloaded = [CLG] config hot-reloaded | up={ $up } down={ $down } floor={ $floor } ceil={ $ceil }
clg-tick-log = [CLG] P{ $pid } util={ $util }% perf={ $perf } freq={ $freq }kHz{ $boost }
clg-resync-boost = [CLG] P{ $pid } resync after boost: perf={ $perf } -> freq={ $freq }kHz
clg-writer-invalid = [CLG] P{ $pid } sysfs writer invalid (max_valid: { $max_valid }, min_valid: { $min_valid }), skipping.

# --- FAS ---
fas-open-failed = [FAS] failed to open { $path }: { $error }
fas-umount2-failed = [FAS] umount2({ $path }) = { $error }
fas-write-freq-failed = [FAS] write freq { $freq } failed: { $error }
fas-freq-mismatch = [FAS] P{ $pid }: freq mismatch! expected { $min }-{ $max }, actual { $actual } -> emergency reapply
fas-auto-capacity = [FAS] auto capacity weight:
fas-auto-capacity-core = [FAS]   P{ $pid }: cap={ $cap } -> w={ $weight }
fas-sysfs-invalid = [FAS] P{ $pid } sysfs writer invalid, freq control may fail!
fas-policy-init = [FAS] P{ $pid } { $min }-{ $max } MHz | w={ $weight }
fas-init-summary = [FAS] init | { $fps }fps margin:{ $margin } clusters:{ $clusters } P:{ $perf } profiles:{ $profiles }
fas-init-pid = [FAS] PID parameters | Kp={ $kp } Ki={ $ki } Kd={ $kd }
fas-app-switch = [FAS] app switch ({ $ms }ms) | P -> { $perf }
fas-loading-start = [FAS] entering loading state ({ $frames } frames, { $ms }ms) | P { $old_perf } -> { $new_perf }
fas-loading-exit = [FAS] exit loading state | P -> { $perf }
fas-gear-switch = [FAS] gear switch { $old } -> { $new }fps | P -> { $perf }
fas-low-perf-upgrade = [FAS] low-load steady frame upgrade | P={ $perf } avg={ $avg } stddev={ $stddev } -> { $fps }fps
fas-downgrade-boost = [FAS] downgrade boost | avg:{ $avg } | P { $old } -> { $new } (inc={ $inc })
fas-boost-expired = [FAS] boost expired, fast-tracking downgrade (confirm={ $confirm })
fas-floor-rescue = [FAS] floor-rescue | stuck { $frames } frames at P={ $old }, avg:{ $avg } -> P:{ $new }
fas-tick-log = [FAS] { $target }fps avg:{ $avg } | { $ms }ms ema:{ $ema } | err:{ $err_ema }/{ $err_inst } | { $act } | P:{ $perf } fg_util:{ $util }{ $cd }{ $damp }{ $temp }
fas-set-game = [FAS] set_game | pkg={ $pkg } | gears={ $gears } | target={ $target }fps
fas-no-profile = [FAS] no per-app profile for '{ $pkg }', using global gears { $gears }
fas-ignore-write = [FAS] P{ $pid } ignore_write = { $ignore }
fas-pid-reloaded = [FAS] PID coefficients hot-reloaded: Kp={ $kp } Ki={ $ki } Kd={ $kd }
fas-rules-reloaded = [FAS] rules hot-reloaded (margin={ $margin }, floor={ $floor }, ceil={ $ceil }, profiles={ $profiles })
fas-policy-writer-invalid = [FAS] P{ $pid } policy writer invalid (max_valid: { $max_valid }, min_valid: { $min_valid }), skipping.

# --- Scheduler: Boost ---
boost-active-defer-config-apply = [Boost] Boost active, deferring config apply.
boost-active-skipping-apply-all-settings = [Boost] Boost active, skipping apply_all_settings.
app-launch-watch-failed = [Boost] Failed to watch for app launch: { $error }
boost-apply-failed = [Boost] Failed to apply boost frequencies: { $error }
boost-restore-freq-failed = [Boost] Failed to restore frequencies: { $error }
boost-mode-changed = [Boost] Mode changed during boost ({ $old } -> { $new }), applying all settings.
boost-mode-apply-failed = [Boost] Failed to apply new mode settings after boost: { $error }
boost-get-mode-failed = [Boost] Could not get current mode in boost loop: { $error }
applaunch-detected-boosting-frequencies = [Boost] App launch detected, boosting frequencies...
boost-finished-restoring-settings = [Boost] Boost finished, restoring settings.
appLaunchboost-thread-created = [Boost] AppLaunchBoost thread created.

# --- Scheduler: Core Allocation ---
pidof-failed = Failed to execute pidof for '{ $name }': { $error }
process-not-found = Process '{ $name }' not found, skipping.
cpuset-write-failed = Failed to write to cpuset ({ $name }): { $error }
cpuctl-write-failed = Failed to write to cpuctl ({ $name }): { $error }
thread-core-allocation-log = Thread core allocation completed.
main-config-watch-thread-create = Main config watcher thread created.

# --- Scheduler: Settings ---
apply-settings-for-mode = Applying settings for mode: { $mode }
settings-applied-success = Settings for mode '{ $mode }' applied successfully.
load-balancing-start = Load balancing settings applied.
apply-cpuset-start = CPU set settings applied.
apply-cpu-idle-governor-start = CPU idle governor settings applied.
apply-io-settings-start = I/O settings applied.
attempted-to-enable-eas-scheduler-settings = Attempted to enable EAS scheduler.
attempted-to-disable-eas-scheduler = Attempted to disable EAS scheduler.

# --- Logger ---
log-level-updated = Log level updated to: { $level }