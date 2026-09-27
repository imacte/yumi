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

use std::sync::{Arc, Mutex, RwLock, mpsc};
use std::thread;
use std::time::Instant;
use std::fs;
use anyhow::Result;

pub mod config;
pub mod scheduler;
pub mod fas;
pub mod cpu_load_governor;
mod monitor_health;

use crate::i18n::{t, load_language, t_with_args};
use crate::fluent_args; 
use crate::utils; 
use crate::common::DaemonEvent; 
use config::Config;
use scheduler::CpuScheduler;
use crate::logger;
use crate::common;

/// CPU 频率策略簇信息
pub struct CpuPolicy {
    pub id: i32,
    /// boost 频率列表（单位 kHz），有的簇没有此文件则为空
    pub boost_frequencies: Vec<u32>,
}

// 动态获取系统中实际可用的 CPU Policy，并读取 boost 频率
pub fn get_cpu_policies() -> Vec<CpuPolicy> {
    let mut policies = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/sys/devices/system/cpu/cpufreq") {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with("policy") {
                    if let Ok(pid) = name["policy".len()..].parse::<i32>() {
                        let boost_freqs = read_boost_frequencies(pid);
                        policies.push(CpuPolicy {
                            id: pid,
                            boost_frequencies: boost_freqs,
                        });
                    }
                }
            }
        }
    }
    policies.sort_unstable_by_key(|p| p.id);
    policies
}

fn read_boost_frequencies(pid: i32) -> Vec<u32> {
    let path = format!(
        "/sys/devices/system/cpu/cpufreq/policy{}/scaling_boost_frequencies",
        pid
    );
    std::fs::read_to_string(&path)
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect()
}

/// 通过 sysfs 探测指定 policy 的 capacity 值
pub(super) fn probe_policy_capacity(policy_id: i32) -> Option<u32> {
    let related_str = fs::read_to_string(
        format!("/sys/devices/system/cpu/cpufreq/policy{}/related_cpus", policy_id))
        .or_else(|_| fs::read_to_string(
            format!("/sys/devices/system/cpu/cpufreq/policy{}/affected_cpus", policy_id)))
        .ok()?;
    let first_cpu: u32 = related_str.split_whitespace().next()?.parse().ok()?;
    fs::read_to_string(format!("/sys/devices/system/cpu/cpu{}/cpu_capacity", first_cpu))
        .ok()?.trim().parse::<u32>().ok()
}

/// 根据 CPU capacity 自动计算每个 cluster 的权重
pub(super) fn auto_compute_capacity_weights(policies: &[CpuPolicy]) -> Option<Vec<(i32, f32)>> {
    let caps: Vec<(i32, u32)> = policies.iter()
        .filter(|p| p.id != -1)
        .filter_map(|p| probe_policy_capacity(p.id).map(|c| (p.id, c)))
        .collect();
    if caps.is_empty() || caps.iter().any(|&(_, c)| c == 0) { return None; }
    let min_cap = caps.iter().map(|&(_, c)| c).min().unwrap() as f32;
    Some(caps.iter().map(|&(pid, cap)| {
        let r = cap as f32 / min_cap;
        (pid, if r <= 1.01 { 1.0 } else { 1.0 + (r - 1.0).sqrt() })
    }).collect())
}

