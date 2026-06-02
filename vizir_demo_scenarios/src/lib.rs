// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared chart scenarios for `VizIR` renderer demos.
//!
//! This crate owns small reusable demo scenarios. It does not own renderer backends or application
//! event loops.

#![no_std]

extern crate alloc;

use alloc::string::ToString;
use alloc::vec::Vec;

use kurbo::{BezPath, Point, Rect};
use peniko::Color;
use peniko::color::palette::css;
use vizir_charts::{
    AxisSpec, AxisStyle, ChartLayoutSpec, ChartSpec, GridStyle, HeuristicTextMeasurer,
    ScaleBandSpec, ScaleLinearSpec, SectorMarkSpec, Size, StrokeStyle, TextMarkSpec, TitleSpec,
};
use vizir_core::{Mark, MarkDiff, MarkId, Scene, TextAnchor, TextBaseline};

mod gallery;

/// A built frame of renderer-agnostic marks.
#[derive(Debug)]
pub struct ScenarioFrame {
    /// View rectangle containing the marks.
    pub view: Rect,
    /// Evaluated mark diffs for the scenario frame.
    pub diffs: Vec<MarkDiff>,
}

/// A static scenario that can rebuild its marks on demand.
#[derive(Clone, Copy, Debug)]
pub struct StaticScenario {
    /// Human-readable scenario name.
    pub title: &'static str,
    /// Short scenario description.
    pub description: &'static str,
    build: fn() -> ScenarioFrame,
}

impl StaticScenario {
    /// Create a static scenario from metadata and a builder.
    #[must_use]
    pub const fn new(
        title: &'static str,
        description: &'static str,
        build: fn() -> ScenarioFrame,
    ) -> Self {
        Self {
            title,
            description,
            build,
        }
    }

    /// Build this scenario's current frame.
    #[must_use]
    pub fn build(self) -> ScenarioFrame {
        (self.build)()
    }
}

