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

mod common;
mod logger;
mod monitor;
mod scheduler;
pub mod i18n;
pub mod utils;
use std::sync::mpsc;
use std::thread;
use anyhow::Result;
use log::{info, error};
use crate::i18n::{t, t_with_args, load_language};
use crate::scheduler::config::Config;

fn main() -> Result<()> {
    // 1. 环境初始化
    if let Some(path) = std::env::args().nth(1) {
        nix::unistd::chdir(path.as_str())?;
    }

    let root = common::get_module_root();
    let log_dir = root.join("logs");
    std::fs::create_dir_all(&log_dir)?;
    
    
    // 2. 提前读取配置
    let config_path: std::path::PathBuf = root.join("config/config.yaml");
    let config = Config::from_file(config_path.to_str().unwrap()).unwrap_or_default();

    // 3. 立即加载语言
    load_language(&config.meta.language);

    // 4. 初始化日志
    logger::init(&config.meta.loglevel)?; 
    
    info!("{}", t("yumi-module-starting"));

    // 3. 创建通信通道
    let (tx, rx) = mpsc::channel::<common::DaemonEvent>();

    // 4. 启动 Scheduler
    if let Err(e) = scheduler::start_scheduler_thread(rx) {
        error!("{}", t_with_args("scheduler-module-start-failed", &fluent_args!("error" => e.to_string())));
        return Err(e);
    }
    info!("{}", t("scheduler-module-started"));

    // 5. 启动 Monitor
    let monitor_thread = thread::Builder::new()
        .name("monitor_core".to_string())
        .spawn(move || {
            if let Err(e) = monitor::start_monitor(tx) {
                error!("{}", t_with_args("monitor-module-crashed", &fluent_args!("error" => e.to_string())));
            }
        })?;
    
    info!("{}", t("monitor-module-started"));

    // 6. 挂起
    monitor_thread.join().unwrap();

    Ok(())
}