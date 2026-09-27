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
#![no_std]
#![no_main]
use aya_ebpf::{
    helpers::bpf_ktime_get_ns,
    macros::{map, tracepoint},
    maps::{LruHashMap, PerCpuArray},
    programs::TracePointContext,
};
// sched_switch 参数布局 (offset → field)
//  0: pad            u64
//  8: prev_comm      [u8; 16]
// 24: prev_pid       i32
// 28: prev_prio      i32
// 32: prev_state     i64
// 40: next_comm      [u8; 16]
// 56: next_pid       i32
// 60: next_prio      i32
const OFF_PREV_PID: usize = 24;
const OFF_NEXT_PID: usize = 56;

/// 每个核心的上次切换时间戳
#[map]
static CORE_LAST_TIME: PerCpuArray<u64> = PerCpuArray::with_max_entries(1, 0);

/// 每个核心累计 Idle 时间 (ns)
#[map]
static CORE_IDLE_TIME: PerCpuArray<u64> = PerCpuArray::with_max_entries(1, 0);

/// 每个核心累计 Busy 时间 (ns)
#[map]
static CORE_BUSY_TIME: PerCpuArray<u64> = PerCpuArray::with_max_entries(1, 0);

/// 每个核心当前运行的 TID
#[map]
static CORE_CURRENT_TID: PerCpuArray<u32> = PerCpuArray::with_max_entries(1, 0);

/// 线程级运行时间 (TID → ns)
#[map]
static THREAD_RUN_TIME: LruHashMap<u32, u64> = LruHashMap::with_max_entries(32768, 0);

const ZERO_KEY: u32 = 0;
const NS_10_SEC: u64 = 10_000_000_000;

#[tracepoint]
pub fn handle_sched_switch(ctx: TracePointContext) -> u32 {
    match try_handle_sched_switch(&ctx) {
        Ok(ret) => ret,
        Err(ret) => ret as u32,
    }
}

fn try_handle_sched_switch(ctx: &TracePointContext) -> Result<u32, i64> {
    let now = unsafe { bpf_ktime_get_ns() };

    let prev_tid: i32 = unsafe { ctx.read_at(OFF_PREV_PID)? };
    let next_tid: i32 = unsafe { ctx.read_at(OFF_NEXT_PID)? };

    // ── 计算上一个任务的耗时并累加 ──
    if let Some(last_ts_ptr) = CORE_LAST_TIME.get_ptr_mut(ZERO_KEY) {
        let last_ts = unsafe { *last_ts_ptr };
        let delta = now.saturating_sub(last_ts);

        if last_ts != 0 && delta > 0 && delta < NS_10_SEC {
            if prev_tid == 0 {
                // Idle 时间
                if let Some(idle_ptr) = CORE_IDLE_TIME.get_ptr_mut(ZERO_KEY) {
                    unsafe {
                        *idle_ptr += delta;
                    }
                }
            } else {
                // Busy 时间
                if let Some(busy_ptr) = CORE_BUSY_TIME.get_ptr_mut(ZERO_KEY) {
                    unsafe {
                        *busy_ptr += delta;
                    }
                }

                // 线程级累计
                add_to_hash(&THREAD_RUN_TIME, prev_tid as u32, delta);
            }
        }
    }

    // ── 更新当前核心状态 ──
    update_percpu(&CORE_LAST_TIME, &ZERO_KEY, &now);
    update_percpu(&CORE_CURRENT_TID, &ZERO_KEY, &(next_tid as u32));

    Ok(0)
}

/// 向 HashMap 累加 delta（查找然后 +=，不存在则 insert）
fn add_to_hash(map: &LruHashMap<u32, u64>, key: u32, delta: u64) {
    if let Some(ptr) = map.get_ptr_mut(&key) {
        unsafe {
            *ptr += delta;
        }
    } else {
        let _ = map.insert(&key, &delta, 0);
    }
}

/// 更新 PerCpuArray 中 key 对应的值
fn update_percpu<T: Copy>(map: &PerCpuArray<T>, key: &u32, val: &T) {
    if let Some(ptr) = map.get_ptr_mut(*key) {
        unsafe {
            *ptr = *val;
        }
    }
}

// ────────────────────────────────────────────────

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
