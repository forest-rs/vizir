// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A tiny measure/arrange layout helper for charts.
//!
//! This follows the same basic shape as WPF-style layout:
//! - **Measure**: determine desired extents (margins) for guides (axes, legends).
//! - **Arrange**: place guides relative to the plot rectangle based on orientation.
//!
//! This module is intentionally small and heuristic-driven. It provides a place
//! to converge with future Understory display layout, while keeping chart logic
//! out of `vizir_core`.

use kurbo::Rect;

use crate::measure::TextMeasurer;

/// A width/height pair used by chart layout.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    /// Width in chart coordinate units.
    pub width: f64,
    /// Height in chart coordinate units.
    pub height: f64,
}

/// Legend orientation settings, matching Vega’s core options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegendOrient {
    /// Place the legend to the left of the chart.
    Left,
    /// Place the legend to the right of the chart.
    Right,
    /// Place the legend above the chart.
    Top,
    /// Place the legend below the chart.
    Bottom,
    /// Place the legend inside the upper-left corner of the plot.
    TopLeft,
    /// Place the legend inside the upper-right corner of the plot.
    TopRight,
    /// Place the legend inside the lower-left corner of the plot.
    BottomLeft,
    /// Place the legend inside the lower-right corner of the plot.
    BottomRight,
    /// Disable automatic placement and use explicit coordinates.
    None,
}

/// Legend placement options (orientation + offset).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegendPlacement {
    /// Legend orientation.
    pub orient: LegendOrient,
    /// Offset in pixels away from the data rectangle / axes (or inward for corners).
    pub offset: f64,
    /// Explicit x position, used only when `orient` is `None`.
    pub x: f64,
    /// Explicit y position, used only when `orient` is `None`.
    pub y: f64,
}

impl Default for LegendPlacement {
    fn default() -> Self {
        Self {
            orient: LegendOrient::Right,
            offset: 18.0,
            x: 0.0,
            y: 0.0,
        }
    }
}

/// Layout inputs for a single chart: a plot area plus optional axes/legend.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ChartLayoutSpec {
    /// Optional chart title thickness (reserved above the plot and guides).
    pub title_top: Option<f64>,
    /// The desired plot size (the “data rectangle” in Vega docs).
    ///
    /// If `view_size` is `Some`, this is treated as a fallback; the plot size is derived
    /// from the available view size instead (Vega-like `autosize: "fit"` behavior).
    pub plot_size: Size,
    /// Optional explicit view size (outer chart bounds).
    ///
    /// If set, `ChartLayout::arrange` will compute the largest plot size that fits within
    /// the given view size after accounting for guides and `outer_padding`.
    pub view_size: Option<Size>,
    /// Extra padding around the whole chart (applied on all sides).
    ///
    /// This is a simple stand-in for Vega’s `padding` behavior and helps avoid
    /// clipping tick labels that lie on the plot edge.
    pub outer_padding: f64,
    /// Extra padding applied inside the plot rectangle.
    ///
    /// This produces a `ChartLayout::data` rectangle that is inset from `ChartLayout::plot`.
    /// For now this is a simple uniform inset; it is a placeholder for a more Vega-like
    /// padding/autosize story (per-side padding, contains = "padding", etc.).
    pub plot_padding: f64,
    /// Whether to include a left axis, and its desired margin thickness.
    pub axis_left: Option<f64>,
    /// Whether to include a right axis, and its desired margin thickness.
    pub axis_right: Option<f64>,
    /// Whether to include a top axis, and its desired margin thickness.
    pub axis_top: Option<f64>,
    /// Whether to include a bottom axis, and its desired margin thickness.
    pub axis_bottom: Option<f64>,
    /// An optional legend, given by its desired size and placement.
    pub legend: Option<(Size, LegendPlacement)>,
}

/// Output of the arrange pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChartLayout {
    /// Outer chart bounds.
    pub view: Rect,
    /// Reserved rectangle for the chart title (if any).
    pub title_top: Option<Rect>,
    /// The plot rectangle (outer data rectangle in Vega docs).
    pub plot: Rect,
    /// The inner data rectangle (plot inset by `plot_padding`).
    pub data: Rect,
    /// Reserved rectangle for the left axis (if any).
    pub axis_left: Option<Rect>,
    /// Reserved rectangle for the right axis (if any).
    pub axis_right: Option<Rect>,
    /// Reserved rectangle for the top axis (if any).
    pub axis_top: Option<Rect>,
    /// Reserved rectangle for the bottom axis (if any).
    pub axis_bottom: Option<Rect>,
    /// Legend placement rectangle (if any).
    pub legend: Option<Rect>,
}

