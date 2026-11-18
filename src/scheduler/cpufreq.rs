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

use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::{FastWriter, write_to_file};
use log::{debug, warn};

use super::CpuPolicy;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FrequencyRange {
    pub min: u32,
    pub max: u32,
}

impl FrequencyRange {
    fn read(path: &Path) -> Option<Self> {
        let min = fs::read_to_string(path.join("scaling_min_freq")).ok()?;
        let max = fs::read_to_string(path.join("scaling_max_freq")).ok()?;
        let (min, max) = (min.trim().parse().ok()?, max.trim().parse().ok()?);
        (min > 0 && min <= max).then_some(Self { min, max })
    }

    fn min_first(self, next: Self) -> bool {
        next.max < self.min
    }
}

fn nearest_index(frequencies: &[u32], perf: f32) -> usize {
    let min = frequencies[0];
    let max = *frequencies.last().unwrap();
    let target = min as f32 + (max - min) as f32 * perf;
    let hi = frequencies.partition_point(|&freq| (freq as f32) < target);
    if hi == 0 || hi == frequencies.len() {
        return hi.saturating_sub(1);
    }
    let lo = hi - 1;
    if frequencies[hi] as f32 - target < target - frequencies[lo] as f32 {
        hi
    } else {
        lo
    }
}

fn frequency_range(frequencies: &[u32], perf: f32, floor: f32, steps_below: u32) -> FrequencyRange {
    let high = nearest_index(frequencies, perf);
    let low = high
        .saturating_sub(steps_below as usize)
        .max(nearest_index(frequencies, floor).min(high));
    FrequencyRange {
        min: frequencies[low],
        max: frequencies[high],
    }
}

pub(super) struct PolicyFrequency {
    pub id: i32,
    path: PathBuf,
    frequencies: Vec<u32>,
    original_governor: String,
    governor: String,
    original: FrequencyRange,
    applied: FrequencyRange,
    min_writer: FastWriter,
    max_writer: FastWriter,
}

impl PolicyFrequency {
    pub fn open(policy: &CpuPolicy) -> Option<Self> {
        let id = policy.id;
        let path = PathBuf::from(format!("/sys/devices/system/cpu/cpufreq/policy{id}"));
        let mut frequencies: Vec<u32> =
            fs::read_to_string(path.join("scaling_available_frequencies"))
                .unwrap_or_default()
                .split_whitespace()
                .filter_map(|freq| freq.parse::<u32>().ok().filter(|&freq| freq > 0))
                .collect();
        if frequencies.is_empty() {
            return None;
        }
        frequencies.extend(
            policy
                .boost_frequencies
                .iter()
                .copied()
                .filter(|&freq| freq > 0),
        );
        frequencies.sort_unstable();
        frequencies.dedup();

        let min_writer = FastWriter::new(path.join("scaling_min_freq"));
        let max_writer = FastWriter::new(path.join("scaling_max_freq"));
        if !min_writer.is_valid() || !max_writer.is_valid() {
            return None;
        }
        let governor = fs::read_to_string(path.join("scaling_governor")).ok();
        let original_governor = governor
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let (Some(original_governor), Some(original)) =
            (original_governor, FrequencyRange::read(&path))
        else {
            warn!("[CPUFreq] Cannot snapshot governor and limits for policy{id}");
            return None;
        };

        Some(Self {
            id,
            path,
            frequencies,
            original_governor: original_governor.to_owned(),
            governor: original_governor.to_owned(),
            original,
            applied: original,
            min_writer,
            max_writer,
        })
    }

    pub fn available_range(&self) -> FrequencyRange {
        FrequencyRange {
            min: self.frequencies[0],
            max: *self.frequencies.last().unwrap(),
        }
    }

    pub fn applied_range(&self) -> FrequencyRange {
        self.applied
    }

    pub fn range_for_perf(&self, perf: f32, floor: f32, steps_below: u32) -> FrequencyRange {
        frequency_range(&self.frequencies, perf, floor, steps_below)
    }

    pub fn select_governor(&mut self, requested: Option<&str>) {
        let requested = requested
            .map(str::trim)
            .filter(|name| !name.is_empty() && *name != "system");
        if let Some(name) = requested {
            if name == self.governor {
                return;
            }
            if name != self.original_governor {
                let supported = fs::read_to_string(self.path.join("scaling_available_governors"))
                    .is_ok_and(|names| names.split_whitespace().any(|available| available == name));
                if supported && self.write_governor(name) {
                    self.governor = name.to_owned();
                    return;
                }
                if !supported {
                    warn!(
                        "[CPUFreq] policy{} does not support governor {name}",
                        self.id
                    );
                }
            }
        }
        self.restore_governor();
    }

