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

use aya::{Ebpf, include_bytes_aligned, programs::TracePoint};
use aya::maps::{PerCpuArray, HashMap as BpfHashMap};
use aya::util::online_cpus;
use std::sync::mpsc::Sender;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use crate::common::DaemonEvent;
use crate::monitor::app_detect;
use log::{info, warn, debug};

use crate::i18n::{t, t_with_args};
use crate::fluent_args;

/// 获取与 BPF ktime_get_ns() 绝对对齐的单调时钟时间 (纳秒)
fn get_ktime_ns() -> u64 {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    (ts.tv_sec as u64) * 1_000_000_000 + (ts.tv_nsec as u64)
}

fn get_thread_tids(pid: u32) -> Vec<u32> {
    let task_dir = format!("/proc/{}/task", pid);
    let mut tids = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&task_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if let Ok(tid) = name.parse::<u32>() {
                    tids.push(tid);
                }
            }
        }
    }
    tids
}

pub async fn start_cpu_loop(tx: Sender<DaemonEvent>) -> Result<(), anyhow::Error> {
    static BPF_DATA: &[u8] = include_bytes_aligned!(env!("BPF_CPU_OBJ_PATH"));
    
    let bpf = Box::leak(Box::new(Ebpf::load(BPF_DATA)?));
    let program: &mut TracePoint = bpf.program_mut("handle_sched_switch").unwrap().try_into()?;
    program.load()?;
    program.attach("sched", "sched_switch")?;
    info!("{}", t("cpu-monitor-started"));

    // 获取准确的物理在线核心列表
    let online_cpus_list = online_cpus().map_err(|e| {
        anyhow::anyhow!("{}", t_with_args("cpu-monitor-online-cpus-failed", &fluent_args!("error" => format!("{:?}", e))))
    })?;
    let max_cpu_id = online_cpus_list.iter().copied().max().unwrap_or(0) as usize;
    info!("{}", t_with_args("cpu-monitor-online-cpus", &fluent_args!("cpus" => format!("{:?}", online_cpus_list))));

    let bpf_ptr = bpf as *mut Ebpf;

    let core_idle_map: PerCpuArray<_, u64> = PerCpuArray::try_from(
        unsafe { &mut *bpf_ptr }.map_mut("core_idle_time").unwrap()
    )?;
    let core_busy_map: PerCpuArray<_, u64> = PerCpuArray::try_from(
        unsafe { &mut *bpf_ptr }.map_mut("core_busy_time").unwrap()
    )?;
    let core_last_time_map: PerCpuArray<_, u64> = PerCpuArray::try_from(
        unsafe { &mut *bpf_ptr }.map_mut("core_last_time").unwrap()
    )?;
    let core_current_tid_map: PerCpuArray<_, u32> = PerCpuArray::try_from(
        unsafe { &mut *bpf_ptr }.map_mut("core_current_tid").unwrap()
    )?;
    let thread_run_map: BpfHashMap<_, u32, u64> = BpfHashMap::try_from(
        unsafe { &mut *bpf_ptr }.map_mut("thread_run_time").unwrap()
    )?;

    // TGID 级聚合运行时间 map
    let tgid_run_map: BpfHashMap<_, u32, u64> = BpfHashMap::try_from(
        unsafe { &mut *bpf_ptr }.map_mut("tgid_run_time").unwrap()
    )?;

    // 每核当前 TGID map (用于 pending delta 补偿)
    let core_current_tgid_map: PerCpuArray<_, u32> = PerCpuArray::try_from(
        unsafe { &mut *bpf_ptr }.map_mut("core_current_tgid").unwrap()
    )?;

    let shared_pid = Arc::new(AtomicU32::new(app_detect::get_current_pid() as u32));
    let pid_arc = shared_pid.clone();

    tokio::spawn(async move {
        let mut last_pid: u32 = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let current_pid = app_detect::get_current_pid() as u32;
            if current_pid != last_pid && current_pid > 0 {
                pid_arc.store(current_pid, Ordering::Relaxed);
                debug!("{}", t_with_args("cpu-monitor-fg-pid-updated", &fluent_args!(
                    "old" => last_pid.to_string(),
                    "new" => current_pid.to_string()
                )));
                last_pid = current_pid;
            }
        }
    });

    tokio::spawn(async move {
        // 根据最大 CPU ID 初始化历史记录向量，避免越界
        let mut last_idle_times = vec![0u64; max_cpu_id + 1];
        let mut last_busy_times = vec![0u64; max_cpu_id + 1];
        let mut last_check_time = get_ktime_ns();

        // TGID 级聚合数据：per-PID 的历史值
        let mut last_tgid_run: u64 = 0;
        let mut last_tgid_pid: u32 = 0; // 上一次采样时的前台 PID
        // 备用: 线程级数据 (当 TGID map 不可用时)
        let mut last_thread_run: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();

        let mut log_counter: u32 = 0;
        
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(200));
        
        loop {
            interval.tick().await;
            let now_ktime = get_ktime_ns();
            let real_delta_ns = now_ktime.saturating_sub(last_check_time);
            last_check_time = now_ktime;

            if real_delta_ns == 0 { continue; }

            let zero_key: u32 = 0;
            let per_cpu_idle_values = core_idle_map.get(&zero_key, 0);
            let per_cpu_busy_values = core_busy_map.get(&zero_key, 0);
            let per_cpu_last_time = core_last_time_map.get(&zero_key, 0);
            let per_cpu_current_tid = core_current_tid_map.get(&zero_key, 0);
            let per_cpu_current_tgid = core_current_tgid_map.get(&zero_key, 0);

            let mut core_utils = Vec::with_capacity(online_cpus_list.len());

            // 1. 全局单核利用率计算（带有实时状态补偿）
            for &cpu_id in &online_cpus_list {
                let idx = cpu_id as usize;
                
                let raw_idle = per_cpu_idle_values.as_ref().ok().and_then(|v| v.get(idx)).copied().unwrap_or(0);
                let raw_busy = per_cpu_busy_values.as_ref().ok().and_then(|v| v.get(idx)).copied().unwrap_or(0);
                let last_switch_time = per_cpu_last_time.as_ref().ok().and_then(|v| v.get(idx)).copied().unwrap_or(0);
                let current_tid = per_cpu_current_tid.as_ref().ok().and_then(|v| v.get(idx)).copied().unwrap_or(0);

                let mut adj_idle = raw_idle;
                let mut adj_busy = raw_busy;

                // 计算当前正在执行的任务积累但未触发 sched_switch 的时间
                let mut pending_delta = now_ktime.saturating_sub(last_switch_time);
                if pending_delta > 1_000_000_000 { 
                    pending_delta = 0; // 防御性保护，剔除极大异常值
                }

                if current_tid == 0 {
                    adj_idle += pending_delta;
                } else {
                    adj_busy += pending_delta;
                }

                let idle_diff = adj_idle.saturating_sub(last_idle_times[idx]);
                let busy_diff = adj_busy.saturating_sub(last_busy_times[idx]);
                let total_diff = idle_diff + busy_diff;

                let util = if total_diff > 0 {
                    (busy_diff as f32 / total_diff as f32).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                core_utils.push(util);
                last_idle_times[idx] = adj_idle;
                last_busy_times[idx] = adj_busy;
            }

            // 2. 前台应用利用率计算
            //    主路径: 使用 tgid_run_time map (TGID 级聚合)
            //    只需查询 1 个 key，不受 thread_run_time HASH 驱逐影响
            let foreground_max_util = {
                let fg_pid = shared_pid.load(Ordering::Relaxed);
                if fg_pid == 0 {
                    0.0_f32
                } else {
                    // PID 切换时重置 TGID 基线，避免跨进程的累计值比较
                    if fg_pid != last_tgid_pid {
                        last_tgid_run = 0;
                        last_tgid_pid = fg_pid;
                        // 同时清空线程级缓存（PID 变了，旧 TID 无意义）
                        last_thread_run.clear();
                    }

                    // ── 主路径: TGID 级聚合 ──
                    let tgid_util = compute_tgid_util(
                        fg_pid,
                        &tgid_run_map,
                        &per_cpu_current_tgid,
                        &per_cpu_last_time,
                        &online_cpus_list,
                        now_ktime,
                        real_delta_ns,
                        &mut last_tgid_run,
                    );

                    if let Some(util) = tgid_util {
                        util
                    } else {
                        // ── 降级路径: 逐 TID 遍历 (原始逻辑，作为 fallback) ──
                        compute_thread_level_util(
                            fg_pid,
                            &thread_run_map,
                            &core_current_tid_map,
                            &per_cpu_last_time,
                            &online_cpus_list,
                            now_ktime,
                            real_delta_ns,
                            &mut last_thread_run,
                        )
                    }
                }
            };

            log_counter += 1;
            if log_counter % 25 == 0 {
                let cores_str = core_utils.iter()
                    .map(|u| format!("{:.0}", u * 100.0))
                    .collect::<Vec<_>>()
                    .join(", ");
                
                debug!("{}", t_with_args("cpu-monitor-tick-log", &fluent_args!(
                    "cores" => cores_str,
                    "pid" => shared_pid.load(Ordering::Relaxed).to_string(),
                    "util" => format!("{:.1}", foreground_max_util * 100.0),
                    "threads" => last_thread_run.len().to_string(),
                    "delta" => (real_delta_ns / 1_000_000).to_string()
                )));
            }

            if tx.send(DaemonEvent::SystemLoadUpdate {
                core_utils,
                foreground_max_util,
            }).is_err() {
                warn!("{}", t("cpu-monitor-channel-closed"));
                break;
            }
        }
    });

    std::future::pending::<()>().await;
    Ok(())
}

