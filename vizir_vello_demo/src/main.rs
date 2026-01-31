// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Native Vello renderer demo for `VizIR`.

use std::num::NonZeroUsize;
use std::time::Instant;
use std::{collections::HashMap, sync::Arc};

use kurbo::{BezPath, Circle, Point, Rect, Shape, Vec2};
use parley::style::{FontFamily, FontStack, GenericFamily, StyleProperty};
use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext};
use peniko::Brush;
use peniko::color::palette::css;
use vello::kurbo::{Affine, Stroke, Vec2 as VelloVec2};
use vello::peniko::{Fill, FontData};
use vello::util::{RenderContext, RenderSurface};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene as VelloScene};
use vizir_charts::{
    AxisSpec, AxisStyle, ChartLayoutSpec, ChartSpec, GridStyle, ScaleBandSpec, ScaleLinearSpec,
    Size, StrokeStyle, TextMarkSpec, TitleSpec,
};
use vizir_core::{
    ColId, InputRef, Mark, MarkDiff, MarkId, MarkPayload, Scene, SignalId, TableData, TableId,
    TextAnchor, TextBaseline,
};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

const SIGNAL_T: SignalId = SignalId(1);
const TABLE_STREAM: TableId = TableId(1);
const STREAM_COL_X: ColId = ColId(0);
const STREAM_COL_Y: ColId = ColId(1);

#[derive(Clone, Debug)]
struct MarkSnapshot {
    z_index: i32,
    payload: MarkPayload,
}

#[derive(Default)]
struct MarkStore {
    marks: HashMap<MarkId, MarkSnapshot>,
}

impl MarkStore {
    fn apply_diffs(&mut self, diffs: &[MarkDiff]) {
        for diff in diffs {
            match diff {
                MarkDiff::Enter {
                    id, z_index, new, ..
                } => {
                    self.marks.insert(
                        *id,
                        MarkSnapshot {
                            z_index: *z_index,
                            payload: (**new).clone(),
                        },
                    );
                }
                MarkDiff::Update {
                    id,
                    new_z_index,
                    new,
                    ..
                } => {
                    self.marks.insert(
                        *id,
                        MarkSnapshot {
                            z_index: *new_z_index,
                            payload: (**new).clone(),
                        },
                    );
                }
                MarkDiff::Exit { id, .. } => {
                    self.marks.remove(id);
                }
            }
        }
    }

    fn sorted(&self) -> Vec<MarkSnapshot> {
        let mut out: Vec<_> = self.marks.values().cloned().collect();
        out.sort_by_key(|m| m.z_index);
        out
    }
}

struct TextShaper {
    font_cx: FontContext,
    layout_cx: LayoutContext<()>,
}

impl TextShaper {
    fn new() -> Self {
        Self {
            font_cx: FontContext::new(),
            layout_cx: LayoutContext::new(),
        }
    }

