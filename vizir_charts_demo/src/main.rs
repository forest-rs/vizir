// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Chart demos for `vizir_core`.
mod html;
mod svg;

use kurbo::{Point, Rect};
use peniko::Color;
use peniko::color::palette::css;
use vizir_charts::{
    AxisSpec, AxisStyle, BarMarkSpec, ChartLayout, ChartLayoutSpec, ChartSpec, GridStyle,
    LegendItem, LegendOrient, LegendPlacement, LegendSwatchesSpec, PLOT_BACKGROUND, RectMarkSpec,
    RuleMarkSpec, ScaleBand, ScaleLinearSpec, ScaleLogSpec, ScaleTimeSpec, SectorMarkSpec, Size,
    StackedAreaChartSpec, StackedAreaMarkSpec, StackedBarChartSpec, StrokeStyle, Symbol,
    TextMarkSpec, TitleSpec,
};
use vizir_core::{ColId, Mark, Scene, Table, TableData, TableId};
use vizir_transforms::{
    AggregateField, AggregateOp, CompareOp, Predicate, Program, StackOffset, Transform,
};

#[derive(Debug)]
struct BarValues {
    y: Vec<f64>,
}

impl TableData for BarValues {
    fn row_count(&self) -> usize {
        self.y.len()
    }

    fn f64(&self, row: usize, col: ColId) -> Option<f64> {
        match col {
            ColId(0) => self.y.get(row).copied(),
            _ => None,
        }
    }
}

fn main() {
    let sections = vec![
        bar_demo(),
        scales_demo(),
        log_time_axes_demo(),
        axis_label_angle_demo(),
        transforms_demo(),
        aggregate_demo(),
        histogram_demo(),
        stack_demo(),
        stacked_area_demo(),
        percent_stack_demo(),
        streamgraph_demo(),
        scatter_demo(),
        line_demo(),
        area_demo(),
        sector_demo(),
    ];

    let html = html::render_report("VizIR charts demo", &sections);
    std::fs::write("vizir_charts_demo.html", html).expect("write vizir_charts_demo.html");
    println!("wrote vizir_charts_demo.html");
}

fn render_chart(
    scene: &mut Scene,
    measurer: &dyn vizir_charts::TextMeasurer,
    chart: &ChartSpec,
    build_series: impl FnOnce(&ChartSpec, Rect) -> Vec<Mark>,
) -> (ChartLayout, String) {
    let (layout, marks) = chart.marks(measurer, build_series);
    let diffs = scene.tick(marks);
    let mut svg_scene = svg::SvgScene::default();
    svg_scene.set_view_box(layout.view);
    svg_scene.apply_diffs(&diffs);
    (layout, svg_scene.to_svg_string())
}

fn demo_measurer() -> Box<dyn vizir_charts::TextMeasurer> {
    #[cfg(feature = "parley")]
    {
        Box::new(vizir_text_parley::ParleyTextMeasurer::new())
    }

    #[cfg(not(feature = "parley"))]
    {
        Box::new(vizir_charts::HeuristicTextMeasurer)
    }
}

fn log_time_axes_demo() -> html::HtmlSection {
    // A combined demo that exercises:
    // - a bottom time axis with the default formatter
    // - a left log axis (base 10)
    // - a line/point series sharing those scale instances
    let mut scene = Scene::new();
    let table_id = TableId(90);
    let x_col = ColId(0);
    let y_col = ColId(1);

    let measurer = demo_measurer();
    let plot_size = Size {
        width: 260.0,
        height: 140.0,
    };

    let x = vec![0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 300.0];
    let y = vec![1.0, 3.0, 10.0, 30.0, 100.0, 300.0, 1000.0];
    let mut table = Table::new(table_id);
    table.row_keys = (0..x.len() as u64).collect();
    table.data = Some(Box::new(ScatterValues { x, y }));
    scene.insert_table(table);

    let axis_bottom = AxisSpec::bottom(0x90_000, ScaleTimeSpec::new((0.0, 300.0)))
        .with_tick_count(6)
        .with_title("time (s)")
        .with_title_offset(10.0);

    let axis_left = AxisSpec::left(0x91_000, ScaleLogSpec::new((1.0, 1000.0)).with_base(10.0))
        .with_tick_count(4)
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("log10(value)")
        .with_title_offset(10.0);

    let title = TitleSpec::new(
        vizir_core::MarkId::from_raw(0x9F_200),
        "Axis: time (x) + log (y)",
    )
    .with_font_size(12.0)
    .with_fill(css::BLACK)
    .with_subtitle("Bottom axis uses default time tick formatting; left is powers of 10.")
    .with_subtitle_font_size(10.0)
    .with_subtitle_fill(css::DARK_GRAY);

    let chart = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec {
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            ..ChartLayoutSpec::default()
        },
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: None,
    };

    let keys = scene.tables[&table_id].row_keys.clone();

    let (_layout, svg) = render_chart(&mut scene, &*measurer, &chart, |chart, plot| {
        let x_scale = chart.x_scale_continuous(plot).expect("expected x scale");
        let y_scale = chart.y_scale_continuous(plot).expect("expected y scale");

        let mut marks: Vec<Mark> = Vec::new();
        marks.extend(
            vizir_charts::LineMarkSpec::new(
                vizir_core::MarkId::from_raw(0x9F_100),
                table_id,
                x_col,
                y_col,
                x_scale,
                y_scale,
            )
            .with_stroke(StrokeStyle::solid(css::BLACK, 2.0))
            .marks(),
        );
        marks.extend(
            vizir_charts::PointMarkSpec::new(table_id, x_col, y_col, x_scale, y_scale)
                .with_symbol(Symbol::Circle)
                .with_size(8.0)
                .with_fill(css::TOMATO)
                .marks(&keys),
        );
        marks.push(
            RectMarkSpec::new(vizir_core::MarkId::from_raw(0x9F_000), plot)
                .with_fill(Color::TRANSPARENT)
                .with_z_index(PLOT_BACKGROUND)
                .mark(),
        );
        marks
    });

    html::HtmlSection {
        title: "Axes: time + log",
        description: "A time x-axis (default formatter) and a log y-axis, with a line/point series sharing those scale instances.",
        svg,
    }
}

fn transforms_demo() -> html::HtmlSection {
    // A simple transform pipeline demo (Filter + Sort) to illustrate a Vega-ish dataflow shape.
    //
    // This is full-recompute today; incremental patches are planned.
    let mut scene = Scene::new();
    let source_id = TableId(20);
    let filtered_id = TableId(21);
    let sorted_id = TableId(22);

    let x_col = ColId(0);
    let y_col = ColId(1);

    // Input table.
    let x = vec![0.0, 2.0, 5.0, 7.0, 9.0, 10.0];
    let y = vec![1.0, 2.0, 6.0, 3.0, 7.5, 9.0];
    let mut table = Table::new(source_id);
    table.row_keys = (0..x.len() as u64).collect();
    table.data = Some(Box::new(ScatterValues {
        x: x.clone(),
        y: y.clone(),
    }));
    scene.insert_table(table);

    let mut program = Program::new();
    program.push(Transform::Filter {
        input: source_id,
        output: filtered_id,
        predicate: Predicate {
            col: y_col,
            op: CompareOp::Ge,
            value: 6.0,
        },
        columns: vec![x_col, y_col],
    });
    program.push(Transform::Sort {
        input: filtered_id,
        output: sorted_id,
        by: x_col,
        order: vizir_transforms::SortOrder::Asc,
        columns: vec![x_col, y_col],
    });

    let out = program.apply_to_scene(&mut scene).expect("apply_to_scene");
    let _ = out;

    // Render: show original points in gray, transformed points in tomato.
    let measurer = demo_measurer();
    let plot_size = Size {
        width: 220.0,
        height: 120.0,
    };

    let axis_bottom = AxisSpec::bottom(0x20_000, ScaleLinearSpec::new((0.0, 10.0)))
        .with_tick_count(6)
        .with_title("x")
        .with_title_offset(10.0);
    let axis_left = AxisSpec::left(0x21_000, ScaleLinearSpec::new((0.0, 10.0)))
        .with_tick_count(6)
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("y")
        .with_title_offset(10.0);

    let title = TitleSpec::new(
        vizir_core::MarkId::from_raw(0x2F_200),
        "Transforms: filter(y>=6) + sort(x)",
    )
    .with_font_size(12.0)
    .with_fill(css::BLACK)
    .with_subtitle("Gray = source, red = transformed output")
    .with_subtitle_font_size(10.0)
    .with_subtitle_fill(css::DARK_GRAY);

    let chart = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec {
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            ..ChartLayoutSpec::default()
        },
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: None,
    };

    let keys_src = scene.tables[&source_id].row_keys.clone();
    let keys_sorted = scene.tables[&sorted_id].row_keys.clone();

    let (_layout, svg) = render_chart(&mut scene, &*measurer, &chart, |chart, plot| {
        let x_scale = chart.x_scale_continuous(plot).expect("expected x scale");
        let y_scale = chart.y_scale_continuous(plot).expect("expected y scale");

        let mut marks: Vec<Mark> = Vec::new();
        marks.extend(
            vizir_charts::PointMarkSpec::new(source_id, x_col, y_col, x_scale, y_scale)
                .with_symbol(Symbol::Circle)
                .with_fill(css::DARK_GRAY)
                .with_size(6.0)
                .marks(&keys_src),
        );
        marks.extend(
            vizir_charts::PointMarkSpec::new(sorted_id, x_col, y_col, x_scale, y_scale)
                .with_symbol(Symbol::Circle)
                .with_fill(css::TOMATO)
                .with_size(8.0)
                .marks(&keys_sorted),
        );
        marks.push(
            RectMarkSpec::new(vizir_core::MarkId::from_raw(0x2F_000), plot)
                .with_fill(Color::TRANSPARENT)
                .with_z_index(PLOT_BACKGROUND)
                .mark(),
        );
        marks
    });

    html::HtmlSection {
        title: "Transforms",
        description: "A Vega-ish dataflow slice: source table -> filter(y>=6) -> sort(x). Gray = source; red = transformed output.",
        svg,
    }
}

