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
use std::path::PathBuf;
use crate::common;
pub use crate::fas_types::FasRulesConfig;

pub fn get_rules_path() -> PathBuf { common::get_module_root().join("rules.yaml") }

// ════════════════════════════════════════════════════════════════
//  Rules 配置
// ════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RulesConfig {
    #[serde(default = "crate::utils::default_true")] pub yumi_scheduler: bool,
    pub dynamic_enabled: bool,
    pub global_mode: String,
    pub app_modes: HashMap<String, String>,
    #[serde(default)] pub ignored_apps: Vec<String>,
    #[serde(default)] pub fas_rules: FasRulesConfig,
}