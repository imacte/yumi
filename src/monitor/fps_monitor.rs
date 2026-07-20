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

use std::collections::{HashMap, VecDeque};
use std::mem::size_of;
use std::num::NonZeroU32;
use std::os::unix::io::{AsRawFd, RawFd};
use std::ptr;
use std::sync::mpsc::Sender;
use std::time::Duration;

use aya::Ebpf;
use aya::maps::RingBuf;
use aya::programs::UProbe;
use aya::programs::uprobe::{UProbeAttachLocation, UProbeAttachPoint, UProbeScope};
use log::{debug, info, warn};
use mio::{Events, Interest, Poll, Token, unix::SourceFd};
use tokio::sync::watch;

use crate::common::DaemonEvent;
use crate::fluent_args;
use crate::i18n::{t, t_with_args};
use crate::monitor::app_detect;

// ─── 常量 ────────────────────────────────────────────────

/// uprobe 符号名（短签名）
const SYMBOL_SHORT: &str = "_ZN7android7Surface11queueBufferEP19ANativeWindowBufferi";
/// uprobe 符号名（长签名，fallback）
const SYMBOL_LONG: &str =
    "_ZN7android7Surface11queueBufferEP19ANativeWindowBufferiPNS_24SurfaceQueueBufferOutputE";
const LIBGUI_PATH: &str = "/system/lib64/libgui.so";

/// RingBuf 输出的帧时间戳事件（与 yumi-ebpf 的 FrameTimestampEvent 内存布局一致）
#[repr(C)]
struct FrameTimestampEvent {
    pid: u32,
    ktime_ns: u64,
}

const MIN_FRAME_NS: u64 = 1_000_000;
const MAX_FRAME_NS: u64 = 200_000_000;
const FRAMETIME_WINDOW: usize = 144;

// ─── ProbeState：单个 PID 的帧统计 ─────────────────────

struct ProbeState {
    last_ktime_ns: Option<u64>,
    frametimes: VecDeque<Duration>,
}

impl ProbeState {
    fn new() -> Self {
        Self { last_ktime_ns: None, frametimes: VecDeque::with_capacity(FRAMETIME_WINDOW) }
    }

    fn ingest(&mut self, ktime_ns: u64) {
        if let Some(last_ns) = self.last_ktime_ns {
            let delta_ns = ktime_ns.saturating_sub(last_ns);
            if (MIN_FRAME_NS..=MAX_FRAME_NS).contains(&delta_ns) {
                if self.frametimes.len() >= FRAMETIME_WINDOW {
                    self.frametimes.pop_back();
                }
                self.frametimes.push_front(Duration::from_nanos(delta_ns));
            }
        }
        self.last_ktime_ns = Some(ktime_ns);
    }

    fn latest_frametime(&self) -> Option<Duration> {
        self.frametimes.front().copied()
    }
}

// ─── FpsManager：单 eBPF 实例，多 PID attach ─────────────

struct FpsManager {
    bpf: Ebpf,
    ring_fd: RawFd,
    /// 当前活跃 PID → UProbeLinkId
    links: HashMap<u32, aya::programs::uprobe::UProbeLinkId>,
    /// 当前活跃 PID → 帧统计
    states: HashMap<u32, ProbeState>,
    /// 当前关注的目标 PID（最近一次 attach 的 PID）
    current_pid: u32,
}

