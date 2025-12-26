// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Tiny scale utilities.
//!
//! These types provide the core coordinate mapping behavior needed for the demos
//! and as a future lowering target for Vega/Vega-Lite-like frontends.

extern crate alloc;

use alloc::vec::Vec;

#[cfg(not(feature = "std"))]
use crate::float::FloatExt;

use vizir_core::{ColId, TableData};

use crate::time;

/// A scale specification (domain + options, no range yet).
#[derive(Clone, Copy, Debug)]
pub enum ScaleSpec {
    /// Continuous linear scale.
    Linear(ScaleLinearSpec),
    /// Continuous log scale.
    Log(ScaleLogSpec),
    /// Continuous time scale (currently numeric seconds).
    Time(ScaleTimeSpec),
    /// Discrete point scale.
    Point(ScalePointSpec),
    /// Discrete band scale.
    Band(ScaleBandSpec),
}

impl From<ScaleLinearSpec> for ScaleSpec {
    fn from(value: ScaleLinearSpec) -> Self {
        Self::Linear(value)
    }
}

impl From<ScaleLogSpec> for ScaleSpec {
    fn from(value: ScaleLogSpec) -> Self {
        Self::Log(value)
    }
}

impl From<ScaleTimeSpec> for ScaleSpec {
    fn from(value: ScaleTimeSpec) -> Self {
        Self::Time(value)
    }
}

impl From<ScalePointSpec> for ScaleSpec {
    fn from(value: ScalePointSpec) -> Self {
        Self::Point(value)
    }
}

impl From<ScaleBandSpec> for ScaleSpec {
    fn from(value: ScaleBandSpec) -> Self {
        Self::Band(value)
    }
}

/// A continuous scale instance.
#[derive(Clone, Copy, Debug)]
pub enum ScaleContinuous {
    /// Linear scale.
    Linear(ScaleLinear),
    /// Log scale.
    Log(ScaleLog),
    /// Time scale.
    Time(ScaleTime),
}

impl ScaleContinuous {
    /// Maps a value from domain space into range space.
    pub fn map(&self, x: f64) -> f64 {
        match self {
            Self::Linear(s) => s.map(x),
            Self::Log(s) => s.map(x),
            Self::Time(s) => s.map(x),
        }
    }

    /// Returns tick values.
    pub fn ticks(&self, count: usize) -> Vec<f64> {
        match self {
            Self::Linear(s) => s.ticks(count),
            Self::Log(s) => s.ticks(count),
            Self::Time(s) => s.ticks(count),
        }
    }

    /// Returns the minimum of the configured domain (as authored).
    pub fn domain_min(&self) -> f64 {
        match self {
            Self::Linear(s) => s.domain_min(),
            Self::Log(s) => s.domain_min(),
            Self::Time(s) => s.domain_min(),
        }
    }

    /// Returns the maximum of the configured domain (as authored).
    pub fn domain_max(&self) -> f64 {
        match self {
            Self::Linear(s) => s.domain_max(),
            Self::Log(s) => s.domain_max(),
            Self::Time(s) => s.domain_max(),
        }
    }
}

/// A linear mapping from a continuous domain to a continuous range.
#[derive(Clone, Copy, Debug)]
pub struct ScaleLinear {
    domain: (f64, f64),
    range: (f64, f64),
}

/// Specification for a linear scale (domain + options, no range yet).
#[derive(Clone, Copy, Debug)]
pub struct ScaleLinearSpec {
    /// Domain in data units.
    pub domain: (f64, f64),
    /// Whether to "nice" the domain based on tick generation.
    pub nice: bool,
}

impl ScaleLinear {
    /// Creates a new scale mapping `domain` values to `range` values.
    pub fn new(domain: (f64, f64), range: (f64, f64)) -> Self {
        Self { domain, range }
    }

    /// Maps a value from domain space into range space.
    pub fn map(&self, x: f64) -> f64 {
        let (d0, d1) = self.domain;
        let (r0, r1) = self.range;
        let denom = d1 - d0;
        if denom == 0.0 {
            return r0;
        }
        let t = (x - d0) / denom;
        r0 + t * (r1 - r0)
    }

    /// Returns the minimum of the configured domain (as authored).
    pub fn domain_min(&self) -> f64 {
        self.domain.0
    }

    /// Returns the maximum of the configured domain (as authored).
    pub fn domain_max(&self) -> f64 {
        self.domain.1
    }

