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
use super::fas::FastWriter;
use log::{info, debug, warn};
use std::fs;

use crate::i18n::{t, t_with_args};
use crate::fluent_args;

// ════════════════════════════════════════════════════════════════
//  ClusterState — 单 cluster 运行时状态
// ════════════════════════════════════════════════════════════════

struct ClusterState {
    policy_id: i32,
    affected_cpus: Vec<usize>,
    available_freqs: Vec<u32>,
    cached_ratios: Vec<f32>,
    _freq_min: f32,
    _freq_max: f32,
    max_writer: FastWriter,
    min_writer: FastWriter,
    current_perf: f32,
    current_freq: u32,
    down_wait: u32,
}

impl ClusterState {
    fn find_nearest_freq(&self, target_ratio: f32) -> u32 {
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

    fn write_freq(&mut self, freq: u32) {
        if freq == self.current_freq { return; }
        if freq >= self.current_freq {
            self.max_writer.write_value_force(freq);
            self.min_writer.write_value_force(freq);
        } else {
            self.min_writer.write_value_force(freq);
            self.max_writer.write_value_force(freq);
        }
        self.current_freq = freq;
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
    cfg: CpuLoadGovernorConfig,
    active: bool,
    log_counter: u32,
}

impl CpuLoadGovernor {
    pub fn new() -> Self {
        Self {
            clusters: Vec::new(),
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
        self.cfg = gov_cfg.clone();

        let clusters = crate::scheduler::get_cpu_policies();

        for pid in clusters {
            let gov_path = format!(
                "/sys/devices/system/cpu/cpufreq/policy{}/scaling_governor", pid);
            let _ = crate::utils::try_write_file(&gov_path, "performance");

            let freq_path = format!(
                "/sys/devices/system/cpu/cpufreq/policy{}/scaling_available_frequencies", pid);
            let mut freqs: Vec<u32> = fs::read_to_string(&freq_path)
                .unwrap_or_default()
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            if freqs.is_empty() { continue; }
            freqs.sort_unstable();
            freqs.dedup();

            let affected = Self::read_affected_cpus(pid);
            if affected.is_empty() { continue; }

            let fmin = *freqs.first().unwrap() as f32;
            let fmax = *freqs.last().unwrap() as f32;
            let range = (fmax - fmin).max(1.0);
            let cached_ratios: Vec<f32> = freqs.iter()
                .map(|&f| (f as f32 - fmin) / range)
                .collect();

            let max_writer = FastWriter::new(format!(
                "/sys/devices/system/cpu/cpufreq/policy{}/scaling_max_freq", pid));
            let min_writer = FastWriter::new(format!(
                "/sys/devices/system/cpu/cpufreq/policy{}/scaling_min_freq", pid));

            if !max_writer.is_valid() || !min_writer.is_valid() {
                warn!("{}", t_with_args("clg-writer-invalid", &fluent_args!(
                    "pid" => pid.to_string(),
                    "max_valid" => max_writer.is_valid().to_string(),
                    "min_valid" => min_writer.is_valid().to_string()
                )));
                continue;
            }

            let init_perf = self.cfg.perf_init.clamp(self.cfg.perf_floor, self.cfg.perf_ceil);
            let mut cluster = ClusterState {
                policy_id: pid,
                affected_cpus: affected.clone(),
                available_freqs: freqs,
                cached_ratios,
                _freq_min: fmin,
                _freq_max: fmax,
                max_writer,
                min_writer,
                current_perf: init_perf,
                current_freq: 0,
                down_wait: 0,
            };

            let init_freq = cluster.find_nearest_freq(init_perf);
            cluster.max_writer.write_value_force(init_freq);
            cluster.min_writer.write_value_force(init_freq);
            cluster.current_freq = init_freq;

            info!("{}", t_with_args("clg-init", &fluent_args!(
                "pid" => pid.to_string(),
                "cpus" => format!("{:?}", affected),
                "fmin" => (fmin / 1000.0).to_string(),
                "fmax" => (fmax / 1000.0).to_string(),
                "perf" => format!("{:.2}", init_perf),
                "freq" => (init_freq / 1000).to_string()
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
        self.clusters.clear();
        self.active = false;
        self.log_counter = 0;
    }

    pub fn reload_config(&mut self, gov_cfg: &CpuLoadGovernorConfig) {
        self.cfg = gov_cfg.clone();
        debug!("{}", t("clg-config-reloaded"));
    }

    pub fn on_load_update(&mut self, core_utils: &[f32]) {
        if !self.active { return; }

        for cluster in &mut self.clusters {
            let util = cluster.max_util(core_utils);
            let target_perf = (util * self.cfg.headroom_factor)
                .clamp(self.cfg.perf_floor, self.cfg.perf_ceil);
            let old_perf = cluster.current_perf;

            if target_perf > old_perf {
                cluster.down_wait = 0;

                let is_high_load = util >= self.cfg.up_threshold; 
                let is_significant_jump = target_perf > old_perf + 0.20; 

                if is_high_load || is_significant_jump {
                    cluster.current_perf += (target_perf - old_perf) * self.cfg.smoothing_up;
                } else {
                    cluster.current_perf += (target_perf - old_perf) * (self.cfg.smoothing_up * 0.05); 
                }
            } else {
                cluster.down_wait += 1;
                if cluster.down_wait >= self.cfg.down_rate_limit_ticks {
                    if util < self.cfg.down_threshold {
                        let active_smoothing_down = if util < 0.10 {
                            self.cfg.smoothing_down * 2.5
                        } else {
                            self.cfg.smoothing_down
                        };
                        cluster.current_perf += (target_perf - old_perf) * active_smoothing_down;
                    }
                }
            }

            cluster.current_perf = cluster.current_perf.clamp(self.cfg.perf_floor, self.cfg.perf_ceil);
            let target_freq = cluster.find_nearest_freq(cluster.current_perf);
            cluster.write_freq(target_freq);
        }

        self.log_counter += 1;
        if self.log_counter % 25 == 0 {
            for c in &self.clusters {
                debug!("{}", t_with_args("clg-tick-log", &fluent_args!(
                    "pid" => c.policy_id.to_string(),
                    "util" => format!("{:.0}", c.max_util(core_utils) * 100.0),
                    "perf" => format!("{:.2}", c.current_perf),
                    "freq" => (c.current_freq / 1000).to_string(),
                    "boost" => ""
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