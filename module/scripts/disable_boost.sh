#!/system/bin/sh

write_value() {
    local path_pattern="$1"
    local value="$2"

    local files=$(ls -d "$path_pattern" 2>/dev/null)

    if [ -n "$files" ]; then
        for file in $files; do
            if [ -e "$file" ]; then
                chmod 644 "$file" 2>/dev/null
                echo "$value" > "$file"
                chmod 444 "$file" 2>/dev/null
            fi
        done
    fi
}

disable_kernel_boost() {
    echo "正在禁用内核级 Boost..."

    write_value "/sys/devices/system/cpu/cpufreq/hotplug/cpu_hotplug_disable" "1"
    write_value "/sys/module/control_center/parameters/*" "N"


    write_value "/sys/power/pnpmgr/touch_boost" "0"
    write_value "/sys/power/pnpmgr/long_duration_touch_boost" "0"
    write_value "/sys/kernel/ems/eff_mode" "0"
    write_value "/sys/kernel/hmp/boost" "0"
    write_value "/sys/kernel/hmp/boostpulse_duration" "0"
    write_value "/sys/kernel/intelli_plug/intelli_plug_active" "0"
    write_value "/sys/kernel/zen_decision/enabled" "0"
    write_value "/sys/devices/system/cpu/sched/sched_boost" "0"
    write_value "/sys/devices/system/cpu/cpuhotplug/enabled" "0"
    write_value "/sys/devices/system/cpu/hyp_core_ctl/enable" "0"
    write_value "/sys/devices/virtual/misc/mako_hotplug_control/enabled" "0"
    write_value "/sys/module/msm_performance/parameters/touchboost" "0"
    write_value "/sys/module/msm_thermal/vdd_restriction/enabled" "0"
    write_value "/sys/module/msm_thermal/core_control/enabled" "0"
    write_value "/sys/module/aigov/parameters/enable" "0"
    write_value "/sys/module/opchain/parameters/chain_on" "0"
    write_value "/sys/module/blu_plug/parameters/enabled" "0"
    write_value "/sys/module/autosmp/parameters/enabled" "0"
    write_value "/proc/mz_thermal_boost/sched_boost_enabled" "0"
    write_value "/proc/mz_scheduler/vip_task/enabled" "0"
    write_value "/proc/sys/fbg/frame_boost_enabled" "0"
    write_value "/proc/sys/fbg/input_boost_enabled" "0"
    write_value "/proc/sys/fbg/slide_boost_enabled" "0"
    write_value "/sys/module/fbt_cpu/parameters/boost_affinity*" "0"
    write_value "/sys/module/mtk_fpsgo/parameters/boost_affinity*" "0"
    write_value "/sys/module/mtk_fpsgo/parameters/perfmgr_enable" "0"
    write_value "/sys/module/perfmgr/parameters/perfmgr_enable" "0"
    write_value "/sys/module/perfmgr_policy/parameters/perfmgr_enable" "0"
    write_value "/sys/kernel/fpsgo/common/fpsgo_enable" "0"
    write_value "/sys/kernel/debug/fpsgo/common/force_onoff" "0"
    write_value "/sys/kernel/ged/hal/dcs_mode" "0"
    write_value "/proc/perfmgr/tchbst/user/usrtch" "0"
    write_value "/sys/kernel/fpsgo/fbt/thrm_temp_th" "0"
    write_value "/sys/kernel/fpsgo/fbt/thrm_limit_cpu" "0"
    write_value "/sys/kernel/fpsgo/fbt/thrm_sub_cpu" "0"
    write_value "/proc/perfmgr/syslimiter/syslimiter_force_disable" "0"
    write_value "/sys/module/mtk_core_ctl/parameters/policy_enable" "0"
    write_value "/sys/kernel/fpsgo/fbt/switch_idleprefer" "0"
    write_value "/sys/module/devfreq_boost/parameters/*" "0"
    write_value "/sys/kernel/cpu_input_boost/*" "0"
    write_value "/sys/devices/system/cpu/cpu*/sched_load_boost" "0"
    write_value "/sys/devices/system/cpu/cpu_boost/*" "0"
    write_value "/sys/devices/system/cpu/cpu_boost/parameters/*" "0"
    write_value "/sys/module/cpu_boost/parameters/*" "0"
    write_value "/sys/module/dsboost/parameters/*" "0"
    write_value "/sys/module/cpu_input_boost/parameters/*" "0"
    write_value "/sys/module/input_cfboost/parameters/*" "0"
    write_value "/sys/class/input_booster/*" "0"
    write_value "/proc/sys/walt/input_boost/*" "0"
}

disable_cpuset_boost() {
    echo "正在禁用 Cpuset Boost..."
    if [ -d /dev/cpuset/foreground/boost ]; then
        rmdir /dev/cpuset/foreground/boost
    fi
    if [ -d /dev/cpuset/background/untrustedapp ]; then
        rmdir /dev/cpuset/background/untrustedapp
    fi
}