    fn draw_text(
        &mut self,
        scene: &mut VelloScene,
        global: Affine,
        text: &str,
        pos: Point,
        font_size: f64,
        angle_deg: f64,
        anchor: TextAnchor,
        baseline: TextBaseline,
        fill: &Brush,
    ) {
        if text.is_empty() {
            return;
        }

        let text = text.split('\n').next().unwrap_or("");
        if text.is_empty() {
            return;
        }

        fn font_size_f32(font_size: f64) -> f32 {
            if !font_size.is_finite() {
                return 0.0;
            }
            let font_size = font_size.max(0.0);
            if font_size >= f64::from(f32::MAX) {
                f32::MAX
            } else {
                #[allow(
                    clippy::cast_possible_truncation,
                    reason = "Value is clamped to f32::MAX above"
                )]
                {
                    font_size as f32
                }
            }
        }

        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(font_size_f32(font_size)));
        builder.push_default(StyleProperty::FontStack(FontStack::from(
            FontFamily::Generic(GenericFamily::SansSerif),
        )));

        let mut layout: parley::Layout<()> = builder.build(text);
        layout.break_all_lines(None);
        layout.align(None, Alignment::Start, AlignmentOptions::default());

        let Some(line) = layout.lines().next() else {
            return;
        };

        let metrics = line.metrics();
        let width = metrics.advance as f64;
        let ascent = metrics.ascent as f64;
        let descent = metrics.descent as f64;
        let leading = metrics.leading as f64;
        let baseline_offset = metrics.baseline as f64;
        let height = ascent + descent + leading;

        let ref_x = match anchor {
            TextAnchor::Start => 0.0,
            TextAnchor::Middle => 0.5 * width,
            TextAnchor::End => width,
        };

        let top = baseline_offset - ascent;
        let ref_y = match baseline {
            TextBaseline::Alphabetic | TextBaseline::Ideographic => baseline_offset,
            TextBaseline::Hanging => top,
            TextBaseline::Middle => top + 0.5 * height,
        };

        let angle = angle_deg.to_radians();
        let transform = global
            * (Affine::translate(Vec2::new(pos.x, pos.y))
                * Affine::rotate(angle)
                * Affine::translate(Vec2::new(-ref_x, -ref_y)));

        for item in line.items() {
            let parley::PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let font: &FontData = run.run().font();
            let glyphs = run.positioned_glyphs().map(|g| vello::Glyph {
                id: g.id,
                x: g.x,
                y: g.y,
            });

            scene
                .draw_glyphs(font)
                .transform(transform)
                .font_size(run.run().font_size())
                .brush(fill)
                .draw(Fill::NonZero, glyphs);
        }
    }
}

fn brush_is_transparent(brush: &Brush) -> bool {
    match brush {
        Brush::Solid(c) => c.components[3] <= 0.0,
        _ => false,
    }
}

fn paint_scene_with_transform(
    scene: &mut VelloScene,
    text: &mut TextShaper,
    marks: &[MarkSnapshot],
    transform: Affine,
) {
    for mark in marks {
        match &mark.payload {
            MarkPayload::Rect(r) => {
                if brush_is_transparent(&r.fill) {
                    continue;
                }
                scene.fill(Fill::NonZero, transform, &r.fill, None, &r.rect);
            }
            MarkPayload::Path(p) => {
                if !brush_is_transparent(&p.fill) {
                    scene.fill(Fill::NonZero, transform, &p.fill, None, &p.path);
                }
                if p.stroke_width > 0.0 && !brush_is_transparent(&p.stroke) {
                    let stroke = Stroke::new(p.stroke_width);
                    scene.stroke(&stroke, transform, &p.stroke, None, &p.path);
                }
            }
            MarkPayload::Text(t) => {
                if brush_is_transparent(&t.fill) {
                    continue;
                }
                text.draw_text(
                    scene,
                    transform,
                    &t.text,
                    t.pos,
                    t.font_size,
                    t.angle,
                    t.anchor,
                    t.baseline,
                    &t.fill,
                );
            }
        }
    }
}

type ChartFn = fn() -> (Rect, Vec<Mark>);

#[derive(Clone, Copy)]
struct ChartEntry {
    name: &'static str,
    kind: ChartKind,
    init: fn(&mut App),
}

#[derive(Clone, Copy)]
enum ChartKind {
    Static(ChartFn),
    Animated(ChartFn),
    Streaming(fn(&App) -> (Rect, Vec<Mark>)),
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
        .with_subtitle("Rendered via Vello + Parley")
        .with_font_size(28.0)
        .with_subtitle_font_size(20.0)
        .with_fill(css::BLACK)
}

fn axis_label_angle_chart() -> (Rect, Vec<Mark>) {
    let plot_size = Size {
        width: 1120.0,
        height: 640.0,
    };

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
                let i = v.round().clamp(0.0, 4.0);
                #[allow(clippy::cast_possible_truncation, reason = "clamped to index range")]
                let i = i as usize;
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

    let title = demo_title(0x12_000, "Axis labelAngle (-45°)");

    let chart = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec::default(),
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: None,
    };

    let measurer = vizir_text_parley::ParleyTextMeasurer::new();
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
                    MarkId::from_raw(0x20_000 + i as u64),
                    Rect::new(x0, y0, x0 + w, y0 + h),
                )
                .with_fill(css::STEEL_BLUE)
                .mark(),
            );
        }

        // Plot background.
        out.push(
            vizir_charts::RectMarkSpec::new(MarkId::from_raw(0x1F_000), plot)
                .with_fill(peniko::Color::TRANSPARENT)
                .with_z_index(vizir_charts::PLOT_BACKGROUND)
                .mark(),
        );

        out
    });

    // Add a little explanatory text inside the plot for sanity.
    marks.push(
        TextMarkSpec::new(
            MarkId::from_raw(0x30_000),
            Point::new(layout.data.x0 + 10.0, layout.data.y0 + 14.0),
            "Vello + Parley text rendering",
        )
        .with_font_size(18.0)
        .with_fill(css::BLACK)
        .mark(),
    );

    (layout.view, marks)
}

