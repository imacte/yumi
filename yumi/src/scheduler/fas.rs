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

use crate::monitor::config::{
    FasRulesConfig, ClusterProfile, PerAppProfile,
};
use std::fs::{self, File, OpenOptions};
use std::io::{Write, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;
use log::{info, warn};

use crate::i18n::{t, t_with_args};
use crate::fluent_args;

// ════════════════════════════════════════════════════════════════
//  FastWriter — 带去重 + unmount 的 sysfs 写入器
// ════════════════════════════════════════════════════════════════

pub struct FastWriter {
    file: Option<File>,
    last_value: Option<u32>,
    buf: [u8; 20],
    path: PathBuf,
}

impl FastWriter {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let path_ref = path.as_ref();
        Self::try_unmount(path_ref);
        let _ = crate::utils::enable_perm(path_ref);
        let file = OpenOptions::new().write(true).open(path_ref)
            .map_err(|e| log::error!("{}", t_with_args("fas-open-failed", &fluent_args!("path" => path_ref.display().to_string(), "error" => e.to_string()))))
            .ok();
        Self { file, last_value: None, buf: [0u8; 20], path: path_ref.to_path_buf() }
    }

    fn try_unmount(path: &Path) {
        if let Some(path_str) = path.to_str() {
            if let Ok(cpath) = std::ffi::CString::new(path_str) {
                let ret = unsafe { libc::umount2(cpath.as_ptr(), libc::MNT_DETACH) };
                if ret != 0 {
                    let errno = std::io::Error::last_os_error();
                    if errno.raw_os_error() != Some(libc::EINVAL)
                        && errno.raw_os_error() != Some(libc::ENOENT) {
                        log::debug!("{}", t_with_args("fas-umount2-failed", &fluent_args!("path" => path_str, "error" => errno.to_string())));
                    }
                }
            }
        }
    }

    pub fn re_unmount(&self) { Self::try_unmount(&self.path); }

    #[allow(dead_code)]
    pub fn write_value(&mut self, value: u32) {
        if self.last_value == Some(value) { return; }
        self.do_write(value);
    }

    pub fn write_value_force(&mut self, value: u32) {
        self.do_write(value);
    }

    pub fn invalidate(&mut self) { self.last_value = None; }
    pub fn is_valid(&self) -> bool { self.file.is_some() }

    fn do_write(&mut self, value: u32) {
        if let Some(file) = &mut self.file {
            let len = Self::u32_to_buf(value, &mut self.buf);
            let _ = file.seek(SeekFrom::Start(0));
            if let Err(e) = file.write_all(&self.buf[..len]) {
                log::error!("{}", t_with_args("fas-write-freq-failed", &fluent_args!("freq" => value.to_string(), "error" => e.to_string())));
            }
            self.last_value = Some(value);
        }
    }

    fn u32_to_buf(mut v: u32, buf: &mut [u8; 20]) -> usize {
        if v == 0 { buf[0] = b'0'; buf[1] = b'\n'; return 2; }
        let mut pos = 18;
        while v > 0 { buf[pos] = b'0' + (v % 10) as u8; v /= 10; pos -= 1; }
        let start = pos + 1;
        let digit_len = 19 - start;
        buf.copy_within(start..19, 0);
        buf[digit_len] = b'\n';
        digit_len + 1
    }
}

// ════════════════════════════════════════════════════════════════
//  PolicyController — 单个 cpufreq policy 的频率控制
// ════════════════════════════════════════════════════════════════

pub struct PolicyController {
    pub max_writer: FastWriter,
    pub min_writer: FastWriter,
    pub available_freqs: Vec<u32>,
    cached_ratios: Vec<f32>,
    pub current_freq: u32,
    pub policy_id: usize,
    pub cluster_profile: ClusterProfile,
    pub freq_hold_frames: u32,
    freq_min: f32,
    freq_max: f32,

    verify_freq: Option<u32>,
    verify_timer: Instant,

    pub ignore_write: bool,
}

impl PolicyController {
    pub fn new(
        max_writer: FastWriter,
        min_writer: FastWriter,
        available_freqs: Vec<u32>,
        policy_id: usize,
        cluster_profile: ClusterProfile,
        current_freq: u32,
    ) -> Self {
        let freq_min = *available_freqs.first().unwrap_or(&0) as f32;
        let freq_max = *available_freqs.last().unwrap_or(&1) as f32;
        let range = (freq_max - freq_min).max(1.0);
        let cached_ratios: Vec<f32> = available_freqs.iter()
            .map(|&f| (f as f32 - freq_min) / range)
            .collect();
        Self {
            max_writer, min_writer, available_freqs, cached_ratios,
            current_freq, policy_id, cluster_profile,
            freq_hold_frames: 0, freq_min, freq_max,
            verify_freq: None,
            verify_timer: Instant::now(),
            ignore_write: false,
        }
    }

    pub fn find_nearest_freq(&self, target_ratio: f32) -> u32 {
        let idx = self.cached_ratios.partition_point(|&r| r < target_ratio);
        if idx == 0 {
            self.available_freqs[0]
        } else if idx >= self.available_freqs.len() {
            *self.available_freqs.last().unwrap()
        } else {
            let lo = idx - 1;
            let hi = idx;
            if (self.cached_ratios[hi] - target_ratio).abs()
                < (self.cached_ratios[lo] - target_ratio).abs()
            { self.available_freqs[hi] } else { self.available_freqs[lo] }
        }
    }

    pub fn current_ratio(&self) -> f32 {
        (self.current_freq as f32 - self.freq_min) / (self.freq_max - self.freq_min).max(1.0)
    }

    /// 锁频写入 (min=max)，用于关键 cluster
    pub fn apply_freq_locked(&mut self, target_freq: u32) {
        if self.ignore_write { return; }
        if target_freq >= self.current_freq {
            self.max_writer.write_value_force(target_freq);
            self.min_writer.write_value_force(target_freq);
        } else {
            self.min_writer.write_value_force(target_freq);
            self.max_writer.write_value_force(target_freq);
        }
        self.current_freq = target_freq;
        self.freq_hold_frames = 2;
        self.do_verify_freq(target_freq);
    }

    /// 松散写入 (min=lowest, max=target)，用于非关键 cluster 或负载较低时
    #[allow(dead_code)]
    pub fn apply_freq_relaxed(&mut self, target_freq: u32) {
        if self.ignore_write { return; }
        let min_f = self.available_freqs[0];
        self.min_writer.write_value_force(min_f);
        self.max_writer.write_value_force(target_freq);
        self.current_freq = target_freq;
        self.freq_hold_frames = 2;
    }

    fn do_verify_freq(&mut self, write_freq: u32) {
        // [Fix] 缩短校验间隔：3秒→1.5秒，更快发现内核频率覆写
        // 日志中104次freq mismatch说明内核覆写非常频繁
        let verify_interval = std::time::Duration::from_millis(1500);
        if self.verify_timer.elapsed() >= verify_interval {
            self.verify_timer = Instant::now();
            if let Some(expected) = self.verify_freq {
                if let Some(actual) = self.read_current_freq() {
                    let min_ok = self.available_freqs.iter()
                        .take_while(|&&f| f <= expected).last().copied().unwrap_or(expected);
                    let max_ok = self.available_freqs.iter()
                        .find(|&&f| f >= expected).copied().unwrap_or(expected);
                    if actual < min_ok || actual > max_ok {
                        warn!("{}", t_with_args("fas-freq-mismatch", &fluent_args!(
                            "pid" => self.policy_id.to_string(),
                            "min" => min_ok.to_string(),
                            "max" => max_ok.to_string(),
                            "actual" => actual.to_string()
                        )));
                        self.max_writer.re_unmount();
                        self.min_writer.re_unmount();
                        self.max_writer.invalidate();
                        self.min_writer.invalidate();
                        if write_freq >= actual {
                            self.max_writer.write_value_force(write_freq);
                            self.min_writer.write_value_force(write_freq);
                        } else {
                            self.min_writer.write_value_force(write_freq);
                            self.max_writer.write_value_force(write_freq);
                        }
                    }
                }
            }
        }
        self.verify_freq = Some(write_freq);
    }

