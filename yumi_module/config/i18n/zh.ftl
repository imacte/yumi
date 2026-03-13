# --- Main & Monitor ---
yumi-module-starting = yumi-module 统一启动中...
scheduler-module-started = 调度器模块已启动
scheduler-module-start-failed = 启动调度器模块失败: { $error }
monitor-module-crashed = 监控模块崩溃: { $error }
monitor-module-started = 监控模块已启动
monitor-starting = 正在启动 yumi-monitor 模块...
monitor-initial-config-failed = [Main] 读取初始配置失败: { $error }. 正在使用默认值。
monitor-screen-watcher-failed = [Main] 屏幕状态监控线程崩溃: { $error }
monitor-config-watcher-failed = [Main] 配置监控线程崩溃: { $error }
monitor-fps-crashed = [Main] FPS 监控崩溃: { $error }
monitor-fps-tokio-failed = [Main] 无法为 FPS 监控创建 Tokio 运行时
monitor-cpu-crashed = [Main] CPU 负载监控崩溃: { $error }
monitor-cpu-tokio-failed = [Main] 无法为 CPU 监控创建 Tokio 运行时

# --- AppDetect ---
app-detect-config-watch = [AppDetect] 开始监控配置文件: { $path }
app-detect-change-detected = [AppDetect] 检测到变更，正在防抖 (100ms)...
app-detect-reloading = [AppDetect] 防抖结束。正在重载配置...
app-detect-load-failed = [AppDetect] 失败: { $error }。使用默认值
app-detect-reload-success = [AppDetect] 配置重载成功
app-detect-loop-started = [AppDetect] 应用检测循环已启动 (3000ms 轮询)
app-detect-screen-changed = [AppDetect] 屏幕状态变更: { $old } -> { $new }
app-detect-mode-change-pkg = [AppDetect] 模式变更: { $old } -> { $new } ({ $pkg })
app-detect-ime-auto = [AppDetect] 自动检测到输入法: { $pkg }
app-detect-ime-fallback = [AppDetect] 自动检测输入法失败，使用后备列表。

# --- ScreenDetect ---
screen-state-change-detected = [Screen] 通过 '{ $source }' 检测到状态变更
screen-state-changed-value = [Screen] 屏幕状态已变更: { $state }
screen-netlink-started = [Screen] 已启动 netlink-sys 套接字监听器

# --- Monitors ---
cpu-monitor-started = [CPU Monitor] eBPF 系统负载监控已启动 (修复长任务盲区)。
cpu-monitor-online-cpus-failed = [CPU Monitor] 获取在线 CPU 失败: { $error }
cpu-monitor-online-cpus = [CPU Monitor] 检测到在线 CPU 核心 ID: { $cpus }
cpu-monitor-fg-pid-updated = [CPU Monitor] 前台 PID 已更新 { $old } -> { $new }
cpu-monitor-tick-log = [CPU Monitor] 核心=[{ $cores }] 前台pid={ $pid } 前台最大利用率={ $util }% 跟踪线程数={ $threads } 耗时={ $delta }ms
cpu-monitor-channel-closed = [CPU Monitor] 通道已关闭，退出循环。
fps-monitor-init = [FPS Monitor] 正在初始化 eBPF FPS 监控...
fps-monitor-attached = [FPS Monitor] 已将 uprobe 挂载到符号: { $sym }
fps-monitor-attach-failed = [FPS Monitor] 未能挂载任何 Uprobe 符号！
fps-monitor-pid-filter-updated = [FPS Monitor] 已更新内核 PID 过滤器: { $old } -> { $new }
fps-monitor-started = [FPS Monitor] eBPF FPS 监控启动成功 (内核 PID 过滤: { $filter })。

# --- Scheduler ---
scheduler-ipc-started = [Scheduler] IPC 通道监听器已启动
scheduler-mode-change-request = [Scheduler] 模式变更请求: { $old } -> { $new } (包名: { $pkg }, 温度: { $temp })
scheduler-apply-failed = [Scheduler] 应用设置失败: { $error }
scheduler-channel-closed = [Scheduler] 通道已关闭！线程退出
scheduler-doze-enable = [Scheduler] 息屏: 启用极致深度睡眠模式 (限制 CPU 最高性能)。
scheduler-doze-restore = [Scheduler] 亮屏: 恢复之前的性能限制。
scheduler-clg-init = [Scheduler] CPU 负载调频器: 在启动时初始化 (模式={ $mode })

# --- Scheduler: Config Watcher ---
config-reloading = [Config] 检测到配置文件变更，正在重载...
config-reloaded-success = [Config] 配置重载成功
config-reload-fail = [Config] 配置重载失败: { $error }
config-watch-error = [Config] 监控配置目录失败: { $error }
config-apply-mode-failed = [Config] 应用重载的模式设置失败: { $error }
config-apply-tweaks-failed = [Config] 应用重载的系统微调失败: { $error }

