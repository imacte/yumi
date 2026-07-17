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

use crate::fas_types::{FasRulesConfig, PerAppProfile};
use std::time::Instant;
use log::{info, warn};

use crate::i18n::{t, t_with_args};
use crate::fluent_args;

use super::fps_window::FpsWindow;
use super::pid::{self, PidController, fps_norm};
use super::policy_controller::PolicyController;

// ════════════════════════════════════════════════════════════════
//  FasController — 主控制器
//
//  帧率档位匹配 + PID 控制
//  CPU 负载集成: core_utils 参与频率分配
// ════════════════════════════════════════════════════════════════

pub struct FasController {
    pub(super) cfg: FasRulesConfig,
    pub(super) fps_margin: f32,

    pub(super) pid: PidController,

    pub(super) fps_gears: Vec<f32>,
    pub(super) current_target_fps: f32,
    pub(super) perf_index: f32,
    pub(super) ema_actual_ms: f32,

    pub policies: Vec<PolicyController>,

    pub(super) fps_window: FpsWindow,
    pub(super) log_counter: u32,
    pub(super) consecutive_normal_frames: u32,

    // 加载
    pub(super) is_loading: bool,
    pub(super) loading_frames: u32,
    pub(super) loading_cumulative_ms: f32,
    pub(super) loading_normal_tolerance: u32,
    pub(super) post_loading_ignore: u32,
    pub(super) post_loading_downgrade_guard: u32,

    // 齿轮
    pub(super) upgrade_confirm_frames: u32,
    pub(super) downgrade_confirm_frames: u32,
    pub(super) upgrade_cooldown: u32,
    pub(super) gear_dampen_frames: u32,
    pub(super) consecutive_downgrade_count: u32,
    pub(super) last_downgrade_from_fps: f32,
    pub(super) stable_gear_frames: u32,

    // 降档 Boost
    pub(super) downgrade_boost_active: bool,
    pub(super) downgrade_boost_remaining: u32,
    pub(super) downgrade_boost_perf_saved: f32,

    // Jank
    pub(super) jank_cooldown: u32,
    pub(super) jank_streak: u32,

    // 时间
    pub(super) init_time: Instant,
    pub(super) freq_force_counter: u32,

    // 缓存
    pub(super) cached_norm: f32,
    pub(super) cached_budget_ms: f32,
    pub(super) cached_ema_budget: f32,

    // 温度感知
    pub(super) current_temperature: f64,
    pub(super) temp_threshold: f64,

    // [新] CPU 负载数据 — 由 SystemLoadUpdate 事件更新
    pub(super) foreground_max_util: f32,
    pub(super) core_utils: Vec<f32>,

    // 当前游戏包名
    pub(super) current_package: String,
    // 当前游戏的 per-app 配置
    pub(super) active_profile: Option<PerAppProfile>,

    // perf 地板死锁连续帧计数
    pub(super) floor_stuck_frames: u32,

    // util_cap EMA 平滑值，防止 200ms 采样周期的滞后数据造成断崖
    pub(super) ema_fg_util: f32,

    // [Jank 恢复保护] crit/heavy 后的 perf 最低值保护
    // 防止恢复帧到来后 PID 在 2-3 帧内将 perf 从 1.0 衰减到 floor，
    // 导致后续帧频率不足再次 jank 形成连锁掉帧
    pub(super) post_jank_perf_floor: f32,
    pub(super) post_jank_guard_frames: u32,

