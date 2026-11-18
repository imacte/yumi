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

use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct Meta {
    #[serde(default = "default_loglevel", alias = "Loglevel")]
    pub loglevel: String,
    
    #[serde(default = "default_language", alias = "Language")]
    pub language: String,
}

fn default_loglevel() -> String { "INFO".to_string() }
fn default_language() -> String { "en".to_string() }

// ════════════════════════════════════════════════════════════════
//  CPU Load Governor 配置
// ════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, Clone)]
pub struct CpuLoadGovernorConfig {
    #[serde(default = "crate::utils::default_true")] pub enabled: bool,
    #[serde(default)] pub governor: Option<String>,
    #[serde(default = "d_clg_freq_range_steps_below")] pub freq_range_steps_below: u32,
    #[serde(default = "d_clg_up_thresh")] pub up_threshold: f32,
    #[serde(default = "d_clg_down_thresh")] pub down_threshold: f32,
    #[serde(default = "d_clg_smooth_up")] pub smoothing_up: f32,
    #[serde(default = "d_clg_smooth_down")] pub smoothing_down: f32,
    #[serde(default = "d_clg_down_rate")] pub down_rate_limit_ticks: u32,
    #[serde(default = "d_clg_up_rate")] pub up_rate_limit_ticks: u32,
    #[serde(default = "d_clg_headroom")] pub headroom_factor: f32,
    /// headroom 在 up_threshold 附近的过渡带宽度：从 up_threshold - headroom_ramp
    /// 到 up_threshold 线性由 1.0 渐变至 headroom_factor，避免阶跃导致振荡
    #[serde(default = "d_clg_headroom_ramp")] pub headroom_ramp: f32,
    #[serde(default = "d_clg_floor")] pub perf_floor: f32,
    #[serde(default = "d_clg_ceil")] pub perf_ceil: f32,
    #[serde(default = "d_clg_init")] pub perf_init: f32,
    /// 升频快速通道判定：target_perf 超过 current_perf 的幅度大于此值时直接快速升频
    #[serde(default = "d_clg_up_jump")] pub up_jump_threshold: f32,
    /// 低负载升频（负载未达 up_threshold 时）对 smoothing_up 的缩放系数
    #[serde(default = "d_clg_slow_up_scale")] pub slow_up_scale: f32,
    /// 滞回带内（down_threshold..up_threshold）降频时对 smoothing_down 的缩放系数，
    /// 用于防抖并避免高频锁定
    #[serde(default = "d_clg_slow_down_scale")] pub slow_down_scale: f32,
    /// 极低负载阈值：util 低于此值触发快速降频
    #[serde(default = "d_clg_down_fast_thresh")] pub down_fast_threshold: f32,
    /// 快速降频时对 smoothing_down 的放大倍数
    #[serde(default = "d_clg_down_fast_mult")] pub down_fast_mult: f32,
    /// 尖峰抑制：单 tick util 跳升超过此值时，其增量按 spike_decay 比例衰减，
    /// 避免孤立瞬时尖峰（如单核 0↔100%）瞬间拉满 perf
    #[serde(default = "d_clg_spike_jump")] pub spike_jump_threshold: f32,
    /// 尖峰增量保留比例（0.0=完全抑制，1.0=不抑制）
    #[serde(default = "d_clg_spike_decay")] pub spike_decay: f32,
}

