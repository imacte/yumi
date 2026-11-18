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

use crate::fas_types::ClusterProfile;
use crate::scheduler::cpufreq::{FrequencyRange, PolicyFrequency};

pub struct PolicyController {
    pub(super) frequency: PolicyFrequency,
    pub(super) freq_hold_frames: u32,
    pub(super) pending: bool,
    cluster_profile: ClusterProfile,
}

impl PolicyController {
    pub(super) fn new(frequency: PolicyFrequency, cluster_profile: ClusterProfile) -> Self {
        Self {
            frequency,
            freq_hold_frames: 0,
            pending: true,
            cluster_profile,
        }
    }

    fn adjust_perf(&self, perf: f32) -> f32 {
        let weight = self.cluster_profile.capacity_weight.max(0.1);
        // Keep the larger clusters responsive by blending the weighted and linear curves.
        let blend = ((weight - 1.0).clamp(0.0, 1.5) / 1.5) * 0.5;
        perf * (1.0 - blend) + perf.powf(weight.sqrt()) * blend
    }

    pub(super) fn range_for_perf(&self, perf: f32, floor: f32, steps_below: u32) -> FrequencyRange {
        self.frequency
            .range_for_perf(self.adjust_perf(perf), self.adjust_perf(floor), steps_below)
    }

    pub(super) fn range_changed(&self, next: FrequencyRange, hysteresis: f32) -> bool {
        let current = self.frequency.applied_range();
        let available = self.frequency.available_range();
        let diff = current
            .min
            .abs_diff(next.min)
            .max(current.max.abs_diff(next.max));
        diff as f32 / (available.max - available.min).max(1) as f32 > hysteresis
    }

    pub(super) fn apply_range(&mut self, range: FrequencyRange) {
        self.pending = !self.frequency.apply(range);
        if !self.pending {
            self.freq_hold_frames = 2;
        }
    }
}
