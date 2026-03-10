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

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use log::{info, warn, debug};
use inotify::{Inotify, WatchMask};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering, AtomicI32};
use std::error::Error;
use std::process::Command;
use std::sync::mpsc::Sender;

use crate::common::DaemonEvent;
use crate::i18n::{t, t_with_args};
use crate::fluent_args;
use crate::utils;
use super::config::{self, RulesConfig};

// 缓存有效的 Cgroup 路径索引，避免每次循环都去探测无效路径
static VALID_CGROUP_IDX: AtomicUsize = AtomicUsize::new(usize::MAX);

static CURRENT_PID: AtomicI32 = AtomicI32::new(0);

// 获取系统已启用的输入法列表
fn get_system_ime_packages() -> HashSet<String> {
    let mut imes = HashSet::new();
    
    let output = Command::new("settings")
        .arg("get")
        .arg("secure")
        .arg("enabled_input_methods")
        .output();

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for entry in stdout.split(':') {
            if let Some(pkg) = entry.split('/').next() {
                let clean_pkg = pkg.trim();
                if !clean_pkg.is_empty() {
                    imes.insert(clean_pkg.to_string());
                    debug!("Auto-detected IME: {}", clean_pkg);
                }
            }
        }
    }
    
    if imes.is_empty() {
        warn!("Failed to auto-detect IME, using fallback list.");
        imes.insert("com.sohu.inputmethod.sogou.xiaomi".to_string());
        imes.insert("com.sohu.inputmethod.sogouoem".to_string());
        imes.insert("com.google.android.inputmethod.latin".to_string());
        imes.insert("com.baidu.input_mi".to_string());
        imes.insert("com.iflytek.inputmethod.miui".to_string());
    }
    
    imes
}

lazy_static::lazy_static! {
    static ref CURRENT_PACKAGE: Arc<Mutex<String>> = Arc::new(Mutex::new("".to_string()));    
    static ref IME_BLOCKLIST: HashSet<String> = get_system_ime_packages();
}

pub fn get_current_pid() -> i32 {
    CURRENT_PID.load(Ordering::Relaxed)
}

// 在检测到新包名时更新它
fn set_current_package(pkg: &str, pid: i32) {
    *CURRENT_PACKAGE.lock().unwrap() = pkg.to_string();
    CURRENT_PID.store(pid, Ordering::Relaxed);
}

// ==================== [核心：纯 Cgroup 检测逻辑] ====================

/// 判断是否为有效的用户应用包名
fn is_valid_user_app(pkg: &str, ignored_apps: &[String]) -> bool {
    if pkg.is_empty() || !pkg.contains('.') || pkg.starts_with('/') || pkg.starts_with('.') || pkg.contains(':') {
        return false;
    }
    if IME_BLOCKLIST.contains(pkg) {
        return false; 
    }
    if ignored_apps.iter().any(|ignored| pkg == ignored || pkg.contains(ignored)) {
        return false;
    }
    match pkg {
        "com.android.systemui" => false,
        "system_server" => false,
        "surfaceflinger" => false,
        "android.hardware.graphics.composer" => false,
        "com.android.phone" => false,
        "com.android.permissioncontroller" => false,
        "yumi" => false,
        "com.xiaomi.vtcamera" => false,
        "com.android.providers.media.module" => false,
        "com.google.android.gms.ui" => false,
        "com.xiaomi.mibrain.speech" => false,
        _ => {
            if pkg.contains("magisk") || pkg.contains("mtiodaemon") { return false; }
            if pkg.contains("ads_monitor") { return false; }
            if pkg.contains("inputmethod") { return false; }
            true
        }
    }
}

// 提取核心检测逻辑
fn check_cgroup_path(path: &str, ignored_apps: &[String]) -> Option<(String, i32)> {
    if let Ok(content) = utils::read_file_content(path) {
        let pids: Vec<&str> = content.split_whitespace().collect();
        for pid_str in pids.iter().rev() {
            let cmdline_path = format!("/proc/{}/cmdline", pid_str);
            if let Ok(cmdline) = utils::read_file_content(&cmdline_path) {
                let pkg_name = cmdline.split('\0').next().unwrap_or("").trim();
                if is_valid_user_app(pkg_name, ignored_apps) {
                    let pid = pid_str.parse::<i32>().unwrap_or(0);
                    return Some((pkg_name.to_string(), pid));
                }
            }
        }
    }
    None
}

/// 从 Cgroup 读取前台应用
fn get_focused_app_from_cgroup(ignored_apps: &[String]) -> Result<(String, i32), Box<dyn Error>> {
    let paths = [
        "/dev/cpuset/top-app/cgroup.procs",
        "/sys/fs/cgroup/cpuset/top-app/cgroup.procs",
        "/dev/stune/top-app/cgroup.procs"
    ];

    let cached = VALID_CGROUP_IDX.load(Ordering::Relaxed);
    if cached < paths.len() {
        if let Some(res) = check_cgroup_path(paths[cached], ignored_apps) {
            return Ok(res); 
        }
    }

    for (i, path) in paths.iter().enumerate() {
        if i == cached { continue; }
        if let Some(res) = check_cgroup_path(path, ignored_apps) {
            VALID_CGROUP_IDX.store(i, Ordering::Relaxed);
            return Ok(res);
        }
    }
    
    Err("No valid app found".into())
}

// ==================== [辅助函数] ====================

fn determine_mode(config: &RulesConfig, current_package: &str) -> String {
    if !config.dynamic_enabled {
        return config.global_mode.clone();
    }
    config.app_modes.get(current_package).cloned().unwrap_or_else(|| config.global_mode.clone())
}

