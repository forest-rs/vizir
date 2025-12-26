// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Symbol helpers for point-like marks.

use kurbo::{BezPath, Circle, Shape};

/// A small set of symbol shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Symbol {
    /// A square (axis-aligned).
    Square,
    /// A circle.
    Circle,
}

impl Symbol {
    /// Returns a path for this symbol centered at `cx, cy`, using `size` as the diameter/side.
    pub fn path(self, cx: f64, cy: f64, size: f64) -> BezPath {
        match self {
            Self::Square => square_path(cx, cy, size),
            Self::Circle => circle_path(cx, cy, size),
        }
    }
}

fn square_path(cx: f64, cy: f64, size: f64) -> BezPath {
    let half = size * 0.5;
    let x0 = cx - half;
    let y0 = cy - half;
    let x1 = cx + half;
    let y1 = cy + half;
    let mut p = BezPath::new();
    p.move_to((x0, y0));
    p.line_to((x1, y0));
    p.line_to((x1, y1));
    p.line_to((x0, y1));
    p.close_path();
    p
}

fn circle_path(cx: f64, cy: f64, size: f64) -> BezPath {
    let r = size * 0.5;
    let circle = Circle::new((cx, cy), r);
    // This is used for demo purposes; in real renderers, the tolerance is usually based on the
    // target device/pixel size.
    let tolerance = 0.1;
    circle.path_elements(tolerance).collect()
}
