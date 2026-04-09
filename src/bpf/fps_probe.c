#include "bpf_abi.h"

// packed 结构体，确保在 Rust 端刚好是 12 字节
struct __attribute__((packed)) frame_event_t {
    __u32 pid;
    __u64 delta;
};

// Map：记录上一帧时间（所有进程都记录，便于切换目标时已有基线）
struct bpf_map_def SEC("maps") last_timestamps = {
    .type = BPF_MAP_TYPE_HASH,
    .key_size = sizeof(__u32),   // PID
    .value_size = sizeof(__u64), // 时间戳 ns
    .max_entries = 2048,
};

// Map：输出帧间隔数据
struct bpf_map_def SEC("maps") frame_events = {
    .type = BPF_MAP_TYPE_PERF_EVENT_ARRAY,
    .key_size = sizeof(__u32),
    .value_size = sizeof(__u32),
    .max_entries = 32,
};

// Map：目标进程 PID（由用户态 fps_monitor 写入）
// key=0 → 要监控的目标 PID；值为 0 表示"放行所有"
struct bpf_map_def SEC("maps") target_pid = {
    .type = BPF_MAP_TYPE_ARRAY,
    .key_size = sizeof(__u32),
    .value_size = sizeof(__u32),
    .max_entries = 1,
};

SEC("uprobe/queueBuffer")
int handle_frame(void *ctx) {
    __u64 pid_tgid = bpf_get_current_pid_tgid();
    __u32 pid = (__u32)(pid_tgid >> 32);
    __u64 now = bpf_ktime_get_ns();

    // 内核侧 PID 过滤
    // 无论是否匹配目标 PID，都先更新时间戳（为未来的 PID 切换保留基线）
    // 但只有匹配目标 PID 时才发送 perf event
    __u32 key = 0;
    __u32 *tp = bpf_map_lookup_elem(&target_pid, &key);
    // tp == NULL：map 查找失败，放行
    // *tp == 0：用户态还没设置目标 PID，放行（启动阶段兼容）
    // *tp != pid：不是目标进程，只记录时间戳，不发送事件
    int pid_match = (!tp || *tp == 0 || *tp == pid);

    __u64 *prev_ts = bpf_map_lookup_elem(&last_timestamps, &pid);

    if (prev_ts && pid_match) {
        __u64 delta = now - *prev_ts;

        // 过滤掉太离谱的帧（比如超过10秒），防止脏数据污染
        if (delta > 0 && delta < 10000000000ULL) {
            struct frame_event_t event = {
                .pid = pid,
                .delta = delta,
            };
            bpf_perf_event_output(ctx, &frame_events, 0xffffffffULL, &event, sizeof(event));
        }
    }

    // 始终更新时间戳（包括非目标进程）
    // 这样当目标 PID 切换时，新目标进程的第一帧就能算出正确的 delta
    bpf_map_update_elem(&last_timestamps, &pid, &now, 0);
    return 0;
}

char _license[] SEC("license") = "GPL";