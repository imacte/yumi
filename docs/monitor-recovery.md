# eBPF 监控故障修复与验收

## 行为

- 构建三个独立 ELF：`cpu`、`fps-ring`、`fps-perf`。CPU 不创建 FPS map。
- `RING_BUF` 创建返回 `EINVAL`、`EOPNOTSUPP` 或 `ENOSYS` 时，尝试 PerfEventArray 后端；权限和内存错误保留完整错误链，不伪装成兼容性问题。
- FPS 在事件缓冲区注册完成后才报告初始化成功；目标 PID 附加失败每 2 秒重试，切换 PID 清空采样基线。
- RingBuf 和 perf 共用 16 字节事件布局（包含显式初始化的 padding）；perf 处理跨缓冲区边界、跨 CPU 时间排序、丢事件和 CPU 上下线。
- 只发送新产生的帧间隔，不重放旧帧；CPU 样本保持 CPU ID 索引，前台负载采用最忙线程的完整累计快照差值。
- CLG 在首个有效 CPU 样本到达后启动。FAS 还需要目标 PID 的有效帧。超过 2 秒无有效样本，或监控上报退出时，释放相关调频控制。
- 控制器交接立即释放旧锁频，不再保留旧 FAS 锁频状态。FAS 保存并尝试恢复 governor、频率上下限及 perfmgr 设置。
- 初始化日志输出内核版本、页大小、memlock 设置失败的 errno；监控错误输出完整原因链。频率日志统一使用 kHz。

## 自动检查

在 Linux、Rust nightly、rust-src 和 bpf-linker 可用的环境执行：

```sh
cargo test -p yumi --bin yumi --locked
```

测试包括帧事件解码、重复/乱序/过期帧、PID 切换基线、CPU pending 时间差分、监控超时恢复、RingBuf 降级错误分类及真实 ELF 中的程序/map 隔离。CI 已加入此步骤。

本次 Windows 环境已执行：

- 三个程序的 `bpfel-unknown-none` release 目标 `cargo check`。
- 用户态与全部测试的 `aarch64-linux-android` 类型检查。检查使用临时 harness 绕过本机缺失的 BPF 链接器，嵌入占位字节，不能代表 ELF 链接成功或内核加载成功。
- 6 项不依赖 Linux 的采样和健康状态测试，均通过。
- build.rs 的 Rust 编译与 `git diff --check`。

本机缺少 bpf-linker，未完成完整 ELF 链接、模块打包或实机加载；错误分类和真实 ELF 隔离测试已通过类型检查，执行留给 Linux CI。

## 实机验收

1. 在已知支持 RingBuf 的设备启动：CPU 与 FPS 分别成功初始化，日志注明 RingBuf。
2. 在不支持 RingBuf、但支持 perf/uprobes 的设备启动：记录创建 RingBuf 的具体 errno，随后注明 PerfEventArray；CPU 不出现 RING_BUF 错误。
3. 启动时无前台 PID，随后打开应用：能够附加并持续收到新帧；附加失败后同一 PID 可以再次尝试。
4. 连续切换游戏、重启同包名进程、息屏/亮屏：PID、应用配置和频率控制器同步切换。
5. 停止渲染超过 2 秒：FAS 释放控制，不继续产生缓存帧事件。恢复渲染后重新建立基线并启动。
6. 注入 CPU map 读取失败或停止 CPU 样本：CLG 与依赖 CPU 数据的 FAS 释放控制；核对各 policy 的 governor、scaling_min_freq、scaling_max_freq 与接管前快照一致。
7. 确认日志中的频率单位为 kHz。不要仅凭 “initialized” 判断正常，应同时确认采样日志及实际 sysfs 状态。

内核 verifier、SELinux、perf 权限、设备 libgui 符号和 sysfs 写入限制仍需通过上述设备检查确认；发生失败时保留新日志中的完整错误链。
