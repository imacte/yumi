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


use crate::scheduler::config::CpuLoadGovernorConfig;
use super::cpufreq::{FrequencyRange, PolicyFrequency};
use log::{info, debug, warn};
use std::fs;

use crate::i18n::{t, t_with_args};
use crate::fluent_args;

struct ClusterState {
    frequency: PolicyFrequency,
    affected_cpus: Vec<usize>,
    boost_max: u32,
    current_perf: f32,
    down_wait: u32,
    up_wait: u32,
    /// 上一 tick 的原始 max_util，用于尖峰跳升检测
    last_util: f32,
}

impl ClusterState {
    fn apply_range(&mut self, floor: f32) -> FrequencyRange {
        let range = self.frequency.range_for_perf(self.current_perf, floor, 2);
        self.frequency.apply(range);
        range
    }

    fn max_util(&self, core_utils: &[f32]) -> f32 {
        self.affected_cpus.iter()
            .filter_map(|&cpu| core_utils.get(cpu))
            .copied()
            .fold(0.0_f32, f32::max)
    }
}

// ════════════════════════════════════════════════════════════════
//  CpuLoadGovernor — 主控制器
// ════════════════════════════════════════════════════════════════

pub struct CpuLoadGovernor {
    clusters: Vec<ClusterState>,
    restore: Vec<PolicyFrequency>,
    cfg: CpuLoadGovernorConfig,
    active: bool,
    log_counter: u32,
}

