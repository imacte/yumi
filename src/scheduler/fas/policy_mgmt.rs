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

use crate::fas_types::{FasRulesConfig, ClusterProfile};
use crate::scheduler::cpufreq::PolicyFrequency;
use std::fs;
use std::time::{Duration, Instant};
use log::{info, warn};

use crate::i18n::{t, t_with_args};
use crate::fluent_args;

use super::pid::PidController;
use super::policy_controller::PolicyController;
use super::FasController;

impl FasController {
    /// 热重载规则
    pub fn reload_rules(&mut self, new_rules: &FasRulesConfig) {
        // 规范化配置（防 NaN/越界导致 clamp panic），遮蔽原引用使后续代码一致
        let mut rules = new_rules.clone();
        rules.normalize();
        let new_rules = &rules;

        if self.cfg.governor != new_rules.governor {
            for policy in &mut self.policies {
                policy.frequency.select_governor(new_rules.governor.as_deref());
            }
            self.cfg.governor.clone_from(&new_rules.governor);
        }

        let old_kp = self.cfg.pid.kp;
        let old_ki = self.cfg.pid.ki;
        let old_kd = self.cfg.pid.kd;
        let limits_changed = self.cfg.perf_floor != new_rules.perf_floor
            || self.cfg.perf_ceil != new_rules.perf_ceil;

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
                self.fps_margin = new_rules.fps_margin;
            }
            self.active_profile = profile;
        } else {
            self.fps_margin = new_rules.fps_margin;
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
        if limits_changed {
            for policy in &mut self.policies {
                policy.pending = true;
            }
            self.apply_freqs();
        }

        info!("{}", t_with_args("fas-rules-reloaded", &fluent_args!(
            "margin" => format!("{:.1}", self.fps_margin),
            "floor" => format!("{:.2}", self.cfg.perf_floor),
            "ceil" => format!("{:.2}", self.cfg.perf_ceil),
            "profiles" => self.cfg.per_app_profiles.len().to_string()
        )));
    }

    pub(super) fn apply_freqs(&mut self) {
        self.freq_force_counter = self.freq_force_counter.wrapping_add(1);
        let interval = self.cfg.freq_force_reapply_interval;
        let refresh = interval > 0 && self.freq_force_counter % interval == 0;

        let verify_interval = self.cfg.verify_freq_interval_secs;
        let verify = verify_interval > 0
            && self.limits_checked_at.elapsed() >= Duration::from_secs(verify_interval.into());
        if verify {
            self.limits_checked_at = Instant::now();
        }

        let floor = self.effective_perf_floor().min(self.cfg.perf_ceil);
        let mut effective_perf = self.perf_index.clamp(floor, self.cfg.perf_ceil);

        // 利用率软封顶 — 使用 EMA 平滑值，增加多重保护防止断崖
        let in_jank = self.jank_cooldown > 0 || self.jank_streak > 0;
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

        for policy in &mut self.policies {
            if verify && !policy.pending {
                policy.frequency.verify_limits();
            }

            let hold = policy.freq_hold_frames > 0;
            policy.freq_hold_frames = policy.freq_hold_frames.saturating_sub(1);
            if hold && !refresh && !policy.pending { continue; }

            let range = policy.range_for_perf(effective_perf, floor);
            if policy.pending || (range != policy.frequency.applied_range()
                && (refresh || policy.range_changed(range, self.cfg.freq_hysteresis)))
            {
                policy.apply_range(range);
            }
        }
    }
    // ════════════════════════════════════════════════════════════
    //  load_policies — 初始化
    // ════════════════════════════════════════════════════════════

    pub fn load_policies(&mut self, fas_rules: &FasRulesConfig) {
        // 规范化配置（防 NaN/越界导致 clamp panic），遮蔽原引用使后续代码一致
        let mut rules = fas_rules.clone();
        rules.normalize();
        let fas_rules = &rules;

        self.reset_all_freqs();
        if !self.restore.is_empty() { return; }
        self.cfg = fas_rules.clone();
        self.cfg.migrate_legacy_margins();

        self.pid = PidController::new(fas_rules.pid.kp, fas_rules.pid.ki, fas_rules.pid.kd);

        if !fas_rules.fps_gears.is_empty() {
            self.fps_gears = fas_rules.fps_gears.clone();
        }
        self.fps_margin = fas_rules.fps_margin;

        for path in ["/sys/module/perfmgr/parameters/perfmgr_enable",
            "/sys/module/mtk_fpsgo/parameters/perfmgr_enable"] {
            if !self.saved_perfmgr.iter().any(|(saved, _)| saved == path) {
                if let Ok(value) = fs::read_to_string(path) {
                    self.saved_perfmgr.push((path.to_string(), value));
                    let _ = crate::utils::write_to_file(path, "0");
                }
            }
        }

        // [修改项] 动态拉取 CPU policy 列表
        let clusters = crate::scheduler::get_cpu_policies();

        let auto_w = if fas_rules.auto_capacity_weight {
            crate::scheduler::auto_compute_capacity_weights(&clusters).map(|w| {
                info!("{}", t("fas-auto-capacity"));
                for &(pid, wt) in &w {
                    info!("{}", t_with_args("fas-auto-capacity-core", &fluent_args!(
                        "pid" => pid.to_string(),
                        "cap" => crate::scheduler::probe_policy_capacity(pid).unwrap_or(0).to_string(),
                        "weight" => format!("{:.2}", wt)
                    )));
                }
                w
            })
        } else { None };

        for (idx, policy) in clusters.iter().enumerate() {
            let pid = policy.id;
            let Some(mut frequency) = PolicyFrequency::open(policy) else { continue; };
            frequency.select_governor(fas_rules.governor.as_deref());
            let available = frequency.available_range();

            let profile = auto_w.as_ref()
                .and_then(|aw| aw.iter().find(|&&(p, _)| p == pid))
                .map(|&(_, w)| ClusterProfile { capacity_weight: w })
                .unwrap_or_else(|| fas_rules.cluster_profiles.get(idx).cloned().unwrap_or_default());

            info!("{}", t_with_args("fas-policy-init", &fluent_args!(
                "pid" => pid.to_string(),
                "min" => (available.min / 1000).to_string(),
                "max" => (available.max / 1000).to_string(),
                "weight" => format!("{:.2}", profile.capacity_weight)
            )));

            self.policies.push(PolicyController::new(frequency, profile));
        }

        self.current_target_fps = *self.fps_gears.iter().reduce(|a, b| if a > b { a } else { b }).unwrap_or(&60.0);
        self.reset_runtime();
        self.refresh_cached_values();
        self.init_time = Instant::now();
        self.limits_checked_at = self.init_time;
        self.perf_index = self.cfg.perf_cold_boot;
        self.temp_threshold = fas_rules.core_temp_threshold;
        self.apply_freqs();

        if self.policies.is_empty() {
            self.reset_all_freqs();
        }

        info!("{}", t_with_args("fas-init-summary", &fluent_args!(
            "fps" => format!("{:.0}", self.current_target_fps),
            "margin" => format!("{:.1}", self.fps_margin),
            "clusters" => self.policies.len().to_string(),
            "perf" => format!("{:.2}", self.perf_index),
            "profiles" => self.cfg.per_app_profiles.len().to_string()
        )));
    }

    /// 重置所有 policy 频率（退出游戏时调用）
    pub fn reset_all_freqs(&mut self) {
        self.restore.extend(self.policies.drain(..).map(|policy| policy.frequency));
        self.retry_restore();
        self.saved_perfmgr.retain(|(path, value)| {
            if crate::utils::write_to_file(path, value.trim()).is_err() {
                warn!("[FAS] Failed to restore {path}");
                true
            } else { false }
        });
    }

    pub fn retry_restore(&mut self) {
        self.restore.retain_mut(|policy| !policy.restore());
    }

    pub fn restore_pending(&self) -> bool {
        !self.restore.is_empty()
    }
}
