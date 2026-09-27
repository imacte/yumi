//! Pure sampling rules shared by the monitor backends.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameSample {
    pub pid: u32,
    pub ktime_ns: u64,
}

impl FrameSample {
    // Perf samples can wrap around their buffer; decode both slices without
    // alignment assumptions. Both probes emit the same 16-byte wire layout.
    pub fn decode(head: &[u8], tail: &[u8]) -> Option<Self> {
        if head.len() + tail.len() < 16 {
            return None;
        }
        let mut bytes = [0u8; 16];
        let head_len = head.len().min(16);
        bytes[..head_len].copy_from_slice(&head[..head_len]);
        bytes[head_len..].copy_from_slice(&tail[..16 - head_len]);
        Some(Self {
            pid: u32::from_le_bytes(bytes[..4].try_into().ok()?),
            ktime_ns: u64::from_le_bytes(bytes[8..].try_into().ok()?),
        })
    }
}

#[derive(Default)]
pub struct FrameClock {
    last: Option<u64>,
    attached_at: u64,
}

impl FrameClock {
    pub fn reset(&mut self, attached_at: u64) {
        self.last = None;
        self.attached_at = attached_at;
    }

    pub fn ingest(&mut self, timestamp: u64, now: u64) -> Option<u64> {
        if timestamp < self.attached_at
            || timestamp > now
            || now - timestamp > 250_000_000
            || self.last.is_some_and(|last| timestamp <= last)
        {
            return None;
        }
        let previous = self.last.replace(timestamp)?;
        let delta = timestamp - previous;
        (1_000_000..=200_000_000).contains(&delta).then_some(delta)
    }
}

/// Difference complete cumulative snapshots (raw + pending), so time already
/// observed before a context switch is not counted again afterwards.
pub fn runtime_delta(previous: Option<u64>, current: u64) -> u64 {
    previous.map_or(0, |previous| current.saturating_sub(previous))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perf_record_wrapping_at_every_offset() {
        let mut bytes = [0u8; 16];
        bytes[..4].copy_from_slice(&123u32.to_le_bytes());
        bytes[8..].copy_from_slice(&987654321u64.to_le_bytes());
        for split in 0..=16 {
            assert_eq!(
                FrameSample::decode(&bytes[..split], &bytes[split..]),
                Some(FrameSample {
                    pid: 123,
                    ktime_ns: 987654321
                })
            );
        }
        assert_eq!(FrameSample::decode(&bytes[..15], &[]), None);
        let padded = [bytes.as_slice(), &[0; 4]].concat();
        assert_eq!(
            FrameSample::decode(&padded, &[]),
            FrameSample::decode(&bytes, &[])
        );
    }

    #[test]
    fn emits_each_new_frame_once_and_resets_after_gap_or_pid_switch() {
        let mut clock = FrameClock::default();
        clock.reset(100_000_000);
        assert_eq!(clock.ingest(90_000_000, 100_000_000), None);
        assert_eq!(clock.ingest(100_000_000, 100_000_000), None);
        assert_eq!(clock.ingest(116_000_000, 116_000_000), Some(16_000_000));
        assert_eq!(clock.ingest(116_000_000, 120_000_000), None);
        assert_eq!(clock.ingest(110_000_000, 120_000_000), None);
        assert_eq!(clock.ingest(132_000_000, 132_000_000), Some(16_000_000));
        assert_eq!(clock.ingest(1_000_000_000, 1_000_000_000), None);
        clock.reset(1_010_000_000);
        assert_eq!(clock.ingest(1_000_000_000, 1_020_000_000), None);
        assert_eq!(clock.ingest(1_020_000_000, 1_020_000_000), None);
        assert_eq!(clock.ingest(1_036_000_000, 1_036_000_000), Some(16_000_000));
    }

    #[test]
    fn rejects_stale_and_future_frames() {
        let mut clock = FrameClock::default();
        assert_eq!(clock.ingest(1, 300_000_000), None);
        assert_eq!(clock.ingest(400_000_000, 300_000_000), None);
        assert_eq!(clock.ingest(300_000_000, 300_000_000), None);
    }

    #[test]
    fn pending_runtime_is_not_counted_twice() {
        // At t1 raw=100, pending=30. At t2 raw=150, pending=10.
        assert_eq!(runtime_delta(Some(100 + 30), 150 + 10), 30);
        assert_eq!(runtime_delta(None, 1000), 0);
        assert_eq!(runtime_delta(Some(1000), 10), 0);
    }
}