    fn read_current_freq(&self) -> Option<u32> {
        let path = format!(
            "/sys/devices/system/cpu/cpufreq/policy{}/scaling_cur_freq",
            self.policy_id
        );
        fs::read_to_string(&path).ok()?.trim().parse::<u32>().ok()
    }

    pub fn force_reapply(&mut self) {
        if self.ignore_write { return; }
        self.max_writer.re_unmount();
        self.min_writer.re_unmount();
        self.max_writer.invalidate();
        self.min_writer.invalidate();
        self.min_writer.write_value_force(self.current_freq);
        self.max_writer.write_value_force(self.current_freq);
    }

    pub fn reset(&mut self) {
        let min_f = self.available_freqs[0];
        let max_f = *self.available_freqs.last().unwrap();
        self.max_writer.write_value_force(max_f);
        self.min_writer.write_value_force(min_f);
        self.current_freq = max_f;
        self.verify_freq = None;
    }
}

// ════════════════════════════════════════════════════════════════
//  FpsWindow — 帧率环形缓冲区
// ════════════════════════════════════════════════════════════════

const WINDOW_SIZE: usize = 120;

struct FpsWindow {
    buf: [f32; WINDOW_SIZE],
    pos: usize,
    len: usize,
    sum: f32,
    sq_sum: f32,
    push_count: u32,
}

impl FpsWindow {
    fn new() -> Self {
        Self { buf: [0.0; WINDOW_SIZE], pos: 0, len: 0, sum: 0.0, sq_sum: 0.0, push_count: 0 }
    }

    fn push(&mut self, fps: f32) {
        if self.len == WINDOW_SIZE {
            let old = self.buf[self.pos];
            self.sum -= old;
            self.sq_sum -= old * old;
        } else {
            self.len += 1;
        }
        self.buf[self.pos] = fps;
        self.sum += fps;
        self.sq_sum += fps * fps;
        self.pos = (self.pos + 1) % WINDOW_SIZE;
        self.push_count += 1;
        // 从 512 降低到 64 帧校准一次，WINDOW_SIZE=120 下每半圈重算一次
        // 在 144fps 下约 0.44 秒校准一次，有效抑制浮点累积误差对齿轮决策的影响
        if self.push_count >= 64 {
            self.recalculate();
            self.push_count = 0;
        }
    }

    fn recalculate(&mut self) {
        let slice = &self.buf[..self.len];
        self.sum = slice.iter().sum();
        self.sq_sum = slice.iter().map(|x| x * x).sum();
    }

    #[inline] fn count(&self) -> usize { self.len }
    #[inline] fn mean(&self) -> f32 {
        if self.len == 0 { 0.0 } else { self.sum / self.len as f32 }
    }

    fn recent_mean(&self, n: usize) -> f32 {
        if self.len == 0 { return 0.0; }
        let count = n.min(self.len);
        let mut sum = 0.0;
        for i in 0..count {
            let idx = (self.pos + WINDOW_SIZE - 1 - i) % WINDOW_SIZE;
            sum += self.buf[idx];
        }
        sum / count as f32
    }

    fn stddev(&self) -> f32 {
        if self.len < 2 { return 0.0; }
        let n = self.len as f32;
        let mean = self.sum / n;
        (self.sq_sum / n - mean * mean).max(0.0).sqrt()
    }

    fn clear(&mut self) {
        self.buf = [0.0; WINDOW_SIZE];
        self.pos = 0; self.len = 0; self.sum = 0.0; self.sq_sum = 0.0;
        self.push_count = 0;
    }
}

// ════════════════════════════════════════════════════════════════
//  PidController
// ════════════════════════════════════════════════════════════════

struct PidController {
    kp: f32, ki: f32, kd: f32,
    integral: f32, prev_error: f32,
    filtered_deriv: f32,
    integral_limit: f32,
}

impl PidController {
    fn new(kp: f32, ki: f32, kd: f32) -> Self {
        Self { kp, ki, kd, integral: 0.0, prev_error: 0.0,
               filtered_deriv: 0.0, integral_limit: 0.15 }
    }

    fn compute(&mut self, error: f32, inst_error: f32, norm: f32) -> f32 {
        // 对 norm 做完整的安全区间限制，防止极端 target_fps 值导致
        // integral 积分、leak 系数、微分除法出现异常数值
        let safe_norm = norm.clamp(0.5, 2.5);

        if error < 0.0 {
            self.integral += error * safe_norm;
        } else {
            let leak = (0.70 + safe_norm * 0.08).clamp(0.70, 0.85);
            self.integral *= leak;
        }
        let dyn_limit = self.integral_limit * safe_norm.clamp(0.7, 1.3);
        self.integral = self.integral.clamp(-dyn_limit, dyn_limit);

        let raw_deriv = (error - self.prev_error) / safe_norm;
        self.filtered_deriv = self.filtered_deriv * 0.7 + raw_deriv * 0.3;
        self.prev_error = error;

        self.kp * inst_error + self.ki * self.integral + self.kd * self.filtered_deriv
    }

    fn reset(&mut self) {
        self.integral = 0.0; self.prev_error = 0.0; self.filtered_deriv = 0.0;
    }

    fn update_coefficients(&mut self, kp: f32, ki: f32, kd: f32) {
        self.kp = kp; self.ki = ki; self.kd = kd;
        self.reset();
    }
}

// ════════════════════════════════════════════════════════════════
//  工具函数
// ════════════════════════════════════════════════════════════════

fn probe_policy_capacity(policy_id: i32) -> Option<u32> {
    let related_str = fs::read_to_string(
        format!("/sys/devices/system/cpu/cpufreq/policy{}/related_cpus", policy_id))
        .or_else(|_| fs::read_to_string(
            format!("/sys/devices/system/cpu/cpufreq/policy{}/affected_cpus", policy_id)))
        .ok()?;
    let first_cpu: u32 = related_str.split_whitespace().next()?.parse().ok()?;
    fs::read_to_string(format!("/sys/devices/system/cpu/cpu{}/cpu_capacity", first_cpu))
        .ok()?.trim().parse::<u32>().ok()
}

fn auto_compute_capacity_weights(policy_ids: &[i32]) -> Option<Vec<(i32, f32)>> {
    let caps: Vec<(i32, u32)> = policy_ids.iter()
        .filter(|&&pid| pid != -1)
        .filter_map(|&pid| probe_policy_capacity(pid).map(|c| (pid, c)))
        .collect();
    if caps.is_empty() || caps.iter().any(|&(_, c)| c == 0) { return None; }
    let min_cap = caps.iter().map(|&(_, c)| c).min().unwrap() as f32;
    Some(caps.iter().map(|&(pid, cap)| {
        let r = cap as f32 / min_cap;
        (pid, if r <= 1.01 { 1.0 } else { 1.0 + (r - 1.0).sqrt() })
    }).collect())
}

#[inline]
fn fps_norm(target_fps: f32) -> f32 {
    (60.0 / target_fps.max(1.0)).sqrt()
}

#[inline]
fn scale_frames(base: u32, target_fps: f32) -> u32 {
    ((base as f32 * target_fps / 60.0).max(base as f32 * 0.4)) as u32
}

// ════════════════════════════════════════════════════════════════
//  GearDecision
// ════════════════════════════════════════════════════════════════

enum GearDecision {
    Hold,
    Upgrade { target: f32, perf: f32, dampen: u32 },
    Downgrade { target: f32, perf: f32, dampen: u32 },
}

// ════════════════════════════════════════════════════════════════
//  FasController — 主控制器 (重构版)
//
//  帧率档位匹配 + PID 控制
//  CPU 负载集成: core_utils 参与频率分配
// ════════════════════════════════════════════════════════════════

pub struct FasController {
    cfg: FasRulesConfig,
    fps_margin: f32,

    pid: PidController,

    fps_gears: Vec<f32>,
    current_target_fps: f32,
    perf_index: f32,
    ema_actual_ms: f32,

    pub policies: Vec<PolicyController>,

    fps_window: FpsWindow,
    log_counter: u32,
    consecutive_normal_frames: u32,

    // 加载
    is_loading: bool,
    loading_frames: u32,
    loading_cumulative_ms: f32,
    loading_normal_tolerance: u32,
    post_loading_ignore: u32,
    post_loading_downgrade_guard: u32,