/// Return the shared static scenarios used by renderer demos.
#[must_use]
pub fn static_scenarios() -> &'static [StaticScenario] {
    static SCENARIOS: &[StaticScenario] = &[
        StaticScenario::new(
            "Axis label angle",
            "Bars with a bottom band axis using rotated labels.",
            axis_label_angle_bars,
        ),
        StaticScenario::new("Bars", "Band-scale bars with axes and gridlines.", bars),
        StaticScenario::new(
            "Line + points",
            "A stroked line path with point symbols.",
            line_points,
        ),
        StaticScenario::new(
            "Sectors",
            "Filled sector marks with centered labels.",
            sectors,
        ),
        StaticScenario::new(
            "Bar",
            "One rect per row; includes gridlines, axes, and a baseline at 0.",
            gallery::bar_demo,
        ),
        StaticScenario::new(
            "Scales",
            "A quick visualization of new scale types. Time is numeric seconds with nice ticks/formatting.",
            gallery::scales_demo,
        ),
        StaticScenario::new(
            "Axes: time + log",
            "A time x-axis and a log y-axis, with a line/point series sharing those scale instances.",
            gallery::log_time_axes_demo,
        ),
        StaticScenario::new(
            "Axis labelAngle",
            "Bottom axis uses labelAngle=-45 deg with deliberately long labels.",
            gallery::axis_label_angle_demo,
        ),
        StaticScenario::new(
            "Transforms",
            "A dataflow slice: source table -> filter(y>=6) -> sort(x).",
            gallery::transforms_demo,
        ),
        StaticScenario::new(
            "Aggregate",
            "A source -> aggregate(groupby) -> bar marks pattern.",
            gallery::aggregate_demo,
        ),
        StaticScenario::new(
            "Lowered UnitSpec",
            "An aggregate bar chart lowered through transforms, guides, and series marks.",
            gallery::lowered_spec_demo,
        ),
        StaticScenario::new(
            "Lowered JSON UnitSpec",
            "An aggregate bar chart parsed from JSON and lowered through the authored seam.",
            gallery::lowered_json_spec_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Calculate",
            "A point chart proving narrow arithmetic calculate lowering.",
            gallery::lowered_json_calculate_demo,
        ),
        StaticScenario::new(
            "Lowered JSON JoinAggregate",
            "A point chart proving grouped aggregates can be written back per row.",
            gallery::lowered_json_joinaggregate_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Fold",
            "A grouped bar chart proving numeric-only fold lowering from wide rows.",
            gallery::lowered_json_fold_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Lookup",
            "A bar chart proving narrow one-key lookup enrichment.",
            gallery::lowered_json_lookup_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Pivot Table",
            "A long-to-wide pivot transform rendered as band plus line overlays.",
            gallery::lowered_json_pivot_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Window",
            "A per-series ranked point chart proving narrow window rank lowering.",
            gallery::lowered_json_window_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Grouped Bars",
            "A grouped bar chart proving categorical color lowering for bars.",
            gallery::lowered_json_grouped_bar_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Stacked Bars",
            "A stacked bar chart proving explicit stack-transform lowering.",
            gallery::lowered_json_stacked_bar_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Facet",
            "A one-field faceted bar chart over the authored unit seam.",
            gallery::lowered_json_facet_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Bar + Text",
            "A shared-plot bar + text overlay from JSON.",
            gallery::lowered_json_bar_text_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Text Labels",
            "A layered line + point chart from JSON using string-backed label data.",
            gallery::lowered_json_text_labels_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Point Size + Shape",
            "A point chart proving point-local size and shape channels.",
            gallery::lowered_json_point_shape_size_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Point Opacity",
            "A point chart proving quantitative opacity lowering.",
            gallery::lowered_json_point_opacity_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Point Stroke + Width",
            "A point chart proving point-local stroke and strokeWidth channels.",
            gallery::lowered_json_point_stroke_width_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Nested Child Units",
            "A styled area + line + point overlay from nested child units.",
            gallery::lowered_json_nested_layer_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Rule Layer",
            "A line with a child-local aggregated mean rule.",
            gallery::lowered_json_rule_layer_demo,
        ),
        StaticScenario::new(
            "Lowered JSON Layer",
            "A shared-plot area + line overlay from JSON.",
            gallery::lowered_json_layer_demo,
        ),
        StaticScenario::new(
            "Lowered Area UnitSpec",
            "A multi-series area chart lowered from authored x/y/color channels.",
            gallery::lowered_area_spec_demo,
        ),
        StaticScenario::new(
            "Lowered Ranged Area",
            "A ribbon-like area lowered from paired-edge range channels.",
            gallery::lowered_ranged_area_spec_demo,
        ),
        StaticScenario::new(
            "Histogram",
            "A source -> bin(step=2) -> aggregate(count) -> sort -> bars pipeline.",
            gallery::histogram_demo,
        ),
        StaticScenario::new(
            "Stack",
            "Stack(offset=zero) per-category accumulation drawn as rects.",
            gallery::stack_demo,
        ),
        StaticScenario::new(
            "Stacked Area",
            "Stacked areas built from stack-produced y0/y1 spans.",
            gallery::stacked_area_demo,
        ),
        StaticScenario::new(
            "Percent Stack",
            "Percent-stacked bars using normalized y0/y1 spans.",
            gallery::percent_stack_demo,
        ),
        StaticScenario::new(
            "Streamgraph Offsets",
            "Compare Stack(offset=center) vs Stack(offset=wiggle) for stacked areas.",
            gallery::streamgraph_demo,
        ),
        StaticScenario::new(
            "Scatter",
            "One point per row with axes and symbol rendering.",
            gallery::scatter_demo,
        ),
        StaticScenario::new(
            "Line",
            "A single path mark derived from table rows.",
            gallery::line_demo,
        ),
        StaticScenario::new(
            "Area",
            "Filled area under a curve with a stroke outline.",
            gallery::area_demo,
        ),
        StaticScenario::new(
            "Sector",
            "Sector marks for pie/donut slices plus a legend.",
            gallery::sector_demo,
        ),
    ];
    SCENARIOS
}

fn demo_axis_style() -> AxisStyle {
    AxisStyle {
        label_font_size: 18.0,
        title_font_size: 20.0,
        ..AxisStyle::default()
    }
}

fn demo_title(id: u64, title: &'static str) -> TitleSpec {
    TitleSpec::new(MarkId::from_raw(id), title)
        .with_subtitle("Shared renderer scenario")
        .with_font_size(28.0)
        .with_subtitle_font_size(20.0)
        .with_fill(css::BLACK)
}