pub fn get_default_rules() -> RulesConfig {
    RulesConfig {
        yumi_scheduler: true,
        dynamic_enabled: true,
        global_mode: "balance".to_string(),
        app_modes: HashMap::new(),
        ignored_apps: Vec::new(),
        fas_rules: super::config::FasRulesConfig::default(),
    }
}

pub fn watch_config_file(
    config_arc: Arc<Mutex<RulesConfig>>,
    force_refresh_arc: Arc<AtomicBool>,
    tx: Sender<DaemonEvent>
) -> Result<(), Box<dyn Error>> {
    let mut inotify = Inotify::init()?;
    let rules_path = config::get_rules_path();
    if !rules_path.exists() { let _ = utils::try_write_file(&rules_path, ""); }
    inotify.watches().add(&rules_path, WatchMask::MODIFY | WatchMask::CLOSE_WRITE)?;
    info!("{}", t_with_args("app-detect-config-watch", &fluent_args!("path" => format!("{:?}", rules_path))));
    let mut buffer = [0u8; 1024];
    loop {
        let events = inotify.read_events_blocking(&mut buffer)?;
        if events.peekable().peek().is_some() {
            info!("{}", t("app-detect-change-detected"));
            thread::sleep(Duration::from_millis(100));
            while let Ok(events) = inotify.read_events(&mut buffer) { if events.peekable().peek().is_none() { break; } }
            info!("{}", t("app-detect-reloading"));
            
            let new_config = config::read_config::<RulesConfig, _>(&rules_path)
                                .unwrap_or_else(|e| { 
                                    warn!("{}", t_with_args("app-detect-load-failed", &fluent_args!("error" => e.to_string()))); 
                                    get_default_rules() 
                                });
            
            *config_arc.lock().unwrap() = new_config.clone();
            
            if let Err(e) = tx.send(DaemonEvent::ConfigReload(new_config)) {
                warn!("[Config] Failed to send ConfigReload event: {}", e);
            }
            
            info!("{}", t("app-detect-reload-success"));
            force_refresh_arc.store(true, Ordering::SeqCst);
        }
    }
}

pub fn app_detection_loop(
    config_arc: Arc<Mutex<RulesConfig>>, 
    screen_state_arc: Arc<Mutex<bool>>,
    force_refresh_arc: Arc<AtomicBool>,
    tx: Sender<DaemonEvent>
) -> Result<(), Box<dyn Error>> {
    info!("{}", t("app-detect-loop-started"));
    
    let temp_sensor_path = utils::find_cpu_temp_path().unwrap_or_default();
    let mut last_package = String::new();
    let mut last_mode = String::new();
    let mut last_screen_state = true; 
    
    // 状态机变量：用于无阻塞防抖
    let mut pending_package = String::new();
    let mut pending_pid = 0;
    let mut debounce_start = Instant::now();
    
    loop {
        let force_refresh = force_refresh_arc.swap(false, Ordering::SeqCst);
        let current_screen_state = { *screen_state_arc.lock().unwrap() };
        
        if current_screen_state != last_screen_state {
            info!("{}", t_with_args("app-detect-screen-changed", &fluent_args!("old" => last_screen_state.to_string(), "new" => current_screen_state.to_string())));
            last_screen_state = current_screen_state;
            let _ = tx.send(DaemonEvent::ScreenStateChange(current_screen_state));
            if current_screen_state {
                last_package.clear();
                pending_package.clear();
            }
        }

        if !current_screen_state { 
            thread::sleep(Duration::from_secs(1));
            continue;
        }
                
        // 合并锁获取：一次拿完所有需要的数据
        let config_snapshot = config_arc.lock().unwrap().clone();
        let ignored_apps = config_snapshot.ignored_apps.clone();

        let (detected_pkg, detected_pid) = get_focused_app_from_cgroup(&ignored_apps)
            .unwrap_or_else(|_| (last_package.clone(), get_current_pid()));

        let mut final_pkg = last_package.clone();
        let mut final_pid = get_current_pid();

        // 无阻塞防抖逻辑
        if detected_pkg != last_package && !detected_pkg.is_empty() {
            if detected_pkg != pending_package {
                pending_package = detected_pkg.clone();
                pending_pid = detected_pid;
                debounce_start = Instant::now();
            } else if debounce_start.elapsed() >= Duration::from_millis(500) {
                final_pkg = pending_package.clone();
                final_pid = pending_pid;
                pending_package.clear();
            }
        } else {
            pending_package.clear();
        }

        let current_temp = if !temp_sensor_path.is_empty() {
            utils::read_f64_from_file(&temp_sensor_path).unwrap_or(0.0) / 1000.0
        } else { 0.0 };
        
        if last_package != final_pkg || force_refresh {
            if !final_pkg.is_empty() {
                set_current_package(&final_pkg, final_pid);
                // 使用已获取的 config_snapshot，不再重复加锁
                let new_mode = determine_mode(&config_snapshot, &final_pkg);

                if last_mode != new_mode || force_refresh {
                    info!("{}", t_with_args("app-detect-mode-change-pkg", &fluent_args!("old" => last_mode.clone(), "new" => new_mode.as_str(), "pkg" => final_pkg.as_str())));
                    // ModeChange 事件现在携带 pid 字段
                    let _ = tx.send(DaemonEvent::ModeChange {
                        package_name: final_pkg.clone(),
                        pid: final_pid,
                        mode: new_mode.clone(),
                        temperature: current_temp,
                    });
                    last_mode = new_mode;
                }
                last_package = final_pkg;
            }
        }

        thread::sleep(Duration::from_millis(1500));
    }
}