impl FpsManager {
    /// 加载 eBPF 程序（只执行一次），获取 RingBuf fd
    fn new() -> Result<Self, anyhow::Error> {
        #[cfg(debug_assertions)]
        let mut bpf = Ebpf::load(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/ebpf_target/bpfel-unknown-none/debug/yumi-ebpf"
        )))?;
        #[cfg(not(debug_assertions))]
        let mut bpf = Ebpf::load(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/ebpf_target/bpfel-unknown-none/release/yumi-ebpf"
        )))?;

        let program: &mut UProbe = bpf.program_mut("handle_frame").unwrap().try_into()?;
        program.load()?;

        let ring_fd = {
            let ring_map = bpf.map_mut("RING_BUF").expect("RING_BUF not found");
            let ring = RingBuf::try_from(ring_map).expect("RingBuf::try_from");
            ring.as_raw_fd()
        };

        Ok(Self {
            bpf,
            ring_fd,
            links: HashMap::new(),
            states: HashMap::new(),
            current_pid: 0,
        })
    }

    /// 切换到新 PID：detach 旧 PID + attach 新 PID
    fn switch_pid(&mut self, new_pid: u32) -> Result<(), anyhow::Error> {
        if new_pid == self.current_pid {
            return Ok(());
        }

        // detach 旧 PID
        if self.current_pid > 0 {
            if let Some(link_id) = self.links.remove(&self.current_pid) {
                let program: &mut UProbe =
                    self.bpf.program_mut("handle_frame").unwrap().try_into()?;
                let _ = program.detach(link_id);
            }
        }

        // attach 新 PID
        let pid_i32 = new_pid as i32;
        let scope =
            UProbeScope::OneProcess(NonZeroU32::new(new_pid).expect("pid must be > 0"));

        let program: &mut UProbe = self.bpf.program_mut("handle_frame").unwrap().try_into()?;
        let link = program
            .attach(
                UProbeAttachPoint::from(UProbeAttachLocation::from(SYMBOL_SHORT)),
                LIBGUI_PATH,
                scope,
            )
            .or_else(|_| {
                program.attach(
                    UProbeAttachPoint::from(UProbeAttachLocation::from(SYMBOL_LONG)),
                    LIBGUI_PATH,
                    scope,
                )
            })?;

        self.links.insert(new_pid, link);
        self.states.entry(new_pid).or_insert_with(ProbeState::new);
        self.current_pid = new_pid;

        info!(
            "{}",
            t_with_args("fps-monitor-attached", &fluent_args!("pid" => pid_i32.to_string()))
        );
        Ok(())
    }

    /// 从共享 RingBuf 读取帧事件，按 PID 分派
    fn poll_frames(&mut self) {
        let ring_map = self.bpf.map_mut("RING_BUF").expect("RING_BUF not found");
        let mut ring = RingBuf::try_from(ring_map).expect("RingBuf::try_from failed");

        while let Some(data) = ring.next() {
            if data.len() < size_of::<FrameTimestampEvent>() {
                continue;
            }
            let event =
                unsafe { ptr::read_unaligned(data.as_ptr().cast::<FrameTimestampEvent>()) };

            if let Some(state) = self.states.get_mut(&event.pid) {
                state.ingest(event.ktime_ns);
            }
        }
    }

    /// 当前 PID 的最新帧间隔
    fn latest_frametime(&self) -> Option<Duration> {
        self.states.get(&self.current_pid)?.latest_frametime()
    }

    fn has_active_probe(&self) -> bool {
        self.current_pid > 0
    }
}

// ─── 主入口 ──────────────────────────────────────────────

pub async fn start_fps_loop(tx: Sender<DaemonEvent>) -> Result<(), anyhow::Error> {
    info!("{}", t("fps-monitor-init"));

    let initial_pid = app_detect::get_current_pid();
    let (tx_pid, mut rx_pid) = watch::channel(initial_pid);

    // PID 检测任务
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

    let tx_clone = tx.clone();
    std::thread::Builder::new()
        .name("fps_probe".into())
        .spawn(move || {
            let mut manager = match FpsManager::new() {
                Ok(m) => m,
                Err(e) => {
                    warn!(
                        "{}",
                        t_with_args(
                            "fps-monitor-attach-failed-initial",
                            &fluent_args!("error" => e.to_string())
                        )
                    );
                    return;
                }
            };

            // 初始 attach
            if initial_pid > 0 {
                if let Err(e) = manager.switch_pid(initial_pid as u32) {
                    warn!(
                        "{}",
                        t_with_args(
                            "fps-monitor-attach-failed-initial",
                            &fluent_args!("error" => e.to_string())
                        )
                    );
                }
            } else {
                info!("{}", t("fps-monitor-init-no-pid"));
            }

            // mio 轮询（只创建一次）
            let mut poll = Poll::new().expect("mio Poll::new");
            let mut events = Events::with_capacity(64);
            let token = Token(0);

            // 注册 RingBuf fd（只注册一次，不会变）
            if manager.has_active_probe() {
                let fd = manager.ring_fd;
                let mut source = SourceFd(&fd);
                poll.registry()
                    .register(&mut source, token, Interest::READABLE)
                    .expect("mio register");
            }

            loop {
                // ── PID 变化 ──
                if rx_pid.has_changed().unwrap_or(false) {
                    let new_pid = *rx_pid.borrow_and_update() as u32;

                    // 无需重新注册 Poll——RingBuf fd 不变
                    if let Err(e) = manager.switch_pid(new_pid) {
                        warn!(
                            "{}",
                            t_with_args(
                                "fps-monitor-pid-switch-failed",
                                &fluent_args!("error" => e.to_string())
                            )
                        );
                    }
                }

                // ── 轮询 ──
                let timeout = if manager.has_active_probe() {
                    Some(Duration::from_millis(100))
                } else {
                    Some(Duration::from_millis(500))
                };

                // mio poll error 只意味着被信号打断，sleep 后重试即可
                if poll.poll(&mut events, timeout).is_err() {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }

                manager.poll_frames();

                if let Some(delta) = manager.latest_frametime() {
                    if tx_clone
                        .send(DaemonEvent::FrameUpdate {
                            frame_delta_ns: delta.as_nanos() as u64,
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        })?;

    info!("{}", t("fps-monitor-started"));
    std::future::pending::<()>().await;
    Ok(())
}