fn chart_spec(
    title: TitleSpec,
    axis_left: Option<AxisSpec>,
    axis_bottom: Option<AxisSpec>,
) -> ChartSpec {
    ChartSpec {
        title: Some(title),
        plot_size: Size {
            width: 1120.0,
            height: 640.0,
        },
        layout: ChartLayoutSpec::default(),
        axis_left,
        axis_right: None,
        axis_top: None,
        axis_bottom,
        legend: None,
    }
}

fn axis_label_angle_bars() -> ScenarioFrame {
    let x_scale = ScaleBandSpec::new(5).with_padding(0.2, 0.1);
    let y_scale = ScaleLinearSpec::new((0.0, 10.0)).with_nice(true);

    let axis_bottom = AxisSpec::bottom(0x10_000, x_scale)
        .with_tick_count(5)
        .with_label_angle(-45.0)
        .with_style(demo_axis_style())
        .with_tick_formatter({
            let labels = [
                "Really long label A",
                "Really long label B",
                "Really long label C",
                "Really long label D",
                "Really long label E",
            ];
            move |v: f64, _step: f64| {
                let i = clamped_label_index(v, 4.0);
                labels.get(i).copied().unwrap_or("?").to_string()
            }
        })
        .with_title("category")
        .with_title_offset(10.0);

    let axis_left = AxisSpec::left(0x11_000, y_scale)
        .with_tick_count(6)
        .with_style(demo_axis_style())
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("value")
        .with_title_offset(10.0);

    let chart = chart_spec(
        demo_title(0x12_000, "Axis labelAngle (-45 deg)"),
        Some(axis_left),
        Some(axis_bottom),
    );

    let measurer = HeuristicTextMeasurer;
    let (layout, mut marks) = chart.marks(&measurer, |chart, plot| {
        let band = chart
            .x_axis()
            .expect("x axis")
            .scale_band(plot)
            .with_padding(0.2, 0.1);
        let y = chart.y_scale_continuous(plot).expect("y scale");

        let mut out = Vec::new();
        for (i, value) in [3.0, 7.0, 4.0, 9.0, 6.0].iter().copied().enumerate() {
            let x0 = band.x(i);
            let w = band.band_width();
            let y0 = y.map(value);
            let h = (y.map(0.0) - y0).max(0.0);

            out.push(
                vizir_charts::RectMarkSpec::new(
                    mark_id(0x20_000, i),
                    Rect::new(x0, y0, x0 + w, y0 + h),
                )
                .with_fill(css::STEEL_BLUE)
                .mark(),
            );
        }

        out.push(plot_background(0x1F_000, plot));
        out
    });

    marks.push(
        TextMarkSpec::new(
            MarkId::from_raw(0x30_000),
            Point::new(layout.data.x0 + 10.0, layout.data.y0 + 14.0),
            "shared VizIR marks",
        )
        .with_font_size(18.0)
        .with_fill(css::BLACK)
        .mark(),
    );

    frame_from_marks(layout.view, marks)
}

fn bars() -> ScenarioFrame {
    let x_scale = ScaleBandSpec::new(6).with_padding(0.2, 0.1);
    let y_scale = ScaleLinearSpec::new((0.0, 10.0)).with_nice(true);

    let axis_bottom = AxisSpec::bottom(0x60_000, x_scale)
        .with_tick_count(6)
        .with_style(demo_axis_style())
        .with_tick_formatter(|v, _step| {
            let i = u8::try_from(clamped_label_index(v, 5.0))
                .expect("label index is clamped to u8 range");
            char::from(b'A' + i).to_string()
        })
        .with_title("category")
        .with_title_offset(10.0);

    let axis_left = AxisSpec::left(0x61_000, y_scale)
        .with_tick_count(6)
        .with_style(demo_axis_style())
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("value")
        .with_title_offset(10.0);

    let chart = chart_spec(
        demo_title(0x62_000, "Bars"),
        Some(axis_left),
        Some(axis_bottom),
    );

    let values = [3.0, 7.0, 4.0, 9.0, 6.0, 2.0];
    let measurer = HeuristicTextMeasurer;
    let (layout, marks) = chart.marks(&measurer, |chart, plot| {
        let band = chart
            .x_axis()
            .expect("x axis")
            .scale_band(plot)
            .with_padding(0.2, 0.1);
        let y = chart.y_scale_continuous(plot).expect("y scale");

        let mut out = Vec::new();
        for (i, value) in values.iter().copied().enumerate() {
            let x0 = band.x(i);
            let w = band.band_width();
            let y0 = y.map(value);
            let h = (y.map(0.0) - y0).max(0.0);
            out.push(
                vizir_charts::RectMarkSpec::new(
                    mark_id(0x6F_000, i),
                    Rect::new(x0, y0, x0 + w, y0 + h),
                )
                .with_fill(css::STEEL_BLUE)
                .mark(),
            );
        }
        out.push(plot_background(0x6F_F00, plot));
        out
    });

    frame_from_marks(layout.view, marks)
}