pub fn start_scheduler_thread(rx: mpsc::Receiver<DaemonEvent>) -> Result<()> {
    let root = common::get_module_root();
    let config_path = root.join("config/config.yaml");
    let config_dir = root.join("config"); 

    let config = Config::from_file(config_path.to_str().unwrap()).unwrap_or_default();

    let shared_config = Arc::new(RwLock::new(config));
    let shared_mode_name = Arc::new(Mutex::new("balance".to_string())); 
    let sys_path_exist = Arc::new(utils::SysPathExist::new());

    // ==========================================
    // Config Watcher 线程
    // ==========================================
    let config_clone = shared_config.clone();
    let sys_path_clone = sys_path_exist.clone();
    
    thread::Builder::new()
        .name("config_watcher".to_string())
        .spawn(move || {
            loop {
                if let Err(e) = utils::watch_path(&config_dir) {
                    log::error!("{}", t_with_args("config-watch-error", &fluent_args!("error" => e.to_string())));
                    // 退避后再重试，避免持续错误时忙循环刷 CPU
                    thread::sleep(std::time::Duration::from_secs(2));
                    continue;
                }
                log::info!("{}", t("config-reloading"));

                let old_lang = config_clone.read().unwrap().meta.language.clone();
                
                match Config::from_file(config_path.to_str().unwrap()) {
                    Ok(new_config) => {
                        logger::update_level(&new_config.meta.loglevel);
                        *config_clone.write().unwrap() = new_config;
                        
                        let new_lang = config_clone.read().unwrap().meta.language.clone();
                        if old_lang != new_lang { load_language(&new_lang); }

                        log::info!("{}", t("config-reloaded-success"));

                        let scheduler = CpuScheduler::new(config_clone.clone(), sys_path_clone.clone());
                        if let Err(e) = scheduler.apply_system_tweaks() {
                            log::error!("{}", t_with_args("config-apply-tweaks-failed", &fluent_args!("error" => e.to_string())));
                        }
                    }
                    Err(load_err) => log::error!("{}", t_with_args("config-reload-fail", &fluent_args!("error" => load_err.to_string()))),
                }
            }
        })?;
    
    log::info!("{}", t("main-config-watch-thread-create"));

    // ==========================================
    // IPC 监听主线程 (负责所有的状态机流转与调度干预)
    // ==========================================
    let config_clone = shared_config.clone();
    let mode_clone = shared_mode_name.clone();

    thread::Builder::new()
        .name("scheduler_ipc".to_string())
        .spawn(move || {
            log::info!("{}", t("scheduler-ipc-started"));
            
            let root = common::get_module_root();
            let mode_file_path = root.join("current_mode.txt");
            
            let mut fas_controller = crate::scheduler::fas::FasController::new();
            let mut cpu_governor = crate::scheduler::cpu_load_governor::CpuLoadGovernor::new();

            let rules_path = crate::monitor::config::get_rules_path();
            let mut current_rules = crate::utils::read_config::<crate::monitor::config::RulesConfig, _>(&rules_path).unwrap_or_default();

            let mut is_screen_on = true;
            let mut cpu_health = monitor_health::MonitorHealth::default();
            let mut fps_health = monitor_health::MonitorHealth::default();
            let mut game: Option<(i32, String, f64)> = None;
            let mut clg_dirty = true;
            let temp_sensor_path = crate::utils::find_cpu_temp_path().unwrap_or_default();
            let mut last_temp_update = Instant::now();

            let get_clg_cfg = |config: &Config, mode: &str| -> crate::scheduler::config::CpuLoadGovernorConfig {
                config.get_mode(mode).map(|m| m.cpu_load_governor.clone()).unwrap_or_else(|| {
                    let mut cfg = crate::scheduler::config::CpuLoadGovernorConfig::default();
                    cfg.enabled = false;
                    cfg
                })
            };

            // Readiness starts false. Check the watchdog on timeout AND on each
            // message, so either an idle channel or a busy one can release control.
            let loop_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                loop {
                    let message = match rx.recv_timeout(std::time::Duration::from_millis(200)) {
                        Ok(message) => Some(message),
                        Err(mpsc::RecvTimeoutError::Timeout) => None,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let now = Instant::now();
                    if !cpu_health.ready(now) && cpu_governor.is_active() {
                        log::warn!("[Scheduler] CPU samples unavailable; restoring CPU policies");
                        cpu_governor.release();
                    }
                    if (!cpu_health.ready(now) || !fps_health.ready(now)) && !fas_controller.policies.is_empty() {
                        log::warn!("[Scheduler] Monitor samples unavailable; releasing FAS policies");
                        fas_controller.reset_all_freqs();
                        fas_controller.policies.clear();
                        fas_controller.clear_game();
                    }

                    if let Some(message) = message {
                        match message {
                            DaemonEvent::ScreenStateChange(screen_on) => {
                                is_screen_on = screen_on;
                                clg_dirty = true;
                                fps_health.invalidate();
                                // Release the previous owner before taking the
                                // next snapshot, including FAS -> Doze transitions.
                                fas_controller.reset_all_freqs();
                                fas_controller.policies.clear();
                                fas_controller.clear_game();
                                log::info!("{}", t(if screen_on { "scheduler-doze-restore" } else { "scheduler-doze-enable" }));
                            }
                            DaemonEvent::ModeChange { package_name, pid, mode, temperature } => {
                                let old_mode = mode_clone.lock().unwrap().clone();
                                let game_changed = mode == "fas" && game.as_ref()
                                    .is_none_or(|(old_pid, old_package, _)| *old_pid != pid || *old_package != package_name);
                                if old_mode != mode || game_changed {
                                    log::info!("{}", t_with_args("scheduler-mode-change-request", &fluent_args!(
                                        "old" => old_mode, "new" => mode.as_str(), "pkg" => package_name.as_str(), "temp" => temperature
                                    )));
                                    // Never retain old FAS locks while CLG takes a
                                    // snapshot: release both owners at a handover.
                                    fas_controller.reset_all_freqs();
                                    fas_controller.policies.clear();
                                    fas_controller.clear_game();
                                    if mode == "fas" && is_screen_on { cpu_governor.release(); }
                                    fps_health.invalidate();
                                    clg_dirty = true;
                                }
                                *mode_clone.lock().unwrap() = mode.clone();
                                let _ = utils::try_write_file(&mode_file_path, mode.as_bytes());
                                game = if mode == "fas" { Some((pid, package_name, temperature)) } else { None };
                                fas_controller.set_temperature(temperature);
                            }
                            DaemonEvent::SystemLoadUpdate { core_utils, foreground_max_util, sampled_at } => {
                                let valid = !core_utils.is_empty()
                                    && core_utils.iter().all(|u| u.is_finite() && (0.0..=1.0).contains(u))
                                    && foreground_max_util.is_finite() && (0.0..=1.0).contains(&foreground_max_util);
                                if valid && cpu_health.observe(sampled_at, now) {
                                    fas_controller.update_cpu_util(foreground_max_util);
                                    fas_controller.update_core_utils(&core_utils);
                                    if cpu_governor.is_active() { cpu_governor.on_load_update(&core_utils); }
                                }
                            }
                            DaemonEvent::FrameUpdate { frame_delta_ns, pid, sampled_at } => {
                                if is_screen_on && cpu_health.ready(now)
                                    && (1_000_000..=200_000_000).contains(&frame_delta_ns)
                                {
                                    if let Some((game_pid, package, temperature)) = &game {
                                        if *game_pid > 0 && pid == *game_pid as u32 && fps_health.observe(sampled_at, now) {
                                            if fas_controller.policies.is_empty() {
                                                cpu_governor.release();
                                                fas_controller.load_policies(&current_rules.fas_rules);
                                                fas_controller.set_game(*game_pid, package);
                                                fas_controller.set_temperature(*temperature);
                                                fas_controller.set_temp_threshold(current_rules.fas_rules.core_temp_threshold);
                                            }
                                            if !temp_sensor_path.is_empty() && last_temp_update.elapsed().as_secs() >= 3 {
                                                if let Ok(raw_temp) = crate::utils::read_f64_from_file(&temp_sensor_path) {
                                                    fas_controller.set_temperature(raw_temp / 1000.0);
                                                }
                                                last_temp_update = now;
                                            }
                                            fas_controller.update_frame(frame_delta_ns);
                                        }
                                    }
                                }
                            }
                            DaemonEvent::CpuMonitorUnavailable => cpu_health.invalidate(),
                            DaemonEvent::FpsProbeUnavailable => fps_health.invalidate(),
                            DaemonEvent::ConfigReload(new_rules) => {
                                current_rules = new_rules;
                                clg_dirty = true;
                                if !fas_controller.policies.is_empty() {
                                    fas_controller.reload_rules(&current_rules.fas_rules);
                                }
                            }
                        }
                    }

                    // Every CLG entry point (startup/mode/wake/Doze/reload/recovery)
                    // passes through this single readiness gate.
                    let current_mode = mode_clone.lock().unwrap().clone();
                    if cpu_health.ready(now) && (!is_screen_on || current_mode != "fas") {
                        if clg_dirty || !cpu_governor.is_active() {
                            let config_lock = config_clone.read().unwrap();
                            let mut cfg = get_clg_cfg(&config_lock, if is_screen_on { &current_mode } else { "powersave" });
                            if !is_screen_on {
                                cfg.enabled = true;
                                cfg.perf_floor = 0.0;
                                cfg.perf_ceil = cfg.perf_ceil.min(0.40);
                                cfg.smoothing_up = 0.10;
                                cfg.smoothing_down = 1.0;
                            }
                            if cfg.enabled {
                                if cpu_governor.is_active() { cpu_governor.reload_config(&cfg); }
                                else { cpu_governor.init_policies(&cfg); }
                            } else { cpu_governor.release(); }
                        }
                    } else if cpu_governor.is_active() {
                        cpu_governor.release();
                    }
                    if (!cpu_health.ready(now) || !fps_health.ready(now)) && !fas_controller.policies.is_empty() {
                        fas_controller.reset_all_freqs();
                        fas_controller.policies.clear();
                        fas_controller.clear_game();
                    }
                    clg_dirty = false;
                }
            }));
            if loop_result.is_err() {
                log::error!("{}", t("scheduler-ipc-panic"));
            }
            log::warn!("{}", t("scheduler-channel-closed"));
            // 收尾：无论 channel 关闭还是 panic，都恢复 CPU 控制状态，避免频率/governor 残留
            cpu_governor.release();
            fas_controller.reset_all_freqs();
            fas_controller.clear_game();
        })?;

    Ok(())
}