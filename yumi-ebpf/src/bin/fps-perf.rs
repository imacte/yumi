#![no_std]
#![no_main]

#[path = "../frame.rs"]
mod frame;
use aya_ebpf::{
    helpers::{bpf_get_current_pid_tgid, bpf_ktime_get_ns},
    macros::{map, uprobe},
    maps::PerfEventArray,
    programs::ProbeContext,
};
use frame::FrameTimestampEvent;

#[map]
static FRAME_EVENTS: PerfEventArray<FrameTimestampEvent> = PerfEventArray::new(0);

#[uprobe]
pub fn handle_frame(ctx: ProbeContext) -> u32 {
    FRAME_EVENTS.output(
        &ctx,
        FrameTimestampEvent {
            pid: (bpf_get_current_pid_tgid() >> 32) as u32,
            reserved: 0,
            ktime_ns: unsafe { bpf_ktime_get_ns() },
        },
        0,
    );
    0
}