#[derive(Debug)]
struct CategoryValues {
    cat: Vec<f64>,
    v: Vec<f64>,
}

impl TableData for CategoryValues {
    fn row_count(&self) -> usize {
        self.cat.len().min(self.v.len())
    }

    fn f64(&self, row: usize, col: ColId) -> Option<f64> {
        match col {
            ColId(0) => self.cat.get(row).copied(),
            ColId(1) => self.v.get(row).copied(),
            _ => None,
        }
    }
}

fn aggregate_demo() -> html::HtmlSection {
    // A Vega-Lite-ish "aggregate then bar" example.
    //
    // Source rows have a categorical key (numeric for now) and a value. We aggregate by category
    // and render bars of the summed value.
    let mut scene = Scene::new();
    let source_id = TableId(30);
    let agg_id = TableId(31);
    let cat_col = ColId(0);
    let val_col = ColId(1);
    let sum_col = ColId(2);

    let cat = vec![0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 3.0];
    let v = vec![1.0, 2.0, 3.0, 4.0, 5.0, 2.0, 1.0, 6.0];
    let mut table = Table::new(source_id);
    table.row_keys = (0..cat.len() as u64).collect();
    table.data = Some(Box::new(CategoryValues { cat, v }));
    scene.insert_table(table);

    let mut program = Program::new();
    program.push(Transform::Aggregate {
        input: source_id,
        output: agg_id,
        group_by: vec![cat_col],
        fields: vec![AggregateField {
            op: AggregateOp::Sum,
            input: val_col,
            output: sum_col,
        }],
    });
    program.apply_to_scene(&mut scene).expect("apply_to_scene");

    let measurer = demo_measurer();
    let plot_size = Size {
        width: 220.0,
        height: 120.0,
    };

    let keys = scene.tables[&agg_id].row_keys.clone();
    let n = keys.len();

    let mut max_sum = 0.0_f64;
    let mut labels: Vec<String> = Vec::with_capacity(n);
    if let Some(data) = scene.tables[&agg_id].data.as_deref() {
        for row in 0..n {
            let cat = data.f64(row, cat_col).unwrap_or(f64::NAN);
            let sum = data.f64(row, sum_col).unwrap_or(0.0);
            max_sum = max_sum.max(sum);
            labels.push(format!("cat={cat:.0}"));
        }
    }
    if max_sum == 0.0 {
        max_sum = 1.0;
    }

    let axis_bottom = AxisSpec::bottom(
        0x30_000,
        ScaleLinearSpec::new((0.0, (n.saturating_sub(1)) as f64)),
    )
    .with_tick_count(n.max(1))
    .with_tick_padding(4.0)
    .with_label_angle(-45.0)
    .with_tick_formatter({
        let labels = labels.clone();
        move |v, _step| {
            let v = v
                .round()
                .clamp(0.0, (labels.len().saturating_sub(1)) as f64);
            #[allow(
                clippy::cast_possible_truncation,
                reason = "clamped to label index range"
            )]
            let i = v as usize;
            labels.get(i).cloned().unwrap_or_else(|| String::from("?"))
        }
    })
    .with_title("category")
    .with_title_offset(10.0);

    let axis_left = AxisSpec::left(0x31_000, ScaleLinearSpec::new((0.0, max_sum)))
        .with_tick_count(6)
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("sum(value)")
        .with_title_offset(10.0);

    let title = TitleSpec::new(vizir_core::MarkId::from_raw(0x3F_200), "Aggregate -> Bar")
        .with_font_size(12.0)
        .with_fill(css::BLACK);
    let chart = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec {
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            ..ChartLayoutSpec::default()
        },
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: None,
    };

    let (_layout, svg) = render_chart(&mut scene, &*measurer, &chart, move |chart, plot| {
        let band = ScaleBand::new((plot.x0, plot.x1), n).with_padding(0.2, 0.1);
        let y_scale = chart.y_scale_continuous(plot).expect("expected y scale");
        let bars = BarMarkSpec::new(agg_id, sum_col, band, y_scale).with_fill(css::CORNFLOWER_BLUE);

        let mut marks: Vec<Mark> = bars.marks(&keys);
        marks.push(
            RectMarkSpec::new(vizir_core::MarkId::from_raw(0x3F_000), plot)
                .with_fill(Color::TRANSPARENT)
                .with_z_index(PLOT_BACKGROUND)
                .mark(),
        );
        marks
    });

    html::HtmlSection {
        title: "Aggregate",
        description: "A Vega-Lite-ish pattern: source -> aggregate(groupby) -> bar marks.",
        svg,
    }
}

#[derive(Debug)]
struct HistogramValues {
    v: Vec<f64>,
}

impl TableData for HistogramValues {
    fn row_count(&self) -> usize {
        self.v.len()
    }

    fn f64(&self, row: usize, col: ColId) -> Option<f64> {
        match col {
            ColId(0) => self.v.get(row).copied(),
            _ => None,
        }
    }
}

fn histogram_demo() -> html::HtmlSection {
    // A Vega-ish histogram pipeline: source -> bin -> aggregate(count) -> sort -> bars.
    let mut scene = Scene::new();
    let source_id = TableId(40);
    let binned_id = TableId(41);
    let agg_id = TableId(42);
    let sorted_id = TableId(43);

    let v_col = ColId(0);
    let bin0_col = ColId(1);
    let count_col = ColId(2);

    let step = 2.0_f64;
    let values = vec![
        0.2, 0.4, 0.9, 1.4, 1.7, 2.2, 2.9, 3.1, 3.6, 4.2, 4.8, 5.1, 5.7, 6.3, 7.0, 7.2, 8.0, 8.4,
        9.7,
    ];

    let mut table = Table::new(source_id);
    table.row_keys = (0..values.len() as u64).collect();
    table.data = Some(Box::new(HistogramValues { v: values }));
    scene.insert_table(table);

    let mut program = Program::new();
    program.push(Transform::Bin {
        input: source_id,
        output: binned_id,
        input_col: v_col,
        output_start: bin0_col,
        step,
        columns: vec![v_col],
    });
    program.push(Transform::Aggregate {
        input: binned_id,
        output: agg_id,
        group_by: vec![bin0_col],
        fields: vec![AggregateField {
            op: AggregateOp::Count,
            input: v_col,
            output: count_col,
        }],
    });
    program.push(Transform::Sort {
        input: agg_id,
        output: sorted_id,
        by: bin0_col,
        order: vizir_transforms::SortOrder::Asc,
        columns: vec![bin0_col, count_col],
    });
    program.apply_to_scene(&mut scene).expect("apply_to_scene");

    let measurer = demo_measurer();
    let plot_size = Size {
        width: 240.0,
        height: 120.0,
    };

    let keys = scene.tables[&sorted_id].row_keys.clone();
    let n = keys.len();

    let mut max_count = 0.0_f64;
    let mut labels: Vec<String> = Vec::with_capacity(n);
    if let Some(data) = scene.tables[&sorted_id].data.as_deref() {
        for row in 0..n {
            let bin0 = data.f64(row, bin0_col).unwrap_or(f64::NAN);
            let count = data.f64(row, count_col).unwrap_or(0.0);
            max_count = max_count.max(count);
            labels.push(format!("{bin0:.0}–{:.0}", bin0 + step));
        }
    }
    if max_count == 0.0 {
        max_count = 1.0;
    }

    let axis_bottom = AxisSpec::bottom(
        0x40_000,
        ScaleLinearSpec::new((0.0, (n.saturating_sub(1)) as f64)),
    )
    .with_tick_count(n.max(1))
    .with_tick_padding(4.0)
    .with_label_angle(-45.0)
    .with_tick_formatter({
        let labels = labels.clone();
        move |v, _step| {
            let v = v
                .round()
                .clamp(0.0, (labels.len().saturating_sub(1)) as f64);
            #[allow(
                clippy::cast_possible_truncation,
                reason = "clamped to label index range"
            )]
            let i = v as usize;
            labels.get(i).cloned().unwrap_or_else(|| String::from("?"))
        }
    })
    .with_title("v (binned)")
    .with_title_offset(10.0);

    let axis_left = AxisSpec::left(0x41_000, ScaleLinearSpec::new((0.0, max_count)))
        .with_tick_count(6)
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("count")
        .with_title_offset(10.0);

    let title = TitleSpec::new(
        vizir_core::MarkId::from_raw(0x4F_200),
        "Bin + Aggregate -> Histogram",
    )
    .with_font_size(12.0)
    .with_fill(css::BLACK);
    let chart = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec {
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            ..ChartLayoutSpec::default()
        },
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: None,
    };

    let (_layout, svg) = render_chart(&mut scene, &*measurer, &chart, move |chart, plot| {
        let band = ScaleBand::new((plot.x0, plot.x1), n).with_padding(0.2, 0.1);
        let y_scale = chart.y_scale_continuous(plot).expect("expected y scale");
        let bars = BarMarkSpec::new(sorted_id, count_col, band, y_scale).with_fill(css::ORANGE);

        let mut marks: Vec<Mark> = bars.marks(&keys);
        marks.push(
            RectMarkSpec::new(vizir_core::MarkId::from_raw(0x4F_000), plot)
                .with_fill(Color::TRANSPARENT)
                .with_z_index(PLOT_BACKGROUND)
                .mark(),
        );
        marks
    });

    html::HtmlSection {
        title: "Histogram",
        description: "A Vega-ish pipeline: source -> bin(step=2) -> aggregate(count) -> sort -> bars.",
        svg,
    }
}