impl ChartLayout {
    /// Computes a layout from the provided specification.
    pub fn arrange(spec: &ChartLayoutSpec) -> Self {
        let outer_padding = spec.outer_padding.max(0.0);
        let plot_padding = spec.plot_padding.max(0.0);
        let title_top_h = spec.title_top.unwrap_or(0.0).max(0.0);
        let axis_left_w = spec.axis_left.unwrap_or(0.0).max(0.0);
        let axis_right_w = spec.axis_right.unwrap_or(0.0).max(0.0);
        let axis_top_h = spec.axis_top.unwrap_or(0.0).max(0.0);
        let axis_bottom_h = spec.axis_bottom.unwrap_or(0.0).max(0.0);

        let mut margin_left = outer_padding + axis_left_w;
        let mut margin_right = outer_padding + axis_right_w;
        let mut margin_top = outer_padding + title_top_h + axis_top_h;
        let mut margin_bottom = outer_padding + axis_bottom_h;

        if let Some((legend_size, placement)) = spec.legend {
            match placement.orient {
                LegendOrient::Left => {
                    margin_left += legend_size.width.max(0.0) + placement.offset.max(0.0);
                }
                LegendOrient::Right => {
                    margin_right += legend_size.width.max(0.0) + placement.offset.max(0.0);
                }
                LegendOrient::Top => {
                    margin_top += legend_size.height.max(0.0) + placement.offset.max(0.0);
                }
                LegendOrient::Bottom => {
                    margin_bottom += legend_size.height.max(0.0) + placement.offset.max(0.0);
                }
                LegendOrient::TopLeft
                | LegendOrient::TopRight
                | LegendOrient::BottomLeft
                | LegendOrient::BottomRight
                | LegendOrient::None => {}
            }
        }

        let (plot_w, plot_h) = match spec.view_size {
            Some(v) => (
                (v.width.max(0.0) - margin_left - margin_right).max(0.0),
                (v.height.max(0.0) - margin_top - margin_bottom).max(0.0),
            ),
            None => (
                spec.plot_size.width.max(0.0),
                spec.plot_size.height.max(0.0),
            ),
        };

        let plot = Rect::new(
            margin_left,
            margin_top,
            margin_left + plot_w,
            margin_top + plot_h,
        );

        let inset_x = plot_padding.min(0.5 * plot.width());
        let inset_y = plot_padding.min(0.5 * plot.height());
        let data = Rect::new(
            plot.x0 + inset_x,
            plot.y0 + inset_y,
            plot.x1 - inset_x,
            plot.y1 - inset_y,
        );

        // Axes are placed adjacent to the *data* rectangle so scale mapping matches marks.
        let axis_left = if axis_left_w > 0.0 {
            Some(Rect::new(data.x0 - axis_left_w, data.y0, data.x0, data.y1))
        } else {
            None
        };

        let axis_right = if axis_right_w > 0.0 {
            Some(Rect::new(data.x1, data.y0, data.x1 + axis_right_w, data.y1))
        } else {
            None
        };

        let axis_top = if axis_top_h > 0.0 {
            Some(Rect::new(data.x0, data.y0 - axis_top_h, data.x1, data.y0))
        } else {
            None
        };

        let axis_bottom = if axis_bottom_h > 0.0 {
            Some(Rect::new(
                data.x0,
                data.y1,
                data.x1,
                data.y1 + axis_bottom_h,
            ))
        } else {
            None
        };

        // Legends are placed outside the axes (relative to the plot+axes block),
        // matching Vega’s default semantics.
        let legend = spec.legend.map(|(legend_size, placement)| {
            legend_rect(
                data,
                axis_left_w,
                axis_right_w,
                axis_top_h,
                axis_bottom_h,
                legend_size,
                placement,
            )
        });

        let view_size = spec.view_size.unwrap_or(Size {
            width: margin_left + plot_w + margin_right,
            height: margin_top + plot_h + margin_bottom,
        });
        let view = Rect::new(0.0, 0.0, view_size.width, view_size.height);

        let title_top = if title_top_h > 0.0 {
            Some(Rect::new(
                0.0,
                outer_padding,
                view.x1,
                outer_padding + title_top_h,
            ))
        } else {
            None
        };

        Self {
            view,
            title_top,
            plot,
            data,
            axis_left,
            axis_right,
            axis_top,
            axis_bottom,
            legend,
        }
    }

