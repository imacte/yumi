/*
 * Copyright (C) 2026 yuki
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::collections::VecDeque;
use std::os::unix::io::{AsRawFd, BorrowedFd, RawFd};
use std::sync::mpsc::Sender;
use std::time::Duration;

use aya::Ebpf;
use aya::include_bytes_aligned;
use aya::maps::RingBuf;
use aya::programs::UProbe;
use log::{debug, info, warn};
use mio::{Events, Interest, Poll, Token, unix::SourceFd};
use tokio::sync::watch;

use crate::common::DaemonEvent;
use crate::fluent_args;
use crate::i18n::{t, t_with_args};
use crate::monitor::app_detect;

// ─── 常量 ────────────────────────────────────────────────

/// RingBuf 输出的帧时间戳事件（与 yumi-ebpf 的 FrameTimestampEvent 内存布局一致）
#[repr(C)]
struct FrameTimestampEvent {
    pid: u32,
    ktime_ns: u64,
}

/// 帧间隔过滤范围（纳秒），参照 fas-rs 的 MIN_FRAME_NS / MAX_FRAME_NS
const MIN_FRAME_NS: u64 = 1_000_000;
const MAX_FRAME_NS: u64 = 200_000_000;

/// 滑动窗口容量（参照 fas-rs AnalyzeTarget::frametimes）
const FRAMETIME_WINDOW: usize = 144;

// ─── FpsProbe ────────────────────────────────────────────

/// 帧数探针：每个 PID 持有一个独立的 BPF 实例 + RingBuf + 帧状态
///
/// 设计参照 fas-rs 的 [`AnalyzeTarget`] + [`UprobeHandler`]：
/// - 独立的 eBPF 实例，按 PID 挂载 uprobe
/// - 内部维护 `last_ktime_ns` 和滑动窗口 `frametimes`
/// - Drop 时自动 detach + unload
struct FpsProbe {
    /// uprobe 挂载句柄，drop 时自动 detach
    _link: aya::programs::UProbeLinkId,
    /// BPF 程序实例，drop 时自动 unload
    _bpf: Ebpf,
    /// RingBuf 的 fd，用于 mio 轮询
    ring_fd: RawFd,
    /// 上一帧的内核时间戳
    last_ktime_ns: Option<u64>,
    /// 最近帧间隔的滑动窗口
    frametimes: VecDeque<Duration>,
}

impl FpsProbe {
    /// 创建一个新的探针，挂载到指定 PID
    ///
    /// 参照 fas-rs `UprobeHandler::attach_app()`
    fn new(pid: i32, bpf_data: &'static [u8]) -> Result<Self, anyhow::Error> {
        let mut bpf = Ebpf::load(bpf_data)?;

        let program: &mut UProbe = bpf.program_mut("handle_frame").unwrap().try_into()?;
        program.load()?;

        // 挂载 uprobe，与 fas-rs 完全相同的符号 + 回退逻辑
        let link = program
            .attach(
                Some("_ZN7android7Surface11queueBufferEP19ANativeWindowBufferi"),
                0,
                "/system/lib64/libgui.so",
                Some(pid),
            )
            .or_else(|_| {
                program.attach(
                    Some("_ZN7android7Surface11queueBufferEP19ANativeWindowBufferiPNS_24SurfaceQueueBufferOutputE"),
                    0,
                    "/system/lib64/libgui.so",
                    Some(pid),
                )
            })?;

        let ring_fd = bpf.map("RING_BUF").expect("RING_BUF not found").as_raw_fd();

        info!(
            "{}",
            t_with_args("fps-monitor-attached", &fluent_args!("pid" => pid.to_string()))
        );

        Ok(Self {
            _link: link,
            _bpf: bpf,
            ring_fd,
            last_ktime_ns: None,
            frametimes: VecDeque::with_capacity(FRAMETIME_WINDOW),
        })
    }

    /// 从 RingBuf 读取所有可用帧事件，更新内部状态
    ///
    /// 参照 fas-rs `AnalyzeTarget::update()`
    fn poll_frames(&mut self) {
        let ring_map = self._bpf.map_mut("RING_BUF").expect("RING_BUF not found");
        let mut ring = RingBuf::try_from(ring_map).expect("RingBuf::try_from failed");

        while let Some(data) = ring.next() {
            if data.len() < 12 {
                continue;
            }

            let _pid = u32::from_ne_bytes(data[0..4].try_into().unwrap());
            let ktime_ns = u64::from_ne_bytes(data[4..12].try_into().unwrap());

            // 计算帧间隔 (delta = current - last)
            if let Some(last_ns) = self.last_ktime_ns {
                let delta_ns = ktime_ns.saturating_sub(last_ns);
                if (MIN_FRAME_NS..=MAX_FRAME_NS).contains(&delta_ns) {
                    // 滑动窗口：参照 fas-rs AnalyzeTarget::frametimes
                    if self.frametimes.len() >= FRAMETIME_WINDOW {
                        self.frametimes.pop_back();
                    }
                    self.frametimes.push_front(Duration::from_nanos(delta_ns));
                }
            }

            self.last_ktime_ns = Some(ktime_ns);
        }
    }

    /// 返回最新的帧间隔（滑动窗口的第一项）
    fn latest_frametime(&self) -> Option<Duration> {
        self.frametimes.front().copied()
    }
}

// ─── 主入口 ──────────────────────────────────────────────

pub async fn start_fps_loop(tx: Sender<DaemonEvent>) -> Result<(), anyhow::Error> {
    static BPF_DATA: &[u8] = include_bytes_aligned!(env!("BPF_OBJ_PATH"));
    info!("{}", t("fps-monitor-init"));

    // 初始 PID
    let initial_pid = app_detect::get_current_pid();

    // watch channel：tokio 任务通知 spawn_blocking 线程 PID 变化
    let (tx_pid, mut rx_pid) = watch::channel(initial_pid);

    // PID 检测任务（tokio 轻量任务）
    {
        tokio::spawn(async move {
            let mut last_pid: i32 = initial_pid;
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let current_pid = app_detect::get_current_pid();

                if current_pid != last_pid && current_pid > 0 {
                    debug!(
                        "{}",
                        t_with_args(
                            "fps-monitor-pid-filter-updated",
                            &fluent_args!(
                                "old" => last_pid.to_string(),
                                "new" => current_pid.to_string()
                            )
                        )
                    );
                    last_pid = current_pid;
                    let _ = tx_pid.send(current_pid);
                }
            }
        });
    }

    // RingBuf 读取 + PID 切换（spawn_blocking 线程）
    let tx_clone = tx.clone();
    std::thread::Builder::new()
        .name("fps_probe".into())
        .spawn(move || {
            let rt = tokio::runtime::Handle::current();

            // 当前活跃的探针
            let mut probe: Option<FpsProbe> = if initial_pid > 0 {
                match FpsProbe::new(initial_pid, BPF_DATA) {
                    Ok(p) => Some(p),
                    Err(e) => {
                        warn!(
                            "{}",
                            t_with_args(
                                "fps-monitor-attach-failed-initial",
                                &fluent_args!("error" => e.to_string())
                            )
                        );
                        None
                    }
                }
            } else {
                info!("{}", t("fps-monitor-init-no-pid"));
                None
            };

            // mio 轮询
            let mut poll = Poll::new().expect("mio Poll::new");
            let mut events = Events::with_capacity(64);
            let token = Token(0);

            // 注册当前探针的 RingBuf fd 到 mio
            let register = |poll: &mut Poll, probe: &FpsProbe| {
                let borrowed = unsafe { BorrowedFd::borrow_raw(probe.ring_fd) };
                let mut source = SourceFd::new(&borrowed).expect("mio SourceFd::new");
                poll.registry()
                    .register(&mut source, token, Interest::READABLE)
                    .expect("mio register");
            };

            if let Some(ref p) = probe {
                register(&mut poll, p);
            }

            loop {
                // ── 检查 PID 变化 ──
                let pid_changed = rx_pid.has_changed().unwrap_or(false);
                if pid_changed {
                    let new_pid = *rx_pid.borrow_and_update();

                    debug!(
                        "{}",
                        t_with_args(
                            "fps-monitor-pid-switching",
                            &fluent_args!("pid" => new_pid.to_string())
                        )
                    );

                    match FpsProbe::new(new_pid, BPF_DATA) {
                        Ok(new_probe) => {
                            // 先挂载新的，再销毁旧的 —— 最小化帧丢失窗口
                            let old_probe = probe.replace(new_probe);

                            // 将 mio 切换到新探针的 RingBuf fd
                            if let Some(ref p) = probe {
                                let borrowed = unsafe { BorrowedFd::borrow_raw(p.ring_fd) };
                                let mut source = SourceFd::new(&borrowed).expect("mio SourceFd::new");
                                let _ = poll.registry().reregister(&mut source, token, Interest::READABLE);
                            }

                            // 显式 drop 旧探针（detach + unload）
                            drop(old_probe);

                            info!(
                                "{}",
                                t_with_args(
                                    "fps-monitor-pid-switched",
                                    &fluent_args!("pid" => new_pid.to_string())
                                )
                            );
                        }
                        Err(e) => {
                            warn!(
                                "{}",
                                t_with_args(
                                    "fps-monitor-pid-switch-failed",
                                    &fluent_args!("error" => e.to_string())
                                )
                            );
                        }
                    }
                }

                // ── 轮询 RingBuf ──
                let timeout = if probe.is_some() {
                    Some(Duration::from_millis(100))
                } else {
                    // 没有探针时用较长超时，减少空转
                    Some(Duration::from_millis(500))
                };

                if poll.poll(&mut events, timeout).is_err() {
                    std::thread::sleep(Duration::from_millis(10));
                    // 如果 poll 出错（比如 kernel fd 失效），重建 poll
                    poll = Poll::new().expect("mio Poll::new");
                    if let Some(ref p) = probe {
                        register(&mut poll, p);
                    }
                    continue;
                }

                // 读取帧数据
                if let Some(ref mut p) = probe {
                    p.poll_frames();

                    // 发送最新帧间隔
                    if let Some(delta) = p.latest_frametime() {
                        let fps = 1_000_000_000.0 / (delta.as_nanos() as f64);
                        if tx_clone
                            .send(DaemonEvent::FrameUpdate {
                                fps: fps as f32,
                                frame_delta_ns: delta.as_nanos() as u64,
                            })
                            .is_err()
                        {
                            return; // channel closed
                        }
                    }
                }
            }
        })?;

    info!("{}", t("fps-monitor-started"));

    // 永久挂起，保持 tokio runtime 存活
    std::future::pending::<()>().await;
    Ok(())
}