    /// Returns “nice-ish” tick values for the domain.
    pub fn ticks(&self, count: usize) -> Vec<f64> {
        nice_ticks(self.domain.0, self.domain.1, count)
    }
}

impl ScaleLinearSpec {
    /// Creates a new linear scale spec.
    pub fn new(domain: (f64, f64)) -> Self {
        Self {
            domain,
            nice: false,
        }
    }

    /// Enables or disables nice-domain behavior.
    pub fn with_nice(mut self, nice: bool) -> Self {
        self.nice = nice;
        self
    }

    /// Returns the effective domain after applying `nice` (if enabled).
    pub fn resolved_domain(&self, tick_count: usize) -> (f64, f64) {
        if !self.nice {
            return self.domain;
        }
        let ticks = nice_ticks(self.domain.0, self.domain.1, tick_count);
        if ticks.len() >= 2 {
            (*ticks.first().unwrap(), *ticks.last().unwrap())
        } else {
            self.domain
        }
    }

    /// Instantiates a concrete scale for a given output range.
    pub fn instantiate(&self, range: (f64, f64)) -> ScaleLinear {
        ScaleLinear::new(self.domain, range)
    }

    /// Instantiates a concrete scale using the `resolved_domain` (respecting `nice`).
    pub fn instantiate_resolved(&self, range: (f64, f64), tick_count: usize) -> ScaleLinear {
        ScaleLinear::new(self.resolved_domain(tick_count), range)
    }
}