#[derive(Debug)]
struct StackValues {
    cat: Vec<f64>,
    series: Vec<f64>,
    v: Vec<f64>,
}

impl TableData for StackValues {
    fn row_count(&self) -> usize {
        self.cat.len().min(self.series.len()).min(self.v.len())
    }

    fn f64(&self, row: usize, col: ColId) -> Option<f64> {
        match col {
            ColId(0) => self.cat.get(row).copied(),
            ColId(1) => self.series.get(row).copied(),
            ColId(2) => self.v.get(row).copied(),
            _ => None,
        }
    }
}

fn stack_demo() -> html::HtmlSection {
    // A Vega-ish pipeline: source -> stack(offset=zero) -> rect marks (stacked bars).
    //
    // Note: `Stack` currently processes rows in input order within each group. For Vega's `sort`
    // semantics, sort upstream (e.g. by series / value).
    let mut scene = Scene::new();
    let source_id = TableId(50);
    let stacked_id = TableId(51);

    let cat_col = ColId(0);
    let series_col = ColId(1);
    let val_col = ColId(2);
    let y0_col = ColId(3);
    let y1_col = ColId(4);

    // Four categories (0..3), three series (0..2).
    // Includes a negative value to exercise downward stacking.
    let cat = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0];
    let series = vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0];
    let v = vec![3.0, 2.0, 1.0, 4.0, 1.5, 2.5, 2.0, -1.0, 3.0, 1.0, 2.0, 2.0];

    let mut table = Table::new(source_id);
    table.row_keys = (0..cat.len() as u64).collect();
    table.data = Some(Box::new(StackValues { cat, series, v }));
    scene.insert_table(table);

    let chart = StackedBarChartSpec::new(
        source_id, stacked_id, cat_col, series_col, val_col, y0_col, y1_col,
    );
    chart
        .program()
        .apply_to_scene(&mut scene)
        .expect("apply_to_scene");

    let measurer = demo_measurer();
    let plot_size = Size {
        width: 260.0,
        height: 120.0,
    };

    let keys = scene.tables[&stacked_id].row_keys.clone();
    let n_rows = keys.len();

    let mut min_y = 0.0_f64;
    let mut max_y = 0.0_f64;
    if let Some(data) = scene.tables[&stacked_id].data.as_deref() {
        for row in 0..n_rows {
            let y0 = data.f64(row, y0_col).unwrap_or(f64::NAN);
            let y1 = data.f64(row, y1_col).unwrap_or(f64::NAN);
            if y0.is_finite() && y1.is_finite() {
                min_y = min_y.min(y0.min(y1));
                max_y = max_y.max(y0.max(y1));
            }
        }
    }
    if min_y == max_y {
        max_y = min_y + 1.0;
    }

    let category_count = 4_usize;
    let labels: Vec<&'static str> = vec!["A", "B", "C", "D"];

    let axis_bottom = AxisSpec::bottom(
        0x50_000,
        ScaleLinearSpec::new((0.0, (category_count - 1) as f64)),
    )
    .with_tick_count(category_count)
    .with_tick_padding(4.0)
    .with_tick_formatter({
        let labels = labels.clone();
        move |v, _step| {
            let v = v
                .round()
                .clamp(0.0, (labels.len().saturating_sub(1)) as f64);
            #[allow(
                clippy::cast_possible_truncation,
                reason = "clamped to label index range"
            )]
            let i = v as usize;
            labels.get(i).copied().unwrap_or("?").to_string()
        }
    })
    .with_title("category")
    .with_title_offset(10.0);

    let axis_left = AxisSpec::left(0x51_000, ScaleLinearSpec::new((min_y, max_y)))
        .with_tick_count(6)
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("stacked value")
        .with_title_offset(10.0);

    let title = TitleSpec::new(
        vizir_core::MarkId::from_raw(0x50_200),
        "Stack (offset=zero) -> Stacked Bars",
    )
    .with_font_size(12.0)
    .with_fill(css::BLACK);

    let fills = StackedBarChartSpec::default_series_fills(3);
    let legend_items = StackedBarChartSpec::legend_items(&["s0", "s1", "s2"], &fills);
    let legend_spec = LegendSwatchesSpec::new(0x52_000, legend_items).with_columns(1);
    let chart_spec = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec {
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            ..ChartLayoutSpec::default()
        },
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: Some((
            legend_spec,
            LegendPlacement {
                orient: LegendOrient::Right,
                offset: 18.0,
                x: 0.0,
                y: 0.0,
            },
        )),
    };

    let (_layout, svg) = render_chart(
        &mut scene,
        &*measurer,
        &chart_spec,
        move |chart_spec, plot| {
            let band = ScaleBand::new((plot.x0, plot.x1), category_count).with_padding(0.2, 0.1);
            let y_scale = chart_spec
                .y_scale_continuous(plot)
                .expect("expected y scale");

            let mut marks: Vec<Mark> = chart.marks(&keys, band, y_scale, fills.clone());

            // Baseline at 0.
            marks.push(
                RuleMarkSpec::horizontal(
                    vizir_core::MarkId::from_raw(0x50_001),
                    y_scale.map(0.0),
                    plot.x0,
                    plot.x1,
                )
                .with_stroke(css::BLACK.with_alpha(120.0 / 255.0), 1.0)
                .mark(),
            );

            marks.push(
                RectMarkSpec::new(vizir_core::MarkId::from_raw(0x50_000), plot)
                    .with_fill(Color::TRANSPARENT)
                    .with_z_index(PLOT_BACKGROUND)
                    .mark(),
            );
            marks
        },
    );

    html::HtmlSection {
        title: "Stack",
        description: "Vega-ish stack(offset=zero): per-category accumulation produces y0/y1, then we draw one rect per row.",
        svg,
    }
}

#[derive(Debug)]
struct StackedAreaSourceValues {
    x: Vec<f64>,
    series: Vec<f64>,
    y: Vec<f64>,
}

impl TableData for StackedAreaSourceValues {
    fn row_count(&self) -> usize {
        self.x.len().min(self.series.len()).min(self.y.len())
    }

    fn f64(&self, row: usize, col: ColId) -> Option<f64> {
        match col {
            ColId(0) => self.x.get(row).copied(),
            ColId(1) => self.series.get(row).copied(),
            ColId(2) => self.y.get(row).copied(),
            _ => None,
        }
    }
}