fn line_chart() -> (Rect, Vec<Mark>) {
    let plot_size = Size {
        width: 1120.0,
        height: 640.0,
    };

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

    let title = demo_title(0x42_000, "Line + points");

    let chart = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec::default(),
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: None,
    };

    let measurer = vizir_text_parley::ParleyTextMeasurer::new();
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
                .fill_brush_const(peniko::Color::TRANSPARENT)
                .stroke_brush_const(css::STEEL_BLUE)
                .stroke_width_const(2.0)
                .build(),
        );
        for (i, (px, py)) in points.iter().copied().enumerate() {
            let cx = x.map(px);
            let cy = y.map(py);
            out.extend(
                vizir_charts::SectorMarkSpec::new(
                    MarkId::from_raw(0x4F_200 + i as u64),
                    Point::new(cx, cy),
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
        out
    });

    (layout.view, marks)
}

fn bar_chart() -> (Rect, Vec<Mark>) {
    let plot_size = Size {
        width: 1120.0,
        height: 640.0,
    };

    let x_scale = ScaleBandSpec::new(6).with_padding(0.2, 0.1);
    let y_scale = ScaleLinearSpec::new((0.0, 10.0)).with_nice(true);

    let axis_bottom = AxisSpec::bottom(0x60_000, x_scale)
        .with_tick_count(6)
        .with_style(demo_axis_style())
        .with_tick_formatter(|v, _step| {
            let i = v.round().clamp(0.0, 5.0);
            #[allow(clippy::cast_possible_truncation, reason = "clamped to index range")]
            let i = i as usize;
            #[allow(clippy::cast_possible_truncation, reason = "clamped to 0..=5 above")]
            let i = i as u8;
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

    let title = demo_title(0x62_000, "Bars");

    let chart = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec::default(),
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: None,
    };

    let values = [3.0, 7.0, 4.0, 9.0, 6.0, 2.0];
    let measurer = vizir_text_parley::ParleyTextMeasurer::new();
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
                    MarkId::from_raw(0x6F_000 + i as u64),
                    Rect::new(x0, y0, x0 + w, y0 + h),
                )
                .with_fill(css::STEEL_BLUE)
                .mark(),
            );
        }
        out
    });

    (layout.view, marks)
}

fn sector_chart() -> (Rect, Vec<Mark>) {
    let plot_size = Size {
        width: 1120.0,
        height: 640.0,
    };

    let title = demo_title(0x70_000, "Sectors (pie)");
    let chart = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec::default(),
        axis_left: None,
        axis_right: None,
        axis_top: None,
        axis_bottom: None,
        legend: None,
    };

    let measurer = vizir_text_parley::ParleyTextMeasurer::new();
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
                vizir_charts::SectorMarkSpec::new(
                    MarkId::from_raw(0x71_000 + i as u64),
                    center,
                    0.0,
                    r,
                    a0,
                    a1,
                )
                .with_fill(fill)
                .with_stroke(StrokeStyle::solid(css::WHITE, 2.0))
                .with_z_index(vizir_charts::SERIES_FILL)
                .marks(),
            );

            // Label at slice centroid.
            let mid = 0.5 * (a0 + a1);
            let p = Point::new(
                center.x + 0.65 * r * mid.cos(),
                center.y + 0.65 * r * mid.sin(),
            );
            out.push(
                TextMarkSpec::new(MarkId::from_raw(0x72_000 + i as u64), p, label)
                    .with_font_size(22.0)
                    .with_fill(css::WHITE)
                    .with_anchor(TextAnchor::Middle)
                    .with_baseline(TextBaseline::Middle)
                    .with_z_index(vizir_charts::SERIES_POINTS)
                    .mark(),
            );

            a0 = a1;
        }
        out
    });

    // A small note.
    marks.push(
        TextMarkSpec::new(
            MarkId::from_raw(0x70_100),
            Point::new(layout.data.x0 + 12.0, layout.data.y0 + 22.0),
            "Left/Right arrows to switch charts",
        )
        .with_font_size(18.0)
        .with_fill(css::BLACK.with_alpha(170.0 / 255.0))
        .with_z_index(vizir_charts::TITLES)
        .mark(),
    );

    (layout.view, marks)
}