    // 齿轮
    upgrade_confirm_frames: u32,
    downgrade_confirm_frames: u32,
    upgrade_cooldown: u32,
    gear_dampen_frames: u32,
    consecutive_downgrade_count: u32,
    last_downgrade_from_fps: f32,
    stable_gear_frames: u32,

    // 降档 Boost
    downgrade_boost_active: bool,
    downgrade_boost_remaining: u32,
    downgrade_boost_perf_saved: f32,

    // Jank
    jank_cooldown: u32,
    jank_streak: u32,

    // 时间
    init_time: Instant,
    freq_force_counter: u32,

    // 缓存
    cached_norm: f32,
    cached_budget_ms: f32,
    cached_ema_budget: f32,

    // 温度感知
    current_temperature: f64,
    temp_threshold: f64,

    // [新] CPU 负载数据 — 由 SystemLoadUpdate 事件更新
    foreground_max_util: f32,
    core_utils: Vec<f32>,

    // 当前游戏包名
    current_package: String,
    // 当前游戏的 per-app 配置
    active_profile: Option<PerAppProfile>,

    // perf 地板死锁连续帧计数
    floor_stuck_frames: u32,

    // util_cap EMA 平滑值，防止 200ms 采样周期的滞后数据造成断崖
    ema_fg_util: f32,
}

impl FasController {
    pub fn new() -> Self {
        let cfg = FasRulesConfig::default();
        let pid_ctrl = PidController::new(cfg.pid.kp, cfg.pid.ki, cfg.pid.kd);
        Self {
            fps_margin: 3.0,
            perf_index: cfg.perf_init,
            pid: pid_ctrl,
            fps_gears: cfg.fps_gears.clone(),
            current_target_fps: 60.0,
            ema_actual_ms: 0.0,
            policies: Vec::new(),
            fps_window: FpsWindow::new(),
            log_counter: 0,
            consecutive_normal_frames: 0,
            is_loading: false,
            loading_frames: 0,
            loading_cumulative_ms: 0.0,
            loading_normal_tolerance: 0,
            post_loading_ignore: 0,
            post_loading_downgrade_guard: 0,
            upgrade_confirm_frames: 0,
            downgrade_confirm_frames: 0,
            upgrade_cooldown: 0,
            gear_dampen_frames: 0,
            consecutive_downgrade_count: 0,
            last_downgrade_from_fps: 0.0,
            stable_gear_frames: 0,
            downgrade_boost_active: false,
            downgrade_boost_remaining: 0,
            downgrade_boost_perf_saved: 0.0,
            jank_cooldown: 0,
            jank_streak: 0,
            init_time: Instant::now(),
            freq_force_counter: 0,
            cached_norm: 1.0,
            cached_budget_ms: 16.67,
            cached_ema_budget: 17.54,
            current_temperature: 0.0,
            temp_threshold: 0.0,
            foreground_max_util: 0.0,
            core_utils: Vec::new(),
            current_package: String::new(),
            active_profile: None,
            floor_stuck_frames: 0,
            ema_fg_util: 0.0,
            cfg,
        }
    }

    // ════════════════════════════════════════════════════════════
    //  CPU 负载接口 (来自 SystemLoadUpdate 事件)
    // ════════════════════════════════════════════════════════════

    /// 更新前台最重线程的 CPU 利用率
    pub fn update_cpu_util(&mut self, fg_util: f32) {
        self.foreground_max_util = fg_util;
        // [Fix] EMA smooth fg_util to prevent 200ms sampling lag causing cliff drops
        if self.ema_fg_util <= 0.001 {
            self.ema_fg_util = fg_util;
        } else {
            // Rise fast (alpha=0.4), fall slow (alpha=0.15) to prevent transient lows from killing freq
            let alpha = if fg_util > self.ema_fg_util { 0.40 } else { 0.15 };
            self.ema_fg_util = self.ema_fg_util * (1.0 - alpha) + fg_util * alpha;
        }
    }

    /// 更新各核心利用率快照
    pub fn update_core_utils(&mut self, utils: &[f32]) {
        self.core_utils.clear();
        self.core_utils.extend_from_slice(utils);
    }

    // ════════════════════════════════════════════════════════════
    //  辅助方法
    // ════════════════════════════════════════════════════════════

    /// 获取有效 perf_floor —— 根据目标帧率动态抬高地板
    /// 高刷游戏 (120/144fps) 的 budget 仅 6.9~8.3ms，perf 过低会导致
    /// CPU 频率不足以在 budget 内渲染完一帧，任何突发负载都立刻卡顿。
    /// 公式: floor = base_floor + (target_fps - 60) * 0.003，上限 0.35
    fn effective_perf_floor(&self) -> f32 {
        let base = self.cfg.perf_floor;
        let fps_bonus = ((self.current_target_fps - 60.0).max(0.0) * 0.003).min(0.17);
        (base + fps_bonus).min(0.35)
    }

    /// 获取有效 perf_ceil
    fn effective_perf_ceil(&self) -> f32 {
        self.cfg.perf_ceil
    }

    // ════════════════════════════════════════════════════════════
    //  辅助方法
    // ════════════════════════════════════════════════════════════

    fn next_gear(&self) -> Option<f32> {
        self.fps_gears.iter().copied()
            .filter(|&g| g > self.current_target_fps + 0.5).reduce(f32::min)
    }

    fn prev_gear(&self) -> Option<f32> {
        self.fps_gears.iter().copied()
            .filter(|&g| g < self.current_target_fps - 0.5).reduce(f32::max)
    }

    fn max_gear(&self) -> f32 {
        self.fps_gears.iter().copied().fold(60.0_f32, f32::max)
    }

    fn min_frame_ns(&self) -> u64 {
        (1_000_000_000.0 / self.max_gear()) as u64 / 2
    }

    fn refresh_cached_values(&mut self) {
        self.cached_norm = fps_norm(self.current_target_fps);
        self.cached_budget_ms = 1000.0 / self.current_target_fps.max(1.0);
        self.cached_ema_budget = 1000.0 / (self.current_target_fps - self.fps_margin).max(1.0);
    }

    fn detect_native_gear(&self, avg_fps: f32) -> Option<f32> {
        if self.fps_window.count() < 20 { return None; }
        if avg_fps > 5.0 && self.fps_window.stddev() < avg_fps * 0.10 {
            self.fps_gears.iter().rev().copied()
                .find(|&g| g < self.current_target_fps - 0.5 && (avg_fps - g).abs() < 8.0)
        } else { None }
    }

    fn do_gear_switch(&mut self, new_fps: f32, perf: f32, dampen: u32) {
        let old = self.current_target_fps;
        self.current_target_fps = new_fps;
        self.refresh_cached_values();
        self.upgrade_confirm_frames = 0;
        self.downgrade_confirm_frames = 0;
        self.ema_actual_ms = 0.0;
        self.pid.reset();
        self.fps_window.clear();
        let final_perf = if new_fps > old {
            let min_upgrade_perf = (new_fps / 144.0).clamp(0.45, 0.70);
            perf.max(min_upgrade_perf)
        } else { 
            let max_downgrade_perf = (new_fps / 144.0 + 0.30).clamp(0.45, 0.75);
            perf.min(max_downgrade_perf)
        };
        self.perf_index = final_perf;
        self.gear_dampen_frames = dampen;
        self.downgrade_boost_active = false;
        self.downgrade_boost_remaining = 0;
        self.floor_stuck_frames = 0;
        info!("{}", t_with_args("fas-gear-switch", &fluent_args!(
            "old" => format!("{:.0}", old),
            "new" => format!("{:.0}", new_fps),
            "perf" => format!("{:.2}", final_perf)
        )));
    }

    fn reset_runtime(&mut self) {
        let floor = self.effective_perf_floor();
        let ceil = self.effective_perf_ceil();
        self.perf_index = self.cfg.perf_init.clamp(floor, ceil);
        self.ema_actual_ms = 0.0;
        self.pid.reset();
        self.fps_window.clear();
        self.log_counter = 0;
        self.consecutive_normal_frames = 0;
        self.is_loading = false;
        self.loading_frames = 0;
        self.loading_cumulative_ms = 0.0;
        self.loading_normal_tolerance = 0;
        self.post_loading_ignore = 0;
        self.post_loading_downgrade_guard = 0;
        self.upgrade_confirm_frames = 0;
        self.downgrade_confirm_frames = 0;
        self.upgrade_cooldown = 0;
        self.gear_dampen_frames = 0;
        self.consecutive_downgrade_count = 0;
        self.last_downgrade_from_fps = 0.0;
        self.stable_gear_frames = 0;
        self.downgrade_boost_active = false;
        self.downgrade_boost_remaining = 0;
        self.jank_cooldown = 0;
        self.jank_streak = 0;
        self.freq_force_counter = 0;
        self.floor_stuck_frames = 0;
        self.ema_fg_util = 0.0;
    }

