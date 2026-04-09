#pragma once

// 1. 基础类型定义 (标准 C 类型)
typedef unsigned char __u8;
typedef unsigned int __u32;
typedef unsigned long long __u64;

// 2. Section 宏定义
#define SEC(NAME) __attribute__((section(NAME), used))
#define __always_inline inline __attribute__((always_inline))

// 3. BPF 辅助函数编号 (参考 Linux 内核 uapi/linux/bpf.h)
// 这些是内核固定的系统调用号，属于公共领域信息
enum bpf_func_id {
    BPF_FUNC_map_lookup_elem = 1,
    BPF_FUNC_map_update_elem = 2,
    BPF_FUNC_ktime_get_ns = 5,
    BPF_FUNC_get_current_pid_tgid = 14,
    BPF_FUNC_perf_event_output = 25,
};

// 4. 辅助函数指针定义
// 使用 Clang 的特殊属性，将函数调用编译为 BPF_CALL 指令
static void *(*bpf_map_lookup_elem)(void *map, const void *key) = (void *)BPF_FUNC_map_lookup_elem;
static int (*bpf_map_update_elem)(void *map, const void *key, const void *value, __u64 flags) = (void *)BPF_FUNC_map_update_elem;
static __u64 (*bpf_ktime_get_ns)(void) = (void *)BPF_FUNC_ktime_get_ns;
static __u64 (*bpf_get_current_pid_tgid)(void) = (void *)BPF_FUNC_get_current_pid_tgid;
static int (*bpf_perf_event_output)(void *ctx, void *map, __u64 flags, void *data, __u64 size) = (void *)BPF_FUNC_perf_event_output;

// 5. Map 结构定义 (Legacy BPF 格式，Aya 兼容性最好)
struct bpf_map_def {
    unsigned int type;
    unsigned int key_size;
    unsigned int value_size;
    unsigned int max_entries;
    unsigned int map_flags;
};

// Map 类型常量
enum {
    BPF_MAP_TYPE_HASH = 1,
    BPF_MAP_TYPE_ARRAY = 2,
    BPF_MAP_TYPE_PERF_EVENT_ARRAY = 4,
};

// Map 更新标志位
#ifndef BPF_ANY
#define BPF_ANY 0
#define BPF_NOEXIST 1
#define BPF_EXIST 2
#endif

// Map 类型常数
#ifndef BPF_MAP_TYPE_PERCPU_ARRAY
#define BPF_MAP_TYPE_PERCPU_ARRAY 6
#endif

// 获取当前运行 CPU 核心号的辅助函数
// 内核系统调用号为 8 (BPF_FUNC_get_smp_processor_id)
static __u32 (*bpf_get_smp_processor_id)(void) = (void *)8;