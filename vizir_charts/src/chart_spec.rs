// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Chart composition helpers.
//!
//! This module provides a small "composition" layer that owns chart layout and common guides
//! (title, axes, legend). The intent is to reduce hand-wired demo code and converge toward
//! Vega-like composition, where a chart is assembled from:
//! - a plot/data rectangle
//! - guide components (axes, legends, titles)
//! - a set of series/annotation marks.

extern crate alloc;

use alloc::vec::Vec;

use kurbo::Rect;
use vizir_core::Mark;

use crate::{
    AxisSpec, ChartLayout, ChartLayoutSpec, LegendPlacement, LegendSwatchesSpec, ScaleContinuous,
    Size, TextMeasurer, TitleSpec,
};

/// A composed chart description that owns guide specs and layout inputs.
#[derive(Clone, Debug, Default)]
pub struct ChartSpec {
    /// Optional title.
    pub title: Option<TitleSpec>,
    /// Desired plot size (data rectangle), used when `layout.view_size` is `None`.
    pub plot_size: Size,
    /// Layout options.
    pub layout: ChartLayoutSpec,
    /// Optional left axis.
    pub axis_left: Option<AxisSpec>,
    /// Optional right axis.
    pub axis_right: Option<AxisSpec>,
    /// Optional top axis.
    pub axis_top: Option<AxisSpec>,
    /// Optional bottom axis.
    pub axis_bottom: Option<AxisSpec>,
    /// Optional legend.
    pub legend: Option<(LegendSwatchesSpec, LegendPlacement)>,
}

impl ChartSpec {
    /// Returns the bottom axis if present, otherwise the top axis.
    pub fn x_axis(&self) -> Option<&AxisSpec> {
        self.axis_bottom.as_ref().or(self.axis_top.as_ref())
    }

    /// Returns the left axis if present, otherwise the right axis.
    pub fn y_axis(&self) -> Option<&AxisSpec> {
        self.axis_left.as_ref().or(self.axis_right.as_ref())
    }

    /// Instantiates the x-axis scale for a given plot rectangle.
    ///
    /// Returns `None` if no x-axis is configured.
    ///
    /// Panics if the configured x-axis is not a continuous scale.
    pub fn x_scale_continuous(&self, plot: Rect) -> Option<ScaleContinuous> {
        self.x_axis().map(|a| a.scale_continuous(plot))
    }

    /// Instantiates the y-axis scale for a given plot rectangle.
    ///
    /// Returns `None` if no y-axis is configured.
    ///
    /// Panics if the configured y-axis is not a continuous scale.
    pub fn y_scale_continuous(&self, plot: Rect) -> Option<ScaleContinuous> {
        self.y_axis().map(|a| a.scale_continuous(plot))
    }

    /// Computes layout for this chart.
    pub fn layout(&self, measurer: &dyn TextMeasurer) -> ChartLayout {
        let title_top = self.title.as_ref().map(|t| t.measure(measurer));

        let axis_left_w = self.axis_left.as_ref().map(|a| a.measure(measurer));
        let axis_right_w = self.axis_right.as_ref().map(|a| a.measure(measurer));
        let axis_top_h = self.axis_top.as_ref().map(|a| a.measure(measurer));
        let axis_bottom_h = self.axis_bottom.as_ref().map(|a| a.measure(measurer));

        let legend = self.legend.as_ref().map(|(spec, placement)| {
            let size = spec.measure(measurer);
            (size, *placement)
        });

        let mut layout = self.layout;
        layout.title_top = title_top;
        layout.plot_size = self.plot_size;
        layout.axis_left = axis_left_w;
        layout.axis_right = axis_right_w;
        layout.axis_top = axis_top_h;
        layout.axis_bottom = axis_bottom_h;
        layout.legend = legend;

        ChartLayout::arrange(&layout)
    }

    /// Generates marks for titles/axes/legend, given a computed layout.
    pub fn guide_marks(&self, measurer: &dyn TextMeasurer, layout: &ChartLayout) -> Vec<Mark> {
        let mut out = Vec::new();

        if let (Some(title), Some(rect)) = (self.title.as_ref(), layout.title_top) {
            out.extend(title.marks(measurer, rect));
        }

        let plot = layout.data;
        if let (Some(axis), Some(axis_rect)) = (self.axis_bottom.as_ref(), layout.axis_bottom) {
            out.extend(axis.marks(plot, axis_rect));
        }
        if let (Some(axis), Some(axis_rect)) = (self.axis_top.as_ref(), layout.axis_top) {
            out.extend(axis.marks(plot, axis_rect));
        }
        if let (Some(axis), Some(axis_rect)) = (self.axis_left.as_ref(), layout.axis_left) {
            out.extend(axis.marks(plot, axis_rect));
        }
        if let (Some(axis), Some(axis_rect)) = (self.axis_right.as_ref(), layout.axis_right) {
            out.extend(axis.marks(plot, axis_rect));
        }

        if let (Some((legend, _placement)), Some(rect)) = (self.legend.as_ref(), layout.legend) {
            out.extend(legend.marks(rect.x0, rect.y0));
        }

        out
    }

    /// Convenience to produce a full mark list: series marks + guide marks.
    ///
    /// The series builder is invoked with the resolved plot rectangle.
    pub fn marks(
        &self,
        measurer: &dyn TextMeasurer,
        build_series: impl FnOnce(&Self, Rect) -> Vec<Mark>,
    ) -> (ChartLayout, Vec<Mark>) {
        let layout = self.layout(measurer);
        let mut marks = build_series(self, layout.data);
        marks.extend(self.guide_marks(measurer, &layout));
        (layout, marks)
    }
}