fn stacked_area_demo() -> html::HtmlSection {
    // Vega-ish stacked area pipeline:
    // source(x, series, y) -> stack(groupby=x, sort=series) -> split per-series -> area marks.
    let mut scene = Scene::new();
    let source_id = TableId(60);
    let stacked_id = TableId(61);
    let s0_id = TableId(62);
    let s1_id = TableId(63);
    let s2_id = TableId(64);

    let x_col = ColId(0);
    let series_col = ColId(1);
    let y_col = ColId(2);
    let y0_col = ColId(3);
    let y1_col = ColId(4);

    // 6 x positions, 3 series each (18 rows total).
    // Data is arranged in x-major order so our downstream per-series sorts are deterministic.
    let x_vals: Vec<f64> = (0..=5).map(|v| v as f64).collect();
    let series_vals = [0.0, 1.0, 2.0];
    let y_by_series = [
        [1.0, 2.0, 1.5, 2.5, 2.0, 3.0], // s0
        [0.5, 1.0, 1.2, 1.0, 1.3, 1.1], // s1
        [0.8, 0.6, 0.7, 1.0, 0.9, 0.8], // s2
    ];

    let mut x: Vec<f64> = Vec::new();
    let mut series: Vec<f64> = Vec::new();
    let mut y: Vec<f64> = Vec::new();
    for (xi, &xv) in x_vals.iter().enumerate() {
        for (si, &sv) in series_vals.iter().enumerate() {
            x.push(xv);
            series.push(sv);
            y.push(y_by_series[si][xi]);
        }
    }

    let mut table = Table::new(source_id);
    table.row_keys = (0..x.len() as u64).collect();
    table.data = Some(Box::new(StackedAreaSourceValues { x, series, y }));
    scene.insert_table(table);

    let chart = StackedAreaChartSpec::new(
        source_id, stacked_id, x_col, series_col, y_col, y0_col, y1_col,
    );

    chart
        .program()
        .apply_to_scene(&mut scene)
        .expect("apply_to_scene");
    for (out_id, series_value) in [(s0_id, 0.0), (s1_id, 1.0), (s2_id, 2.0)] {
        chart
            .series_program(out_id, series_value)
            .apply_to_scene(&mut scene)
            .expect("apply_to_scene");
    }

    let measurer = demo_measurer();
    let plot_size = Size {
        width: 260.0,
        height: 120.0,
    };

    let mut max_y1 = 0.0_f64;
    if let Some(data) = scene.tables[&stacked_id].data.as_deref() {
        let n = scene.tables[&stacked_id].row_keys.len();
        for row in 0..n {
            let y1 = data.f64(row, y1_col).unwrap_or(f64::NAN);
            if y1.is_finite() {
                max_y1 = max_y1.max(y1);
            }
        }
    }
    if max_y1 == 0.0 {
        max_y1 = 1.0;
    }

    let axis_bottom = AxisSpec::bottom(0x60_000, ScaleLinearSpec::new((0.0, 5.0)))
        .with_tick_count(6)
        .with_title("x")
        .with_title_offset(10.0);
    let axis_left = AxisSpec::left(0x61_000, ScaleLinearSpec::new((0.0, max_y1)))
        .with_tick_count(6)
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("stacked y")
        .with_title_offset(10.0);

    let title = TitleSpec::new(
        vizir_core::MarkId::from_raw(0x60_2000),
        "Stack -> Stacked Areas",
    )
    .with_font_size(12.0)
    .with_fill(css::BLACK);

    let fills = StackedAreaChartSpec::default_series_fills(3);
    let legend_items = StackedAreaChartSpec::legend_items(&["s0", "s1", "s2"], &fills);
    let legend_spec = LegendSwatchesSpec::new(0x62_000, legend_items).with_columns(1);
    let chart_spec = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec {
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            ..ChartLayoutSpec::default()
        },
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: Some((
            legend_spec,
            LegendPlacement {
                orient: LegendOrient::Right,
                offset: 18.0,
                x: 0.0,
                y: 0.0,
            },
        )),
    };

    let (_layout, svg) = render_chart(
        &mut scene,
        &*measurer,
        &chart_spec,
        move |chart_spec, plot| {
            let x_scale = chart_spec
                .x_scale_continuous(plot)
                .expect("expected x scale");
            let y_scale = chart_spec
                .y_scale_continuous(plot)
                .expect("expected y scale");

            let mut marks: Vec<Mark> = Vec::new();

            // Back-to-front fill order.
            marks.extend(
                StackedAreaMarkSpec::new(0x60_100, s0_id, x_col, y0_col, y1_col, x_scale, y_scale)
                    .with_fill(fills[0].clone())
                    .with_z_index(vizir_charts::SERIES_FILL)
                    .marks(),
            );
            marks.extend(
                StackedAreaMarkSpec::new(0x60_200, s1_id, x_col, y0_col, y1_col, x_scale, y_scale)
                    .with_fill(fills[1].clone())
                    .with_z_index(vizir_charts::SERIES_FILL + 1)
                    .marks(),
            );
            marks.extend(
                StackedAreaMarkSpec::new(0x60_300, s2_id, x_col, y0_col, y1_col, x_scale, y_scale)
                    .with_fill(fills[2].clone())
                    .with_z_index(vizir_charts::SERIES_FILL + 2)
                    .marks(),
            );

            // Baseline at 0.
            marks.push(
                RuleMarkSpec::horizontal(
                    vizir_core::MarkId::from_raw(0x60_001),
                    y_scale.map(0.0),
                    plot.x0,
                    plot.x1,
                )
                .with_stroke(css::BLACK.with_alpha(120.0 / 255.0), 1.0)
                .mark(),
            );

            marks.push(
                RectMarkSpec::new(vizir_core::MarkId::from_raw(0x60_000), plot)
                    .with_fill(Color::TRANSPARENT)
                    .with_z_index(PLOT_BACKGROUND)
                    .mark(),
            );
            marks
        },
    );

    html::HtmlSection {
        title: "Stacked Area",
        description: "Stacked areas built from Stack-produced y0/y1, rendered as one filled path per series.",
        svg,
    }
}

#[derive(Debug)]
struct PercentStackValues {
    cat: Vec<f64>,
    series: Vec<f64>,
    v: Vec<f64>,
}

impl TableData for PercentStackValues {
    fn row_count(&self) -> usize {
        self.cat.len().min(self.series.len()).min(self.v.len())
    }

    fn f64(&self, row: usize, col: ColId) -> Option<f64> {
        match col {
            ColId(0) => self.cat.get(row).copied(),
            ColId(1) => self.series.get(row).copied(),
            ColId(2) => self.v.get(row).copied(),
            _ => None,
        }
    }
}

fn percent_stack_demo() -> html::HtmlSection {
    // A percent-stacked bar chart using Stack(offset="normalize").
    let mut scene = Scene::new();
    let source_id = TableId(70);
    let stacked_id = TableId(71);

    let cat_col = ColId(0);
    let series_col = ColId(1);
    let val_col = ColId(2);
    let y0_col = ColId(3);
    let y1_col = ColId(4);

    let cat = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0];
    let series = vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0];
    let v = vec![3.0, 2.0, 1.0, 4.0, 1.0, 2.0, 2.0, 1.0, 3.0, 1.0, 2.0, 2.0];

    let mut table = Table::new(source_id);
    table.row_keys = (0..cat.len() as u64).collect();
    table.data = Some(Box::new(PercentStackValues { cat, series, v }));
    scene.insert_table(table);

    let chart = StackedBarChartSpec::new(
        source_id, stacked_id, cat_col, series_col, val_col, y0_col, y1_col,
    )
    .with_stack_offset(StackOffset::Normalize);

    chart
        .program()
        .apply_to_scene(&mut scene)
        .expect("apply_to_scene");

    let measurer = demo_measurer();
    let plot_size = Size {
        width: 260.0,
        height: 120.0,
    };

    let keys = scene.tables[&stacked_id].row_keys.clone();

    let axis_bottom = AxisSpec::bottom(0x70_000, ScaleLinearSpec::new((0.0, 3.0)))
        .with_tick_count(4)
        .with_title("category")
        .with_title_offset(10.0);
    let axis_left = AxisSpec::left(0x71_000, ScaleLinearSpec::new((0.0, 1.0)))
        .with_tick_count(6)
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_tick_formatter(|v, _step| format!("{:.0}%", v * 100.0))
        .with_title("percent")
        .with_title_offset(10.0);

    let title = TitleSpec::new(
        vizir_core::MarkId::from_raw(0x70_200),
        "Stack (normalize) -> Percent Stacked Bars",
    )
    .with_font_size(12.0)
    .with_fill(css::BLACK);

    let fills = StackedBarChartSpec::default_series_fills(3);
    let legend_items = StackedBarChartSpec::legend_items(&["s0", "s1", "s2"], &fills);
    let legend_spec = LegendSwatchesSpec::new(0x72_000, legend_items).with_columns(1);
    let chart_spec = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec {
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            ..ChartLayoutSpec::default()
        },
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: Some((
            legend_spec,
            LegendPlacement {
                orient: LegendOrient::Right,
                offset: 18.0,
                x: 0.0,
                y: 0.0,
            },
        )),
    };

    let (_layout, svg) = render_chart(
        &mut scene,
        &*measurer,
        &chart_spec,
        move |chart_spec, plot| {
            let band = ScaleBand::new((plot.x0, plot.x1), 4).with_padding(0.2, 0.1);
            let y_scale = chart_spec
                .y_scale_continuous(plot)
                .expect("expected y scale");

            let mut marks: Vec<Mark> = chart.marks(&keys, band, y_scale, fills.clone());
            marks.push(
                RectMarkSpec::new(vizir_core::MarkId::from_raw(0x70_000), plot)
                    .with_fill(Color::TRANSPARENT)
                    .with_z_index(PLOT_BACKGROUND)
                    .mark(),
            );
            marks
        },
    );

    html::HtmlSection {
        title: "Percent Stack",
        description: "Percent-stacked bars using Stack(offset=\"normalize\"), producing y0/y1 in [0,1].",
        svg,
    }
}

