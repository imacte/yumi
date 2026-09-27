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

use crate::{
    common::DaemonEvent,
    monitor::{
        app_detect,
        sampling::{FrameClock, FrameSample},
    },
    utils::get_ktime_ns,
};
use anyhow::{Context, Result};
use aya::{
    Ebpf, EbpfError,
    maps::{
        MapData, MapError, PerfEventArray, RingBuf,
        perf::{PerfEvent, PerfEventArrayBuffer},
    },
    programs::{
        UProbe,
        uprobe::{UProbeAttachLocation, UProbeAttachPoint, UProbeLinkId, UProbeScope},
    },
};
use log::{info, warn};
use mio::{Events, Interest, Poll, Registry, Token, unix::SourceFd};
use std::{
    collections::BTreeMap,
    num::NonZeroU32,
    os::fd::AsRawFd,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};

const LIBGUI_PATH: &str = "/system/lib64/libgui.so";
const SYMBOL_SHORT: &str = "_ZN7android7Surface11queueBufferEP19ANativeWindowBufferi";
const SYMBOL_LONG: &str =
    "_ZN7android7Surface11queueBufferEP19ANativeWindowBufferiPNS_24SurfaceQueueBufferOutputE";

enum Transport {
    Ring(RingBuf<MapData>),
    Perf {
        array: PerfEventArray<MapData>,
        buffers: BTreeMap<u32, PerfEventArrayBuffer<MapData>>,
    },
}

impl Transport {
    fn register(&mut self, registry: &Registry) -> Result<()> {
        if let Self::Ring(ring) = self {
            registry.register(
                &mut SourceFd(&ring.as_raw_fd()),
                Token(0),
                Interest::READABLE,
            )?;
        }
        self.refresh_cpus(registry)
    }

    fn refresh_cpus(&mut self, registry: &Registry) -> Result<()> {
        if let Self::Perf { array, buffers } = self {
            let online = aya::util::online_cpus().map_err(|(_, error)| error)?;
            let offline: Vec<_> = buffers
                .keys()
                .filter(|cpu| !online.contains(cpu))
                .copied()
                .collect();
            for cpu in offline {
                if let Some(buffer) = buffers.remove(&cpu) {
                    registry.deregister(&mut SourceFd(&buffer.as_raw_fd()))?;
                }
            }
            for cpu in online {
                if let std::collections::btree_map::Entry::Vacant(entry) = buffers.entry(cpu) {
                    let buffer = array
                        .open(cpu, Some(8))
                        .with_context(|| format!("open FPS perf buffer on CPU {cpu}"))?;
                    registry.register(
                        &mut SourceFd(&buffer.as_raw_fd()),
                        Token(cpu as usize + 1),
                        Interest::READABLE,
                    )?;
                    entry.insert(buffer);
                }
            }
        }
        Ok(())
    }

    fn drain(&mut self) -> (Vec<FrameSample>, u64) {
        let mut frames = Vec::new();
        let mut lost = 0;
        match self {
            Self::Ring(ring) => {
                while let Some(data) = ring.next() {
                    if let Some(frame) = FrameSample::decode(&data, &[]) {
                        frames.push(frame);
                    }
                }
            }
            Self::Perf { buffers, .. } => {
                for buffer in buffers.values_mut() {
                    buffer.for_each(|event| match event {
                        PerfEvent::Sample { head, tail } => {
                            if let Some(frame) = FrameSample::decode(head, tail) {
                                frames.push(frame);
                            }
                        }
                        PerfEvent::Lost { count } => lost += count,
                    });
                }
            }
        }
        // Perf buffers are per CPU; merge by kernel timestamp before computing
        // intervals for a render thread that migrated between cores.
        frames.sort_unstable_by_key(|frame| frame.ktime_ns);
        (frames, lost)
    }
}

struct FpsManager {
    bpf: Ebpf,
    transport: Transport,
    link: Option<UProbeLinkId>,
    current_pid: u32,
    clock: FrameClock,
}

fn ring_unsupported(error: &EbpfError) -> bool {
    matches!(error, EbpfError::MapError(MapError::CreateError { name, io_error })
        if name == "RING_BUF" && matches!(io_error.raw_os_error(),
            Some(libc::EINVAL) | Some(libc::EOPNOTSUPP) | Some(libc::ENOSYS)))
}

