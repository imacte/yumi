/// Explicit padding keeps every byte initialized for bpf_perf_event_output.
#[repr(C)]
pub struct FrameTimestampEvent {
    pub pid: u32,
    pub reserved: u32,
    pub ktime_ns: u64,
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