/// 主路径: 使用 TGID 级聚合 map 计算前台进程的 CPU 利用率
///
/// 优势:
/// - 只需查询 1 个 key (TGID)，不依赖逐 TID 遍历
/// - tgid_run_time map 容量 1024，远够用（系统不会有 1024 个活跃进程）
/// - 完全规避 thread_run_time HASH 容量不足 / LRU 驱逐问题
///
/// 关键设计: 基线只保存 raw 值（不含 pending delta），避免 pending 累积漂移
///
/// 返回 Some(util) 表示成功，None 表示需要走降级路径
fn compute_tgid_util(
    fg_pid: u32,
    tgid_run_map: &BpfHashMap<&mut aya::maps::MapData, u32, u64>,
    per_cpu_current_tgid: &Result<aya::maps::PerCpuValues<u32>, aya::maps::MapError>,
    per_cpu_last_time: &Result<aya::maps::PerCpuValues<u64>, aya::maps::MapError>,
    online_cpus: &[u32],
    now_ktime: u64,
    real_delta_ns: u64,
    last_tgid_run: &mut u64,
) -> Option<f32> {
    // 读取 TGID 的累计运行时间 (BPF 侧只在 sched_switch 时更新)
    let raw_tgid_time = tgid_run_map.get(&fg_pid, 0).unwrap_or(0);
    
    // 如果 TGID 在 map 中完全不存在，且没有历史基线
    if raw_tgid_time == 0 && *last_tgid_run == 0 {
        return None;
    }

    // 计算当前 pending delta：正在核心上运行但还没经过 sched_switch 的时间
    // 这是一个瞬时快照值，每轮独立计算，不累积到基线中
    let mut current_pending: u64 = 0;
    if let Ok(per_cpu_tgids) = per_cpu_current_tgid.as_ref() {
        for &cpu_id in online_cpus {
            let idx = cpu_id as usize;
            let current_tgid = per_cpu_tgids.get(idx).copied().unwrap_or(0);
            
            if current_tgid == fg_pid {
                let last_switch = per_cpu_last_time.as_ref().ok()
                    .and_then(|v| v.get(idx)).copied().unwrap_or(0);
                let pending = now_ktime.saturating_sub(last_switch);
                if pending < 1_000_000_000 {
                    current_pending += pending;
                }
            }
        }
    }

    // 基线只用 raw 值（不含 pending），避免 pending 累积漂移
    // adj = raw + pending 只用于本轮差值计算
    let prev_raw = *last_tgid_run;
    *last_tgid_run = raw_tgid_time;  // 保存 raw，不保存 adj

    if prev_raw == 0 {
        // 第一次采样（PID 刚切换或首次运行），只建立基线
        return Some(0.0);
    }

    // raw 值是单调递增的（BPF 侧只做 += delta）
    // 如果 raw < prev_raw 说明 map 被重置或异常
    if raw_tgid_time < prev_raw {
        return Some(0.0);
    }

    // 总增量 = (raw 增量) + (当前 pending)
    // 注意：不减去"上次 pending"，因为上次的 pending 在这轮的 raw 增量中
    // 已经被 sched_switch 消化了。如果上次 pending 的线程还在跑（没有
    // sched_switch），那它的时间会同时出现在 raw 增量和 current_pending 中，
    // 但 raw 增量中不会包含它（因为没有 sched_switch 来触发累加）。
    // 所以：total_delta = raw_delta + current_pending 是正确的。
    let raw_delta = raw_tgid_time - prev_raw;
    let total_delta = raw_delta + current_pending;
    
    // 利用率 = 进程总 CPU 时间增量 / 实际墙钟时间
    let util = (total_delta as f32 / real_delta_ns as f32).clamp(0.0, 1.0);
    
    Some(util)
}