# --- SysFS (共享 FastWriter) ---
sysfs-open-failed = [SysFS] 打开 { $path } 失败: { $error }
sysfs-umount2-failed = [SysFS] umount2({ $path }) 失败: { $error }
sysfs-write-freq-failed = [SysFS] 写入频率 { $freq } 失败: { $error }

# --- CLG ---
clg-init = [CLG] P{ $pid } 初始化 | 核心={ $cpus } | 频率={ $fmin }-{ $fmax } MHz | P={ $perf } -> { $freq } kHz
clg-activated = [CLG] CPU 负载调频器已激活，共接管 { $count } 个集群
clg-no-clusters = [CLG] CPU 负载调频器: 未找到有效集群，保持未激活状态
clg-deactivated = [CLG] CPU 负载调频器已停用
clg-config-reloaded = [CLG] 配置已热重载 | 升频={ $up } 降频={ $down } 地板={ $floor } 天花板={ $ceil }
clg-tick-log = [CLG] P{ $pid } 利用率={ $util }% perf={ $perf } 频率={ $freq }kHz{ $boost }
clg-writer-invalid = [CLG] P{ $pid } sysfs 写入器无效 (max_valid: { $max_valid }, min_valid: { $min_valid })，已跳过。

# --- FAS ---
fas-freq-mismatch = [FAS] P{ $pid }: 频率不匹配！预期 { $min }-{ $max }，实际 { $actual } -> 正在紧急重写
fas-auto-capacity = [FAS] 自动计算算力权重:
fas-auto-capacity-core = [FAS]   P{ $pid }: 算力={ $cap } -> 权重={ $weight }
fas-policy-init = [FAS] P{ $pid } { $min }-{ $max } MHz | 权重={ $weight }
fas-init-summary = [FAS] 初始化 | { $fps }fps 冗余:{ $margin } 集群:{ $clusters } P:{ $perf } 配置数:{ $profiles }
fas-app-switch = [FAS] 应用切换 ({ $ms }ms) | P -> { $perf }
fas-loading-start = [FAS] 进入加载状态 ({ $frames } 帧, { $ms }ms) | P { $old_perf } -> { $new_perf }
fas-loading-exit = [FAS] 退出加载状态 | P -> { $perf }
fas-gear-switch = [FAS] 档位切换 { $old } -> { $new }fps | P -> { $perf }
fas-low-perf-upgrade = [FAS] 低负载稳帧升档 | P={ $perf } 平均帧={ $avg } 标准差={ $stddev } -> { $fps }fps
fas-downgrade-boost = [FAS] 降档加速 | 平均帧:{ $avg } | P { $old } -> { $new } (增量={ $inc })
fas-boost-expired = [FAS] 加速期满，开启降档快车道 (确认帧={ $confirm })
fas-floor-rescue = [FAS] 触底救援 | 卡在地板 { $frames }帧 P={ $old }, 平均帧:{ $avg } -> P:{ $new }
fas-tick-log = [FAS] { $target }fps 平均:{ $avg } | { $ms }ms ema:{ $ema } | 误差:{ $err_ema }/{ $err_inst } | { $act } | P:{ $perf } 前台利用率:{ $util }{ $cd }{ $damp }{ $temp }
fas-set-game = [FAS] 设置游戏 | 包名={ $pkg } | 档位={ $gears } | 目标={ $target }fps
fas-no-profile = [FAS] 未找到 '{ $pkg }' 的专属配置，使用全局档位 { $gears }
fas-ignore-write = [FAS] P{ $pid } 忽略写入 = { $ignore }
fas-pid-reloaded = [FAS] PID 系数热重载: Kp={ $kp } Ki={ $ki } Kd={ $kd }
fas-rules-reloaded = [FAS] 规则已热重载 (冗余={ $margin }, 地板={ $floor }, 天花板={ $ceil }, 配置数={ $profiles })
fas-policy-writer-invalid = [FAS] P{ $pid } 策略写入器无效 (max_valid: { $max_valid }, min_valid: { $min_valid })，已跳过。

# --- Scheduler: Settings ---
apply-settings-for-mode = 正在应用模式: { $mode }
settings-applied-success = 模式 '{ $mode }' 的设置已成功应用
apply-cpu-idle-governor-start = CPU 空闲调速器设置已完成
apply-io-settings-start = I/O 设置已完成
main-config-watch-thread-create = 主配置监控线程已创建

# --- Logger ---
log-level-updated = 日志级别已更新为: { $level }