fn line_points() -> ScenarioFrame {
    let x_scale = ScaleLinearSpec::new((0.0, 10.0)).with_nice(true);
    let y_scale = ScaleLinearSpec::new((0.0, 10.0)).with_nice(true);

    let axis_bottom = AxisSpec::bottom(0x40_000, x_scale)
        .with_tick_count(6)
        .with_style(demo_axis_style())
        .with_title("x")
        .with_title_offset(10.0);
    let axis_left = AxisSpec::left(0x41_000, y_scale)
        .with_tick_count(6)
        .with_style(demo_axis_style())
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("y")
        .with_title_offset(10.0);

    let chart = chart_spec(
        demo_title(0x42_000, "Line + points"),
        Some(axis_left),
        Some(axis_bottom),
    );

    let measurer = HeuristicTextMeasurer;
    let (layout, marks) = chart.marks(&measurer, |chart, plot| {
        let x = chart.x_scale_continuous(plot).expect("x scale");
        let y = chart.y_scale_continuous(plot).expect("y scale");

        let points = [
            (0.0, 1.0),
            (1.0, 3.0),
            (2.0, 2.0),
            (4.0, 6.0),
            (6.0, 5.0),
            (8.0, 9.0),
            (10.0, 8.0),
        ];

        let mut path = BezPath::new();
        for (i, (px, py)) in points.iter().copied().enumerate() {
            let p = Point::new(x.map(px), y.map(py));
            if i == 0 {
                path.move_to(p);
            } else {
                path.line_to(p);
            }
        }

        let mut out = Vec::new();
        out.push(
            Mark::builder(MarkId::from_raw(0x4F_100))
                .path()
                .z_index(vizir_charts::SERIES_STROKE)
                .path_const(path)
                .no_fill()
                .stroke_brush_const(css::STEEL_BLUE)
                .stroke_width_const(2.0)
                .build(),
        );
        for (i, (px, py)) in points.iter().copied().enumerate() {
            let center = Point::new(x.map(px), y.map(py));
            out.extend(
                SectorMarkSpec::new(
                    mark_id(0x4F_200, i),
                    center,
                    0.0,
                    3.5,
                    0.0,
                    core::f64::consts::TAU,
                )
                .with_fill(css::TOMATO)
                .with_stroke(StrokeStyle::solid(
                    css::BLACK.with_alpha(120.0 / 255.0),
                    1.0,
                ))
                .with_z_index(vizir_charts::SERIES_POINTS)
                .marks(),
            );
        }
        out.push(plot_background(0x4F_F00, plot));
        out
    });

    frame_from_marks(layout.view, marks)
}

fn sectors() -> ScenarioFrame {
    let chart = chart_spec(demo_title(0x70_000, "Sectors"), None, None);

    let measurer = HeuristicTextMeasurer;
    let (layout, mut marks) = chart.marks(&measurer, |_chart, plot| {
        let center = Point::new(0.5 * (plot.x0 + plot.x1), 0.5 * (plot.y0 + plot.y1));
        let r = 0.35 * plot.width().min(plot.height());

        let parts = [
            ("A", 0.25, css::STEEL_BLUE),
            ("B", 0.10, css::TOMATO),
            ("C", 0.30, css::MEDIUM_SEA_GREEN),
            ("D", 0.15, css::GOLDENROD),
            ("E", 0.20, css::SLATE_BLUE),
        ];

        let mut out = Vec::new();
        let mut a0 = 0.0;
        for (i, (label, frac, fill)) in parts.iter().copied().enumerate() {
            let a1 = a0 + frac * core::f64::consts::TAU;
            out.extend(
                SectorMarkSpec::new(mark_id(0x71_000, i), center, 0.0, r, a0, a1)
                    .with_fill(fill)
                    .with_stroke(StrokeStyle::solid(css::WHITE, 2.0))
                    .with_z_index(vizir_charts::SERIES_FILL)
                    .marks(),
            );

            let mid = 0.5 * (a0 + a1);
            let p = Point::new(
                center.x + 0.65 * r * cos_f64(mid),
                center.y + 0.65 * r * sin_f64(mid),
            );
            out.push(
                TextMarkSpec::new(mark_id(0x72_000, i), p, label)
                    .with_font_size(22.0)
                    .with_fill(Color::TRANSPARENT)
                    .with_anchor(TextAnchor::Middle)
                    .with_baseline(TextBaseline::Middle)
                    .with_z_index(vizir_charts::SERIES_POINTS)
                    .mark(),
            );

            a0 = a1;
        }
        out.push(plot_background(0x70_F00, plot));
        out
    });

    marks.push(
        TextMarkSpec::new(
            MarkId::from_raw(0x70_100),
            Point::new(layout.data.x0 + 12.0, layout.data.y0 + 22.0),
            "shared across demos",
        )
        .with_font_size(18.0)
        .with_fill(css::BLACK.with_alpha(170.0 / 255.0))
        .with_z_index(vizir_charts::TITLES)
        .mark(),
    );

    frame_from_marks(layout.view, marks)
}

