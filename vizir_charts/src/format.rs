// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Tick formatting helpers.

extern crate alloc;

use alloc::string::String;
use alloc::string::ToString;

#[cfg(not(feature = "std"))]
use crate::float::FloatExt;

pub(crate) fn format_tick_with_step(v: f64, step: f64) -> String {
    if !v.is_finite() {
        return v.to_string();
    }

    let decimals = decimals_for_step(step);
    let v = round_to_decimals(v, decimals);
    let v = if v == 0.0 { 0.0 } else { v }; // normalize `-0.0`

    if decimals == 0 {
        alloc::format!("{v:.0}")
    } else {
        alloc::format!("{v:.decimals$}")
    }
}

fn decimals_for_step(step: f64) -> usize {
    let step = step.abs();
    if step == 0.0 || !step.is_finite() {
        return 0;
    }

    // Find the smallest decimal precision that makes `step * 10^d` integral.
    // This handles steps like 2.5 (d=1) and 0.25 (d=2), and is robust against
    // common float representation error in "nice" tick steps.
    for decimals in 0..=6 {
        let factor = 10_f64.powi(i32::try_from(decimals).unwrap_or(i32::MAX));
        let scaled = step * factor;
        if is_approx_integer(scaled) {
            return decimals;
        }
    }

    6
}

fn is_approx_integer(x: f64) -> bool {
    if !x.is_finite() {
        return false;
    }
    let nearest = x.round();
    let err = (x - nearest).abs();
    err <= 1e-9 * x.abs().max(1.0)
}

fn round_to_decimals(x: f64, decimals: usize) -> f64 {
    if decimals == 0 {
        return x.round();
    }
    let factor = 10_f64.powi(i32::try_from(decimals.min(9)).unwrap_or(i32::MAX));
    if !factor.is_finite() || factor == 0.0 {
        return x;
    }
    (x * factor).round() / factor
}

#[cfg(test)]
mod tests {
    use super::format_tick_with_step;

    #[test]
    fn formats_integer_steps_without_decimals() {
        assert_eq!(format_tick_with_step(0.0, 2.0), "0");
        assert_eq!(format_tick_with_step(10.0, 2.0), "10");
        assert_eq!(format_tick_with_step(2.0, 2.0), "2");
    }

    #[test]
    fn formats_fractional_steps_consistently() {
        assert_eq!(format_tick_with_step(2.5, 2.5), "2.5");
        assert_eq!(format_tick_with_step(5.0, 2.5), "5.0");
        assert_eq!(format_tick_with_step(0.25, 0.25), "0.25");
    }

    #[test]
    fn normalizes_negative_zero() {
        assert_eq!(format_tick_with_step(-0.0, 2.0), "0");
        assert_eq!(format_tick_with_step(-0.0, 0.2), "0.0");
    }
}