    fn cancel_boost(&mut self) {
        if self.downgrade_boost_active {
            self.downgrade_boost_active = false;
            self.downgrade_boost_remaining = 0;
        }
    }

    fn scaled_boost_inc(&self) -> f32 {
        let base = self.cfg.downgrade_boost_perf_inc;
        let fps_ratio = 60.0 / self.current_target_fps.max(30.0);
        (base * fps_ratio.sqrt()).clamp(0.06, 0.20)
    }

    // ════════════════════════════════════════════════════════════
    //  公共接口：游戏生命周期
    // ════════════════════════════════════════════════════════════

    /// 通知 FAS 当前前台游戏变化
    pub fn set_game(&mut self, _pid: i32, package: &str) {
        self.current_package = package.to_string();
        let profile = self.cfg.per_app_profiles.get(package).cloned();
        if let Some(ref p) = profile {
            if let Some(m) = p.fps_margin { self.fps_margin = m; }
            if let Some(ref gears) = p.target_fps {
                if !gears.is_empty() {
                    self.fps_gears = gears.clone();
                    if !self.fps_gears.iter().any(|&g| (g - self.current_target_fps).abs() < 0.5) {
                        self.current_target_fps = self.fps_gears.iter().copied()
                            .fold(60.0_f32, f32::max);
                    }
                    self.refresh_cached_values();
                }
            }
            info!("{}", t_with_args("fas-set-game", &fluent_args!(
                "pkg" => package,
                "gears" => format!("{:?}", self.fps_gears),
                "target" => format!("{:.0}", self.current_target_fps)
            )));
        } else {
            warn!("{}", t_with_args("fas-no-profile", &fluent_args!(
                "pkg" => package,
                "gears" => format!("{:?}", self.fps_gears)
            )));
        }
        self.active_profile = profile;
    }

    /// 通知 FAS 退出游戏
    pub fn clear_game(&mut self) {
        self.current_package.clear();
        self.active_profile = None;
        self.foreground_max_util = 0.0;
        self.ema_fg_util = 0.0;
        self.core_utils.clear();
        // 恢复全局 margin 和 gears
        if let Ok(m) = self.cfg.fps_margin.parse::<f32>() { self.fps_margin = m; }
        self.fps_gears = self.cfg.fps_gears.clone();
    }

    pub fn set_temperature(&mut self, temp: f64) { self.current_temperature = temp; }
    pub fn set_temp_threshold(&mut self, thresh: f64) { self.temp_threshold = thresh; }

    #[allow(dead_code)]
    pub fn set_ignore_policy(&mut self, policy_id: usize, ignore: bool) {
        for p in &mut self.policies {
            if p.policy_id == policy_id {
                p.ignore_write = ignore;
                info!("{}", t_with_args("fas-ignore-write", &fluent_args!("pid" => policy_id.to_string(), "ignore" => ignore.to_string())));
            }
        }
    }

    /// 热重载规则
    pub fn reload_rules(&mut self, new_rules: &FasRulesConfig) {
        let old_kp = self.cfg.pid.kp;
        let old_ki = self.cfg.pid.ki;
        let old_kd = self.cfg.pid.kd;

        // 安全更新所有不影响状态机连续性的参数
        self.cfg.perf_floor = new_rules.perf_floor;
        self.cfg.perf_ceil = new_rules.perf_ceil;
        self.cfg.perf_init = new_rules.perf_init;
        self.cfg.perf_cold_boot = new_rules.perf_cold_boot;
        self.cfg.freq_hysteresis = new_rules.freq_hysteresis;
        self.cfg.heavy_frame_threshold_ms = new_rules.heavy_frame_threshold_ms;
        self.cfg.loading_cumulative_ms = new_rules.loading_cumulative_ms;
        self.cfg.loading_normal_tolerance = new_rules.loading_normal_tolerance;
        self.cfg.loading_perf_floor = new_rules.loading_perf_floor;
        self.cfg.loading_perf_ceiling = new_rules.loading_perf_ceiling;
        self.cfg.post_loading_ignore_frames = new_rules.post_loading_ignore_frames;
        self.cfg.post_loading_perf = new_rules.post_loading_perf;
        self.cfg.post_loading_downgrade_guard = new_rules.post_loading_downgrade_guard;
        self.cfg.upgrade_confirm_frames = new_rules.upgrade_confirm_frames;
        self.cfg.downgrade_confirm_frames = new_rules.downgrade_confirm_frames;
        self.cfg.upgrade_cooldown_after_downgrade = new_rules.upgrade_cooldown_after_downgrade;
        self.cfg.gear_dampen_frames = new_rules.gear_dampen_frames;
        self.cfg.downgrade_boost_perf_inc = new_rules.downgrade_boost_perf_inc;
        self.cfg.downgrade_boost_duration = new_rules.downgrade_boost_duration;
        self.cfg.fast_decay_frame_threshold = new_rules.fast_decay_frame_threshold;
        self.cfg.fast_decay_perf_threshold = new_rules.fast_decay_perf_threshold;
        self.cfg.fast_decay_max_step = new_rules.fast_decay_max_step;
        self.cfg.fast_decay_min_step = new_rules.fast_decay_min_step;
        self.cfg.jank_cooldown_frames = new_rules.jank_cooldown_frames;
        self.cfg.max_inc_damped = new_rules.max_inc_damped;
        self.cfg.max_inc_normal = new_rules.max_inc_normal;
        self.cfg.damped_perf_cap = new_rules.damped_perf_cap;
        self.cfg.app_switch_gap_ms = new_rules.app_switch_gap_ms;
        self.cfg.app_switch_resume_perf = new_rules.app_switch_resume_perf;
        self.cfg.freq_force_reapply_interval = new_rules.freq_force_reapply_interval;
        self.cfg.fixed_max_frame_ms = new_rules.fixed_max_frame_ms;
        self.cfg.cold_boot_ms = new_rules.cold_boot_ms;
        self.cfg.verify_freq_interval_secs = new_rules.verify_freq_interval_secs;
        self.cfg.per_app_profiles = new_rules.per_app_profiles.clone();
        self.cfg.core_temp_threshold = new_rules.core_temp_threshold;
        self.cfg.core_temp_throttle_perf = new_rules.core_temp_throttle_perf;
        self.cfg.pid = new_rules.pid.clone();
        self.cfg.fps_margin = new_rules.fps_margin.clone();
        self.cfg.util_cap_divisor = new_rules.util_cap_divisor;

        // PID 系数变更时重置积分器
        if (old_kp - new_rules.pid.kp).abs() > 0.001
            || (old_ki - new_rules.pid.ki).abs() > 0.001
            || (old_kd - new_rules.pid.kd).abs() > 0.001
        {
            self.pid.update_coefficients(new_rules.pid.kp, new_rules.pid.ki, new_rules.pid.kd);
            info!("{}", t_with_args("fas-pid-reloaded", &fluent_args!(
                "kp" => format!("{:.4}", new_rules.pid.kp),
                "ki" => format!("{:.4}", new_rules.pid.ki),
                "kd" => format!("{:.4}", new_rules.pid.kd)
            )));
        }

        // 更新当前应用的 profile
        if !self.current_package.is_empty() {
            let profile = new_rules.per_app_profiles.get(&self.current_package).cloned();
            if let Some(ref p) = profile {
                if let Some(m) = p.fps_margin { self.fps_margin = m; }
                // 更新 target_fps 齿轮列表
                if let Some(ref gears) = p.target_fps {
                    if !gears.is_empty() {
                        self.fps_gears = gears.clone();
                        if !self.fps_gears.iter().any(|&g| (g - self.current_target_fps).abs() < 0.5) {
                            self.current_target_fps = self.fps_gears.iter().copied()
                                .fold(60.0_f32, f32::max);
                        }
                    }
                }
            } else {
                // 无 per-app 配置，用全局 margin
                if let Ok(m) = new_rules.fps_margin.parse::<f32>() { self.fps_margin = m; }
            }
            self.active_profile = profile;
        } else {
            if let Ok(m) = new_rules.fps_margin.parse::<f32>() { self.fps_margin = m; }
        }

        // 齿轮列表变更
        if !new_rules.fps_gears.is_empty() && new_rules.fps_gears != self.cfg.fps_gears {
            self.cfg.fps_gears = new_rules.fps_gears.clone();
            // 仅在没有 per-app 覆写时更新运行时齿轮
            if self.active_profile.as_ref().and_then(|p| p.target_fps.as_ref()).is_none() {
                self.fps_gears = new_rules.fps_gears.clone();
                if !self.fps_gears.iter().any(|&g| (g - self.current_target_fps).abs() < 0.5) {
                    self.current_target_fps = self.fps_gears.iter().copied()
                        .fold(60.0_f32, f32::max);
                }
            }
        }

        self.temp_threshold = new_rules.core_temp_threshold;
        self.refresh_cached_values();

        info!("{}", t_with_args("fas-rules-reloaded", &fluent_args!(
            "margin" => format!("{:.1}", self.fps_margin),
            "floor" => format!("{:.2}", self.cfg.perf_floor),
            "ceil" => format!("{:.2}", self.cfg.perf_ceil),
            "profiles" => self.cfg.per_app_profiles.len().to_string()
        )));
    }

