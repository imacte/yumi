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

const WINDOW_SIZE: usize = 120;

pub(super) struct FpsWindow {
    buf: [f32; WINDOW_SIZE],
    pos: usize,
    len: usize,
    sum: f32,
    sq_sum: f32,
    push_count: u32,
}

impl FpsWindow {
    pub(super) fn new() -> Self {
        Self { buf: [0.0; WINDOW_SIZE], pos: 0, len: 0, sum: 0.0, sq_sum: 0.0, push_count: 0 }
    }

    pub(super) fn push(&mut self, fps: f32) {
        if self.len == WINDOW_SIZE {
            let old = self.buf[self.pos];
            self.sum -= old;
            self.sq_sum -= old * old;
        } else {
            self.len += 1;
        }
        self.buf[self.pos] = fps;
        self.sum += fps;
        self.sq_sum += fps * fps;
        self.pos = (self.pos + 1) % WINDOW_SIZE;
        self.push_count += 1;
        // 从 512 降低到 64 帧校准一次，WINDOW_SIZE=120 下每半圈重算一次
        // 在 144fps 下约 0.44 秒校准一次，有效抑制浮点累积误差对齿轮决策的影响
        if self.push_count >= 64 {
            self.recalculate();
            self.push_count = 0;
        }
    }

    fn recalculate(&mut self) {
        let slice = &self.buf[..self.len];
        self.sum = slice.iter().sum();
        self.sq_sum = slice.iter().map(|x| x * x).sum();
    }

    #[inline] pub(super) fn count(&self) -> usize { self.len }
    #[inline] pub(super) fn mean(&self) -> f32 {
        if self.len == 0 { 0.0 } else { self.sum / self.len as f32 }
    }

    pub(super) fn recent_mean(&self, n: usize) -> f32 {
        if self.len == 0 { return 0.0; }
        let count = n.min(self.len);
        let mut sum = 0.0;
        for i in 0..count {
            let idx = (self.pos + WINDOW_SIZE - 1 - i) % WINDOW_SIZE;
            sum += self.buf[idx];
        }
        sum / count as f32
    }

    pub(super) fn stddev(&self) -> f32 {
        if self.len < 2 { return 0.0; }
        let n = self.len as f32;
        let mean = self.sum / n;
        (self.sq_sum / n - mean * mean).max(0.0).sqrt()
    }

    pub(super) fn clear(&mut self) {
        self.buf = [0.0; WINDOW_SIZE];
        self.pos = 0; self.len = 0; self.sum = 0.0; self.sq_sum = 0.0;
        self.push_count = 0;
    }
}