    /// Convenience helper to compute a left-axis thickness using the provided measurer.
    pub fn measure_axis_left(
        measurer: &impl TextMeasurer,
        tick_labels: &[&str],
        tick_size: f64,
        tick_padding: f64,
        label_padding: f64,
        font_size: f64,
    ) -> f64 {
        let mut max_w = 0.0_f64;
        for s in tick_labels {
            let (w, _h) = measurer.measure(s, font_size);
            max_w = max_w.max(w);
        }
        tick_size.abs() + tick_padding.max(0.0) + label_padding.max(0.0) + max_w
    }

    /// Convenience helper to compute a bottom-axis thickness using the provided measurer.
    pub fn measure_axis_bottom(
        measurer: &impl TextMeasurer,
        tick_size: f64,
        tick_padding: f64,
        label_padding: f64,
        font_size: f64,
    ) -> f64 {
        let (_w, h) = measurer.measure("Mg", font_size);
        tick_size.abs() + tick_padding.max(0.0) + label_padding.max(0.0) + h
    }
}

fn legend_rect(
    plot: Rect,
    axis_left_w: f64,
    axis_right_w: f64,
    axis_top_h: f64,
    axis_bottom_h: f64,
    size: Size,
    placement: LegendPlacement,
) -> Rect {
    let w = size.width.max(0.0);
    let h = size.height.max(0.0);
    let offset = placement.offset.max(0.0);

    match placement.orient {
        LegendOrient::Right => Rect::new(
            plot.x1 + axis_right_w + offset,
            plot.y0,
            plot.x1 + axis_right_w + offset + w,
            plot.y0 + h,
        ),
        LegendOrient::Left => {
            let x1 = plot.x0 - axis_left_w - offset;
            Rect::new(x1 - w, plot.y0, x1, plot.y0 + h)
        }
        LegendOrient::Top => {
            let y1 = plot.y0 - axis_top_h - offset;
            Rect::new(plot.x0, y1 - h, plot.x0 + w, y1)
        }
        LegendOrient::Bottom => {
            let y0 = plot.y1 + axis_bottom_h + offset;
            Rect::new(plot.x0, y0, plot.x0 + w, y0 + h)
        }
        LegendOrient::TopLeft => Rect::new(
            plot.x0 + offset,
            plot.y0 + offset,
            plot.x0 + offset + w,
            plot.y0 + offset + h,
        ),
        LegendOrient::TopRight => Rect::new(
            plot.x1 - offset - w,
            plot.y0 + offset,
            plot.x1 - offset,
            plot.y0 + offset + h,
        ),
        LegendOrient::BottomLeft => Rect::new(
            plot.x0 + offset,
            plot.y1 - offset - h,
            plot.x0 + offset + w,
            plot.y1 - offset,
        ),
        LegendOrient::BottomRight => Rect::new(
            plot.x1 - offset - w,
            plot.y1 - offset - h,
            plot.x1 - offset,
            plot.y1 - offset,
        ),
        LegendOrient::None => Rect::new(placement.x, placement.y, placement.x + w, placement.y + h),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_reserves_space_above_plot() {
        let spec = ChartLayoutSpec {
            title_top: Some(20.0),
            plot_size: Size {
                width: 100.0,
                height: 50.0,
            },
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            axis_left: Some(30.0),
            axis_right: None,
            axis_top: Some(12.0),
            axis_bottom: Some(18.0),
            legend: None,
        };

        let layout = ChartLayout::arrange(&spec);
        let title = layout.title_top.expect("missing title rect");
        assert!((title.y0 - 10.0).abs() < 1e-9);
        assert!((title.y1 - 30.0).abs() < 1e-9);

        // plot.y0 = padding + title + axis_top
        assert!((layout.plot.y0 - (10.0 + 20.0 + 12.0)).abs() < 1e-9);

        // view includes all margins.
        assert!((layout.view.y1 - (10.0 + 20.0 + 12.0 + 50.0 + 10.0 + 18.0)).abs() < 1e-9);
    }
}
