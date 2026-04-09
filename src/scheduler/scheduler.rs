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

use super::config::Config;
use super::utils::SysPathExist;
use anyhow::Result;
use std::fs;
use std::sync::{Arc, Mutex, RwLock};

use crate::i18n::{t, t_with_args};
use crate::fluent_args; 
use crate::utils; 

pub struct CpuScheduler {
    config: Arc<RwLock<Config>>,
    current_mode_name: Arc<Mutex<String>>,
    sys_path_exist: Arc<SysPathExist>,
}

impl CpuScheduler {
    pub fn new(
        config: Arc<RwLock<Config>>,
        initial_mode: Arc<Mutex<String>>,
        sys_path_exist: Arc<SysPathExist>,
    ) -> Self {
        Self {
            config,
            current_mode_name: initial_mode,
            sys_path_exist,
        }
    }

    /// 应用所有与当前性能模式相关的设置 (非频率)
    pub fn apply_all_settings(&self) -> Result<()> {
        let mode_name = self.current_mode_name.lock().unwrap().clone();

        if mode_name == "fas" {
            log::debug!("Currently in FAS mode, Scheduler is skipping static settings application.");
            return Ok(());
        }

        log::info!("{}", t_with_args(
            "apply-settings-for-mode",
            &fluent_args!{"mode" => mode_name.as_str()}
        ));
            
        log::info!("{}", t_with_args(
            "settings-applied-success",
            &fluent_args!{"mode" => mode_name.as_str()}
        ));
        Ok(())
    }

    /// 应用所有一次性的、与模式无关的系统调整
    pub fn apply_system_tweaks(&self) -> Result<()> {
        self.apply_cpu_idle_governor()?;
        self.apply_io_settings()?;
        Ok(())
    }

    fn apply_cpu_idle_governor(&self) -> Result<()> {
        let config = self.config.read().unwrap();
        if config.function.cpu_idle_scaling_governor && !config.cpu_idle.current_governor.is_empty() {
            if self.sys_path_exist.cpuidle_governor_exist {
                let _ = utils::try_write_file("/sys/devices/system/cpu/cpuidle/current_governor", &config.cpu_idle.current_governor);
            }
        }
        log::info!("{}",t("apply-cpu-idle-governor-start"));
        Ok(())
    }

    fn apply_io_settings(&self) -> Result<()> {
        let config = self.config.read().unwrap();
        if !config.function.io_optimization {
            log::info!("{}", t("apply-io-settings-start"));
            return Ok(());
        }

        let io = &config.io_settings;
        let block_dir = std::path::Path::new("/sys/block");
        if !block_dir.exists() {
            log::warn!("IOOptimization: /sys/block does not exist, skipping");
            return Ok(());
        }

        if let Ok(entries) = fs::read_dir(block_dir) {
            for entry in entries.flatten() {
                let dev_path = entry.path();
                let queue_path = dev_path.join("queue");
                if !queue_path.exists() { continue; }

                if !io.scheduler.is_empty() {
                    let p = queue_path.join("scheduler");
                    if p.exists() { let _ = utils::try_write_file(&p, &io.scheduler); }
                }
                if !io.read_ahead_kb.is_empty() {
                    let p = queue_path.join("read_ahead_kb");
                    if p.exists() { let _ = utils::try_write_file(&p, &io.read_ahead_kb); }
                }
                if !io.nomerges.is_empty() {
                    let p = queue_path.join("nomerges");
                    if p.exists() { let _ = utils::try_write_file(&p, &io.nomerges); }
                }
                if !io.iostats.is_empty() {
                    let p = queue_path.join("iostats");
                    if p.exists() { let _ = utils::try_write_file(&p, &io.iostats); }
                }
                log::debug!("IOOptimization: applied to {:?}", dev_path.file_name().unwrap_or_default());
            }
        }

        log::info!("{}", t("apply-io-settings-start"));
        Ok(())
    }
}