#[derive(Debug)]
struct StreamValues {
    x: Vec<f64>,
    series: Vec<f64>,
    y: Vec<f64>,
}

impl TableData for StreamValues {
    fn row_count(&self) -> usize {
        self.x.len().min(self.series.len()).min(self.y.len())
    }

    fn f64(&self, row: usize, col: ColId) -> Option<f64> {
        match col {
            ColId(0) => self.x.get(row).copied(),
            ColId(1) => self.series.get(row).copied(),
            ColId(2) => self.y.get(row).copied(),
            _ => None,
        }
    }
}

fn streamgraph_demo() -> html::HtmlSection {
    fn build_streamgraph_svg(offset: StackOffset, base: u64) -> String {
        // Streamgraph-ish stacked area and one area path per series.
        let mut scene = Scene::new();
        let source_id = TableId(80);
        let stacked_id = TableId(81);
        let s0_id = TableId(82);
        let s1_id = TableId(83);
        let s2_id = TableId(84);

        let x_col = ColId(0);
        let series_col = ColId(1);
        let y_col = ColId(2);
        let y0_col = ColId(3);
        let y1_col = ColId(4);

        let x_vals: Vec<f64> = (0..=8).map(|v| v as f64).collect();
        let series_vals = [0.0, 1.0, 2.0];
        let y_by_series = [
            [1.0, 1.2, 1.6, 2.0, 2.3, 2.0, 1.6, 1.2, 1.0], // s0
            [0.6, 0.8, 1.0, 1.3, 1.1, 1.0, 0.9, 0.7, 0.6], // s1
            [0.7, 0.6, 0.7, 0.9, 1.2, 1.1, 0.9, 0.8, 0.7], // s2
        ];

        let mut x: Vec<f64> = Vec::new();
        let mut series: Vec<f64> = Vec::new();
        let mut y: Vec<f64> = Vec::new();
        for (xi, &xv) in x_vals.iter().enumerate() {
            for (si, &sv) in series_vals.iter().enumerate() {
                x.push(xv);
                series.push(sv);
                y.push(y_by_series[si][xi]);
            }
        }

        let mut table = Table::new(source_id);
        table.row_keys = (0..x.len() as u64).collect();
        table.data = Some(Box::new(StreamValues { x, series, y }));
        scene.insert_table(table);

        let chart = StackedAreaChartSpec::new(
            source_id, stacked_id, x_col, series_col, y_col, y0_col, y1_col,
        )
        .with_stack_offset(offset);

        chart
            .program()
            .apply_to_scene(&mut scene)
            .expect("apply_to_scene");
        for (out_id, series_value) in [(s0_id, 0.0), (s1_id, 1.0), (s2_id, 2.0)] {
            chart
                .series_program(out_id, series_value)
                .apply_to_scene(&mut scene)
                .expect("apply_to_scene");
        }

        let measurer = demo_measurer();
        let plot_size = Size {
            width: 260.0,
            height: 120.0,
        };

        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        if let Some(data) = scene.tables[&stacked_id].data.as_deref() {
            let n = scene.tables[&stacked_id].row_keys.len();
            for row in 0..n {
                let y0 = data.f64(row, y0_col).unwrap_or(f64::NAN);
                let y1 = data.f64(row, y1_col).unwrap_or(f64::NAN);
                if y0.is_finite() {
                    min_y = min_y.min(y0);
                }
                if y1.is_finite() {
                    max_y = max_y.max(y1);
                }
            }
        }
        if !min_y.is_finite() || !max_y.is_finite() || min_y == max_y {
            min_y = 0.0;
            max_y = 1.0;
        }

        let axis_bottom = AxisSpec::bottom(base + 0x01_000, ScaleLinearSpec::new((0.0, 8.0)))
            .with_tick_count(9)
            .with_title("x")
            .with_title_offset(10.0);
        let axis_title = match offset {
            StackOffset::Center => "stack offset: center",
            StackOffset::Wiggle => "stack offset: wiggle",
            StackOffset::Normalize => "stack offset: normalize",
            StackOffset::Zero => "stack offset: zero",
        };
        let axis_left = AxisSpec::left(base + 0x02_000, ScaleLinearSpec::new((min_y, max_y)))
            .with_tick_count(6)
            .with_grid(GridStyle {
                stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
            })
            .with_title(axis_title)
            .with_title_offset(10.0);

        let plot_title = match offset {
            StackOffset::Center => "Stack(offset=\"center\")",
            StackOffset::Wiggle => "Stack(offset=\"wiggle\")",
            StackOffset::Normalize => "Stack(offset=\"normalize\")",
            StackOffset::Zero => "Stack(offset=\"zero\")",
        };
        let title = TitleSpec::new(vizir_core::MarkId::from_raw(base + 0x0F_000), plot_title)
            .with_font_size(12.0)
            .with_fill(css::BLACK);

        let fills = StackedAreaChartSpec::default_series_fills(3);
        let legend_items = StackedAreaChartSpec::legend_items(&["s0", "s1", "s2"], &fills);
        let legend_spec = LegendSwatchesSpec::new(base + 0x03_000, legend_items).with_columns(1);
        let chart_spec = ChartSpec {
            title: Some(title),
            plot_size,
            layout: ChartLayoutSpec {
                view_size: None,
                outer_padding: 10.0,
                plot_padding: 0.0,
                ..ChartLayoutSpec::default()
            },
            axis_left: Some(axis_left),
            axis_right: None,
            axis_top: None,
            axis_bottom: Some(axis_bottom),
            legend: Some((
                legend_spec,
                LegendPlacement {
                    orient: LegendOrient::Right,
                    offset: 18.0,
                    x: 0.0,
                    y: 0.0,
                },
            )),
        };

        let (_layout, svg) = render_chart(
            &mut scene,
            &*measurer,
            &chart_spec,
            move |chart_spec, plot| {
                let x_scale = chart_spec
                    .x_scale_continuous(plot)
                    .expect("expected x scale");
                let y_scale = chart_spec
                    .y_scale_continuous(plot)
                    .expect("expected y scale");

                let mut marks: Vec<Mark> = Vec::new();
                marks.extend(
                    StackedAreaMarkSpec::new(
                        base + 0x10_000,
                        s0_id,
                        x_col,
                        y0_col,
                        y1_col,
                        x_scale,
                        y_scale,
                    )
                    .with_fill(fills[0].clone())
                    .with_z_index(vizir_charts::SERIES_FILL)
                    .marks(),
                );
                marks.extend(
                    StackedAreaMarkSpec::new(
                        base + 0x11_000,
                        s1_id,
                        x_col,
                        y0_col,
                        y1_col,
                        x_scale,
                        y_scale,
                    )
                    .with_fill(fills[1].clone())
                    .with_z_index(vizir_charts::SERIES_FILL + 1)
                    .marks(),
                );
                marks.extend(
                    StackedAreaMarkSpec::new(
                        base + 0x12_000,
                        s2_id,
                        x_col,
                        y0_col,
                        y1_col,
                        x_scale,
                        y_scale,
                    )
                    .with_fill(fills[2].clone())
                    .with_z_index(vizir_charts::SERIES_FILL + 2)
                    .marks(),
                );

                marks.push(
                    RectMarkSpec::new(vizir_core::MarkId::from_raw(base), plot)
                        .with_fill(Color::TRANSPARENT)
                        .with_z_index(PLOT_BACKGROUND)
                        .mark(),
                );
                marks
            },
        );
        svg
    }

    let center_svg = build_streamgraph_svg(StackOffset::Center, 0x90_000);
    let wiggle_svg = build_streamgraph_svg(StackOffset::Wiggle, 0xA0_000);

    html::HtmlSection {
        title: "Streamgraph Offsets",
        description: "Compare Stack(offset=\"center\") vs Stack(offset=\"wiggle\") for stacked areas.",
        svg: format!(
            "<div style=\"display:flex; flex-wrap:wrap; gap:16px; align-items:flex-start;\">{center_svg}{wiggle_svg}</div>"
        ),
    }
}

