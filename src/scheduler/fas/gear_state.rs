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

// ════════════════════════════════════════════════════════════════
//  GearDecision
// ════════════════════════════════════════════════════════════════

pub(super) enum GearDecision {
    Hold,
    Upgrade { target: f32, perf: f32, dampen: u32 },
    Downgrade { target: f32, perf: f32, dampen: u32 },
}

impl FasController {
    pub(super) fn detect_native_gear(&self, avg_fps: f32) -> Option<f32> {
        if self.fps_window.count() < 20 { return None; }
        if avg_fps > 5.0 && self.fps_window.stddev() < avg_fps * 0.10 {
            self.fps_gears.iter().rev().copied()
                .find(|&g| g < self.current_target_fps - 0.5 && (avg_fps - g).abs() < 8.0)
        } else { None }
    }

    pub(super) fn do_gear_switch(&mut self, new_fps: f32, perf: f32, dampen: u32) {
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
        // 齿轮切换是重大状态变更，重置 util 偏移让 PID 从零开始适应新档位
        self.target_fps_offset = 0.0;
        self.util_sample_timer = std::time::Instant::now();
        // 齿轮切换后清除 jank 保护（新档位有自己的 perf 基线）
        self.post_jank_perf_floor = 0.0;
        self.post_jank_guard_frames = 0;
        info!("{}", t_with_args("fas-gear-switch", &fluent_args!(
            "old" => format!("{:.0}", old),
            "new" => format!("{:.0}", new_fps),
            "perf" => format!("{:.2}", final_perf)
        )));
    }

    // ════════════════════════════════════════════════════════════
    //  Phase 3: 齿轮决策
    // ════════════════════════════════════════════════════════════

    pub(super) fn evaluate_gear(&mut self, avg_fps: f32, recent30: f32) -> GearDecision {
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
                        // Gradual decay instead of instant cliff restore
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
}
