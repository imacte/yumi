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
    #[serde(default = "default_true")] pub enabled: bool,
    #[serde(default = "d_clg_up_thresh")] pub up_threshold: f32,
    #[serde(default = "d_clg_down_thresh")] pub down_threshold: f32,
    #[serde(default = "d_clg_smooth_up")] pub smoothing_up: f32,
    #[serde(default = "d_clg_smooth_down")] pub smoothing_down: f32,
    #[serde(default = "d_clg_down_rate")] pub down_rate_limit_ticks: u32,
    #[serde(default = "d_clg_headroom")] pub headroom_factor: f32,
    #[serde(default = "d_clg_floor")] pub perf_floor: f32,
    #[serde(default = "d_clg_ceil")] pub perf_ceil: f32,
    #[serde(default = "d_clg_init")] pub perf_init: f32,
}

fn default_true() -> bool { true }
fn d_clg_up_thresh() -> f32 { 0.80 }
fn d_clg_down_thresh() -> f32 { 0.50 }
fn d_clg_smooth_up() -> f32 { 0.60 }
fn d_clg_smooth_down() -> f32 { 0.30 }
fn d_clg_down_rate() -> u32 { 3 }
fn d_clg_headroom() -> f32 { 1.25 }
fn d_clg_floor() -> f32 { 0.15 }
fn d_clg_ceil() -> f32 { 1.0 }
fn d_clg_init() -> f32 { 0.50 }

impl Default for CpuLoadGovernorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            up_threshold: d_clg_up_thresh(),
            down_threshold: d_clg_down_thresh(),
            smoothing_up: d_clg_smooth_up(),
            smoothing_down: d_clg_smooth_down(),
            down_rate_limit_ticks: d_clg_down_rate(),
            headroom_factor: d_clg_headroom(),
            perf_floor: d_clg_floor(),
            perf_ceil: d_clg_ceil(),
            perf_init: d_clg_init(),
        }
    }
}

// ════════════════════════════════════════════════════════════════
//  核心模式与杂项配置
// ════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Mode {
    #[serde(default)]
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