#[derive(Debug)]
struct AngleValues {
    x: Vec<f64>,
    y: Vec<f64>,
}

impl TableData for AngleValues {
    fn row_count(&self) -> usize {
        self.x.len().min(self.y.len())
    }

    fn f64(&self, row: usize, col: ColId) -> Option<f64> {
        match col {
            ColId(0) => self.x.get(row).copied(),
            ColId(1) => self.y.get(row).copied(),
            _ => None,
        }
    }
}

fn axis_label_angle_demo() -> html::HtmlSection {
    // Demonstrates rotated axis labels and long label formatting.
    let mut scene = Scene::new();
    let table_id = TableId(6);
    let x_col = ColId(0);
    let y_col = ColId(1);

    let measurer = demo_measurer();
    let plot_size = Size {
        width: 240.0,
        height: 120.0,
    };

    let x: Vec<f64> = (0..=5).map(|v| v as f64).collect();
    let y: Vec<f64> = [2.0, 5.0, 3.0, 7.0, 4.0, 6.0].into();
    let mut table = Table::new(table_id);
    table.row_keys = (0..x.len() as u64).collect();
    table.data = Some(Box::new(AngleValues { x, y }));
    scene.insert_table(table);

    let rule = StrokeStyle::solid(css::BLACK, 1.0);
    let axis_style = AxisStyle {
        rule: rule.clone(),
        label_fill: rule.brush.clone(),
        label_font_size: 10.0,
        title_fill: rule.brush.clone(),
        title_font_size: 11.0,
    };

    let axis_bottom = AxisSpec::bottom(0x61_000, ScaleLinearSpec::new((0.0, 5.0)))
        .with_tick_count(6)
        .with_style(axis_style.clone())
        .with_tick_padding(4.0)
        .with_label_padding(2.0)
        .with_label_angle(-45.0)
        .with_tick_formatter(|v, _step| {
            let v = v.round().clamp(0.0, 5.0);
            #[allow(clippy::cast_possible_truncation, reason = "clamped to 0..=5")]
            let i = v as i32;
            format!("Category {i} — very long label",)
        });

    let axis_left = AxisSpec::left(0x62_000, ScaleLinearSpec::new((0.0, 8.0)))
        .with_tick_count(5)
        .with_style(axis_style.clone())
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("value")
        .with_title_offset(10.0);

    let title = TitleSpec::new(
        vizir_core::MarkId::from_raw(0x6F_200),
        "Axis labelAngle (-45°)",
    )
    .with_font_size(12.0)
    .with_fill(css::BLACK);
    let keys = scene.tables[&table_id].row_keys.clone();
    let chart_spec = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec {
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            ..ChartLayoutSpec::default()
        },
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: None,
    };

    let (_layout, svg) = render_chart(
        &mut scene,
        &*measurer,
        &chart_spec,
        move |chart_spec, plot| {
            let x_scale = chart_spec
                .x_scale_continuous(plot)
                .expect("expected x scale");
            let y_scale = chart_spec
                .y_scale_continuous(plot)
                .expect("expected y scale");

            let points = vizir_charts::PointMarkSpec::new(table_id, x_col, y_col, x_scale, y_scale)
                .with_symbol(Symbol::Circle)
                .with_fill(css::TOMATO);

            let mut marks: Vec<Mark> = points.marks(&keys);
            marks.push(
                RectMarkSpec::new(vizir_core::MarkId::from_raw(0x6F_000), plot)
                    .with_fill(Color::TRANSPARENT)
                    .with_z_index(PLOT_BACKGROUND)
                    .mark(),
            );
            marks
        },
    );

    html::HtmlSection {
        title: "Axis labelAngle",
        description: "Bottom axis uses labelAngle=-45° with deliberately long labels to test measure/arrange and clipping.",
        svg,
    }
}

fn scales_demo() -> html::HtmlSection {
    // A tiny "scale gallery" that exercises ScalePoint, ScaleLog, and ScaleTime.
    //
    // This is intentionally not a full axis implementation; it just visualizes mapping and ticks.
    let mut scene = Scene::new();

    let origin = Point::new(20.0, 20.0);
    let w = 520.0;
    let h = 140.0;
    let view = Rect::new(0.0, 0.0, origin.x + w + 20.0, origin.y + h + 20.0);

    let mut marks: Vec<Mark> = Vec::new();
    marks.push(
        RectMarkSpec::new(vizir_core::MarkId::from_raw(0x06_000), view)
            .with_fill(Color::TRANSPARENT)
            .with_z_index(PLOT_BACKGROUND)
            .mark(),
    );

    // Section titles.
    marks.push(
        TextMarkSpec::new(
            vizir_core::MarkId::from_raw(0x06_010),
            Point::new(origin.x, origin.y - 6.0),
            "Scales: point / log / time",
        )
        .with_font_size(12.0)
        .with_fill(css::BLACK)
        .with_anchor(vizir_core::TextAnchor::Start)
        .mark(),
    );

    // Point scale row.
    let y0 = origin.y + 20.0;
    let point = vizir_charts::ScalePoint::new((origin.x, origin.x + w), 9).with_padding(0.5);
    marks.push(
        TextMarkSpec::new(
            vizir_core::MarkId::from_raw(0x06_100),
            Point::new(origin.x, y0 - 10.0),
            "ScalePoint (9 categories)",
        )
        .with_font_size(10.0)
        .with_fill(css::BLACK)
        .with_anchor(vizir_core::TextAnchor::Start)
        .mark(),
    );
    for i in 0..9 {
        let x = point.x(i);
        marks.push(
            RuleMarkSpec::vertical(
                vizir_core::MarkId::from_raw(0x06_200 + i as u64),
                x,
                y0,
                y0 + 24.0,
            )
            .with_stroke(css::BLACK.with_alpha(50.0 / 255.0), 1.0)
            .mark(),
        );
        marks.push(
            TextMarkSpec::new(
                vizir_core::MarkId::from_raw(0x06_300 + i as u64),
                Point::new(x, y0 + 34.0),
                format!("{i}"),
            )
            .with_font_size(9.0)
            .with_fill(css::BLACK)
            .with_anchor(vizir_core::TextAnchor::Middle)
            .mark(),
        );
    }

    // Log scale row.
    let y1 = y0 + 56.0;
    let log = vizir_charts::ScaleLog::new((1.0, 1000.0), (origin.x, origin.x + w));
    marks.push(
        TextMarkSpec::new(
            vizir_core::MarkId::from_raw(0x06_400),
            Point::new(origin.x, y1 - 10.0),
            "ScaleLog (domain 1..1000)",
        )
        .with_font_size(10.0)
        .with_fill(css::BLACK)
        .with_anchor(vizir_core::TextAnchor::Start)
        .mark(),
    );
    let log_ticks = log.ticks(10);
    for (i, t) in log_ticks.iter().copied().enumerate() {
        let x = log.map(t);
        marks.push(
            RuleMarkSpec::vertical(
                vizir_core::MarkId::from_raw(0x06_500 + i as u64),
                x,
                y1,
                y1 + 24.0,
            )
            .with_stroke(css::BLACK.with_alpha(50.0 / 255.0), 1.0)
            .mark(),
        );
        marks.push(
            TextMarkSpec::new(
                vizir_core::MarkId::from_raw(0x06_600 + i as u64),
                Point::new(x, y1 + 34.0),
                format!("{t:.0}"),
            )
            .with_font_size(9.0)
            .with_fill(css::BLACK)
            .with_anchor(vizir_core::TextAnchor::Middle)
            .mark(),
        );
    }

    // Time scale row (seconds) with "nice" ticks + formatting.
    let y2 = y1 + 56.0;
    let time = vizir_charts::ScaleTime::new((0.0, 60.0), (origin.x, origin.x + w));
    marks.push(
        TextMarkSpec::new(
            vizir_core::MarkId::from_raw(0x06_700),
            Point::new(origin.x, y2 - 10.0),
            "ScaleTime (0..60s, nice ticks + formatting)",
        )
        .with_font_size(10.0)
        .with_fill(css::BLACK)
        .with_anchor(vizir_core::TextAnchor::Start)
        .mark(),
    );
    let time_ticks = time.ticks(6);
    let step = time_ticks
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(f64::INFINITY, f64::min);
    for (i, t) in time_ticks.into_iter().enumerate() {
        let x = time.map(t);
        marks.push(
            RuleMarkSpec::vertical(
                vizir_core::MarkId::from_raw(0x06_800 + i as u64),
                x,
                y2,
                y2 + 24.0,
            )
            .with_stroke(css::BLACK.with_alpha(50.0 / 255.0), 1.0)
            .mark(),
        );
        marks.push(
            TextMarkSpec::new(
                vizir_core::MarkId::from_raw(0x06_900 + i as u64),
                Point::new(x, y2 + 34.0),
                vizir_charts::format_time_seconds(t, if step.is_finite() { step } else { 0.0 }),
            )
            .with_font_size(9.0)
            .with_fill(css::BLACK)
            .with_anchor(vizir_core::TextAnchor::Middle)
            .mark(),
        );
    }

    // Evaluate.
    let diffs = scene.tick(marks);
    let mut svg_scene = svg::SvgScene::default();
    svg_scene.set_view_box(view);
    svg_scene.apply_diffs(&diffs);

    html::HtmlSection {
        title: "Scales",
        description: "A quick visualization of new scale types. (Time is numeric seconds with nice ticks/formatting.)",
        svg: svg_scene.to_svg_string(),
    }
}