fn charts() -> &'static [ChartEntry] {
    &[
        ChartEntry {
            name: "Axis labelAngle (bars)",
            kind: ChartKind::Static(axis_label_angle_chart),
            init: |_app| {},
        },
        ChartEntry {
            name: "Bars",
            kind: ChartKind::Static(bar_chart),
            init: |_app| {},
        },
        ChartEntry {
            name: "Sectors",
            kind: ChartKind::Static(sector_chart),
            init: |_app| {},
        },
        ChartEntry {
            name: "Line + points",
            kind: ChartKind::Static(line_chart),
            init: |_app| {},
        },
        ChartEntry {
            name: "Animated (sine)",
            kind: ChartKind::Animated(animated_sine_chart),
            init: |app| {
                app.stream = None;
                app.viz_scene.insert_signal(SIGNAL_T, 0.0_f64);
            },
        },
        ChartEntry {
            name: "Streaming (table)",
            kind: ChartKind::Streaming(streaming_table_chart),
            init: |app| {
                app.init_streaming_table();
            },
        },
    ]
}

struct App {
    window: Option<Arc<Window>>,
    window_id: Option<WindowId>,
    render_cx: RenderContext,
    surface: Option<RenderSurface<'static>>,
    renderer: Option<Renderer>,
    vello_scene: VelloScene,
    viz_scene: Scene,
    store: MarkStore,
    text: TextShaper,
    view: Rect,
    chart_index: usize,
    last_redraw: Instant,
    t: f64,
    stream: Option<StreamingState>,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            window_id: None,
            render_cx: RenderContext::new(),
            surface: None,
            renderer: None,
            vello_scene: VelloScene::new(),
            viz_scene: Scene::new(),
            store: MarkStore::default(),
            text: TextShaper::new(),
            view: Rect::new(0.0, 0.0, 1.0, 1.0),
            chart_index: 0,
            last_redraw: Instant::now(),
            t: 0.0,
            stream: None,
        }
    }

    fn set_chart(&mut self, chart_index: usize) {
        self.chart_index = chart_index % charts().len();
        self.viz_scene = Scene::new();
        self.store = MarkStore::default();
        self.vello_scene.reset();
        self.last_redraw = Instant::now();
        self.t = 0.0;
        self.stream = None;
        (self.current_chart().init)(self);
    }

    fn current_chart(&self) -> ChartEntry {
        charts()
            .get(self.chart_index)
            .copied()
            .unwrap_or_else(|| charts()[0])
    }

    fn update_window_title(&self) {
        let Some(w) = &self.window else {
            return;
        };
        let entry = self.current_chart();
        match entry.kind {
            ChartKind::Static(_) => {
                w.set_title(&format!("vizir_vello_demo — {}", entry.name));
            }
            ChartKind::Animated(_) => {
                w.set_title(&format!(
                    "vizir_vello_demo — {} — t={:.2}s",
                    entry.name, self.t
                ));
            }
            ChartKind::Streaming(_) => {
                let samples = self
                    .viz_scene
                    .tables
                    .get(&TABLE_STREAM)
                    .map_or(0, |t| t.row_count());
                w.set_title(&format!(
                    "vizir_vello_demo — {} — samples={samples}",
                    entry.name
                ));
            }
        }
    }

    fn next_chart(&mut self) {
        self.set_chart(self.chart_index.wrapping_add(1));
    }

    fn prev_chart(&mut self) {
        let len = charts().len();
        let next = if self.chart_index == 0 {
            len.saturating_sub(1)
        } else {
            self.chart_index - 1
        };
        self.set_chart(next);
    }

    fn rebuild_scene(&mut self) {
        self.vello_scene.reset();
        let marks = self.store.sorted();
        let transform = self.fit_transform();
        paint_scene_with_transform(&mut self.vello_scene, &mut self.text, &marks, transform);
    }

    fn ensure_content(&mut self) {
        if !self.store.marks.is_empty() {
            return;
        }
        let entry = self.current_chart();
        if matches!(entry.kind, ChartKind::Animated(_)) {
            let _ = self.viz_scene.set_signal(SIGNAL_T, self.t);
        }
        let (view, marks) = match entry.kind {
            ChartKind::Static(f) => f(),
            ChartKind::Animated(f) => f(),
            ChartKind::Streaming(f) => f(self),
        };
        self.view = view;
        let diffs = self.viz_scene.tick(marks);
        self.store.apply_diffs(&diffs);
        self.rebuild_scene();
    }

    fn update_animation(&mut self) {
        let entry = self.current_chart();
        match entry.kind {
            ChartKind::Animated(_) => {}
            ChartKind::Streaming(_) => {}
            ChartKind::Static(_) => return,
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_redraw).as_secs_f64();
        self.last_redraw = now;
        // Clamp dt to keep things sane during debugger stops.
        // Use a fixed-step fallback so animation is visibly progressing even if timing is odd.
        let dt = dt.clamp(0.0, 0.1);
        let dt = if dt == 0.0 { 1.0 / 60.0 } else { dt };
        self.t += dt;

        match entry.kind {
            ChartKind::Animated(_) => {
                let _ = self.viz_scene.set_signal(SIGNAL_T, self.t);
                let diffs = self.viz_scene.update();
                self.store.apply_diffs(&diffs);
                self.rebuild_scene();
                self.update_window_title();
            }
            ChartKind::Streaming(f) => {
                self.step_streaming_table(dt);
                let (view, marks) = f(self);
                self.view = view;
                let diffs = self.viz_scene.tick(marks);
                self.store.apply_diffs(&diffs);
                self.rebuild_scene();
                self.update_window_title();
            }
            ChartKind::Static(_) => {}
        }
    }

    fn fit_transform(&self) -> Affine {
        let Some(surface) = self.surface.as_ref() else {
            return Affine::IDENTITY;
        };
        let view_w = self.view.width().max(1.0);
        let view_h = self.view.height().max(1.0);

        let w = f64::from(surface.config.width.max(1));
        let h = f64::from(surface.config.height.max(1));
        let scale = (w / view_w).min(h / view_h);
        let scale = scale.max(1.0e-6);

        let tx = -self.view.x0;
        let ty = -self.view.y0;
        let content_w = view_w * scale;
        let content_h = view_h * scale;
        let pad_x = 0.5 * (w - content_w).max(0.0);
        let pad_y = 0.5 * (h - content_h).max(0.0);

        Affine::translate(VelloVec2::new(pad_x, pad_y))
            * Affine::scale(scale)
            * Affine::translate(VelloVec2::new(tx, ty))
    }

    fn init_streaming_table(&mut self) {
        let window = 160;
        let mut state = StreamingState::new(window);
        // Seed some initial data so the first frame is non-empty.
        for _ in 0..window {
            state.push_sample();
        }
        state.apply_to_scene(&mut self.viz_scene);
        self.stream = Some(state);
    }

    fn step_streaming_table(&mut self, dt: f64) {
        let Some(state) = self.stream.as_mut() else {
            self.init_streaming_table();
            return;
        };
        state.step(dt);
        state.apply_to_scene(&mut self.viz_scene);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let initial = charts()[0].name;
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(format!("vizir_vello_demo — {initial}"))
                        .with_inner_size(PhysicalSize::new(1400_u32, 900_u32)),
                )
                .expect("create window"),
        );
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let surface = pollster::block_on(self.render_cx.create_surface(
            window.clone(),
            width,
            height,
            wgpu::PresentMode::AutoVsync,
        ))
        .expect("create surface");

        let device_handle = &self.render_cx.devices[surface.dev_id];
        let renderer = Renderer::new(
            &device_handle.device,
            RendererOptions {
                antialiasing_support: AaSupport::all(),
                num_init_threads: NonZeroUsize::new(1),
                ..RendererOptions::default()
            },
        )
        .expect("create vello renderer");

        self.window_id = Some(window.id());
        self.window = Some(window);
        self.surface = Some(surface);
        self.renderer = Some(renderer);

        self.ensure_content();
        self.update_window_title();
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // For animated charts, drive a continuous redraw loop.
        if matches!(
            self.current_chart().kind,
            ChartKind::Animated(_) | ChartKind::Streaming(_)
        ) && let Some(w) = &self.window
        {
            w.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if Some(id) != self.window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                if width == 0 || height == 0 {
                    return;
                }
                if let Some(surface) = self.surface.as_mut() {
                    self.render_cx.resize_surface(surface, width, height);
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => event_loop.exit(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::ArrowRight),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                self.next_chart();
                self.ensure_content();
                self.update_window_title();
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::ArrowLeft),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                self.prev_chart();
                self.ensure_content();
                self.update_window_title();
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                self.ensure_content();
                self.update_animation();
                let Some(surface) = self.surface.as_mut() else {
                    return;
                };
                let Some(renderer) = self.renderer.as_mut() else {
                    return;
                };
                let device_handle = &self.render_cx.devices[surface.dev_id];

                let surface_texture = match surface.surface.get_current_texture() {
                    Ok(tex) => tex,
                    Err(_) => {
                        self.render_cx.resize_surface(
                            surface,
                            surface.config.width,
                            surface.config.height,
                        );
                        return;
                    }
                };
                let surface_view = surface_texture
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                renderer
                    .render_to_texture(
                        &device_handle.device,
                        &device_handle.queue,
                        &self.vello_scene,
                        &surface.target_view,
                        &RenderParams {
                            base_color: css::WHITE,
                            width: surface.config.width,
                            height: surface.config.height,
                            antialiasing_method: AaConfig::Msaa16,
                        },
                    )
                    .expect("render");

                let mut encoder =
                    device_handle
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("blit"),
                        });
                surface.blitter.copy(
                    &device_handle.device,
                    &mut encoder,
                    &surface.target_view,
                    &surface_view,
                );
                device_handle.queue.submit([encoder.finish()]);
                surface_texture.present();

                // If the active chart is animated, keep the redraw loop going.
                if let Some(w) = &self.window
                    && matches!(
                        charts()[self.chart_index].kind,
                        ChartKind::Animated(_) | ChartKind::Streaming(_)
                    )
                {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn animated_sine_chart() -> (Rect, Vec<Mark>) {
    let plot_size = Size {
        width: 1120.0,
        height: 640.0,
    };

    let x_scale = ScaleLinearSpec::new((0.0, 10.0)).with_nice(true);
    let y_scale = ScaleLinearSpec::new((0.0, 10.0)).with_nice(true);

    let axis_bottom = AxisSpec::bottom(0x80_000, x_scale)
        .with_tick_count(6)
        .with_style(demo_axis_style())
        .with_title("x")
        .with_title_offset(10.0);
    let axis_left = AxisSpec::left(0x81_000, y_scale)
        .with_tick_count(6)
        .with_style(demo_axis_style())
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("y")
        .with_title_offset(10.0);

    let title = demo_title(0x82_000, "Animated (sine)").with_subtitle("t is a Signal<f64>");

    let chart = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec::default(),
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: None,
    };

    let measurer = vizir_text_parley::ParleyTextMeasurer::new();
    let (layout, marks) = chart.marks(&measurer, |chart, plot| {
        let x = chart.x_scale_continuous(plot).expect("x scale");
        let y = chart.y_scale_continuous(plot).expect("y scale");

        let deps = [InputRef::Signal { signal: SIGNAL_T }];

        let out = vec![
            Mark::builder(MarkId::from_raw(0x8F_000))
                .path()
                .z_index(vizir_charts::SERIES_STROKE)
                .path_compute(deps, move |ctx, _id| {
                    let t = ctx.signal::<f64>(SIGNAL_T).unwrap_or(0.0);
                    let samples = 120;
                    let mut path = BezPath::new();
                    for i in 0..=samples {
                        let fx = 10.0 * (i as f64 / samples as f64);
                        let fy = 5.0 + 4.0 * (fx * 0.9 + t * 2.0).sin();
                        let p = Point::new(x.map(fx), y.map(fy));
                        if i == 0 {
                            path.move_to(p);
                        } else {
                            path.line_to(p);
                        }
                    }
                    path
                })
                .fill_brush_const(peniko::Color::TRANSPARENT)
                .stroke_brush_const(css::MEDIUM_SEA_GREEN)
                .stroke_width_const(2.5)
                .build(),
            Mark::builder(MarkId::from_raw(0x8F_100))
                .path()
                .z_index(vizir_charts::SERIES_POINTS)
                .path_compute(deps, move |ctx, _id| {
                    let t = ctx.signal::<f64>(SIGNAL_T).unwrap_or(0.0);
                    let fx = (t * 1.2).rem_euclid(10.0);
                    let fy = 5.0 + 4.0 * (fx * 0.9 + t * 2.0).sin();
                    let p = Point::new(x.map(fx), y.map(fy));
                    Circle::new(p, 5.0).to_path(0.1)
                })
                .fill_brush_const(css::TOMATO)
                .stroke_brush_const(css::BLACK.with_alpha(140.0 / 255.0))
                .stroke_width_const(1.0)
                .build(),
        ];

        out
    });

    (layout.view, marks)
}

