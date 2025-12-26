// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Time tick generation and formatting helpers.
//!
//! This is intentionally small and `no_std`-friendly. It models time as a numeric value in
//! **seconds**, and provides:
//! - "nice" tick steps for seconds/minutes/hours
//! - formatting for tick labels (e.g. `1:05`, `2:03:00`)

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

#[cfg(not(feature = "std"))]
use crate::float::FloatExt;

/// Returns a vector of "nice-ish" tick values for a time domain expressed in seconds.
pub fn nice_time_ticks_seconds(mut min: f64, mut max: f64, count: usize) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    if !min.is_finite() || !max.is_finite() {
        return Vec::new();
    }
    if min == max {
        return alloc::vec![min];
    }
    if min > max {
        core::mem::swap(&mut min, &mut max);
    }

    let span = max - min;
    let step0 = span / count.max(1) as f64;
    let step = nice_time_step_seconds(step0);
    if step == 0.0 {
        return alloc::vec![min, max];
    }

    let start = (min / step).floor() * step;
    let stop = (max / step).ceil() * step;
    let n_f = ((stop - start) / step).round();
    let n = if n_f.is_finite() && n_f >= 0.0 {
        let n_f = n_f.min(10_000.0);
        #[allow(
            clippy::cast_possible_truncation,
            reason = "guarded by finite/non-negative checks and capped at 10k"
        )]
        {
            n_f as u64
        }
    } else {
        0
    };

    (0..=n).map(|i| start + step * i as f64).collect()
}

fn nice_time_step_seconds(step: f64) -> f64 {
    if !step.is_finite() || step <= 0.0 {
        return 0.0;
    }

    // Candidate steps in seconds, spanning seconds/minutes/hours.
    //
    // This is deliberately small. We can extend this later (days/months) when we have a richer
    // time representation and formatting requirements.
    const STEPS: &[f64] = &[
        1.0,
        2.0,
        5.0,
        10.0,
        15.0,
        30.0,
        60.0,
        2.0 * 60.0,
        5.0 * 60.0,
        10.0 * 60.0,
        15.0 * 60.0,
        30.0 * 60.0,
        60.0 * 60.0,
        2.0 * 60.0 * 60.0,
        3.0 * 60.0 * 60.0,
        6.0 * 60.0 * 60.0,
        12.0 * 60.0 * 60.0,
    ];

    for &s in STEPS {
        if s >= step {
            return s;
        }
    }
    // Fallback: round up to the next hour-ish magnitude.
    let hours = (step / 3600.0).ceil();
    (hours.max(1.0)) * 3600.0
}

/// Formats a tick value (seconds) given the tick step (seconds).
///
/// Intended for use with [`crate::axis::AxisSpec::with_tick_formatter`].
pub fn format_time_seconds(v: f64, step: f64) -> String {
    if !v.is_finite() {
        return alloc::format!("{v}");
    }

    let sign = if v < 0.0 { "-" } else { "" };
    let secs = {
        let secs_f = v.abs().round().clamp(i64::MIN as f64, i64::MAX as f64);
        #[allow(clippy::cast_possible_truncation, reason = "clamped to the i64 range")]
        {
            secs_f as i64
        }
    };
    let step = step.abs();

    let h = secs / 3600;
    let m = (secs / 60) % 60;
    let s = secs % 60;

    if step >= 3600.0 || h > 0 {
        alloc::format!("{sign}{h}:{m:02}:{s:02}")
    } else if step >= 60.0 || m > 0 {
        alloc::format!("{sign}{m}:{s:02}")
    } else {
        alloc::format!("{sign}{s}")
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn time_ticks_choose_minute_steps_for_minute_spans() {
        let ticks = nice_time_ticks_seconds(0.0, 300.0, 5);
        assert!(ticks.len() >= 2);
        let step = (ticks[1] - ticks[0]).abs();
        assert!(step >= 60.0);
    }

    #[test]
    fn time_format_seconds_minutes_hours() {
        assert_eq!(format_time_seconds(5.0, 1.0), "5");
        assert_eq!(format_time_seconds(65.0, 1.0), "1:05");
        assert_eq!(format_time_seconds(3723.0, 60.0), "1:02:03");
    }
}
