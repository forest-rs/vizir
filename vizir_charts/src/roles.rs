// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Semantic mark roles used by chart builders.

use vizir_core::MarkRole;

/// Role for a bar or stacked-bar segment in a data series.
pub const ROLE_SERIES_BAR: MarkRole = MarkRole::new("series.bar");

/// Role for a point or symbol in a data series.
pub const ROLE_SERIES_POINT: MarkRole = MarkRole::new("series.point");

/// Role for a line path in a data series.
pub const ROLE_SERIES_LINE: MarkRole = MarkRole::new("series.line");

/// Role for an area path in a data series.
pub const ROLE_SERIES_AREA: MarkRole = MarkRole::new("series.area");

/// Role for a sector slice.
pub const ROLE_SERIES_SECTOR: MarkRole = MarkRole::new("series.sector");