#[derive(Debug)]
struct StreamTableData {
    ys: Arc<[f64]>,
}

impl TableData for StreamTableData {
    fn row_count(&self) -> usize {
        self.ys.len()
    }

    fn f64(&self, row: usize, col: ColId) -> Option<f64> {
        match col.0 {
            0 => Some(row as f64),
            1 => self.ys.get(row).copied(),
            _ => None,
        }
    }
}

struct StreamingState {
    window: usize,
    row_keys: Vec<u64>,
    ys: Vec<f64>,
    next_key: u64,
    t: f64,
    accum: f64,
}

impl StreamingState {
    fn new(window: usize) -> Self {
        Self {
            window,
            row_keys: Vec::new(),
            ys: Vec::new(),
            next_key: 0,
            t: 0.0,
            accum: 0.0,
        }
    }

    fn step(&mut self, dt: f64) {
        self.t += dt;
        self.accum += dt;

        // Add samples at a fixed-ish rate so we get a steady stream even if redraw timing wobbles.
        let sample_dt = 1.0 / 20.0;
        while self.accum >= sample_dt {
            self.accum -= sample_dt;
            self.push_sample();
        }
    }

    fn push_sample(&mut self) {
        let key = self.next_key;
        self.next_key = self.next_key.wrapping_add(1);

        let y = (0.7 * self.t).sin() + 0.25 * (2.1 * self.t).cos();
        self.row_keys.push(key);
        self.ys.push(y);
        if self.row_keys.len() > self.window {
            self.row_keys.remove(0);
            self.ys.remove(0);
        }
    }

