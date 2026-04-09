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
use std::fs::File;
use std::io::Read;
use std::sync::{Arc, Mutex};
use log::{info};
use std::process; 
use std::thread;
use std::time::Duration;
use kobject_uevent::{UEvent, ActionType};
use netlink_sys::{protocols::NETLINK_KOBJECT_UEVENT, Socket, SocketAddr};

use crate::i18n::{t, t_with_args};
use crate::fluent_args;

fn read_int_file(path: &str) -> Result<i32, Box<dyn Error>> {
    let mut content = String::new();
    File::open(path)?.read_to_string(&mut content)?;
    Ok(content.trim().parse()?)
}

fn update_state_if_changed(state_arc: &Arc<Mutex<bool>>, new_state: bool, source: &str) {
    let mut state_lock = state_arc.lock().unwrap();
    if *state_lock != new_state {
        info!("{}", t_with_args("screen-state-change-detected", &fluent_args!("source" => source)));
        *state_lock = new_state;
        let state_str = if new_state { "ON" } else { "OFF" };
        info!("{}", t_with_args("screen-state-changed-value", &fluent_args!("state" => state_str)));
    }
}

pub fn monitor_screen_state_uevent(state_arc: Arc<Mutex<bool>>) -> Result<(), Box<dyn Error>> {
    let mut socket = Socket::new(NETLINK_KOBJECT_UEVENT)?;
    let sa = SocketAddr::new(process::id(), 1);
    socket.bind(&sa)?;
    let _ = socket.set_rx_buf_sz(2 * 1024 * 1024);
    info!("{}", t("screen-netlink-started"));

    loop {
        match socket.recv_from_full() {
            Ok((buf, _)) => {
                if let Ok(event) = UEvent::from_netlink_packet(&buf) {
                    if event.subsystem == "power" {
                         if let Some(action) = event.env.get("POWER_ACTION") {
                            if action == "early_suspend" { update_state_if_changed(&state_arc, false, "power"); }
                            else if action == "late_resume" { update_state_if_changed(&state_arc, true, "power"); }
                         }
                    } else if event.subsystem == "backlight" && event.action == ActionType::Change {
                        thread::sleep(Duration::from_millis(100));
                        let dev = event.devpath.display();
                        let bl_power = format!("/sys{}/bl_power", dev);
                        let actual = format!("/sys{}/actual_brightness", dev);
                        
                        let new_state = read_int_file(&bl_power).map(|v| v == 0)
                            .or_else(|_| read_int_file(&actual).map(|v| v > 0)).ok();
                        
                        if let Some(state) = new_state {
                            update_state_if_changed(&state_arc, state, "backlight");
                        }
                    }
                }
            },
            Err(_) => thread::sleep(Duration::from_secs(1)),
        }
    }
}