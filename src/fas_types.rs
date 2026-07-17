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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ════════════════════════════════════════════════════════════════
//  PID 系数 (60fps 基准值，运行时根据 target_fps 动态缩放)
//
//  kp: 比例增益 — 按 target_fps/60 线性缩放
//  ki: 积分增益 — 按 sqrt(target_fps/60) 缩放（防高刷积分饱和）
//  kd: 微分增益 — 按 (target_fps/60)^0.3 缩放（高刷噪声大）
// ════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PidCoefficients {
    /// 比例增益基准 (60fps)，高刷时自动放大
    #[serde(default = "default_kp")]  pub kp: f32,
    /// 积分增益基准 (60fps)，高刷时缓增
    #[serde(default = "default_ki")]  pub ki: f32,
    /// 微分增益基准 (60fps)，高刷时微增
    #[serde(default = "default_kd")]  pub kd: f32,
}
fn default_kp() -> f32 { 0.050 }
fn default_ki() -> f32 { 0.010 }
fn default_kd() -> f32 { 0.006 }
impl Default for PidCoefficients {
    fn default() -> Self { Self { kp: default_kp(), ki: default_ki(), kd: default_kd() } }
}

// ════════════════════════════════════════════════════════════════
//  Cluster 配置
// ════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClusterProfile {
    #[serde(default = "default_capacity_weight")]
    pub capacity_weight: f32,
}
fn default_capacity_weight() -> f32 { 1.0 }
impl Default for ClusterProfile {
    fn default() -> Self { Self { capacity_weight: 1.0 } }
}
pub fn default_cluster_profiles() -> Vec<ClusterProfile> {
    vec![
        ClusterProfile { capacity_weight: 1.0 },
        ClusterProfile { capacity_weight: 1.5 },
        ClusterProfile { capacity_weight: 2.5 },
        ClusterProfile { capacity_weight: 3.5 },
    ]
}

// ════════════════════════════════════════════════════════════════
//  Per-App 配置
// ════════════════════════════════════════════════════════════════

/// 每个游戏的配置档案
///
/// 只需要指定 target_fps 数组，
/// 运行时根据实际帧率动态匹配最近的档位。
///
/// YAML 示例:
/// ```yaml
/// per_app_profiles:
///   "com.miHoYo.GenshinImpact":
///     target_fps: [30, 60]
///     fps_margin: 4.0
///
///   "com.tencent.tmgp.sgame":
///     target_fps: [60, 90, 120]
///     fps_margin: 3.0
/// ```
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PerAppProfile {
    /// 该游戏会渲染到的目标帧率数组，运行时动态匹配
    /// 例如 [30, 60] 表示游戏可能以 30fps 或 60fps 渲染
    #[serde(default)]
    pub target_fps: Option<Vec<f32>>,

    /// 该应用的帧率余量（覆盖全局 fps_margin）
    #[serde(default)]
    pub fps_margin: Option<f32>,
}