fn bar_demo() -> html::HtmlSection {
    // A minimal “bar chart”: one rect mark per row with height driven by a numeric column.
    let mut scene = Scene::new();
    let table_id = TableId(1);
    let y_col = ColId(0);

    let measurer = demo_measurer();
    let plot_size = Size {
        width: 180.0,
        height: 100.0,
    };

    let y = vec![3.0, -4.0, 10.0, 6.0, -1.0];
    let mut table = Table::new(table_id);
    table.row_keys = (0..y.len() as u64).collect();
    table.data = Some(Box::new(BarValues { y }));
    scene.insert_table(table);

    let rule = StrokeStyle::solid(css::BLACK, 1.0);
    let axis_style = AxisStyle {
        rule: rule.clone(),
        label_fill: rule.brush.clone(),
        label_font_size: 10.0,
        title_fill: rule.brush.clone(),
        title_font_size: 11.0,
    };

    let axis_bottom = AxisSpec::bottom(0x10_000, ScaleLinearSpec::new((0.0, 10.0)))
        .with_tick_count(6)
        .with_style(axis_style.clone())
        .with_tick_formatter(|v, step| {
            if step.abs() >= 1.0 {
                format!("i={}", v.round())
            } else {
                format!("i={v:.2}")
            }
        })
        .with_title("index")
        .with_title_offset(10.0);

    let axis_left = AxisSpec::left(0x11_000, ScaleLinearSpec::new((-5.0, 10.0)))
        .with_tick_count(6)
        .with_style(axis_style.clone())
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("value")
        .with_title_offset(10.0);

    let title = TitleSpec::new(vizir_core::MarkId::from_raw(0x1F_200), "Bar")
        .with_font_size(12.0)
        .with_fill(css::BLACK);

    let legend = LegendSwatchesSpec::new(
        0x12_000,
        vec![LegendItem::solid("bars", css::CORNFLOWER_BLUE)],
    )
    .with_text_fill(css::BLACK);
    let keys = scene.tables[&table_id].row_keys.clone();
    let n = keys.len();

    let chart_spec = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec {
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            ..ChartLayoutSpec::default()
        },
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: Some((
            legend,
            LegendPlacement {
                orient: LegendOrient::Right,
                ..LegendPlacement::default()
            },
        )),
    };

    let (_layout, svg) = render_chart(
        &mut scene,
        &*measurer,
        &chart_spec,
        move |chart_spec, plot| {
            let band = ScaleBand::new((plot.x0, plot.x1), n).with_padding(0.2, 0.1);
            let y_scale = chart_spec
                .y_scale_continuous(plot)
                .expect("expected y scale");
            let y0 = y_scale.map(0.0);

            let bars = BarMarkSpec::new(table_id, y_col, band, y_scale)
                .with_baseline(0.0)
                .with_fill(css::CORNFLOWER_BLUE);

            let mut marks: Vec<Mark> = bars.marks(&keys);

            // Zero baseline.
            marks.push(
                RuleMarkSpec::horizontal(
                    vizir_core::MarkId::from_raw(0x1F_100),
                    y0,
                    plot.x0,
                    plot.x1,
                )
                .with_stroke(css::BLACK, 1.0)
                .mark(),
            );

            // Plot frame.
            marks.push(
                RectMarkSpec::new(vizir_core::MarkId::from_raw(0x1F_000), plot)
                    .with_fill(Color::TRANSPARENT)
                    .with_z_index(PLOT_BACKGROUND)
                    .mark(),
            );
            marks
        },
    );
    html::HtmlSection {
        title: "Bar",
        description: "One rect per row; includes gridlines, axes, and a baseline at 0.",
        svg,
    }
}

#[derive(Debug)]
struct ScatterValues {
    x: Vec<f64>,
    y: Vec<f64>,
}

impl TableData for ScatterValues {
    fn row_count(&self) -> usize {
        self.x.len().min(self.y.len())
    }

    fn f64(&self, row: usize, col: ColId) -> Option<f64> {
        match col {
            ColId(0) => self.x.get(row).copied(),
            ColId(1) => self.y.get(row).copied(),
            _ => None,
        }
    }
}

fn scatter_demo() -> html::HtmlSection {
    // A minimal scatter plot: one rect mark per row with x/y driven by two numeric columns.
    let mut scene = Scene::new();
    let table_id = TableId(2);
    let x_col = ColId(0);
    let y_col = ColId(1);

    let measurer = demo_measurer();
    let plot_size = Size {
        width: 180.0,
        height: 100.0,
    };

    let x = vec![0.0, 2.0, 5.0, 7.0, 9.0, 10.0];
    let y = vec![1.0, 2.0, 6.0, 3.0, 7.5, 9.0];

    let mut table = Table::new(table_id);
    table.row_keys = (0..x.len() as u64).collect();
    table.data = Some(Box::new(ScatterValues { x, y }));
    scene.insert_table(table);

    let rule = StrokeStyle::solid(css::BLACK, 1.0);
    let axis_style = AxisStyle {
        rule: rule.clone(),
        label_fill: rule.brush.clone(),
        label_font_size: 10.0,
        title_fill: rule.brush.clone(),
        title_font_size: 11.0,
    };
    let grid = GridStyle {
        stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
    };

    let axis_bottom = AxisSpec::bottom(0x20_000, ScaleLinearSpec::new((0.0, 10.0)))
        .with_tick_count(6)
        .with_style(axis_style.clone())
        .with_ticks(false)
        .with_domain(false)
        .with_grid(grid.clone())
        .with_title("x")
        .with_title_offset(10.0);

    let axis_left = AxisSpec::left(0x21_000, ScaleLinearSpec::new((0.0, 10.0)))
        .with_tick_count(6)
        .with_style(axis_style.clone())
        .with_grid(grid)
        .with_title("y")
        .with_title_offset(10.0);

    let legend = LegendSwatchesSpec::new(0x22_000, vec![LegendItem::solid("points", css::TOMATO)])
        .with_text_fill(css::BLACK);

    let title = TitleSpec::new(vizir_core::MarkId::from_raw(0x2F_200), "Scatter")
        .with_font_size(12.0)
        .with_fill(css::BLACK);

    let keys = scene.tables[&table_id].row_keys.clone();
    let chart_spec = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec {
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            ..ChartLayoutSpec::default()
        },
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: Some((
            legend,
            LegendPlacement {
                orient: LegendOrient::Right,
                ..LegendPlacement::default()
            },
        )),
    };

    let (_layout, svg) = render_chart(
        &mut scene,
        &*measurer,
        &chart_spec,
        move |chart_spec, plot| {
            let x_scale = chart_spec
                .x_scale_continuous(plot)
                .expect("expected x scale");
            let y_scale = chart_spec
                .y_scale_continuous(plot)
                .expect("expected y scale");

            let points = vizir_charts::PointMarkSpec::new(table_id, x_col, y_col, x_scale, y_scale)
                .with_size(6.0)
                .with_symbol(Symbol::Circle)
                .with_fill(css::TOMATO);

            let mut marks: Vec<Mark> = points.marks(&keys);
            marks.push(
                RectMarkSpec::new(vizir_core::MarkId::from_raw(0x2F_000), plot)
                    .with_fill(Color::TRANSPARENT)
                    .with_z_index(PLOT_BACKGROUND)
                    .mark(),
            );
            marks
        },
    );
    html::HtmlSection {
        title: "Scatter",
        description: "One point per row; demonstrates axis/label toggles and symbol rendering.",
        svg,
    }
}

