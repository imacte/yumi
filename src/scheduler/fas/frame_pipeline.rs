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

use log::info;

use crate::i18n::{t, t_with_args};
use crate::fluent_args;

use super::pid::scale_frames;
use super::FasController;
use super::gear_state::GearDecision;

impl FasController {
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
    //  Phase 4.5: EMA 更新
    // ════════════════════════════════════════════════════════════

    fn update_ema(&mut self, actual_ms: f32, avg_fps: f32) {
        // [动态 PID] 使用偏移后的目标 fps 计算 EMA baseline，
        // 保证 EMA 和 PID 看到的 budget 一致
        let eff_target = self.effective_target_fps();
        let budget_ms = 1000.0 / eff_target.max(1.0);
        let norm = self.cached_norm;
        let ema_input_ms = {
            let base = if self.fps_window.count() >= 8 && avg_fps > 5.0
                && avg_fps < eff_target * 0.50
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

    // ════════════════════════════════════════════════════════════
    //  Phase 6: 快速衰减
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

        // Phase 4.5: 动态 target_fps 偏移
        self.adjust_target_for_util();

        // Phase 5: PID + Jank
        let act = self.update_pid_and_jank(actual_ms);

        // Phase 6: 衰减
        self.apply_fast_decay(avg_fps);
        self.update_stability_forgiveness(avg_fps);

        // 心跳日志
        self.log_counter = self.log_counter.wrapping_add(1);
        if self.log_counter % 30 == 0 {
            let eff_target = self.effective_target_fps();
            let ema_err = 1000.0 / (eff_target - self.fps_margin).max(1.0) - self.ema_actual_ms;
            let inst_err = 1000.0 / eff_target.max(1.0) - actual_ms;
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
                "temp" => if self.current_temperature > 0.0 { format!(" T:{:.0}℃", self.current_temperature) } else { "".to_string() },
                "offset" => if self.target_fps_offset.abs() > 0.05 { format!(" off:{:+.1}", self.target_fps_offset) } else { "".to_string() }
            )));
        }

        self.apply_freqs();
    }
}