    fn apply_to_scene(&self, scene: &mut Scene) {
        scene.set_table_row_keys(TABLE_STREAM, self.row_keys.clone());
        let ys: Arc<[f64]> = Arc::from(self.ys.clone().into_boxed_slice());
        scene.set_table_data(
            TABLE_STREAM,
            Some(Box::new(StreamTableData { ys }) as Box<dyn TableData>),
        );
    }
}

fn streaming_table_chart(app: &App) -> (Rect, Vec<Mark>) {
    let plot_size = Size {
        width: 1120.0,
        height: 640.0,
    };

    let window = app.stream.as_ref().map_or(160, |s| s.window.clamp(2, 2000));
    let x_scale = ScaleLinearSpec::new((0.0, (window - 1) as f64)).with_nice(false);
    let y_scale = ScaleLinearSpec::new((-1.5, 1.5)).with_nice(false);

    let axis_bottom = AxisSpec::bottom(0x90_000, x_scale)
        .with_tick_count(5)
        .with_style(demo_axis_style())
        .with_title("samples (window)")
        .with_title_offset(10.0);
    let axis_left = AxisSpec::left(0x91_000, y_scale)
        .with_tick_count(7)
        .with_style(demo_axis_style())
        .with_grid(GridStyle {
            stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
        })
        .with_title("value")
        .with_title_offset(10.0);

    let title = demo_title(0x92_000, "Streaming (table)")
        .with_subtitle("Table v1 + reconciliation (Enter/Update/Exit)");

    let chart = ChartSpec {
        title: Some(title),
        plot_size,
        layout: ChartLayoutSpec::default(),
        axis_left: Some(axis_left),
        axis_right: None,
        axis_top: None,
        axis_bottom: Some(axis_bottom),
        legend: None,
    };

    let row_keys: Vec<u64> = app
        .viz_scene
        .tables
        .get(&TABLE_STREAM)
        .map(|t| t.row_keys.clone())
        .unwrap_or_default();
    let head_idx = row_keys.len().saturating_sub(1);

    let measurer = vizir_text_parley::ParleyTextMeasurer::new();
    let (layout, marks) = chart.marks(&measurer, move |chart, plot| {
        let x = chart.x_scale_continuous(plot).expect("x scale");
        let y = chart.y_scale_continuous(plot).expect("y scale");

        let deps = [InputRef::Table {
            table: TABLE_STREAM,
        }];

        let mut out = Vec::new();
        out.push(
            Mark::builder(MarkId::from_raw(0x9F_000))
                .path()
                .z_index(vizir_charts::SERIES_STROKE)
                .path_compute(deps, move |ctx, _id| {
                    let n = ctx.table_row_count(TABLE_STREAM).unwrap_or(0);
                    let mut path = BezPath::new();
                    for i in 0..n {
                        let fx = ctx
                            .table_f64(TABLE_STREAM, i, STREAM_COL_X)
                            .unwrap_or(i as f64);
                        let fy = ctx.table_f64(TABLE_STREAM, i, STREAM_COL_Y).unwrap_or(0.0);
                        let p = Point::new(x.map(fx), y.map(fy));
                        if i == 0 {
                            path.move_to(p);
                        } else {
                            path.line_to(p);
                        }
                    }
                    path
                })
                .fill_brush_const(peniko::Color::TRANSPARENT)
                .stroke_brush_const(css::STEEL_BLUE)
                .stroke_width_const(2.0)
                .build(),
        );

        // Per-row points keyed by stable row key, so Enter/Exit is visible as the window slides.
        for (i, row_key) in row_keys.iter().copied().enumerate() {
            let idx = i;
            out.push(
                Mark::builder(MarkId::for_row(TABLE_STREAM, row_key))
                    .path()
                    .z_index(vizir_charts::SERIES_POINTS)
                    .path_compute(deps, move |ctx, _id| {
                        let fx = ctx
                            .table_f64(TABLE_STREAM, idx, STREAM_COL_X)
                            .unwrap_or(idx as f64);
                        let fy = ctx
                            .table_f64(TABLE_STREAM, idx, STREAM_COL_Y)
                            .unwrap_or(0.0);
                        let p = Point::new(x.map(fx), y.map(fy));
                        Circle::new(p, 3.25).to_path(0.1)
                    })
                    .fill_brush_const(css::TOMATO)
                    .stroke_brush_const(css::BLACK.with_alpha(90.0 / 255.0))
                    .stroke_width_const(1.0)
                    .build(),
            );
        }

        // "Head" marker for the newest sample.
        out.push(
            Mark::builder(MarkId::from_raw(0x9F_100))
                .path()
                .z_index(vizir_charts::SERIES_POINTS + 1)
                .path_compute(deps, move |ctx, _id| {
                    let fx = ctx
                        .table_f64(TABLE_STREAM, head_idx, STREAM_COL_X)
                        .unwrap_or(head_idx as f64);
                    let fy = ctx
                        .table_f64(TABLE_STREAM, head_idx, STREAM_COL_Y)
                        .unwrap_or(0.0);
                    let p = Point::new(x.map(fx), y.map(fy));
                    Circle::new(p, 6.0).to_path(0.1)
                })
                .fill_brush_const(css::GOLDENROD)
                .stroke_brush_const(css::BLACK.with_alpha(120.0 / 255.0))
                .stroke_width_const(1.0)
                .build(),
        );

        out
    });

    (layout.view, marks)
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("run");
}
