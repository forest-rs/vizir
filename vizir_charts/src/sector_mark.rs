// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Sector (arc) mark generation.
//!
//! Vega models pie/donut slices using the `arc` mark, typically driven by start/end angles and
//! inner/outer radii. Swift Charts exposes a similar primitive as `SectorMark`.

extern crate alloc;

use alloc::vec::Vec;

use kurbo::{Circle, Point, Shape};
use peniko::Brush;
use vizir_core::{Mark, MarkId};

use crate::axis::StrokeStyle;

/// A sector (arc slice), suitable for pie/donut charts.
///
/// Angles are in radians, matching Vega’s internal representation.
#[derive(Clone, Debug)]
pub struct SectorMarkSpec {
    /// Stable mark id.
    pub id: MarkId,
    /// Center in scene coordinates.
    pub center: Point,
    /// Inner radius in scene coordinates (0 for a pie slice).
    pub inner_radius: f64,
    /// Outer radius in scene coordinates.
    pub outer_radius: f64,
    /// Start angle in radians.
    pub start_angle: f64,
    /// End angle in radians.
    pub end_angle: f64,
    /// Fill paint for the sector.
    pub fill: Brush,
    /// Optional outline stroke.
    pub stroke: Option<StrokeStyle>,
    /// Curve flattening tolerance when converting the sector to a `BezPath`.
    pub tolerance: f64,
    /// Rendering order hint (`vizir_core::Mark::z_index`).
    pub z_index: i32,
}

impl SectorMarkSpec {
    /// Creates a new sector mark spec.
    pub fn new(
        id: MarkId,
        center: Point,
        inner_radius: f64,
        outer_radius: f64,
        start_angle: f64,
        end_angle: f64,
    ) -> Self {
        Self {
            id,
            center,
            inner_radius,
            outer_radius,
            start_angle,
            end_angle,
            fill: Brush::default(),
            stroke: None,
            tolerance: 0.1,
            z_index: crate::z_order::SERIES_FILL,
        }
    }

    /// Sets the fill paint.
    pub fn with_fill(mut self, fill: impl Into<Brush>) -> Self {
        self.fill = fill.into();
        self
    }

    /// Sets the outline stroke.
    pub fn with_stroke(mut self, stroke: StrokeStyle) -> Self {
        self.stroke = Some(stroke);
        self
    }

    /// Disables the outline stroke.
    pub fn without_stroke(mut self) -> Self {
        self.stroke = None;
        self
    }

    /// Sets the curve flattening tolerance used for `BezPath` conversion.
    pub fn with_tolerance(mut self, tolerance: f64) -> Self {
        self.tolerance = tolerance;
        self
    }

    /// Sets the z-index used for render ordering.
    pub fn with_z_index(mut self, z_index: i32) -> Self {
        self.z_index = z_index;
        self
    }

    /// Generates marks for this spec.
    pub fn marks(&self) -> Vec<Mark> {
        let circle = Circle::new(self.center, self.outer_radius);
        let sweep = self.end_angle - self.start_angle;
        let segment = circle.segment(self.inner_radius, self.start_angle, sweep);
        let path = segment.path_elements(self.tolerance).collect();

        let mut builder = Mark::builder(self.id)
            .path()
            .path_const(path)
            .z_index(self.z_index)
            .fill_brush_const(self.fill.clone());

        if let Some(stroke) = self.stroke.clone() {
            builder = builder
                .stroke_brush_const(stroke.brush)
                .stroke_width_const(stroke.stroke_width);
        } else {
            builder = builder.stroke_width_const(0.0);
        }

        alloc::vec![builder.build()]
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use peniko::Color;
    use peniko::color::palette::css;
    use vizir_core::{MarkDiff, MarkKind, MarkPayload, Scene};

    use super::*;

    #[test]
    fn sector_emits_a_path_mark_with_bounds() {
        let sector = SectorMarkSpec::new(
            MarkId::from_raw(1),
            Point::new(50.0, 50.0),
            10.0,
            20.0,
            0.0,
            core::f64::consts::FRAC_PI_2,
        )
        .with_fill(css::TOMATO)
        .with_stroke(StrokeStyle::solid(css::BLACK, 2.0));

        let mut scene = Scene::new();
        let diffs = scene.tick(sector.marks());
        let [
            MarkDiff::Enter {
                id,
                kind,
                new,
                bounds,
                ..
            },
        ] = &diffs[..]
        else {
            panic!("expected a single enter diff");
        };
        assert_eq!(*id, MarkId::from_raw(1));
        assert_eq!(*kind, MarkKind::Path);
        assert!(bounds.is_some());

        let MarkPayload::Path(p) = &**new else {
            panic!("expected path payload");
        };
        assert_eq!(p.fill, css::TOMATO.into());
        assert_eq!(p.stroke, css::BLACK.into());
        assert_eq!(p.stroke_width, 2.0);
        assert_ne!(p.path.bounding_box(), kurbo::Rect::new(0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn sector_without_stroke_has_zero_stroke_width() {
        let sector = SectorMarkSpec::new(
            MarkId::from_raw(1),
            Point::new(0.0, 0.0),
            0.0,
            10.0,
            0.0,
            core::f64::consts::PI,
        )
        .with_fill(Color::TRANSPARENT);

        let mut scene = Scene::new();
        let diffs = scene.tick(sector.marks());
        let [MarkDiff::Enter { new, .. }] = &diffs[..] else {
            panic!("expected a single enter diff");
        };
        let MarkPayload::Path(p) = &**new else {
            panic!("expected path payload");
        };
        assert_eq!(p.stroke_width, 0.0);
    }
}