    fn write_governor(&self, name: &str) -> bool {
        match write_to_file(self.path.join("scaling_governor"), name) {
            Ok(()) => true,
            Err(error) => {
                warn!(
                    "[CPUFreq] Cannot set governor {name} for policy{}: {error}",
                    self.id
                );
                false
            }
        }
    }

    fn restore_governor(&mut self) -> bool {
        if self.governor != self.original_governor {
            if !self.write_governor(&self.original_governor) {
                return false;
            }
            self.governor.clone_from(&self.original_governor);
        }
        true
    }

    pub fn apply(&mut self, range: FrequencyRange) -> bool {
        // A partially successful update still leaves the cached endpoints in write order.
        if self.applied.min_first(range) {
            self.write_min(range.min, false) && self.write_max(range.max, false)
        } else {
            self.write_max(range.max, false) && self.write_min(range.min, false)
        }
    }

    fn write_min(&mut self, freq: u32, force: bool) -> bool {
        if (force || freq != self.applied.min) && !self.min_writer.write_value_force(freq) {
            return false;
        }
        self.applied.min = freq;
        true
    }

    fn write_max(&mut self, freq: u32, force: bool) -> bool {
        if (force || freq != self.applied.max) && !self.max_writer.write_value_force(freq) {
            return false;
        }
        self.applied.max = freq;
        true
    }

    pub fn verify_limits(&mut self) {
        let Some(actual) = FrequencyRange::read(&self.path) else {
            return;
        };
        let expected = self.applied;
        // A tighter effective upper limit can come from thermal or power QoS.
        if actual.max < expected.max {
            return;
        }
        if actual.max > expected.max || actual.min < expected.min {
            debug!(
                "[CPUFreq] policy{} limits widened to {}-{} kHz, requested {}-{} kHz",
                self.id, actual.min, actual.max, expected.min, expected.max
            );
            if actual.max == expected.max || self.write_max(expected.max, true) {
                if actual.min < expected.min {
                    self.write_min(expected.min, true);
                }
            }
        }
    }

    pub fn restore(&mut self) -> bool {
        let limits_restored = self.apply(self.original);
        let governor_restored = self.restore_governor();
        limits_restored && governor_restored
    }
}

#[cfg(test)]
mod tests {
    use super::{FrequencyRange, frequency_range};

    #[test]
    fn range_uses_available_steps_and_honors_floor() {
        let frequencies = [300_000, 600_000, 1_200_000, 1_500_000];
        assert_eq!(
            frequency_range(&frequencies, 1.0, 0.0, 2),
            FrequencyRange {
                min: 600_000,
                max: 1_500_000
            }
        );
        assert_eq!(
            frequency_range(&frequencies, 1.0, 0.75, 2),
            FrequencyRange {
                min: 1_200_000,
                max: 1_500_000
            }
        );
        assert_eq!(
            frequency_range(&frequencies, 0.0, 0.75, 1),
            FrequencyRange {
                min: 300_000,
                max: 300_000
            }
        );
        assert_eq!(
            frequency_range(&[800_000], 1.0, 0.0, 2),
            FrequencyRange {
                min: 800_000,
                max: 800_000
            }
        );
    }

    #[test]
    fn configurable_width_can_pin_the_target_or_reach_the_floor() {
        let frequencies = [300_000, 600_000, 1_200_000, 1_500_000];
        assert_eq!(
            frequency_range(&frequencies, 1.0, 0.25, 0),
            FrequencyRange {
                min: 1_500_000,
                max: 1_500_000
            }
        );
        assert_eq!(
            frequency_range(&frequencies, 1.0, 0.25, u32::MAX),
            FrequencyRange {
                min: 600_000,
                max: 1_500_000
            }
        );
    }

    #[test]
    fn update_order_keeps_intermediate_bounds_valid() {
        let mut ranges = Vec::new();
        for min in 1..=4 {
            for max in min..=4 {
                ranges.push(FrequencyRange { min, max });
            }
        }
        for &current in &ranges {
            for &next in &ranges {
                if current.min_first(next) {
                    assert!(next.min <= current.max);
                } else {
                    assert!(current.min <= next.max);
                }
            }
        }
    }
}