impl FpsManager {
    fn new() -> Result<Self> {
        let (mut bpf, use_ring) = match Ebpf::load(aya::include_bytes_aligned!(concat!(
            env!("OUT_DIR"),
            "/fps-ring.ebpf"
        ))) {
            Ok(bpf) => (bpf, true),
            Err(error) if ring_unsupported(&error) => {
                warn!("[FPS Monitor] RingBuf unavailable: {error:?}; trying PerfEventArray");
                let bpf = Ebpf::load(aya::include_bytes_aligned!(concat!(
                    env!("OUT_DIR"),
                    "/fps-perf.ebpf"
                )))
                .with_context(|| {
                    format!("load FPS perf fallback after RingBuf failure: {error:?}")
                })?;
                (bpf, false)
            }
            Err(error) => return Err(error).context("load FPS RingBuf object"),
        };
        let program: &mut UProbe = bpf
            .program_mut("handle_frame")
            .context("missing handle_frame")?
            .try_into()?;
        program.load().context("load FPS uprobe")?;
        let transport = if use_ring {
            Transport::Ring(
                bpf.take_map("RING_BUF")
                    .context("missing RING_BUF")?
                    .try_into()?,
            )
        } else {
            Transport::Perf {
                array: bpf
                    .take_map("FRAME_EVENTS")
                    .context("missing FRAME_EVENTS")?
                    .try_into()?,
                buffers: BTreeMap::new(),
            }
        };
        Ok(Self {
            bpf,
            transport,
            link: None,
            current_pid: 0,
            clock: FrameClock::default(),
        })
    }

    fn switch_pid(&mut self, new_pid: u32) -> Result<()> {
        self.current_pid = 0;
        self.clock.reset(get_ktime_ns());
        let program: &mut UProbe = self
            .bpf
            .program_mut("handle_frame")
            .context("missing handle_frame")?
            .try_into()?;
        if let Some(link) = self.link.take() {
            program.detach(link).context("detach old FPS probe")?;
        }
        let Some(pid) = NonZeroU32::new(new_pid) else {
            return Ok(());
        };
        let scope = UProbeScope::OneProcess(pid);
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
            })
            .with_context(|| format!("attach FPS probe to PID {new_pid}"))?;
        self.link = Some(link);
        self.current_pid = new_pid;
        info!("[FPS Monitor] Attached to PID {new_pid}");
        Ok(())
    }
}

// This function owns the entire worker. Returning an error reaches the monitor
// supervisor; there are no detached PID watchers or permanently pending tasks.
pub fn start_fps_loop(tx: Sender<DaemonEvent>) -> Result<()> {
    let mut manager = FpsManager::new()?;
    let mut poll = Poll::new()?;
    manager
        .transport
        .register(poll.registry())
        .context("register FPS event buffers")?;
    let backend = match &manager.transport {
        Transport::Ring(_) => "RingBuf",
        _ => "PerfEventArray",
    };
    info!("[FPS Monitor] {backend} initialized; waiting for target PID");
    let mut events = Events::with_capacity(64);
    let mut target_pid = 0;
    let mut next_attach = Instant::now();
    let mut next_cpu_refresh = Instant::now() + Duration::from_secs(1);
    loop {
        let now = Instant::now();
        let pid = app_detect::get_current_pid().max(0) as u32;
        if pid != target_pid {
            target_pid = pid;
            next_attach = now;
        }
        if manager.current_pid != target_pid && now >= next_attach {
            if tx.send(DaemonEvent::FpsProbeUnavailable).is_err() {
                return Ok(());
            }
            if let Err(error) = manager.switch_pid(target_pid) {
                warn!("[FPS Monitor] {error:#}; retrying in 2 seconds");
            }
            next_attach = now + Duration::from_secs(2);
        }
        if now >= next_cpu_refresh {
            manager.transport.refresh_cpus(poll.registry())?;
            next_cpu_refresh = now + Duration::from_secs(1);
        }
        if let Err(error) = poll.poll(&mut events, Some(Duration::from_millis(100))) {
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error).context("poll FPS event buffers");
        }
        let (frames, lost) = manager.transport.drain();
        let ktime = get_ktime_ns();
        if lost > 0 {
            warn!("[FPS Monitor] Lost {lost} perf events; resetting frame baseline");
            manager.clock.reset(ktime);
        }
        for frame in frames {
            if manager.current_pid == 0 || frame.pid != manager.current_pid {
                continue;
            }
            if let Some(frame_delta_ns) = manager.clock.ingest(frame.ktime_ns, ktime) {
                let sampled_at = Instant::now() - Duration::from_nanos(ktime - frame.ktime_ns);
                if tx
                    .send(DaemonEvent::FrameUpdate {
                        frame_delta_ns,
                        pid: frame.pid,
                        sampled_at,
                    })
                    .is_err()
                {
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallback_only_for_unsupported_ring_creation() {
        let error = |name: &str, errno| {
            EbpfError::MapError(MapError::CreateError {
                name: name.to_string(),
                io_error: std::io::Error::from_raw_os_error(errno),
            })
        };
        assert!(ring_unsupported(&error("RING_BUF", libc::EINVAL)));
        assert!(ring_unsupported(&error("RING_BUF", libc::EOPNOTSUPP)));
        assert!(!ring_unsupported(&error("RING_BUF", libc::EPERM)));
        assert!(!ring_unsupported(&error("RING_BUF", libc::ENOMEM)));
        assert!(!ring_unsupported(&error("OTHER", libc::EINVAL)));
    }
}