    // ════════════════════════════════════════════════════════════
    //  频率应用 — CPU 负载感知
    //
    //  利用 core_utils 判断 cluster 负载，低负载 cluster 用 relaxed 模式
    // ════════════════════════════════════════════════════════════

    fn apply_freqs(&mut self) {
        self.freq_force_counter = self.freq_force_counter.wrapping_add(1);
        let force = self.freq_force_counter % self.cfg.freq_force_reapply_interval == 0;

        let mut effective_perf = self.perf_index.clamp(0.0, 1.0);

        // 利用率软封顶 — 使用 EMA 平滑值，增加多重保护防止断崖
        let in_jank = self.jank_cooldown > 0 || self.jank_streak > 0;
        let floor = self.effective_perf_floor();
        let near_floor = self.perf_index < floor + 0.10;
        // 热降频检测：fg_util 骤降但 perf_index 高 → 内核在限频，util 数据不可信
        let thermal_suspected = self.ema_fg_util < 0.25 && self.perf_index > 0.50;

        if !in_jank && !near_floor && !thermal_suspected && self.ema_fg_util > 0.05 {
            let divisor = self.cfg.util_cap_divisor.max(0.1);
            let util_cap = (self.ema_fg_util / divisor).clamp(0.40, 1.0);
            if effective_perf > util_cap {
                // 降低激进程度：0.92/0.08 替代原来的 0.80/0.20
                effective_perf = effective_perf * 0.92 + util_cap * 0.08;
            }
        }

        let ratio = effective_perf;

        // 计算各 policy 的目标频率
        for policy in &mut self.policies {
            if policy.freq_hold_frames > 0 && !force {
                policy.freq_hold_frames = policy.freq_hold_frames.saturating_sub(1);
                continue;
            }
            policy.freq_hold_frames = policy.freq_hold_frames.saturating_sub(1);

            let w = policy.cluster_profile.capacity_weight.max(0.1);
            // 原始 ratio.powf(w) 对大核 (w=2.3) 惩罚过重：
            // perf=0.50 时超大核只给 0.50^2.3 ≈ 20% 频率，导致高刷游戏跑不上去。
            // 改为 sqrt(w) 指数 + 线性混合，大幅缓解大核频率被压制。
            let pow_adj = ratio.powf(w.sqrt());
            let linear_adj = ratio;
            let blend = (w - 1.0).clamp(0.0, 1.5) / 1.5;
            let adj = linear_adj * (1.0 - blend * 0.5) + pow_adj * (blend * 0.5);
            let target_freq = policy.find_nearest_freq(adj);

            if target_freq != policy.current_freq {
                let diff = (policy.find_nearest_freq(ratio) as f32 / policy.freq_max
                    - policy.current_ratio()).abs();

                // 变化小于迟滞阈值且非强制时跳过
                if diff <= self.cfg.freq_hysteresis && !force { continue; }

                // 根据 cluster 实际负载选择写入策略
                // 如果该 cluster 对应的核心利用率低于 30%，用 relaxed 模式节电
                policy.apply_freq_locked(target_freq);
            } else if force {
                policy.force_reapply();
            }
        }
    }
    // ════════════════════════════════════════════════════════════
    //  load_policies — 初始化
    // ════════════════════════════════════════════════════════════

    pub fn load_policies(&mut self, fas_rules: &FasRulesConfig) {
        self.policies.clear();
        self.cfg = fas_rules.clone();
        self.cfg.migrate_legacy_margins();

        self.pid = PidController::new(fas_rules.pid.kp, fas_rules.pid.ki, fas_rules.pid.kd);

        if !fas_rules.fps_gears.is_empty() {
            self.fps_gears = fas_rules.fps_gears.clone();
        }
        if let Ok(m) = fas_rules.fps_margin.parse::<f32>() { self.fps_margin = m; }

        let _ = crate::utils::try_write_file("/sys/module/perfmgr/parameters/perfmgr_enable", "0");
        let _ = crate::utils::try_write_file("/sys/module/mtk_fpsgo/parameters/perfmgr_enable", "0");

        // [修改项] 动态拉取 CPU policy 列表
        let clusters = crate::scheduler::get_cpu_policies();

        let auto_w = if fas_rules.auto_capacity_weight {
            auto_compute_capacity_weights(&clusters).map(|w| {
                info!("{}", t("fas-auto-capacity"));
                for &(pid, wt) in &w {
                    info!("{}", t_with_args("fas-auto-capacity-core", &fluent_args!(
                        "pid" => pid.to_string(),
                        "cap" => probe_policy_capacity(pid).unwrap_or(0).to_string(),
                        "weight" => format!("{:.2}", wt)
                    )));
                }
                w
            })
        } else { None };

        for (idx, &pid) in clusters.iter().enumerate() {
            let _ = crate::utils::try_write_file(
                &format!("/sys/devices/system/cpu/cpufreq/policy{}/scaling_governor", pid),
                "performance");

            let mut freqs: Vec<u32> = fs::read_to_string(
                format!("/sys/devices/system/cpu/cpufreq/policy{}/scaling_available_frequencies", pid))
                .unwrap_or_default()
                .split_whitespace()
                .filter_map(|s| s.parse().ok()).collect();
            if freqs.is_empty() { continue; }
            freqs.sort_unstable();
            freqs.dedup();

            let max_f = *freqs.last().unwrap();
            let mut mw = FastWriter::new(format!(
                "/sys/devices/system/cpu/cpufreq/policy{}/scaling_max_freq", pid));
            let mut nw = FastWriter::new(format!(
                "/sys/devices/system/cpu/cpufreq/policy{}/scaling_min_freq", pid));

            // 检查 FastWriter 是否成功打开文件，跳过无效的 policy
            if !mw.is_valid() || !nw.is_valid() {
                warn!("{}", t_with_args("fas-policy-writer-invalid", &fluent_args!(
                    "pid" => pid.to_string(),
                    "max_valid" => mw.is_valid().to_string(),
                    "min_valid" => nw.is_valid().to_string()
                )));
                continue;
            }

            mw.write_value_force(max_f);
            nw.write_value_force(max_f);

            let profile = auto_w.as_ref()
                .and_then(|aw| aw.iter().find(|&&(p, _)| p == pid))
                .map(|&(_, w)| ClusterProfile { capacity_weight: w })
                .unwrap_or_else(|| fas_rules.cluster_profiles.get(idx).cloned().unwrap_or_default());

            info!("{}", t_with_args("fas-policy-init", &fluent_args!(
                "pid" => pid.to_string(),
                "min" => (freqs.first().unwrap()/1000).to_string(),
                "max" => (max_f/1000).to_string(),
                "weight" => format!("{:.2}", profile.capacity_weight)
            )));

            self.policies.push(PolicyController::new(mw, nw, freqs, pid as usize, profile, max_f));
        }

        self.current_target_fps = *self.fps_gears.iter().reduce(|a, b| if a > b { a } else { b }).unwrap_or(&60.0);
        self.reset_runtime();
        self.refresh_cached_values();
        self.init_time = Instant::now();
        self.perf_index = self.cfg.perf_cold_boot;
        self.temp_threshold = fas_rules.core_temp_threshold;
        self.apply_freqs();

        info!("{}", t_with_args("fas-init-summary", &fluent_args!(
            "fps" => format!("{:.0}", self.current_target_fps),
            "margin" => format!("{:.1}", self.fps_margin),
            "clusters" => self.policies.len().to_string(),
            "perf" => format!("{:.2}", self.perf_index),
            "profiles" => self.cfg.per_app_profiles.len().to_string()
        )));
    }

