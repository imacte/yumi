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

use std::error::Error;
use std::thread;
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use log::{error, info};

pub mod config;
pub mod app_detect;
pub mod screen_detect;
pub mod fps_monitor;
pub mod cpu_monitor;

use crate::common::DaemonEvent;
use crate::fluent_args;
use crate::i18n::{t, t_with_args};

// 启动函数
pub fn start_monitor(tx: Sender<DaemonEvent>) -> Result<(), Box<dyn Error>> {
    info!("{}", t("monitor-starting"));

    // --- 初始化共享配置 ---
    let rules_path = config::get_rules_path();
    
    // --- 初始化配置 ---
    let initial_config = config::read_config(&rules_path) 
                            .unwrap_or_else(|e| {
                                log::warn!("{}", t_with_args("monitor-initial-config-failed", &fluent_args!("error" => e.to_string())));
                                app_detect::get_default_rules()
                            });

    let config_arc = Arc::new(Mutex::new(initial_config));
    let config_arc_clone_for_watcher = Arc::clone(&config_arc);

    // --- 初始化共享的屏幕状态 ---
    let screen_state_arc = Arc::new(Mutex::new(true));
    let screen_state_clone_for_watcher = Arc::clone(&screen_state_arc);
    let screen_state_clone_for_app_detect = Arc::clone(&screen_state_arc);

    // 初始化共享的强制刷新标志
    let force_refresh_arc = Arc::new(AtomicBool::new(false));
    let force_refresh_clone_for_watcher = Arc::clone(&force_refresh_arc);

    // 3. 启动屏幕状态监控线程
    thread::Builder::new()
        .name("screen_watcher".to_string())
        .spawn(move || {
            if let Err(e) = screen_detect::monitor_screen_state_uevent(screen_state_clone_for_watcher) {
                error!("{}", t_with_args("monitor-screen-watcher-failed", &fluent_args!("error" => e.to_string())));
            }
        })?;

    // 4. 启动配置监控线程
    let tx_config = tx.clone();
    thread::Builder::new()
        .name("config_watcher".to_string())
        .spawn(move || {
            if let Err(e) = app_detect::watch_config_file(
                config_arc_clone_for_watcher,
                force_refresh_clone_for_watcher,
                tx_config
            ) {
                error!("{}", t_with_args("monitor-config-watcher-failed", &fluent_args!("error" => e.to_string())));
            }
        })?;

    // 5. 启动 eBPF FPS 监控线程 (带有独立的 Tokio 运行时)
    let tx_fps = tx.clone();
    thread::Builder::new()
        .name("fps_monitor_ebpf".to_string())
        .spawn(move || {
            if let Ok(rt) = tokio::runtime::Runtime::new() {
                rt.block_on(async {
                    if let Err(e) = fps_monitor::start_fps_loop(tx_fps).await {
                        error!("{}", t_with_args("monitor-fps-crashed", &fluent_args!("error" => e.to_string())));
                    }
                });
            } else {
                error!("{}", t("monitor-fps-tokio-failed"));
            }
        })?;

    // 6. 启动 eBPF CPU 负载监控线程
    let tx_cpu = tx.clone();
    thread::Builder::new()
        .name("cpu_monitor_ebpf".to_string())
        .spawn(move || {
            if let Ok(rt) = tokio::runtime::Runtime::new() {
                rt.block_on(async {
                    if let Err(e) = cpu_monitor::start_cpu_loop(tx_cpu).await {
                        error!("{}", t_with_args("monitor-cpu-crashed", &fluent_args!("error" => e.to_string())));
                    }
                });
            } else {
                error!("{}", t("monitor-cpu-tokio-failed"));
            }
        })?;

    // 7. 启动应用检测主循环 (阻塞)
    app_detect::app_detection_loop(
        config_arc,
        screen_state_clone_for_app_detect,
        force_refresh_arc,
        tx
    )?;

    Ok(())
}