// ════════════════════════════════════════════════════════════════
//  FAS Rules 配置
// ════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FasRulesConfig {
    #[serde(default = "default_fps_gears")]       pub fps_gears: Vec<f32>,
    #[serde(default = "default_fps_margin")]       pub fps_margin: f32,
    #[serde(default)]                              pub pid: PidCoefficients,
    #[serde(default = "default_cluster_profiles")] pub cluster_profiles: Vec<ClusterProfile>,
    #[serde(default = "d_auto_cap")]               pub auto_capacity_weight: bool,

    #[serde(default = "d_perf_floor")]   pub perf_floor: f32,
    #[serde(default = "d_perf_ceil")]    pub perf_ceil: f32,
    #[serde(default = "d_perf_init")]    pub perf_init: f32,
    #[serde(default = "d_perf_cold")]    pub perf_cold_boot: f32,
    #[serde(default = "d_hysteresis")]   pub freq_hysteresis: f32,

    #[serde(default = "d_heavy_ms")]     pub heavy_frame_threshold_ms: f32,
    #[serde(default = "d_load_ms")]      pub loading_cumulative_ms: f32,
    #[serde(default = "d_load_tol")]     pub loading_normal_tolerance: u32,
    #[serde(default = "d_load_pf")]      pub loading_perf_floor: f32,
    #[serde(default = "d_load_pc")]      pub loading_perf_ceiling: f32,

    #[serde(default = "d_post_ign")]     pub post_loading_ignore_frames: u32,
    #[serde(default = "d_post_perf")]    pub post_loading_perf: f32,
    #[serde(default = "d_post_guard")]   pub post_loading_downgrade_guard: u32,

    #[serde(default = "d_up_confirm")]   pub upgrade_confirm_frames: u32,
    #[serde(default = "d_dn_confirm")]   pub downgrade_confirm_frames: u32,
    #[serde(default = "d_up_cd")]        pub upgrade_cooldown_after_downgrade: u32,
    #[serde(default = "d_dampen")]       pub gear_dampen_frames: u32,

    #[serde(default = "d_boost_inc")]    pub downgrade_boost_perf_inc: f32,
    #[serde(default = "d_boost_dur")]    pub downgrade_boost_duration: u32,

    #[serde(default = "d_fd_thresh")]    pub fast_decay_frame_threshold: u32,
    #[serde(default = "d_fd_perf")]      pub fast_decay_perf_threshold: f32,
    #[serde(default = "d_fd_max")]       pub fast_decay_max_step: f32,
    #[serde(default = "d_fd_min")]       pub fast_decay_min_step: f32,

    #[serde(default = "d_jank_cd")]      pub jank_cooldown_frames: u32,

    #[serde(default = "d_max_inc_d")]    pub max_inc_damped: f32,
    #[serde(default = "d_max_inc_n")]    pub max_inc_normal: f32,
    #[serde(default = "d_damped_cap")]   pub damped_perf_cap: f32,

    #[serde(default = "d_switch_ms")]    pub app_switch_gap_ms: f32,
    #[serde(default = "d_switch_perf")]  pub app_switch_resume_perf: f32,

    #[serde(default = "d_force_int")]    pub freq_force_reapply_interval: u32,
    #[serde(default = "d_max_frame")]    pub fixed_max_frame_ms: f32,
    #[serde(default = "d_cold_ms")]      pub cold_boot_ms: u64,

    #[serde(default = "d_verify_interval")]
    pub verify_freq_interval_secs: u32,

    #[serde(default)]
    pub per_app_profiles: HashMap<String, PerAppProfile>,

    #[serde(default)]
    pub per_app_margins: HashMap<String, f32>,

    /// 温度降频阈值（℃），0 = 禁用
    #[serde(default = "d_temp_thresh")]
    pub core_temp_threshold: f64,

    /// 温度降频时的最低 perf
    #[serde(default = "d_temp_perf")]
    pub core_temp_throttle_perf: f32,

    /// CPU 负载辅助：前台线程利用率封顶的除数 (越小越激进)
    #[serde(default = "d_util_cap_divisor")]
    pub util_cap_divisor: f32,
}