    // ════════════════════════════════════════════════════════════
    //  Phase 1: 冷启动 & 应用切换
    // ════════════════════════════════════════════════════════════

    fn handle_early_exit(&mut self, actual_ms: f32) -> bool {
        if self.init_time.elapsed().as_millis() < self.cfg.cold_boot_ms as u128 {
            if self.perf_index < self.cfg.perf_cold_boot {
                self.perf_index = self.cfg.perf_cold_boot;
                self.apply_freqs();
            }
            return true;
        }

        if actual_ms > self.cfg.app_switch_gap_ms {
            self.reset_runtime();
            self.perf_index = self.cfg.app_switch_resume_perf;
            self.gear_dampen_frames = scale_frames(self.cfg.gear_dampen_frames, self.current_target_fps);
            self.post_loading_downgrade_guard = self.cfg.post_loading_downgrade_guard;
            self.apply_freqs();
            info!("{}", t_with_args("fas-app-switch", &fluent_args!(
                "ms" => format!("{:.0}", actual_ms),
                "perf" => format!("{:.2}", self.perf_index)
            )));
            return true;
        }

        false
    }

    // ════════════════════════════════════════════════════════════
    //  Phase 2: 加载检测
    // ════════════════════════════════════════════════════════════

    fn handle_loading(&mut self, actual_ms: f32, is_heavy: bool) -> bool {
        if is_heavy {
            self.loading_frames += 1;
            self.loading_cumulative_ms += actual_ms;
            self.loading_normal_tolerance = 0;
            if !self.is_loading && self.loading_cumulative_ms > self.cfg.loading_cumulative_ms {
                self.is_loading = true;
                let old = self.perf_index;
                self.perf_index = self.perf_index
                    .clamp(self.cfg.loading_perf_floor, self.cfg.loading_perf_ceiling);
                if old != self.perf_index { self.apply_freqs(); }
                info!("{}", t_with_args("fas-loading-start", &fluent_args!(
                    "frames" => self.loading_frames.to_string(),
                    "ms" => format!("{:.0}", self.loading_cumulative_ms),
                    "old_perf" => format!("{:.2}", old),
                    "new_perf" => format!("{:.2}", self.perf_index)
                )));
            }
            return true;
        }

        if self.loading_frames > 0 {
            self.loading_normal_tolerance += 1;
            if self.loading_normal_tolerance < self.cfg.loading_normal_tolerance { return true; }
            self.loading_frames = 0;
            self.loading_cumulative_ms = 0.0;
            self.loading_normal_tolerance = 0;
        }

        if self.is_loading {
            self.is_loading = false;
            self.fps_window.clear();
            self.ema_actual_ms = 0.0;
            self.pid.reset();
            self.downgrade_confirm_frames = 0;
            self.downgrade_boost_active = false;
            self.downgrade_boost_remaining = 0;
            let floor = self.effective_perf_floor();
            let ceil = self.effective_perf_ceil();
            self.perf_index = self.cfg.post_loading_perf.clamp(floor, ceil);
            self.post_loading_ignore = self.cfg.post_loading_ignore_frames;
            self.gear_dampen_frames = scale_frames(self.cfg.gear_dampen_frames, self.current_target_fps);
            self.post_loading_downgrade_guard = self.cfg.post_loading_downgrade_guard;
            self.apply_freqs();
            info!("{}", t_with_args("fas-loading-exit", &fluent_args!("perf" => format!("{:.2}", self.perf_index))));
        }

        false
    }

    // ════════════════════════════════════════════════════════════
    //  Phase 3: 齿轮决策
    // ════════════════════════════════════════════════════════════

    fn evaluate_gear(&mut self, avg_fps: f32, recent30: f32) -> GearDecision {
        let tfps = self.current_target_fps;

        // ── 升档判断 ──
        if let Some(next) = self.next_gear() {
            let overshoot = if tfps > 1.0 { avg_fps / tfps } else { 1.0 };

            if overshoot > 1.35
                && self.fps_window.count() >= 15
                && recent30 > tfps * 1.2
                && self.perf_index < 0.45
                && !self.downgrade_boost_active
            {
                let ref_fps = self.fps_window.recent_mean(15).max(avg_fps);
                let best = self.fps_gears.iter().copied()
                    .filter(|&g| g <= ref_fps + 15.0 && g > tfps + 0.5)
                    .reduce(f32::max).unwrap_or(next);
                self.consecutive_downgrade_count = 0;
                self.stable_gear_frames = 0;
                return GearDecision::Upgrade { target: best, perf: 0.60, dampen: scale_frames(30, best) };
            }

            if self.upgrade_cooldown == 0 {
                let confirm = scale_frames(self.cfg.upgrade_confirm_frames, tfps);
                if recent30 >= next - 10.0 && avg_fps >= tfps * 0.9 && self.fps_window.count() >= 60 {
                    self.upgrade_confirm_frames += 1;
                    self.downgrade_confirm_frames = 0;
                    if self.upgrade_confirm_frames >= confirm {
                        self.consecutive_downgrade_count = 0;
                        self.stable_gear_frames = 0;
                        return GearDecision::Upgrade {
                            target: next, perf: self.perf_index,
                            dampen: scale_frames(self.cfg.gear_dampen_frames, next),
                        };
                    }
                } else if avg_fps >= tfps - 5.0 && self.perf_index < 0.50
                    && self.fps_window.count() >= 90 && !self.downgrade_boost_active
                {
                    self.upgrade_confirm_frames += 1;
                    if self.upgrade_confirm_frames >= confirm * 2 {
                        return GearDecision::Upgrade {
                            target: next, perf: (self.perf_index + 0.15).min(0.65),
                            dampen: scale_frames(self.cfg.gear_dampen_frames, next),
                        };
                    }
                // 低 perf 稳帧升档路径：
                // 打破 "频率被压住 → 帧率上不去 → 升不了档" 的死锁。
                // 当 perf_index 很低时，说明当前档位下频率有大量余量，
                // 游戏轻松跑满当前目标帧率且帧率稳定（stddev 低），
                // 此时即使 recent30 没达到 next-10，也应给机会升档。
                } else if avg_fps >= tfps - 2.0
                    && self.perf_index < 0.35
                    && self.fps_window.count() >= 90
                    && self.fps_window.stddev() < tfps * 0.08
                    && !self.downgrade_boost_active
                {
                    self.upgrade_confirm_frames += 2;
                    self.downgrade_confirm_frames = 0;
                    if self.upgrade_confirm_frames >= confirm {
                        self.consecutive_downgrade_count = 0;
                        self.stable_gear_frames = 0;
                        info!("{}", t_with_args("fas-low-perf-upgrade", &fluent_args!(
                            "perf" => format!("{:.2}", self.perf_index),
                            "avg" => format!("{:.1}", avg_fps),
                            "stddev" => format!("{:.1}", self.fps_window.stddev()),
                            "fps" => format!("{:.0}", next)
                        )));
                        return GearDecision::Upgrade {
                            target: next, perf: (self.perf_index + 0.20).min(0.65),
                            dampen: scale_frames(self.cfg.gear_dampen_frames, next),
                        };
                    }
                } else {
                    self.upgrade_confirm_frames = self.upgrade_confirm_frames.saturating_sub(3);
                }
            } else {
                self.upgrade_confirm_frames = 0;
            }
        } else {
            self.upgrade_confirm_frames = 0;
        }

        // ── 降档判断 ──
        if let Some(prev) = self.prev_gear() {
            if self.post_loading_downgrade_guard > 0 {
                self.downgrade_confirm_frames = 0;
                self.cancel_boost();
            } else if avg_fps < self.current_target_fps - 10.0 {
                let is_extreme = avg_fps < tfps * 0.40
                    && self.fps_window.count() >= 10
                    && self.fps_window.stddev() < avg_fps.max(1.0) * 0.25;

                if is_extreme {
                    if let Some(native) = self.detect_native_gear(avg_fps) {
                        return GearDecision::Downgrade {
                            target: native, perf: 0.55, dampen: scale_frames(30, native),
                        };
                    }
                    self.cancel_boost();
                    self.downgrade_confirm_frames += 1;
                } else if recent30 >= tfps - 5.0 {
                    self.cancel_boost();
                    self.downgrade_confirm_frames = 0;
                } else if !self.downgrade_boost_active && self.downgrade_confirm_frames == 0 {
                    let boost_inc = self.scaled_boost_inc();
                    self.downgrade_boost_active = true;
                    let scaled_duration = scale_frames(self.cfg.downgrade_boost_duration, self.current_target_fps);
                    self.downgrade_boost_remaining = scaled_duration;
                    self.downgrade_boost_perf_saved = self.perf_index;
                    self.perf_index = (self.perf_index + boost_inc).min(0.90);
                    info!("{}", t_with_args("fas-downgrade-boost", &fluent_args!(
                        "avg" => format!("{:.1}", avg_fps),
                        "old" => format!("{:.2}", self.downgrade_boost_perf_saved),
                        "new" => format!("{:.2}", self.perf_index),
                        "inc" => format!("{:.3}", boost_inc)
                    )));
                } else if self.downgrade_boost_active && self.downgrade_boost_remaining > 0 {
                    self.downgrade_boost_remaining -= 1;
                    if self.downgrade_boost_remaining == 0 {
                        // [Fix] Gradual decay instead of instant cliff restore
                        let blended = self.perf_index * 0.70 + self.downgrade_boost_perf_saved * 0.30;
                        self.perf_index = blended.max(self.downgrade_boost_perf_saved);
                        self.downgrade_boost_active = false;
                        self.downgrade_confirm_frames += 10;
                        info!("{}", t_with_args("fas-boost-expired", &fluent_args!(
                            "confirm" => self.downgrade_confirm_frames.to_string()
                        )));
                    }
                } else {
                    self.downgrade_confirm_frames += 1;
                }

                let confirm = scale_frames(self.cfg.downgrade_confirm_frames, tfps);
                if self.downgrade_confirm_frames >= confirm {
                    let old_fps = tfps;
                    if (old_fps - self.last_downgrade_from_fps).abs() < 1.0 {
                        self.consecutive_downgrade_count += 1;
                    } else { self.consecutive_downgrade_count = 1; }
                    self.last_downgrade_from_fps = old_fps;
                    let backoff = 1u32 << self.consecutive_downgrade_count.min(4);
                    self.upgrade_cooldown = self.cfg.upgrade_cooldown_after_downgrade * backoff;
                    self.stable_gear_frames = 0;
                    return GearDecision::Downgrade {
                        target: prev, perf: self.perf_index,
                        dampen: scale_frames(self.cfg.gear_dampen_frames, prev),
                    };
                }
            } else {
                self.cancel_boost();
                self.downgrade_confirm_frames = 0;
            }
        }

        GearDecision::Hold
    }