disable_system_boost() {
    echo "正在禁用系统级服务 Boost..."
    stop miuibooster 2>/dev/null
    stop oneplus_brain_service 2>/dev/null
    stop vendor.perfservice 2>/dev/null
    stop perfd 2>/dev/null
    stop orms-hal-1-0 vendor.oplus.ormsHalService-aidl-default 2>/dev/null
    
    setprop persist.sys.hardcoder.name "" 2>/dev/null
    setprop persist.miui.miperf.enable false 2>/dev/null

    write_value "/proc/ppm/enabled" "1"
    write_value "/proc/hps/enabled" "0"
    write_value "/sys/devices/system/cpu/eas/enable" "2"

    chmod 0000 /proc/ppm/policy/* 2>/dev/null
    chmod 0000 /proc/ppm/* 2>/dev/null
    chmod 0000 /sys/module/migt/parameters/* 2>/dev/null
    chmod 0000 /dev/migt 2>/dev/null
}

disable_oneplus_game_boost() {
    if [ -e /proc/game_opt ]; then
        echo "正在禁用 OnePlus Game Boost..."
        write_value "/proc/game_opt/cpu_max_freq" "'0:2147483647 1:2147483647 2:2147483647 3:2147483647 4:2147483647 5:2147483647 6:2147483647 7:2147483647'"
        write_value "/proc/game_opt/cpu_min_freq" "'0:0 1:0 2:0 3:0 4:0 5:0 6:0 7:0'"
        write_value "/proc/game_opt/disable_cpufreq_limit" "1"
        write_value "/proc/game_opt/game_pid" "-1"
        write_value "/sys/devices/platform/soc/soc:oplus-omrg/oplus-omrg0/ruler_enable" "0"
        write_value "/sys/module/oplus_bsp_sched_assist/parameters/boost_kill" "0"
    fi
}

disable_schedtune_boost() {
    if [ -d /dev/stune/ ]; then
        echo "正在禁用 Schedtune Boost..."
        write_value "/dev/stune/schedtune.boost" "0"
        write_value "/dev/stune/schedtune.prefer_idle" "0"
        write_value "/dev/stune/*/schedtune.prefer_idle" "0"
        write_value "/dev/stune/*/schedtune.schedtune.boost" "0"
        write_value "/dev/stune/*/schedtune.sched_boost_no_override" "0"
        
        write_value "/dev/stune/top-app/schedtune.prefer_idle" "1"
        write_value "/dev/stune/top-app/schedtune.schedtune.boost" "1"
        write_value "/dev/stune/top-app/schedtune.sched_boost_no_override" "0"
    fi
}

disable_uclamp_boost() {
    if [ -d /dev/cpuctl/ ]; then
        echo "正在禁用 Uclamp Boost..."
        write_value "/dev/cpuctl/cpu.uclamp.sched_boost_no_override" "0"
        write_value "/dev/cpuctl/cpu.uclamp.min" "0"
        write_value "/dev/cpuctl/cpu.uclamp.latency_sensitive" "0"
        write_value "/dev/cpuctl/cpu.shares" "0"
        
        write_value "/dev/cpuctl/*/cpu.uclamp.sched_boost_no_override" "0"
        write_value "/dev/cpuctl/*/cpu.uclamp.latency_sensitive" "0"
        write_value "/dev/cpuctl/*/cpu.shares" "0"
        
        # 恢复部分默认值
        write_value "/dev/cpuctl/cpu.shares" "1024"
        write_value "/dev/cpuctl/top-app/cpu.shares" "1024"
        write_value "/dev/cpuctl/background/cpu.shares" "256"
        write_value "/dev/cpuctl/foreground/cpu.shares" "512"
    fi
}

disable_core_ctl() {
    echo "正在禁用 CoreCtl..."
    write_value "/sys/devices/system/cpu/cpu*/core_ctl/enable" "0"
    write_value "/sys/devices/system/cpu/cpu*/core_ctl/core_ctl_boost" "0"
}

disable_thermal() {
    echo "正在禁用温控降频..."
    for cpu in $(seq 0 7); do
        write_value "/sys/class/thermal/thermal_message/cpu_limits" "cpu$cpu 2147483647"
    done
    
    write_value "/sys/class/thermal/thermal_message/temp_state" "0"
    write_value "/sys/class/thermal/thermal_message/market_download_limit" "0"
    write_value "/sys/class/thermal/thermal_message/cpu_nolimit_temp" "49500"
}

disable_flyme() {
    if [ -e /sys/class/meizu/wireless/wls_level ]; then
        echo "正在禁用 Flyme 相关设置..."
        write_value "/sys/class/meizu/wireless/wls_level" "10"
        pm disable com.meizu.pps 2>/dev/null
        setprop ro.surface_flinger.use_content_detection_for_refresh_rate 0 2>/dev/null
    fi
}

disable_msm_performance() {
    echo "正在禁用 MSM Performance Boost..."
    write_value "/sys/module/msm_thermal/parameters/thermal_mitigation" "0"
    write_value "/sys/kernel/msm_performance/parameters/cpu_min_freq" "0:0 1:0 2:0 3:0 4:0 5:0 6:0 7:0"
    write_value "/sys/kernel/msm_performance/parameters/cpu_max_freq" "0:9999999 1:9999999 2:9999999 3:9999999 4:9999999 5:9999999 6:9999999 7:9999999"
}

main() {
    echo "--- 开始执行系统 Boost 禁用脚本 ---"
    
    disable_kernel_boost
    disable_system_boost
    disable_cpuset_boost
    disable_schedtune_boost
    disable_uclamp_boost
    disable_core_ctl
    disable_thermal
    disable_oneplus_game_boost
    disable_msm_performance
    disable_flyme
    
    echo "--- 所有禁用操作已执行完毕 ---"
}

main