pub fn default_fps_gears() -> Vec<f32> { vec![30.0, 60.0, 90.0, 120.0, 144.0] }
pub fn default_fps_margin() -> f32 { 3.0 }
fn d_auto_cap() -> bool { true }
fn d_perf_floor() -> f32 { 0.22 }
fn d_perf_ceil() -> f32 { 1.0 }
fn d_perf_init() -> f32 { 0.45 }
fn d_perf_cold() -> f32 { 0.85 }
pub fn d_hysteresis() -> f32 { 0.015 }
pub fn d_heavy_ms() -> f32 { 150.0 }
pub fn d_load_ms() -> f32 { 2500.0 }
fn d_load_tol() -> u32 { 3 }
fn d_load_pf() -> f32 { 0.60 }
fn d_load_pc() -> f32 { 0.70 }
pub fn d_post_ign() -> u32 { 5 }
pub fn d_post_perf() -> f32 { 0.65 }
fn d_post_guard() -> u32 { 90 }
fn d_up_confirm() -> u32 { 60 }
fn d_dn_confirm() -> u32 { 90 }
fn d_up_cd() -> u32 { 90 }
fn d_dampen() -> u32 { 60 }
fn d_boost_inc() -> f32 { 0.18 }
fn d_boost_dur() -> u32 { 45 }
fn d_fd_thresh() -> u32 { 75 }
fn d_fd_perf() -> f32 { 0.70 }
fn d_fd_max() -> f32 { 0.022 }
fn d_fd_min() -> f32 { 0.004 }
fn d_jank_cd() -> u32 { 15 }
fn d_max_inc_d() -> f32 { 0.045 }
fn d_max_inc_n() -> f32 { 0.075 }
fn d_damped_cap() -> f32 { 0.92 }
fn d_switch_ms() -> f32 { 3000.0 }
fn d_switch_perf() -> f32 { 0.60 }
fn d_force_int() -> u32 { 30 }
fn d_max_frame() -> f32 { 500.0 }
fn d_cold_ms() -> u64 { 3500 }
fn d_verify_interval() -> u32 { 3 }
fn d_temp_thresh() -> f64 { 0.0 }
fn d_temp_perf() -> f32 { 0.70 }
fn d_util_cap_divisor() -> f32 { 0.45 }

impl FasRulesConfig {
    /// 将旧的 per_app_margins 迁移到 per_app_profiles
    pub fn migrate_legacy_margins(&mut self) {
        for (pkg, margin) in self.per_app_margins.drain() {
            self.per_app_profiles
                .entry(pkg)
                .or_default()
                .fps_margin = Some(margin);
        }
    }
}

impl Default for FasRulesConfig {
    fn default() -> Self {
        Self {
            fps_gears: default_fps_gears(), fps_margin: default_fps_margin(),
            pid: PidCoefficients::default(),
            cluster_profiles: default_cluster_profiles(),
            auto_capacity_weight: d_auto_cap(),
            perf_floor: d_perf_floor(), perf_ceil: d_perf_ceil(),
            perf_init: d_perf_init(), perf_cold_boot: d_perf_cold(),
            freq_hysteresis: d_hysteresis(),
            heavy_frame_threshold_ms: d_heavy_ms(),
            loading_cumulative_ms: d_load_ms(),
            loading_normal_tolerance: d_load_tol(),
            loading_perf_floor: d_load_pf(), loading_perf_ceiling: d_load_pc(),
            post_loading_ignore_frames: d_post_ign(),
            post_loading_perf: d_post_perf(),
            post_loading_downgrade_guard: d_post_guard(),
            upgrade_confirm_frames: d_up_confirm(),
            downgrade_confirm_frames: d_dn_confirm(),
            upgrade_cooldown_after_downgrade: d_up_cd(),
            gear_dampen_frames: d_dampen(),
            downgrade_boost_perf_inc: d_boost_inc(),
            downgrade_boost_duration: d_boost_dur(),
            fast_decay_frame_threshold: d_fd_thresh(),
            fast_decay_perf_threshold: d_fd_perf(),
            fast_decay_max_step: d_fd_max(), fast_decay_min_step: d_fd_min(),
            jank_cooldown_frames: d_jank_cd(),
            max_inc_damped: d_max_inc_d(), max_inc_normal: d_max_inc_n(),
            damped_perf_cap: d_damped_cap(),
            app_switch_gap_ms: d_switch_ms(), app_switch_resume_perf: d_switch_perf(),
            freq_force_reapply_interval: d_force_int(),
            fixed_max_frame_ms: d_max_frame(), cold_boot_ms: d_cold_ms(),
            verify_freq_interval_secs: d_verify_interval(),
            per_app_profiles: HashMap::new(),
            per_app_margins: HashMap::new(),
            core_temp_threshold: d_temp_thresh(),
            core_temp_throttle_perf: d_temp_perf(),
            util_cap_divisor: d_util_cap_divisor(),
        }
    }
}