fn d_clg_freq_range_steps_below() -> u32 { 2 }
fn d_clg_up_thresh() -> f32 { 0.80 }
fn d_clg_down_thresh() -> f32 { 0.50 }
fn d_clg_smooth_up() -> f32 { 0.60 }
fn d_clg_smooth_down() -> f32 { 0.30 }
fn d_clg_down_rate() -> u32 { 3 }
fn d_clg_up_rate() -> u32 { 2 }
fn d_clg_headroom() -> f32 { 1.25 }
fn d_clg_headroom_ramp() -> f32 { 0.15 }
fn d_clg_floor() -> f32 { 0.15 }
fn d_clg_ceil() -> f32 { 1.0 }
fn d_clg_init() -> f32 { 0.50 }
fn d_clg_up_jump() -> f32 { 0.35 }
fn d_clg_slow_up_scale() -> f32 { 0.02 }
fn d_clg_slow_down_scale() -> f32 { 0.5 }
fn d_clg_down_fast_thresh() -> f32 { 0.10 }
fn d_clg_down_fast_mult() -> f32 { 2.5 }
fn d_clg_spike_jump() -> f32 { 0.35 }
fn d_clg_spike_decay() -> f32 { 0.30 }

impl Default for CpuLoadGovernorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            governor: None,
            freq_range_steps_below: d_clg_freq_range_steps_below(),
            up_threshold: d_clg_up_thresh(),
            down_threshold: d_clg_down_thresh(),
            smoothing_up: d_clg_smooth_up(),
            smoothing_down: d_clg_smooth_down(),
            down_rate_limit_ticks: d_clg_down_rate(),
            up_rate_limit_ticks: d_clg_up_rate(),
            headroom_factor: d_clg_headroom(),
            headroom_ramp: d_clg_headroom_ramp(),
            perf_floor: d_clg_floor(),
            perf_ceil: d_clg_ceil(),
            perf_init: d_clg_init(),
            up_jump_threshold: d_clg_up_jump(),
            slow_up_scale: d_clg_slow_up_scale(),
            slow_down_scale: d_clg_slow_down_scale(),
            down_fast_threshold: d_clg_down_fast_thresh(),
            down_fast_mult: d_clg_down_fast_mult(),
            spike_jump_threshold: d_clg_spike_jump(),
            spike_decay: d_clg_spike_decay(),
        }
    }
}

impl CpuLoadGovernorConfig {
    /// 校验并规范化配置：
    /// - 非有限值（NaN/±Inf，如 YAML 溢出值）回退默认，防止污染控制链
    /// - 阈值/系数限制在合理区间
    /// - floor/ceil/init 交叉约束，保证 f32::clamp 永不 panic
    pub fn normalize(&mut self) {
        if !self.up_threshold.is_finite() { self.up_threshold = d_clg_up_thresh(); }
        if !self.down_threshold.is_finite() { self.down_threshold = d_clg_down_thresh(); }
        if !self.smoothing_up.is_finite() { self.smoothing_up = d_clg_smooth_up(); }
        if !self.smoothing_down.is_finite() { self.smoothing_down = d_clg_smooth_down(); }
        if !self.headroom_factor.is_finite() { self.headroom_factor = d_clg_headroom(); }
        if !self.headroom_ramp.is_finite() { self.headroom_ramp = d_clg_headroom_ramp(); }
        if !self.perf_floor.is_finite() { self.perf_floor = d_clg_floor(); }
        if !self.perf_ceil.is_finite() { self.perf_ceil = d_clg_ceil(); }
        if !self.perf_init.is_finite() { self.perf_init = d_clg_init(); }
        if !self.up_jump_threshold.is_finite() { self.up_jump_threshold = d_clg_up_jump(); }
        if !self.slow_up_scale.is_finite() { self.slow_up_scale = d_clg_slow_up_scale(); }
        if !self.slow_down_scale.is_finite() { self.slow_down_scale = d_clg_slow_down_scale(); }
        if !self.down_fast_threshold.is_finite() { self.down_fast_threshold = d_clg_down_fast_thresh(); }
        if !self.down_fast_mult.is_finite() { self.down_fast_mult = d_clg_down_fast_mult(); }
        if !self.spike_jump_threshold.is_finite() { self.spike_jump_threshold = d_clg_spike_jump(); }
        if !self.spike_decay.is_finite() { self.spike_decay = d_clg_spike_decay(); }

        // 区间限制（语义约束）
        self.up_threshold = self.up_threshold.clamp(0.0, 1.0);
        self.down_threshold = self.down_threshold.clamp(0.0, 1.0);
        // 滞回语义：降频阈值不得高于升频阈值
        if self.down_threshold > self.up_threshold {
            self.down_threshold = self.up_threshold;
        }
        self.smoothing_up = self.smoothing_up.clamp(0.0, 1.0);
        self.smoothing_down = self.smoothing_down.clamp(0.0, 1.0);
        self.slow_up_scale = self.slow_up_scale.clamp(0.0, 1.0);
        self.slow_down_scale = self.slow_down_scale.clamp(0.0, 1.0);
        self.up_jump_threshold = self.up_jump_threshold.clamp(0.0, 1.0);
        self.down_fast_threshold = self.down_fast_threshold.clamp(0.0, 1.0);
        self.spike_jump_threshold = self.spike_jump_threshold.clamp(0.0, 1.0);
        self.spike_decay = self.spike_decay.clamp(0.0, 1.0);
        self.headroom_ramp = self.headroom_ramp.clamp(0.0, 1.0);
        // headroom 语义 >= 1（余量放大），down_fast_mult 语义 >= 1（放大）
        self.headroom_factor = self.headroom_factor.clamp(1.0, 3.0);
        self.down_fast_mult = self.down_fast_mult.clamp(1.0, 10.0);

        // 交叉约束（顺序保证 clamp 边界合法）
        if self.perf_floor > self.perf_ceil {
            self.perf_floor = self.perf_ceil;
        }
        self.perf_floor = self.perf_floor.clamp(0.0, 1.0);
        self.perf_ceil = self.perf_ceil.clamp(0.0, 1.0);
        if self.perf_floor > self.perf_ceil {
            self.perf_floor = self.perf_ceil;
        }
        self.perf_init = self.perf_init.clamp(self.perf_floor, self.perf_ceil);
    }
}