fn plot_background(id: u64, plot: Rect) -> Mark {
    vizir_charts::RectMarkSpec::new(MarkId::from_raw(id), plot)
        .with_fill(Color::TRANSPARENT)
        .with_z_index(vizir_charts::PLOT_BACKGROUND)
        .mark()
}

fn frame_from_marks(view: Rect, marks: Vec<Mark>) -> ScenarioFrame {
    let mut scene = Scene::new();
    frame_from_scene_marks(&mut scene, view, marks)
}

fn frame_from_scene_marks(scene: &mut Scene, view: Rect, marks: Vec<Mark>) -> ScenarioFrame {
    let diffs = scene.tick(marks);
    ScenarioFrame { view, diffs }
}

fn mark_id(base: u64, index: usize) -> MarkId {
    MarkId::from_raw(base + u64::try_from(index).expect("scenario mark index should fit in u64"))
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "rounded value is clamped to the small label index range before casting"
)]
fn clamped_label_index(value: f64, max: f64) -> usize {
    round_f64(value).clamp(0.0, max) as usize
}

fn round_f64(value: f64) -> f64 {
    #[cfg(feature = "std")]
    {
        value.round()
    }
    #[cfg(not(feature = "std"))]
    {
        kurbo::common::FloatFuncs::round(value)
    }
}

fn sin_f64(value: f64) -> f64 {
    #[cfg(feature = "std")]
    {
        value.sin()
    }
    #[cfg(not(feature = "std"))]
    {
        kurbo::common::FloatFuncs::sin(value)
    }
}

fn cos_f64(value: f64) -> f64 {
    #[cfg(feature = "std")]
    {
        value.cos()
    }
    #[cfg(not(feature = "std"))]
    {
        kurbo::common::FloatFuncs::cos(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_scenarios_build_non_empty_frames() {
        for scenario in static_scenarios() {
            let frame = scenario.build();
            assert!(
                !frame.diffs.is_empty(),
                "{} should build marks",
                scenario.title
            );
            assert!(frame.view.width() > 0.0, "{} view width", scenario.title);
            assert!(frame.view.height() > 0.0, "{} view height", scenario.title);
        }
    }

    #[test]
    fn text_label_scenario_uses_string_data_with_plotted_marks() {
        let scenario = static_scenarios()
            .iter()
            .find(|scenario| scenario.title == "Lowered JSON Text Labels")
            .expect("text label scenario");
        let frame = scenario.build();
        let mut path_count = 0;
        let labels: Vec<&str> = frame
            .diffs
            .iter()
            .filter_map(|diff| {
                let MarkDiff::Enter { new, .. } = diff else {
                    return None;
                };
                match &**new {
                    vizir_core::MarkPayload::Text(text) => Some(text.text.as_str()),
                    vizir_core::MarkPayload::Path(_) => {
                        path_count += 1;
                        None
                    }
                    vizir_core::MarkPayload::Rect(_) => None,
                }
            })
            .collect();

        assert!(path_count >= 2);
        assert!(labels.contains(&"alpha"));
        assert!(labels.contains(&"beta"));
        assert!(labels.contains(&"gamma"));
        assert!(labels.contains(&"delta"));
    }
}
