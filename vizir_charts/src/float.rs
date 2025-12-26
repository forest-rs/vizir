// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Float helpers for `no_std` builds.
//!
//! Rust's float math methods like `f64::log10` and `f64::powf` are not available in `core`.
//! We provide a small trait that dispatches to either `std` or `libm` depending on features.

/// Float math helpers for `f64` in `no_std` mode.
pub(crate) trait FloatExt {
    fn floor(self) -> Self;
    fn ceil(self) -> Self;
    fn round(self) -> Self;
    fn log10(self) -> Self;
    fn ln(self) -> Self;
    fn powf(self, n: Self) -> Self;
    fn powi(self, n: i32) -> Self;
    fn sin(self) -> Self;
    fn cos(self) -> Self;
}

#[cfg(all(not(feature = "std"), feature = "libm"))]
impl FloatExt for f64 {
    fn floor(self) -> Self {
        libm::floor(self)
    }

    fn ceil(self) -> Self {
        libm::ceil(self)
    }

    fn round(self) -> Self {
        libm::round(self)
    }

    fn log10(self) -> Self {
        libm::log10(self)
    }

    fn ln(self) -> Self {
        libm::log(self)
    }

    fn powf(self, n: Self) -> Self {
        libm::pow(self, n)
    }

    fn powi(self, n: i32) -> Self {
        if n == 0 {
            return 1.0;
        }

        let mut exp = i64::from(n);
        let mut base = self;
        if exp < 0 {
            base = 1.0 / base;
            exp = -exp;
        }

        let mut acc = 1.0;
        let mut e = exp as u64;
        while e != 0 {
            if (e & 1) != 0 {
                acc *= base;
            }
            base *= base;
            e >>= 1;
        }
        acc
    }

    fn sin(self) -> Self {
        libm::sin(self)
    }

    fn cos(self) -> Self {
        libm::cos(self)
    }
}

#[cfg(all(not(feature = "std"), not(feature = "libm")))]
compile_error!("vizir_charts requires either the `std` or `libm` feature");
