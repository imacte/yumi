#![no_std]
#![no_main]

#[path = "../frame.rs"]
mod frame;
use aya_ebpf::{
    helpers::{bpf_get_current_pid_tgid, bpf_ktime_get_ns},
    macros::{map, uprobe},
    maps::RingBuf,
    programs::ProbeContext,
};
use frame::FrameTimestampEvent;

#[map]
static RING_BUF: RingBuf = RingBuf::with_byte_size(0x8000, 0);

#[uprobe]
pub fn handle_frame(_ctx: ProbeContext) -> u32 {
    if let Some(mut entry) = RING_BUF.reserve::<FrameTimestampEvent>(0) {
        entry.write(FrameTimestampEvent {
            pid: (bpf_get_current_pid_tgid() >> 32) as u32,
            reserved: 0,
            ktime_ns: unsafe { bpf_ktime_get_ns() },
        });
        entry.submit(0);
    }
    0
}