// ════════════════════════════════════════════════════════════════
//  核心模式与杂项配置
// ════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Mode {
    #[serde(default, alias = "CpuLoadGovernor")]
    pub cpu_load_governor: CpuLoadGovernorConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct IOSettings {
    #[serde(default, rename = "Scheduler")] pub scheduler: String,
    #[serde(default = "default_read_ahead_kb")] pub read_ahead_kb: String,
    #[serde(default = "default_nomerges")] pub nomerges: String,
    #[serde(default = "default_iostats")] pub iostats: String,
}

impl Default for IOSettings {
    fn default() -> Self {
        Self {
            scheduler: String::new(),
            read_ahead_kb: default_read_ahead_kb(),
            nomerges: default_nomerges(),
            iostats: default_iostats(),
        }
    }
}

fn default_read_ahead_kb() -> String { "128".to_string() }
fn default_nomerges() -> String { "2".to_string() }
fn default_iostats() -> String { "0".to_string() }

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct CpuIdle {
    pub current_governor: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct FunctionToggles {
    #[serde(rename = "CpuIdleScalingGovernor")] pub cpu_idle_scaling_governor: bool,
    #[serde(rename = "IOOptimization")] pub io_optimization: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    #[serde(default, alias = "Meta")]
    pub meta: Meta,
    #[serde(default)]
    pub function: FunctionToggles,
    #[serde(default, rename = "IO_Settings")]
    pub io_settings: IOSettings,
    #[serde(default, rename = "CpuIdle")]
    pub cpu_idle: CpuIdle,
    
    // 按场景划分的性能模式
    #[serde(default)] pub powersave: Mode,
    #[serde(default)] pub balance: Mode,
    #[serde(default)] pub performance: Mode,
    #[serde(default)] pub fast: Mode,
}

impl Config {
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    pub fn get_mode(&self, mode_name: &str) -> Option<&Mode> {
        match mode_name {
            "powersave" => Some(&self.powersave),
            "balance" => Some(&self.balance),
            "performance" => Some(&self.performance),
            "fast" => Some(&self.fast),
            _ => None,
        }
    }
}