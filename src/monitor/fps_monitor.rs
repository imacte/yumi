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


use aya::{Ebpf, include_bytes_aligned, programs::UProbe, maps::perf::AsyncPerfEventArray};
use aya::util::online_cpus;
use bytes::BytesMut;
use std::sync::mpsc::Sender;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use crate::common::DaemonEvent;
use crate::monitor::app_detect;
use log::{info, debug};

use crate::i18n::{t, t_with_args};
use crate::fluent_args;

pub async fn start_fps_loop(tx: Sender<DaemonEvent>) -> Result<(), anyhow::Error> {
    static BPF_DATA: &[u8] = include_bytes_aligned!(env!("BPF_FPS_OBJ_PATH"));
    info!("{}", t("fps-monitor-init"));

    let bpf = Box::leak(Box::new(Ebpf::load(BPF_DATA)?));
    let program: &mut UProbe = bpf.program_mut("handle_frame").unwrap().try_into()?;
    program.load()?;
    
    let syms = [
        "_ZN7android7Surface11queueBufferEP19ANativeWindowBufferi", 
        "_ZN7android7Surface11queueBufferEP19ANativeWindowBufferiPNS_24SurfaceQueueBufferOutputE",
        "_ZN7android16BufferQueueProducer11queueBufferEiRKNS_10IGraphicBufferProducer10QueueBufferInputEPNS1_11QueueBufferOutputE",
        "_ZN7android16BufferQueueProducer11queueBufferEiRKNS_22IGraphicBufferProducer10QueueBufferInputEPNS1_11QueueBufferOutputE"
    ];
    
    let mut attached_count = 0;
    for sym in syms {
        if program.attach(Some(sym), 0, "/system/lib64/libgui.so", None).is_ok() {
            info!("{}", t_with_args("fps-monitor-attached", &fluent_args!("sym" => sym)));
            attached_count += 1;
        }
    }

    if attached_count == 0 {
        return Err(anyhow::anyhow!("{}", t("fps-monitor-attach-failed")));
    }

    let bpf_ptr = bpf as *mut Ebpf;
    let mut target_pid_arr = if let Some(map) = unsafe { &mut *bpf_ptr }.map_mut("target_pid") {
        aya::maps::Array::<_, u32>::try_from(map).ok()
    } else {
        None
    };

    let has_kernel_filter = target_pid_arr.is_some();
    let map = unsafe { &mut *bpf_ptr }.map_mut("frame_events").expect("frame_events map not found");
    let mut perf_array = AsyncPerfEventArray::try_from(map)?;

    let shared_pid = Arc::new(AtomicU32::new(app_detect::get_current_pid() as u32));

    // 独立轻量任务：仅负责更新内核 BPF Map 和 PID 原子变量
    {
        let pid_arc = shared_pid.clone();
        tokio::spawn(async move {
            let mut last_pid: u32 = 0;
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let current_pid = app_detect::get_current_pid() as u32;

                if current_pid != last_pid && current_pid > 0 {
                    pid_arc.store(current_pid, Ordering::Relaxed);
                    if let Some(arr) = &mut target_pid_arr {
                        let _ = arr.set(0, current_pid, 0);
                        debug!("{}", t_with_args("fps-monitor-pid-filter-updated", &fluent_args!("old" => last_pid.to_string(), "new" => current_pid.to_string())));
                    }
                    last_pid = current_pid;
                }
            }
        });
    }

    for cpu_id in online_cpus().map_err(|e| anyhow::anyhow!("CPU access error: {:?}", e))? {
        let mut buf = perf_array.open(cpu_id, None)?;
        let tx_clone = tx.clone();
        let pid_arc = shared_pid.clone();

        tokio::spawn(async move {
            let mut buffers = vec![BytesMut::with_capacity(1024); 10];
            loop {
                match buf.read_events(&mut buffers).await {
                    Ok(events) => {
                        for i in 0..events.read {
                            let data = &buffers[i];
                            if data.len() < 12 { continue; }

                            let event_pid = u32::from_ne_bytes(data[0..4].try_into().unwrap());
                            let delta = u64::from_ne_bytes(data[4..12].try_into().unwrap());

                            if delta == 0 || event_pid != pid_arc.load(Ordering::Relaxed) { continue; }

                            let fps = 1_000_000_000.0 / (delta as f64);

                            // 不再检查 package_name 或在热路径中做 String clone
                            // FAS 控制器已通过 set_game() 持有自己的包名缓存
                            // 只检查 PID 是否有效即可
                            if event_pid == 0 { continue; }

                            // FrameUpdate 不再携带 package_name，避免 144fps 下每帧一次 String clone
                            if tx_clone.send(DaemonEvent::FrameUpdate {
                                fps: fps as f32,
                                frame_delta_ns: delta,
                            }).is_err() {
                                return;
                            }
                        }
                    }
                    Err(_) => {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                }
            }
        });
    }

    info!("{}", t_with_args("fps-monitor-started", &fluent_args!("filter" => if has_kernel_filter { "active" } else { "disabled" })));
    
    std::future::pending::<()>().await;
    Ok(())
}