fn nice_ticks(mut min: f64, mut max: f64, count: usize) -> Vec<f64> {
    if count == 0 {
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
    let step = nice_step(step0);
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

fn nice_step(step: f64) -> f64 {
    if !step.is_finite() || step <= 0.0 {
        return 0.0;
    }
    let power = step.log10().floor();
    let base = 10_f64.powf(power);
    let error = step / base;
    let nice = if error >= 7.5 {
        10.0
    } else if error >= 3.5 {
        5.0
    } else if error >= 1.5 {
        2.0
    } else {
        1.0
    };
    nice * base
}

/// A discrete band scale for categorical charts.
#[derive(Clone, Copy, Debug)]
pub struct ScaleBand {
    range: (f64, f64),
    count: usize,
    padding_inner: f64,
    padding_outer: f64,
}

/// Specification for a band scale (count + padding, no range yet).
#[derive(Clone, Copy, Debug)]
pub struct ScaleBandSpec {
    /// Number of bands.
    pub count: usize,
    /// Inner padding in band units.
    pub padding_inner: f64,
    /// Outer padding in band units.
    pub padding_outer: f64,
}

impl ScaleBand {
    /// Creates a new band scale covering `count` bands over `range`.
    pub fn new(range: (f64, f64), count: usize) -> Self {
        Self {
            range,
            count,
            padding_inner: 0.1,
            padding_outer: 0.1,
        }
    }

    /// Sets inner and outer padding in band units.
    pub fn with_padding(mut self, inner: f64, outer: f64) -> Self {
        self.padding_inner = inner.max(0.0);
        self.padding_outer = outer.max(0.0);
        self
    }

    /// Returns the computed band width.
    pub fn band_width(&self) -> f64 {
        let (r0, r1) = self.range;
        let n = self.count as f64;
        if n <= 0.0 {
            return 0.0;
        }
        let span = (r1 - r0).abs();
        let denom = n + self.padding_inner * (n - 1.0) + 2.0 * self.padding_outer;
        if denom == 0.0 { 0.0 } else { span / denom }
    }

    /// Returns the number of bands.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Returns the x-position for a band at `index`.
    pub fn x(&self, index: usize) -> f64 {
        let (r0, r1) = self.range;
        let bw = self.band_width();
        let step = bw * (1.0 + self.padding_inner);
        let start = if r1 >= r0 { r0 } else { r1 };
        start + bw * self.padding_outer + step * index as f64
    }
}

impl ScaleBandSpec {
    /// Creates a new band scale spec with default padding.
    pub fn new(count: usize) -> Self {
        Self {
            count,
            padding_inner: 0.1,
            padding_outer: 0.1,
        }
    }

    /// Sets inner and outer padding in band units.
    pub fn with_padding(mut self, inner: f64, outer: f64) -> Self {
        self.padding_inner = inner.max(0.0);
        self.padding_outer = outer.max(0.0);
        self
    }

    /// Instantiates a concrete scale for a given output range.
    pub fn instantiate(&self, range: (f64, f64)) -> ScaleBand {
        ScaleBand::new(range, self.count).with_padding(self.padding_inner, self.padding_outer)
    }
}

/// A discrete point scale (like band without width).
#[derive(Clone, Copy, Debug)]
pub struct ScalePoint {
    range: (f64, f64),
    count: usize,
    padding: f64,
}

/// Specification for a point scale (count + padding, no range yet).
#[derive(Clone, Copy, Debug)]
pub struct ScalePointSpec {
    /// Number of points.
    pub count: usize,
    /// Outer padding in point steps.
    pub padding: f64,
}

impl ScalePoint {
    /// Creates a new point scale.
    pub fn new(range: (f64, f64), count: usize) -> Self {
        Self {
            range,
            count,
            padding: 0.5,
        }
    }

    /// Sets the outer padding in point steps.
    pub fn with_padding(mut self, padding: f64) -> Self {
        self.padding = padding.max(0.0);
        self
    }

    fn step(&self) -> f64 {
        let (r0, r1) = self.range;
        let n = self.count as f64;
        if n <= 1.0 {
            return 0.0;
        }
        let span = (r1 - r0).abs();
        let denom = (n - 1.0) + 2.0 * self.padding;
        if denom == 0.0 { 0.0 } else { span / denom }
    }

    /// Returns the x-position for a point at `index`.
    pub fn x(&self, index: usize) -> f64 {
        let (r0, r1) = self.range;
        let step = self.step();
        let start = if r1 >= r0 { r0 } else { r1 };
        start + self.padding * step + step * index as f64
    }
}

impl ScalePointSpec {
    /// Creates a new point scale spec.
    pub fn new(count: usize) -> Self {
        Self {
            count,
            padding: 0.5,
        }
    }

    /// Sets the outer padding in point steps.
    pub fn with_padding(mut self, padding: f64) -> Self {
        self.padding = padding.max(0.0);
        self
    }

    /// Instantiates a concrete scale for a given output range.
    pub fn instantiate(&self, range: (f64, f64)) -> ScalePoint {
        ScalePoint::new(range, self.count).with_padding(self.padding)
    }
}

/// A log-scale mapping from a positive domain to a range.
#[derive(Clone, Copy, Debug)]
pub struct ScaleLog {
    domain: (f64, f64),
    range: (f64, f64),
    base: f64,
}

/// Specification for a log scale (domain + base, no range yet).
#[derive(Clone, Copy, Debug)]
pub struct ScaleLogSpec {
    /// Domain in data units (must be positive).
    pub domain: (f64, f64),
    /// Log base (default 10).
    pub base: f64,
}

impl ScaleLog {
    /// Creates a new log scale.
    pub fn new(domain: (f64, f64), range: (f64, f64)) -> Self {
        Self {
            domain,
            range,
            base: 10.0,
        }
    }

    /// Sets the log base.
    pub fn with_base(mut self, base: f64) -> Self {
        self.base = if base.is_finite() && base > 0.0 && base != 1.0 {
            base
        } else {
            10.0
        };
        self
    }

    fn log_base(&self, x: f64) -> f64 {
        let denom = self.base.ln();
        if denom == 0.0 { x.ln() } else { x.ln() / denom }
    }

    /// Maps a value from domain space into range space.
    pub fn map(&self, x: f64) -> f64 {
        let (d0, d1) = self.domain;
        let (r0, r1) = self.range;
        if x <= 0.0 || d0 <= 0.0 || d1 <= 0.0 {
            return r0;
        }
        let ld0 = self.log_base(d0);
        let ld1 = self.log_base(d1);
        let denom = ld1 - ld0;
        if denom == 0.0 {
            return r0;
        }
        let t = (self.log_base(x) - ld0) / denom;
        r0 + t * (r1 - r0)
    }

    /// Returns “nice-ish” tick values for a log domain.
    ///
    /// This currently returns powers of `base` that fall within the domain, capped by `count`.
    pub fn ticks(&self, count: usize) -> Vec<f64> {
        let (mut min, mut max) = self.domain;
        if min > max {
            core::mem::swap(&mut min, &mut max);
        }
        if min <= 0.0 || !min.is_finite() || !max.is_finite() {
            return Vec::new();
        }
        let min_e = {
            let e = self
                .log_base(min)
                .floor()
                .clamp(i32::MIN as f64, i32::MAX as f64);
            #[allow(clippy::cast_possible_truncation, reason = "clamped to the i32 range")]
            {
                e as i32
            }
        };
        let max_e = {
            let e = self
                .log_base(max)
                .ceil()
                .clamp(i32::MIN as f64, i32::MAX as f64);
            #[allow(clippy::cast_possible_truncation, reason = "clamped to the i32 range")]
            {
                e as i32
            }
        };
        let mut out = Vec::new();
        for e in min_e..=max_e {
            out.push(self.base.powi(e));
            if count != 0 && out.len() >= count {
                break;
            }
        }
        out
    }

    /// Returns the minimum of the configured domain (as authored).
    pub fn domain_min(&self) -> f64 {
        self.domain.0
    }

    /// Returns the maximum of the configured domain (as authored).
    pub fn domain_max(&self) -> f64 {
        self.domain.1
    }
}

impl ScaleLogSpec {
    /// Creates a new log scale spec.
    pub fn new(domain: (f64, f64)) -> Self {
        Self { domain, base: 10.0 }
    }

    /// Sets the log base.
    pub fn with_base(mut self, base: f64) -> Self {
        self.base = base;
        self
    }

    /// Instantiates a concrete scale for a given output range.
    pub fn instantiate(&self, range: (f64, f64)) -> ScaleLog {
        ScaleLog::new(self.domain, range).with_base(self.base)
    }
}

/// A time scale (currently a linear scale over numeric timestamps).
///
/// This models time as seconds and provides “nice” ticks over seconds/minutes/hours.
#[derive(Clone, Copy, Debug)]
pub struct ScaleTime {
    inner: ScaleLinear,
}

/// Specification for a time scale (domain, no range yet).
#[derive(Clone, Copy, Debug)]
pub struct ScaleTimeSpec {
    /// Domain in timestamp units (seconds, milliseconds, etc).
    pub domain: (f64, f64),
}

impl ScaleTime {
    /// Creates a new time scale.
    pub fn new(domain: (f64, f64), range: (f64, f64)) -> Self {
        Self {
            inner: ScaleLinear::new(domain, range),
        }
    }

    /// Maps a timestamp value into range space.
    pub fn map(&self, t: f64) -> f64 {
        self.inner.map(t)
    }

    /// Returns “nice-ish” tick values for the time domain (currently numeric).
    pub fn ticks(&self, count: usize) -> Vec<f64> {
        time::nice_time_ticks_seconds(self.inner.domain_min(), self.inner.domain_max(), count)
    }

    /// Returns the minimum of the configured domain (as authored).
    pub fn domain_min(&self) -> f64 {
        self.inner.domain_min()
    }

    /// Returns the maximum of the configured domain (as authored).
    pub fn domain_max(&self) -> f64 {
        self.inner.domain_max()
    }
}

impl ScaleTimeSpec {
    /// Creates a new time scale spec.
    pub fn new(domain: (f64, f64)) -> Self {
        Self { domain }
    }

    /// Instantiates a concrete scale for a given output range.
    pub fn instantiate(&self, range: (f64, f64)) -> ScaleTime {
        ScaleTime::new(self.domain, range)
    }
}

/// Infer a `(min, max)` domain for a numeric column.
///
/// Non-finite values are ignored. Returns `None` if no finite values are present.
pub fn infer_domain_f64(data: &dyn TableData, col: ColId) -> Option<(f64, f64)> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let n = data.row_count();
    for row in 0..n {
        let Some(v) = data.f64(row, col) else {
            continue;
        };
        if !v.is_finite() {
            continue;
        }
        min = min.min(v);
        max = max.max(v);
    }
    if min.is_finite() && max.is_finite() {
        Some((min, max))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn point_scale_positions_are_monotonic() {
        let scale = ScalePoint::new((0.0, 100.0), 5);
        let a = scale.x(0);
        let b = scale.x(1);
        let c = scale.x(2);
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn log_scale_maps_endpoints_to_range() {
        let s = ScaleLog::new((1.0, 100.0), (0.0, 10.0));
        assert!((s.map(1.0) - 0.0).abs() < 1e-9);
        assert!((s.map(100.0) - 10.0).abs() < 1e-9);
    }
}
