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

use crate::i18n::t_with_args;
use crate::fluent_args;

use super::pid::scale_frames;
use super::FasController;

impl FasController {
    pub(super) fn cancel_boost(&mut self) {
        if self.downgrade_boost_active {
            self.downgrade_boost_active = false;
            self.downgrade_boost_remaining = 0;
        }
    }

    pub(super) fn scaled_boost_inc(&self) -> f32 {
        let base = self.cfg.downgrade_boost_perf_inc;
        let fps_ratio = 60.0 / self.current_target_fps.max(30.0);
        (base * fps_ratio.sqrt()).clamp(0.06, 0.20)
    }

    // ════════════════════════════════════════════════════════════
    //  Phase 4: PID + Jank
    // ════════════════════════════════════════════════════════════

    pub(super) fn update_pid_and_jank(&mut self, actual_ms: f32) -> &'static str {
        let norm = self.cached_norm;
        let damped = self.gear_dampen_frames > 0;
        let floor = self.effective_perf_floor();
        let ceil = self.effective_perf_ceil();

        // [动态 PID] 使用 util 偏移后的有效 target 计算 budget
        // 齿轮判断仍用原始 target_fps，但 PID 控制用偏移后的目标
        // 这样 GPU bound 场景下 PID 的 error 更小，输出更保守
        let eff_target = self.effective_target_fps();
        let budget_ms = 1000.0 / eff_target.max(1.0);
        let ema_budget = 1000.0 / (eff_target - self.fps_margin).max(1.0);

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
            // 紧急避险：Jank 发生时立刻重置 util 偏移，
            // 恢复严格的 target_fps 目标，避免 PID 在团战突发时还在"偷懒"
            self.target_fps_offset = 0.0;
            self.util_sample_timer = std::time::Instant::now(); // 防止下次 tick 立刻重新降低

            // [紧急跳频] 超大帧 (>50ms, 即掉了 6+ 个 120fps vsync) 直接跳到较高 perf，
            // 不走渐进式爬升，因为按 max_inc ≈ 0.09/帧 从 floor=0.40 爬到 0.70 需要 3-4 帧，
            // 在 120fps 下意味着 25-33ms 的额外卡顿窗口。
            if actual_ms > 50.0 && self.perf_index < 0.70 {
                self.perf_index = 0.70;
            }

            // [Jank 恢复保护] 设置 post-jank perf 地板：
            // crit 后 PID 会在恢复帧 (error 变正) 立刻走 pid-decay，
            // 原版 3 帧内 1.0→0.35，频率断崖导致后续帧再次 jank。
            // 保护期内 perf 不会低于此值，给出平滑衰减窗口。
            let guard_perf = (self.perf_index * 0.55).max(0.50);
            if guard_perf > self.post_jank_perf_floor {
                self.post_jank_perf_floor = guard_perf;
            }
            self.post_jank_guard_frames = scale_frames(60, self.current_target_fps);
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
            // heavy jank 也重置偏移，但只在偏移较大时才强制重置
            if self.target_fps_offset < -1.0 {
                self.target_fps_offset = 0.0;
                self.util_sample_timer = std::time::Instant::now();
            }

            // [Jank 恢复保护] heavy 也设置保护地板，但比 crit 保守一些
            let guard_perf = (self.perf_index * 0.50).max(0.45);
            if guard_perf > self.post_jank_perf_floor {
                self.post_jank_perf_floor = guard_perf;
            }
            self.post_jank_guard_frames = self.post_jank_guard_frames.max(
                scale_frames(30, self.current_target_fps));
        } else {
            self.jank_streak = 0;
            self.consecutive_normal_frames += 1;
            // jank_cooldown 期间传入 0.0 让 util_gain=1.0，
            // 确保从卡顿恢复的过渡期 PID 全力拉频，不被旧的低 util 数据拖后腿
            let pid_util = if self.jank_cooldown > 0 { 0.0 } else { self.ema_fg_util };
            let raw = self.pid.compute(ema_err, inst_err, norm, pid_util);
            if raw > 0.0 {
                let d = if self.downgrade_boost_active { 0.0 } else { 1.0 };
                let floor_guard = if self.perf_index < floor + 0.15 { 0.3 } else { 1.0 };
                // 目标分裂安全护栏：
                // 当 target_fps_offset < 0 时，PID 的 effective target 低于齿轮的 raw target。
                // 如果实际 fps 低于 raw target，PID 不应该认为"任务完成"而激进衰减，
                // 否则齿轮看到 perf_index 过低会产生假升档/震荡。
                // 此时把衰减力度降到 30%，让 perf 缓慢下降而非断崖。
                let avg = self.fps_window.mean();
                let split_guard = if self.target_fps_offset < -0.5
                    && avg < self.current_target_fps - 1.0
                    && avg > self.effective_target_fps() - 1.0
                { 0.3 } else { 1.0 };
                self.perf_index -= raw * d * norm * floor_guard * split_guard;
                act = "pid-decay";
            } else {
                // jank_cooldown 期间 PID 增频力度大幅衰减 (0.25)，
                // 防止恢复帧到来后 perf 在 2-3 帧内从高位断崖回落。
                // 原值 0.70 太激进，日志显示 crit 后 P 从 1.0 → 0.35 仅需 3 帧。
                let jd = if self.jank_cooldown > 0 { 0.25 } else { 1.0 };
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

        // [Jank 恢复保护] 在保护期内抬高 perf 下限，防止断崖衰减
        let effective_floor = if self.post_jank_guard_frames > 0 {
            self.post_jank_guard_frames -= 1;
            // 保护地板随时间线性衰减到 base floor，提供平滑过渡
            let guard_ratio = self.post_jank_guard_frames as f32
                / scale_frames(60, self.current_target_fps).max(1) as f32;
            let decaying_guard = floor + (self.post_jank_perf_floor - floor) * guard_ratio;
            floor.max(decaying_guard)
        } else {
            floor
        };

        // Clamp (使用场景 override 的范围 + jank 保护地板)
        self.perf_index = self.perf_index.clamp(effective_floor, ceil);
        // base * (fps/60)^0.3，高刷时允许更大的单帧增量。
        let scale = (self.current_target_fps / 60.0).powf(0.3).clamp(0.8, 1.8);
        // crit 和 emergency 不受常规 max_inc 限制，允许单帧大幅拉升
        // 原版 max_inc ≈ 0.092 在 120fps 下需要 7-8 帧才能从 floor 爬到 1.0，
        // 超大帧(139ms)已经卡了 16+ 个 vsync，不能再用渐进式爬升
        let max_inc = if act == "crit" || act == "emergency-inc" {
            (self.cfg.max_inc_normal * scale * 2.5).max(0.15)
        } else if damped {
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
}