impl CpuLoadGovernor {
    pub fn new() -> Self {
        Self {
            clusters: Vec::new(),
            restore: Vec::new(),
            cfg: CpuLoadGovernorConfig::default(),
            active: false,
            log_counter: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn init_policies(&mut self, gov_cfg: &CpuLoadGovernorConfig) {
        self.release();
        if !self.restore.is_empty() { return; }
        self.cfg = gov_cfg.clone();
        self.normalize_cfg();

        for policy in &crate::scheduler::get_cpu_policies() {
            let pid = policy.id;
            let affected_cpus = Self::read_affected_cpus(pid);
            if affected_cpus.is_empty() { continue; }
            let Some(mut frequency) = PolicyFrequency::open(policy) else { continue; };
            frequency.select_governor(self.cfg.governor.as_deref());
            let available = frequency.available_range();

            let init_perf = self.cfg.perf_init.clamp(self.cfg.perf_floor, self.cfg.perf_ceil);
            let mut cluster = ClusterState {
                frequency,
                affected_cpus,
                boost_max: policy.boost_frequencies.iter().copied().max().unwrap_or(0),
                current_perf: init_perf,
                down_wait: 0,
                up_wait: 0,
                last_util: 0.0,
            };
            let range = cluster.apply_range(self.cfg.perf_floor);

            info!("{}", t_with_args("clg-init", &fluent_args!(
                "pid" => pid.to_string(),
                "cpus" => format!("{:?}", cluster.affected_cpus),
                "fmin" => (available.min / 1000).to_string(),
                "fmax" => (available.max / 1000).to_string(),
                "perf" => format!("{:.2}", init_perf),
                "freq" => format!("{}-{}", range.min, range.max)
            )));

            self.clusters.push(cluster);
        }

        self.active = !self.clusters.is_empty();
        if self.active {
            info!("{}", t_with_args("clg-activated", &fluent_args!("count" => self.clusters.len().to_string())));
        } else {
            warn!("{}", t("clg-no-clusters"));
        }
    }

    pub fn release(&mut self) {
        if self.active { info!("{}", t("clg-deactivated")); }
        self.restore.extend(self.clusters.drain(..).map(|cluster| cluster.frequency));
        self.retry_restore();
        self.active = false;
        self.log_counter = 0;
    }

    pub fn retry_restore(&mut self) {
        self.restore.retain_mut(|policy| !policy.restore());
    }

    pub fn restore_pending(&self) -> bool {
        !self.restore.is_empty()
    }

    pub fn reload_config(&mut self, gov_cfg: &CpuLoadGovernorConfig) {
        let governor_changed = self.cfg.governor != gov_cfg.governor;
        self.cfg = gov_cfg.clone();
        self.normalize_cfg();
        for cluster in &mut self.clusters {
            if governor_changed {
                cluster.frequency.select_governor(self.cfg.governor.as_deref());
            }
            cluster.current_perf = cluster.current_perf.clamp(self.cfg.perf_floor, self.cfg.perf_ceil);
            cluster.apply_range(self.cfg.perf_floor);
        }
        debug!("{}", t_with_args("clg-config-reloaded", &fluent_args!(
            "up" => format!("{:.2}", self.cfg.up_threshold),
            "down" => format!("{:.2}", self.cfg.down_threshold),
            "floor" => format!("{:.2}", self.cfg.perf_floor),
            "ceil" => format!("{:.2}", self.cfg.perf_ceil)
        )));
    }

    /// 校验并规范化配置：防止 perf_floor > perf_ceil / NaN 导致 f32::clamp panic
    fn normalize_cfg(&mut self) {
        let floor = self.cfg.perf_floor;
        let ceil = self.cfg.perf_ceil;
        if floor.is_finite() && ceil.is_finite() && floor > ceil {
            warn!("{}", t_with_args("clg-perf-clamped", &fluent_args!(
                "floor" => format!("{:.2}", floor),
                "ceil" => format!("{:.2}", ceil)
            )));
        }
        self.cfg.normalize();
    }

    pub fn on_load_update(&mut self, core_utils: &[f32]) {
        if !self.active { return; }

        for cluster in &mut self.clusters {
            let raw_util = cluster.max_util(core_utils);
            // 尖峰抑制：单 tick 跳升超过阈值时衰减其增量，
            // 孤立瞬时尖峰（如单核 0↔100%）不瞬间拉满 perf；
            // 持续负载下一 tick jump 归零即全量生效，不拖慢真实升频
            let util = if raw_util > cluster.last_util + self.cfg.spike_jump_threshold {
                cluster.last_util + (raw_util - cluster.last_util) * self.cfg.spike_decay
            } else {
                raw_util
            };
            cluster.last_util = raw_util;

            // headroom 在 up_threshold 附近线性过渡，避免阶跃导致的振荡
            let ramp_start = self.cfg.up_threshold - self.cfg.headroom_ramp;
            let headroom = if util >= self.cfg.up_threshold {
                self.cfg.headroom_factor
            } else if util > ramp_start {
                let t = ((util - ramp_start) / self.cfg.headroom_ramp.max(1e-6)).clamp(0.0, 1.0);
                1.0 + (self.cfg.headroom_factor - 1.0) * t
            } else {
                1.0
            };

            let target_perf = (util * headroom)
                .clamp(self.cfg.perf_floor, self.cfg.perf_ceil);
            let old_perf = cluster.current_perf;

            if target_perf > old_perf {
                cluster.down_wait = 0;
                cluster.up_wait += 1;

                // 升频速率限制：必须连续 up_rate_limit_ticks 才执行
                if cluster.up_wait < self.cfg.up_rate_limit_ticks {
                    cluster.apply_range(self.cfg.perf_floor);
                    continue;
                }

                let is_high_load = util >= self.cfg.up_threshold; 
                let is_significant_jump = target_perf > old_perf + self.cfg.up_jump_threshold; 

                if is_high_load || is_significant_jump {
                    cluster.current_perf += (target_perf - old_perf) * self.cfg.smoothing_up;
                } else {
                    // 滞回带内升频：速率随 util 接近 up_threshold 线性提升——
                    // 低 util 端用 slow_up_scale 防抖，高 util 端逼近全速，
                    // 避免中等负载（如 73%）下 0.008/tick 的慢速爬升导致体验卡顿
                    let span = (self.cfg.up_threshold - self.cfg.down_threshold).max(1e-6);
                    let gap = ((util - self.cfg.down_threshold) / span).clamp(0.0, 1.0);
                    let speed = self.cfg.smoothing_up
                        * (self.cfg.slow_up_scale + (1.0 - self.cfg.slow_up_scale) * gap);
                    cluster.current_perf += (target_perf - old_perf) * speed; 
                }
            } else {
                cluster.up_wait = 0;
                cluster.down_wait += 1;
                // 极低负载立即快速降频（跳过 down_wait 确认期），
                // 消除尖峰消失后 perf 长时间悬停高位的滞后
                if cluster.down_wait >= self.cfg.down_rate_limit_ticks
                    || util < self.cfg.down_fast_threshold
                {
                    // 降频门控：只要目标低于当前即可降，避免滞回带内锁死高位
                    let active_smoothing_down = if util < self.cfg.down_fast_threshold {
                        // 极低负载：快速回落
                        self.cfg.smoothing_down * self.cfg.down_fast_mult
                    } else if util < self.cfg.down_threshold {
                        // 跌破降频阈值：正常速率降频
                        self.cfg.smoothing_down
                    } else {
                        // 滞回带内（down_threshold..up_threshold）：慢速下探防抖
                        self.cfg.smoothing_down * self.cfg.slow_down_scale
                    };
                    cluster.current_perf += (target_perf - old_perf) * active_smoothing_down;
                }
            }

            cluster.current_perf = cluster.current_perf.clamp(self.cfg.perf_floor, self.cfg.perf_ceil);
            cluster.apply_range(self.cfg.perf_floor);
        }

        self.log_counter += 1;
        if self.log_counter % 25 == 0 {
            for c in &self.clusters {
                let range = c.frequency.applied_range();
                debug!("{}", t_with_args("clg-tick-log", &fluent_args!(
                    "pid" => c.frequency.id.to_string(),
                    "util" => format!("{:.0}", c.max_util(core_utils) * 100.0),
                    "perf" => format!("{:.2}", c.current_perf),
                    "freq" => format!("{}-{}", range.min, range.max),
                    "boost" => c.boost_max.to_string()
                )));
            }
        }
    }

    fn read_affected_cpus(policy_id: i32) -> Vec<usize> {
        let path = format!(
            "/sys/devices/system/cpu/cpufreq/policy{}/affected_cpus", policy_id);
        fs::read_to_string(&path)
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|s| s.parse::<usize>().ok())
            .collect()
    }
}