    // ════════════════════════════════════════════════════════════
    //  Phase 4: PID + Jank
    // ════════════════════════════════════════════════════════════

    fn update_pid_and_jank(&mut self, actual_ms: f32) -> &'static str {
        let norm = self.cached_norm;
        let budget_ms = self.cached_budget_ms;
        let ema_budget = self.cached_ema_budget;
        let damped = self.gear_dampen_frames > 0;
        let floor = self.effective_perf_floor();
        let ceil = self.effective_perf_ceil();

        let ema_err = ema_budget - self.ema_actual_ms;
        let inst_err = budget_ms - actual_ms;
        let old_perf = self.perf_index;

        let crit_ratio = (0.40 + (1.0 - norm) * 0.60).clamp(0.40, 0.80);
        let crit_ms = budget_ms * crit_ratio;
        let heavy_ms = (ema_budget * (0.15 + (1.0 - norm) * 0.20).clamp(0.15, 0.35)).max(2.0);
        let jank_scale = norm;

        let mut act;

        if inst_err < -crit_ms {
            self.jank_streak += 1;
            let streak_m = (1.0 - (-0.4 * self.jank_streak as f32).exp()).clamp(0.30, 0.85);
            let fps_urgency = (self.current_target_fps / 60.0).sqrt().clamp(1.0, 1.6);
            let inc = if damped { 0.050 * fps_urgency } else { 0.080 * fps_urgency };
            self.perf_index += inc * jank_scale * streak_m;
            act = "crit";
            self.consecutive_normal_frames = 0;
            self.jank_cooldown = scale_frames(self.cfg.jank_cooldown_frames * 3, self.current_target_fps);
        } else if ema_err < -heavy_ms {
            self.jank_streak += 1;
            let streak_m = (1.0 - (-0.35 * self.jank_streak as f32).exp()).clamp(0.30, 0.70);
            let fps_urgency = (self.current_target_fps / 60.0).sqrt().clamp(1.0, 1.4);
            let inc = if damped { 0.025 * fps_urgency } else { 0.040 * fps_urgency };
            self.perf_index += inc * jank_scale * streak_m;
            act = "heavy";
            self.consecutive_normal_frames = 0;
            let fps_cd_scale = (self.current_target_fps / 60.0).clamp(1.0, 2.5);
            self.jank_cooldown = self.jank_cooldown.max(
                (scale_frames(self.cfg.jank_cooldown_frames, self.current_target_fps) as f32 * fps_cd_scale) as u32);
        } else {
            self.jank_streak = 0;
            self.consecutive_normal_frames += 1;
            let raw = self.pid.compute(ema_err, inst_err, norm);
            if raw > 0.0 {
                let d = if self.downgrade_boost_active { 0.0 } else { 1.0 };
                let floor_guard = if self.perf_index < floor + 0.15 { 0.3 } else { 1.0 };
                self.perf_index -= raw * d * norm * floor_guard;
                act = "pid-decay";
            } else {
                let jd = if self.jank_cooldown > 0 { 0.70 } else { 1.0 };
                let bd = if self.downgrade_boost_active { 0.0 } else { 1.0 };
                let dd = if damped { 0.5 } else { 1.0 };
                self.perf_index += (-raw) * jd * bd * dd * norm;
                act = "pid-inc";
            }
        }

        // 紧急兜底
        let avg = self.fps_window.mean();
        if self.fps_window.count() >= 10
            && avg > 3.0
            && avg < self.current_target_fps * 0.65
            && self.perf_index < 0.50
            && !self.is_loading
            && self.init_time.elapsed().as_millis() > self.cfg.cold_boot_ms as u128
        {
            let deficit_ratio = 1.0 - (avg / self.current_target_fps.max(1.0));
            let emergency_inc = (0.06 * deficit_ratio * norm).clamp(0.02, 0.10);
            self.perf_index += emergency_inc;
            act = "emergency-inc";
        }

        // perf_floor 长期死锁检测
        if self.perf_index <= floor + 0.01
            && avg > 3.0
            && avg < self.current_target_fps * 0.50
        {
            self.floor_stuck_frames += 1;
            let stuck_threshold = (self.current_target_fps * 2.0) as u32;
            if self.floor_stuck_frames >= stuck_threshold {
                let old = self.perf_index;
                self.perf_index = self.cfg.perf_cold_boot;
                self.pid.reset();
                self.floor_stuck_frames = 0;
                act = "floor-rescue";
                info!("{}", t_with_args("fas-floor-rescue", &fluent_args!(
                    "frames" => stuck_threshold.to_string(),
                    "old" => format!("{:.2}", old),
                    "avg" => format!("{:.1}", avg),
                    "new" => format!("{:.2}", self.perf_index)
                )));
            }
        } else {
            self.floor_stuck_frames = 0;
        }