fn line_demo() -> html::HtmlSection {
    // A minimal line chart: a single `Path` mark derived from a table.
    let mut scene = Scene::new();
    let table_id = TableId(3);
    let x_col = ColId(0);
    let y_col = ColId(1);

    let measurer = demo_measurer();
    let plot_size = Size {
        width: 180.0,
        height: 100.0,
    };

    let x = vec![0.0, 2.0, 5.0, 7.0, 9.0, 10.0];
    let y = vec![1.0, 2.0, 6.0, 3.0, 7.5, 9.0];

    let mut table = Table::new(table_id);
    table.row_keys = (0..x.len() as u64).collect();
    table.data = Some(Box::new(ScatterValues { x, y }));
    scene.insert_table(table);

    let rule = StrokeStyle::solid(css::BLACK, 1.0);
    let axis_style = AxisStyle {
        rule: rule.clone(),
        label_fill: rule.brush.clone(),
        label_font_size: 10.0,
        title_fill: rule.brush.clone(),
        title_font_size: 11.0,
    };

    let axis_bottom = AxisSpec::bottom(0x30_000, ScaleLinearSpec::new((0.0, 10.0)))
        .with_tick_count(6)
        .with_style(axis_style.clone())
        .with_title("x")
        .with_title_offset(10.0);

    let axis_left = AxisSpec::left(0x31_000, ScaleLinearSpec::new((0.0, 10.0)))
        .with_tick_count(6)
        .with_style(axis_style.clone())
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("y")
        .with_title_offset(10.0);

    let legend = LegendSwatchesSpec::new(0x32_000, vec![LegendItem::solid("line", css::BLACK)])
        .with_text_fill(css::BLACK);

    let title = TitleSpec::new(vizir_core::MarkId::from_raw(0x3F_200), "Line")
        .with_font_size(12.0)
        .with_fill(css::BLACK);
    let chart_spec = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec {
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            ..ChartLayoutSpec::default()
        },
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: Some((
            legend,
            LegendPlacement {
                orient: LegendOrient::Right,
                ..LegendPlacement::default()
            },
        )),
    };

    let (_layout, svg) = render_chart(
        &mut scene,
        &*measurer,
        &chart_spec,
        move |chart_spec, plot| {
            let x_scale = chart_spec
                .x_scale_continuous(plot)
                .expect("expected x scale");
            let y_scale = chart_spec
                .y_scale_continuous(plot)
                .expect("expected y scale");

            let line = vizir_charts::LineMarkSpec::new(
                vizir_core::MarkId::from_raw(0x100),
                table_id,
                x_col,
                y_col,
                x_scale,
                y_scale,
            )
            .with_stroke(StrokeStyle::solid(css::BLACK, 2.0));

            let mut marks = line.marks();
            marks.push(
                RectMarkSpec::new(vizir_core::MarkId::from_raw(0x3F_000), plot)
                    .with_fill(css::ALICE_BLUE)
                    .with_z_index(PLOT_BACKGROUND)
                    .mark(),
            );
            marks
        },
    );
    html::HtmlSection {
        title: "Line",
        description: "A single path mark derived from table rows; plot background behind content.",
        svg,
    }
}

fn area_demo() -> html::HtmlSection {
    // A minimal area chart: a filled area with an optional stroked outline.
    let mut scene = Scene::new();
    let table_id = TableId(4);
    let x_col = ColId(0);
    let y_col = ColId(1);

    let measurer = demo_measurer();
    let plot_size = Size {
        width: 180.0,
        height: 100.0,
    };

    let x = vec![0.0, 2.0, 5.0, 7.0, 9.0, 10.0];
    let y = vec![1.0, 2.0, 6.0, 3.0, 7.5, 9.0];

    let mut table = Table::new(table_id);
    table.row_keys = (0..x.len() as u64).collect();
    table.data = Some(Box::new(ScatterValues { x, y }));
    scene.insert_table(table);

    let rule = StrokeStyle::solid(css::BLACK, 1.0);
    let axis_style = AxisStyle {
        rule: rule.clone(),
        label_fill: rule.brush.clone(),
        label_font_size: 10.0,
        title_fill: rule.brush.clone(),
        title_font_size: 11.0,
    };

    let axis_bottom = AxisSpec::bottom(0x40_000, ScaleLinearSpec::new((0.0, 10.0)))
        .with_tick_count(6)
        .with_style(axis_style.clone())
        .with_title("x")
        .with_title_offset(10.0);

    let axis_left = AxisSpec::left(0x41_000, ScaleLinearSpec::new((0.0, 10.0)))
        .with_tick_count(6)
        .with_style(axis_style.clone())
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("y")
        .with_title_offset(10.0);

    let legend = LegendSwatchesSpec::new(
        0x42_000,
        vec![LegendItem::solid(
            "area",
            css::CORNFLOWER_BLUE.with_alpha(0.3),
        )],
    )
    .with_text_fill(css::BLACK);

    let title = TitleSpec::new(vizir_core::MarkId::from_raw(0x4F_200), "Area")
        .with_font_size(12.0)
        .with_fill(css::BLACK);
    let chart_spec = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec {
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            ..ChartLayoutSpec::default()
        },
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: Some((
            legend,
            LegendPlacement {
                orient: LegendOrient::Right,
                ..LegendPlacement::default()
            },
        )),
    };

    let (_layout, svg) = render_chart(
        &mut scene,
        &*measurer,
        &chart_spec,
        move |chart_spec, plot| {
            let x_scale = chart_spec
                .x_scale_continuous(plot)
                .expect("expected x scale");
            let y_scale = chart_spec
                .y_scale_continuous(plot)
                .expect("expected y scale");

            let area =
                vizir_charts::AreaMarkSpec::new(0x400, table_id, x_col, y_col, x_scale, y_scale)
                    .with_fill(css::CORNFLOWER_BLUE.with_alpha(0.3))
                    .with_stroke(StrokeStyle::solid(css::CORNFLOWER_BLUE, 2.0));

            let mut marks = area.marks();
            marks.push(
                RectMarkSpec::new(vizir_core::MarkId::from_raw(0x4F_000), plot)
                    .with_fill(Color::TRANSPARENT)
                    .with_z_index(PLOT_BACKGROUND)
                    .mark(),
            );
            marks
        },
    );
    html::HtmlSection {
        title: "Area",
        description: "Filled area under a curve with an optional stroke outline.",
        svg,
    }
}

fn sector_demo() -> html::HtmlSection {
    // A minimal pie/donut chart demo: a few `SectorMarkSpec`s.
    let mut scene = Scene::new();

    let measurer = demo_measurer();
    let plot_size = Size {
        width: 180.0,
        height: 100.0,
    };

    let legend = LegendSwatchesSpec::new(
        0x52_000,
        vec![
            LegendItem::solid("A", css::CORNFLOWER_BLUE),
            LegendItem::solid("B", css::TOMATO),
            LegendItem::solid("C", css::GOLD),
        ],
    )
    .with_columns(2)
    .with_column_gap(14.0)
    .with_text_fill(css::BLACK);

    let title = TitleSpec::new(vizir_core::MarkId::from_raw(0x5F_200), "Sector")
        .with_font_size(12.0)
        .with_fill(css::BLACK);

    let values = [2.0, 1.0, 3.0];
    let total: f64 = values.iter().sum();
    let colors = [css::CORNFLOWER_BLUE, css::TOMATO, css::GOLD];

    let chart_spec = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec {
            view_size: None,
            outer_padding: 10.0,
            plot_padding: 0.0,
            ..ChartLayoutSpec::default()
        },
        axis_left: None,
        axis_right: None,
        axis_top: None,
        axis_bottom: None,
        legend: Some((
            legend,
            LegendPlacement {
                orient: LegendOrient::Right,
                ..LegendPlacement::default()
            },
        )),
    };

    let (_layout, svg) = render_chart(&mut scene, &*measurer, &chart_spec, move |_chart, plot| {
        let cx = (plot.x0 + plot.x1) * 0.5;
        let cy = (plot.y0 + plot.y1) * 0.5;
        let r = plot.width().min(plot.height()) * 0.45;

        let mut marks: Vec<Mark> = Vec::new();
        let mut a0 = 0.0_f64;
        for (i, (v, color)) in values.iter().copied().zip(colors).enumerate() {
            let frac = if total == 0.0 { 0.0 } else { v / total };
            let a1 = a0 + frac * core::f64::consts::TAU;
            marks.extend(
                SectorMarkSpec::new(
                    vizir_core::MarkId::from_raw(0x500 + i as u64),
                    Point::new(cx, cy),
                    r * 0.55,
                    r,
                    a0,
                    a1,
                )
                .with_fill(color)
                .with_stroke(StrokeStyle::solid(css::WHITE, 1.0))
                .marks(),
            );
            a0 = a1;
        }

        marks.push(
            RectMarkSpec::new(vizir_core::MarkId::from_raw(0x5F_000), plot)
                .with_fill(Color::TRANSPARENT)
                .with_z_index(PLOT_BACKGROUND)
                .mark(),
        );

        marks
    });
    html::HtmlSection {
        title: "Sector",
        description: "SectorMarkSpec for pie/donut slices plus a multi-column legend.",
        svg,
    }
}
