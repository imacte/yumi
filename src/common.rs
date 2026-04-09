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

use crate::monitor::config::RulesConfig;
use std::path::PathBuf;
use std::env;

/// 守护进程全局事件总线
#[derive(Debug, Clone)]
pub enum DaemonEvent {
    /// 低频事件：前台应用切换或环境温度变化引起的模式改变
    ModeChange {
        package_name: String,
        pid: i32,
        mode: String,
        temperature: f64,
    },
    /// 高频事件：eBPF 捕获到的底层渲染帧数据
    FrameUpdate {
        fps: f32,
        frame_delta_ns: u64, // 纳秒级帧间隔
    },
    /// eBPF 全局系统负载更新 (每 X 毫秒触发一次)
    SystemLoadUpdate {
        /// 每个 CPU 核心的真实利用率 (0.0 ~ 1.0)，数组索引即 cpu_id
        core_utils: Vec<f32>,
        /// 如果当前有前台应用，这是该应用最吃 CPU 的那 1 个线程的利用率
        foreground_max_util: f32, 
    },

    ConfigReload(RulesConfig),

    ScreenStateChange(bool),
}

/// 获取模块根目录的绝对路径
pub fn get_module_root() -> PathBuf {
    // 获取当前执行文件的绝对路径
    let exe_path = env::current_exe().unwrap_or_else(|_| PathBuf::from("/"));
    
    // 回溯两级目录:
    // core/bin/yumi -> core/bin -> core -> yumi
    exe_path
        .parent().unwrap_or(&exe_path) // .../core/bin
        .parent().unwrap_or(&exe_path) // .../core
        .parent().unwrap_or(&exe_path) // .../yumi (Root)
        .to_path_buf()
}