        // Clamp (使用场景 override 的范围)
        self.perf_index = self.perf_index.clamp(floor, ceil);
        // base * (fps/60)^0.3，高刷时允许更大的单帧增量。
        let scale = (self.current_target_fps / 60.0).powf(0.3).clamp(0.8, 1.8);
        let max_inc = if damped {
            (self.cfg.max_inc_damped * scale).max(0.040)
        } else {
            (self.cfg.max_inc_normal * scale).max(0.065)
        };
        if self.perf_index > old_perf + max_inc { self.perf_index = old_perf + max_inc; }
        if damped && self.perf_index > self.cfg.damped_perf_cap {
            self.perf_index = self.cfg.damped_perf_cap;
        }

        act
    }

    // ════════════════════════════════════════════════════════════
    //  Phase 5: 快速衰减
    // ════════════════════════════════════════════════════════════

    fn apply_fast_decay(&mut self, avg_fps: f32) {
        let floor = self.effective_perf_floor();
        let ceil = self.effective_perf_ceil();
        let thresh = scale_frames(self.cfg.fast_decay_frame_threshold, self.current_target_fps);
        let high_fps_factor = (self.current_target_fps / 60.0).powf(0.70).max(1.0);
        let adjusted_thresh = (thresh as f32 * high_fps_factor) as u32;

        // 阈值随 fps 升高而升高，120fps 时约 0.75，144fps 时约 0.80
        let dynamic_decay_threshold = self.cfg.fast_decay_perf_threshold
            + ((self.current_target_fps - 60.0).max(0.0) * 0.002).min(0.15);

        if self.consecutive_normal_frames >= adjusted_thresh
            && self.perf_index > dynamic_decay_threshold
            && self.jank_cooldown == 0
            && !self.downgrade_boost_active
            && self.init_time.elapsed().as_millis() > self.cfg.cold_boot_ms as u128
        {
            let fps_dampen = (60.0 / self.current_target_fps.max(30.0)).powf(0.40);
            // 衰减步长额外乘以 0.6，降低高刷下的衰减激进度
            let decay_scale = if self.current_target_fps > 90.0 { 0.6 } else { 1.0 };
            let step = ((self.perf_index - 0.50) / 0.50 * self.cfg.fast_decay_max_step * fps_dampen * decay_scale)
                .clamp(self.cfg.fast_decay_min_step * fps_dampen, self.cfg.fast_decay_max_step * fps_dampen * decay_scale);
            self.perf_index -= step;
            self.consecutive_normal_frames = 0;
        }

        if avg_fps > self.current_target_fps + self.fps_margin * 2.5
            && self.perf_index > floor
            && self.jank_cooldown == 0
            && self.fps_window.count() >= 30
        {
            let fps_scale = (60.0 / self.current_target_fps.max(30.0)).clamp(0.4, 1.0);
            let s = ((avg_fps - self.current_target_fps) / self.current_target_fps * 0.04 * fps_scale)
                .clamp(0.0, 0.008 * fps_scale.max(0.5));
            if s > 0.0005 { self.perf_index -= s; }
        }

        self.perf_index = self.perf_index.clamp(floor, ceil);
    }

    fn update_stability_forgiveness(&mut self, avg_fps: f32) {
        if self.consecutive_downgrade_count > 0 {
            if avg_fps >= self.current_target_fps - 3.0 && self.fps_window.count() >= 60 {
                self.stable_gear_frames += 1;
            } else {
                self.stable_gear_frames = self.stable_gear_frames.saturating_sub(3);
            }
            if self.stable_gear_frames >= 900 {
                self.consecutive_downgrade_count = self.consecutive_downgrade_count.saturating_sub(1);
                self.stable_gear_frames = 0;
            }
        }
    }

    // ════════════════════════════════════════════════════════════
    //  update_frame — 主入口
    // ════════════════════════════════════════════════════════════

    pub fn update_frame(&mut self, frame_delta_ns: u64) {
        if frame_delta_ns == 0 || self.policies.is_empty() { return; }

        let actual_ms = frame_delta_ns as f32 / 1_000_000.0;
        let is_heavy = actual_ms > self.cfg.heavy_frame_threshold_ms;
        let max_ns = (self.cfg.fixed_max_frame_ms * 1_000_000.0) as u64;

        if frame_delta_ns < self.min_frame_ns() { return; }

        // Phase 1
        if self.handle_early_exit(actual_ms) { return; }

        // Phase 2
        if self.handle_loading(actual_ms, is_heavy) { return; }
        if self.is_loading { return; }
        if self.post_loading_ignore > 0 { self.post_loading_ignore -= 1; return; }
        if frame_delta_ns > max_ns { return; }

        // 帧率采样
        let current_fps = 1_000_000_000.0 / frame_delta_ns as f32;
        self.fps_window.push(current_fps);
        let avg_fps = self.fps_window.mean();
        let recent30 = self.fps_window.recent_mean(30);

        // 冷却递减
        self.upgrade_cooldown = self.upgrade_cooldown.saturating_sub(1);
        self.gear_dampen_frames = self.gear_dampen_frames.saturating_sub(1);
        self.post_loading_downgrade_guard = self.post_loading_downgrade_guard.saturating_sub(1);
        self.jank_cooldown = self.jank_cooldown.saturating_sub(1);

        // Phase 3: 齿轮
        match self.evaluate_gear(avg_fps, recent30) {
            GearDecision::Upgrade { target, perf, dampen } |
            GearDecision::Downgrade { target, perf, dampen } => {
                self.do_gear_switch(target, perf, dampen);
                self.apply_freqs();
                return;
            }
            GearDecision::Hold => {}
        }

        // Phase 4: EMA
        self.update_ema(actual_ms, avg_fps);

        // Phase 5: PID + Jank
        let act = self.update_pid_and_jank(actual_ms);

        // Phase 6: 衰减
        self.apply_fast_decay(avg_fps);
        self.update_stability_forgiveness(avg_fps);

        // 心跳日志
        self.log_counter = self.log_counter.wrapping_add(1);
        if self.log_counter % 30 == 0 {
            let ema_err = self.cached_ema_budget - self.ema_actual_ms;
            let inst_err = self.cached_budget_ms - actual_ms;
            info!("{}", t_with_args("fas-tick-log", &fluent_args!(
                "target" => format!("{:.0}", self.current_target_fps),
                "avg" => format!("{:.1}", avg_fps),
                "ms" => format!("{:.2}", actual_ms),
                "ema" => format!("{:.2}", self.ema_actual_ms),
                "err_ema" => format!("{:+.2}", ema_err),
                "err_inst" => format!("{:+.2}", inst_err),
                "act" => act,
                "perf" => format!("{:.3}", self.perf_index),
                "util" => format!("{:.2}", self.foreground_max_util),
                "cd" => if self.upgrade_cooldown > 0 { " cd" } else { "" },
                "damp" => if self.gear_dampen_frames > 0 { " damp" } else { "" },
                "temp" => if self.current_temperature > 0.0 { format!(" T:{:.0}℃", self.current_temperature) } else { "".to_string() }
            )));
        }

        self.apply_freqs();
    }

    fn update_ema(&mut self, actual_ms: f32, avg_fps: f32) {
        let budget_ms = self.cached_budget_ms;
        let norm = self.cached_norm;
        let ema_input_ms = {
            let base = if self.fps_window.count() >= 8 && avg_fps > 5.0
                && avg_fps < self.current_target_fps * 0.50
            { 1000.0 / avg_fps } else { budget_ms };
            let cap = base * 2.0;
            let extreme = base * 4.0;
            if actual_ms > extreme { base + 1.0 }
            else if actual_ms > cap { cap }
            else { actual_ms }
        };
        if self.ema_actual_ms <= 0.0 {
            self.ema_actual_ms = ema_input_ms;
        } else {
            let fps_factor = (self.current_target_fps / 60.0).clamp(0.5, 2.5);
            let a_up = (0.15 * fps_factor).clamp(0.10, 0.35);
            let a_down = ((0.25 + (1.0 - norm) * 0.25) * fps_factor).clamp(0.15, 0.45);
            let a = if ema_input_ms > self.ema_actual_ms { a_up } else { a_down };
            self.ema_actual_ms = self.ema_actual_ms * (1.0 - a) + ema_input_ms * a;
        }
    }

    /// 重置所有 policy 频率（退出游戏时调用）
    pub fn reset_all_freqs(&mut self) {
        for policy in &mut self.policies {
            policy.reset();
        }
    }
}