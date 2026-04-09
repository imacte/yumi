#include "bpf_abi.h"

struct sched_switch_args {
    unsigned long long pad;
    char prev_comm[16];
    int prev_pid;
    int prev_prio;
    long long prev_state;
    char next_comm[16];
    int next_pid;
    int next_prio;
};

// Map 1: 记录每个核心最后一次切换的时间戳
struct bpf_map_def SEC("maps") core_last_time = {
    .type = BPF_MAP_TYPE_PERCPU_ARRAY,
    .key_size = sizeof(__u32),
    .value_size = sizeof(__u64),
    .max_entries = 1,
};

// Map 2: 记录每个核心累计的 Idle 时间 (纳秒)
struct bpf_map_def SEC("maps") core_idle_time = {
    .type = BPF_MAP_TYPE_PERCPU_ARRAY,
    .key_size = sizeof(__u32),
    .value_size = sizeof(__u64),
    .max_entries = 1,
};

// Map 3: 记录每个核心累计的 Busy 时间 (纳秒)
struct bpf_map_def SEC("maps") core_busy_time = {
    .type = BPF_MAP_TYPE_PERCPU_ARRAY,
    .key_size = sizeof(__u32),
    .value_size = sizeof(__u64),
    .max_entries = 1,
};

// Map 4: 记录每个核心当前正在运行的 TID
struct bpf_map_def SEC("maps") core_current_tid = {
    .type = BPF_MAP_TYPE_PERCPU_ARRAY,
    .key_size = sizeof(__u32),
    .value_size = sizeof(__u32),
    .max_entries = 1,
};

// Map 5: 线程运行时间 (HASH)
// 从 8192 扩容到 32768，减少 LRU 驱逐导致前台线程数据丢失
struct bpf_map_def SEC("maps") thread_run_time = {
    .type = BPF_MAP_TYPE_HASH,
    .key_size = sizeof(__u32),
    .value_size = sizeof(__u64),
    .max_entries = 32768,
};

// Map 6: 进程(TGID)级聚合运行时间 (HASH)
// key = TGID (用户态 PID), value = 该进程所有线程的累计运行纳秒
// 用户态只需查询一个 key 即可获得前台进程的总 CPU 时间，
// 不再依赖逐 TID 遍历，彻底规避 thread_run_time 的驱逐问题
struct bpf_map_def SEC("maps") tgid_run_time = {
    .type = BPF_MAP_TYPE_HASH,
    .key_size = sizeof(__u32),
    .value_size = sizeof(__u64),
    .max_entries = 1024,
};

// Map 7: 记录每个核心当前运行任务的 TGID
// 用于用户态 pending delta 补偿时，快速判断当前核心是否在跑目标进程
struct bpf_map_def SEC("maps") core_current_tgid = {
    .type = BPF_MAP_TYPE_PERCPU_ARRAY,
    .key_size = sizeof(__u32),
    .value_size = sizeof(__u32),
    .max_entries = 1,
};

SEC("tracepoint/sched/sched_switch")
int handle_sched_switch(struct sched_switch_args *ctx) {
    __u64 now = bpf_ktime_get_ns();
    __u32 zero_key = 0;

    __u32 prev_tid = ctx->prev_pid;
    __u32 next_tid = ctx->next_pid;

    // 获取 next 任务的 TGID (用户态 PID)
    // bpf_get_current_pid_tgid() 在 sched_switch tracepoint 中返回的是
    // **即将被调度上来的 next 任务**的 pid_tgid
    __u64 pid_tgid = bpf_get_current_pid_tgid();
    __u32 next_tgid = (__u32)(pid_tgid >> 32);

    // --- 计算上一个任务的耗时并累加 ---
    __u64 *last_ts = bpf_map_lookup_elem(&core_last_time, &zero_key);
    if (last_ts) {
        __u64 delta = now - *last_ts;
        
        // 加入容错保护：忽略大于 10 秒的异常大差值（防止设备刚唤醒时出现极大值）
        if (delta > 0 && delta < 10000000000ULL) {
            if (prev_tid == 0) {
                __u64 *idle_total = bpf_map_lookup_elem(&core_idle_time, &zero_key);
                if (idle_total) {
                    *idle_total += delta;
                } else {
                    bpf_map_update_elem(&core_idle_time, &zero_key, &delta, BPF_ANY);
                }
            } else {
                __u64 *busy_total = bpf_map_lookup_elem(&core_busy_time, &zero_key);
                if (busy_total) {
                    *busy_total += delta;
                } else {
                    bpf_map_update_elem(&core_busy_time, &zero_key, &delta, BPF_ANY);
                }

                // 线程级累计 (保留，用于精细调试)
                __u64 *thread_total = bpf_map_lookup_elem(&thread_run_time, &prev_tid);
                if (thread_total) {
                    *thread_total += delta;
                } else {
                    bpf_map_update_elem(&thread_run_time, &prev_tid, &delta, BPF_ANY);
                }

                // TGID 级聚合累计
                // 注意：prev 任务的 TGID 需要从 core_current_tgid 中读取
                // (因为 bpf_get_current_pid_tgid 返回的是 next 的)
                __u32 *prev_tgid_ptr = bpf_map_lookup_elem(&core_current_tgid, &zero_key);
                if (prev_tgid_ptr) {
                    __u32 prev_tgid = *prev_tgid_ptr;
                    if (prev_tgid > 0) {
                        __u64 *tgid_total = bpf_map_lookup_elem(&tgid_run_time, &prev_tgid);
                        if (tgid_total) {
                            *tgid_total += delta;
                        } else {
                            bpf_map_update_elem(&tgid_run_time, &prev_tgid, &delta, BPF_ANY);
                        }
                    }
                }
            }
        }
    }
    
    // --- 更新当前核心的状态 ---
    bpf_map_update_elem(&core_last_time, &zero_key, &now, BPF_ANY);
    bpf_map_update_elem(&core_current_tid, &zero_key, &next_tid, BPF_ANY);
    bpf_map_update_elem(&core_current_tgid, &zero_key, &next_tgid, BPF_ANY);

    return 0;
}

char _license[] SEC("license") = "GPL";