/// 降级路径: 逐 TID 遍历计算前台最重线程的利用率 (原始逻辑)
/// 增加防驱逐保护：如果 map 返回值 < 上次记录值，跳过该 TID
fn compute_thread_level_util(
    fg_pid: u32,
    thread_run_map: &BpfHashMap<&mut aya::maps::MapData, u32, u64>,
    core_current_tid_map: &PerCpuArray<&mut aya::maps::MapData, u32>,
    per_cpu_last_time: &Result<aya::maps::PerCpuValues<u64>, aya::maps::MapError>,
    online_cpus: &[u32],
    now_ktime: u64,
    real_delta_ns: u64,
    last_thread_run: &mut std::collections::HashMap<u32, u64>,
) -> f32 {
    let tids = get_thread_tids(fg_pid);
    let mut max_util: f32 = 0.0;
    let mut current_thread_run = std::collections::HashMap::with_capacity(tids.len());
    let zero_key: u32 = 0;

    let per_cpu_current_tid = core_current_tid_map.get(&zero_key, 0);

    for &tid in &tids {
        let mut adj_thread_time = thread_run_map.get(&tid, 0).unwrap_or(0);

        // 如果该线程正在某个核心上跑，补上它的 Pending Delta
        for &cpu_id in online_cpus {
            let idx = cpu_id as usize;
            let current_tid_on_core = per_cpu_current_tid.as_ref().ok()
                .and_then(|v| v.get(idx)).copied().unwrap_or(0);
            
            if current_tid_on_core == tid {
                let last_switch_time = per_cpu_last_time.as_ref().ok()
                    .and_then(|v| v.get(idx)).copied().unwrap_or(0);
                let pending_delta = now_ktime.saturating_sub(last_switch_time);
                if pending_delta < 1_000_000_000 {
                    adj_thread_time += pending_delta;
                }
            }
        }

        current_thread_run.insert(tid, adj_thread_time);

        if let Some(&last_run) = last_thread_run.get(&tid) {
            // 防驱逐保护：如果新值 < 旧值，说明 HASH map 条目被驱逐后
            // 重新创建，数据不连续，跳过此 TID 本轮的计算
            if adj_thread_time >= last_run {
                let thread_delta = adj_thread_time - last_run;
                let util = (thread_delta as f32 / real_delta_ns as f32).clamp(0.0, 1.0);
                if util > max_util {
                    max_util = util;
                }
            }
        }
    }
    
    *last_thread_run = current_thread_run;
    max_util
}