    // [动态 PID] 基于 CPU 利用率的 target_fps 偏移
    // 范围 [-3.0, 0.0]：当 CPU 利用率持续偏低时逐步降低有效 target_fps，
    // 让 PID 少给频率，节省功耗；利用率回升时逐步恢复
    pub(super) target_fps_offset: f32,
    pub(super) util_sample_timer: Instant,
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
            post_jank_perf_floor: 0.0,
            post_jank_guard_frames: 0,
            target_fps_offset: 0.0,
            util_sample_timer: Instant::now(),
            cfg,
        }
    }

    // ════════════════════════════════════════════════════════════
    //  CPU 负载接口 (来自 SystemLoadUpdate 事件)
    // ════════════════════════════════════════════════════════════

    /// 更新前台最重线程的 CPU 利用率
    pub fn update_cpu_util(&mut self, fg_util: f32) {
        self.foreground_max_util = fg_util;
        // EMA smooth fg_util to prevent 200ms sampling lag causing cliff drops
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
    ///
    /// 旧公式硬顶 0.35，导致 120fps 下 perf 完全贴地运行(日志中稳态P=0.350)，
    /// 遇到突发负载需要多帧才能爬升到足够频率，造成可感知卡顿。
    /// 新公式: floor = base + (target_fps - 60) * 0.004, 上限 0.45
    ///   60fps  → 0.22 (不变)
    ///   90fps  → 0.34
    ///   120fps → 0.40 (原 0.35，多出 5% headroom)
    ///   144fps → 0.45 (原 0.35)
    pub(super) fn effective_perf_floor(&self) -> f32 {
        let base = self.cfg.perf_floor;
        let fps_bonus = ((self.current_target_fps - 60.0).max(0.0) * 0.004).min(0.25);
        (base + fps_bonus).min(0.45)
    }

    /// 获取有效 perf_ceil
    pub(super) fn effective_perf_ceil(&self) -> f32 {
        self.cfg.perf_ceil
    }

    pub(super) fn next_gear(&self) -> Option<f32> {
        self.fps_gears.iter().copied()
            .filter(|&g| g > self.current_target_fps + 0.5).reduce(f32::min)
    }

    pub(super) fn prev_gear(&self) -> Option<f32> {
        self.fps_gears.iter().copied()
            .filter(|&g| g < self.current_target_fps - 0.5).reduce(f32::max)
    }

    pub(super) fn max_gear(&self) -> f32 {
        self.fps_gears.iter().copied().fold(60.0_f32, f32::max)
    }

    pub(super) fn min_frame_ns(&self) -> u64 {
        (1_000_000_000.0 / self.max_gear()) as u64 / 2
    }

    pub(super) fn refresh_cached_values(&mut self) {
        self.cached_norm = fps_norm(self.current_target_fps);
        self.cached_budget_ms = 1000.0 / self.current_target_fps.max(1.0);
        self.cached_ema_budget = 1000.0 / (self.current_target_fps - self.fps_margin).max(1.0);
        // 动态适配 PID 系数到当前 target_fps
        self.pid.adapt_to_target_fps(self.current_target_fps);
    }

    /// 基于 CPU 利用率动态偏移 target_fps
    ///
    /// 每秒采样一次 ema_fg_util：
    ///   util ≤ 0.10 → 重置偏移 (可能在菜单/暂停画面)
    ///   util ≤ 0.55 → 逐步降低 target (-0.1/s)，最多 -3fps
    ///   util ≥ 0.65 → 逐步恢复 (+0.1/s) 至 0
    ///
    /// 效果：GPU bound 场景自动放宽帧率目标，减少无效拉频
    pub(super) fn adjust_target_for_util(&mut self) {
        if self.util_sample_timer.elapsed().as_millis() < 1000 { return; }
        self.util_sample_timer = Instant::now();

        // jank_cooldown 期间禁止降低 target，只允许恢复
        // 防止刚从团战卡顿恢复，util 还没爬满就又把目标降下去
        let allow_decrease = self.jank_cooldown == 0 && self.jank_streak == 0;

        let util = self.ema_fg_util;
        if util <= 0.10 {
            self.target_fps_offset = 0.0;
        } else if util <= 0.55 && allow_decrease {
            self.target_fps_offset = (self.target_fps_offset - 0.1).max(-3.0);
        } else if util >= 0.65 {
            self.target_fps_offset = (self.target_fps_offset + 0.1).min(0.0);
        }
    }

    /// 获取经过 util 偏移后的有效 target_fps
    #[inline]
    pub(super) fn effective_target_fps(&self) -> f32 {
        (self.current_target_fps + self.target_fps_offset).max(10.0)
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
        self.target_fps_offset = 0.0;
        // 恢复全局 margin 和 gears
        self.fps_margin = self.cfg.fps_margin;
        self.fps_gears = self.cfg.fps_gears.clone();
    }

    pub fn set_temperature(&mut self, temp: f64) { self.current_temperature = temp; }
    pub fn set_temp_threshold(&mut self, thresh: f64) { self.temp_threshold = thresh; }

    pub(super) fn reset_runtime(&mut self) {
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
        self.post_jank_perf_floor = 0.0;
        self.post_jank_guard_frames = 0;
        self.target_fps_offset = 0.0;
        self.util_sample_timer = Instant::now();
    }
}
