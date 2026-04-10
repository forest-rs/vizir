// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Experimental authored-spec and lowering support.
//!
//! This module is the first compilation seam between a Vega-Lite-like authored chart description
//! and the existing `vizir_core` / `vizir_transforms` / `vizir_charts` runtime pieces.
//!
//! The supported slice is intentionally small:
//! - one unit chart,
//! - one input table already present in a [`vizir_core::Scene`],
//! - a small transform subset,
//! - `bar`, `line`, `point`, and `area` marks,
//! - `x`, `x2`, `y`, `y2`, and categorical `color` channels,
//! - optional chart titles.
//!
//! It is not a JSON parser and not a full Vega/Vega-Lite implementation.

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use kurbo::Rect;
use peniko::Brush;
use peniko::color::palette::css;
use vizir_core::{ColumnId, Mark, MarkDiff, MarkId, Scene, TableId};
use vizir_transforms::{
    AggregateField, AggregateOp, Predicate, Program, SceneExecutionError, SortOrder, StackOffset,
    TableFrame, TableFrameError, Transform,
};

#[cfg(not(feature = "std"))]
use crate::float::FloatExt;

use crate::{
    AxisSpec, ChartLayout, ChartLayoutSpec, ChartSpec, GridStyle, LegendItem, LegendOrient,
    LegendPlacement, LegendSwatchesSpec, PointMarkSpec, ScaleBandSpec, ScaleLinearSpec,
    ScaleTimeSpec, Size, StrokeStyle, Symbol, TextMeasurer, TitleSpec, format_time_seconds,
};

/// A reference to authored chart data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataRef {
    /// Use an existing table already present in the scene.
    Table(TableId),
}

/// The authored field kind for a channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// Continuous numeric data.
    Quantitative,
    /// Ordered categories.
    Ordinal,
    /// Unordered categories.
    Nominal,
    /// Time values.
    ///
    /// In the current implementation this maps to Vizir's numeric-seconds time scale.
    Temporal,
}

/// The authored mark kind for the unit chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkDef {
    /// Vertical bars over a discrete x axis.
    Bar,
    /// A polyline over continuous x/y axes.
    Line,
    /// One symbol per row over continuous x/y axes.
    Point,
    /// A filled area over continuous x/y axes.
    Area,
}

/// A single authored channel definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelDef {
    field: ColumnId,
    kind: FieldKind,
    aggregate: Option<AggregateOp>,
    title: Option<String>,
}

impl ChannelDef {
    /// Creates a quantitative channel over the given field.
    pub fn quantitative(field: ColumnId) -> Self {
        Self {
            field,
            kind: FieldKind::Quantitative,
            aggregate: None,
            title: None,
        }
    }

    /// Creates an ordinal channel over the given field.
    pub fn ordinal(field: ColumnId) -> Self {
        Self {
            field,
            kind: FieldKind::Ordinal,
            aggregate: None,
            title: None,
        }
    }

    /// Creates a nominal channel over the given field.
    pub fn nominal(field: ColumnId) -> Self {
        Self {
            field,
            kind: FieldKind::Nominal,
            aggregate: None,
            title: None,
        }
    }

    /// Creates a temporal channel over the given field.
    pub fn temporal(field: ColumnId) -> Self {
        Self {
            field,
            kind: FieldKind::Temporal,
            aggregate: None,
            title: None,
        }
    }

    /// Sets the aggregate applied before this channel is lowered.
    pub fn with_aggregate(mut self, aggregate: AggregateOp) -> Self {
        self.aggregate = Some(aggregate);
        self
    }

    /// Sets the authored title used for guides derived from this channel.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    fn field(&self) -> ColumnId {
        self.field
    }

    fn kind(&self) -> FieldKind {
        self.kind
    }

    fn aggregate(&self) -> Option<AggregateOp> {
        self.aggregate
    }

    fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
}

/// The authored encoding set for a unit chart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EncodingSet {
    x: Option<ChannelDef>,
    x2: Option<ChannelDef>,
    y: Option<ChannelDef>,
    y2: Option<ChannelDef>,
    color: Option<ChannelDef>,
    text: Option<ChannelDef>,
}

impl EncodingSet {
    /// Creates an empty encoding set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the x channel.
    pub fn with_x(mut self, x: ChannelDef) -> Self {
        self.x = Some(x);
        self
    }

    /// Sets the x2 channel.
    pub fn with_x2(mut self, x2: ChannelDef) -> Self {
        self.x2 = Some(x2);
        self
    }

    /// Sets the y channel.
    pub fn with_y(mut self, y: ChannelDef) -> Self {
        self.y = Some(y);
        self
    }

    /// Sets the y2 channel.
    pub fn with_y2(mut self, y2: ChannelDef) -> Self {
        self.y2 = Some(y2);
        self
    }

    /// Sets the color channel.
    pub fn with_color(mut self, color: ChannelDef) -> Self {
        self.color = Some(color);
        self
    }

    /// Sets the text channel.
    pub fn with_text(mut self, text: ChannelDef) -> Self {
        self.text = Some(text);
        self
    }

    fn x(&self) -> Option<&ChannelDef> {
        self.x.as_ref()
    }

    fn x2(&self) -> Option<&ChannelDef> {
        self.x2.as_ref()
    }

    fn y(&self) -> Option<&ChannelDef> {
        self.y.as_ref()
    }

    fn y2(&self) -> Option<&ChannelDef> {
        self.y2.as_ref()
    }

    fn color(&self) -> Option<&ChannelDef> {
        self.color.as_ref()
    }

    fn text(&self) -> Option<&ChannelDef> {
        self.text.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq)]
enum TransformSpecKind {
    Filter {
        predicate: Predicate,
        columns: Vec<ColumnId>,
    },
    Sort {
        by: ColumnId,
        order: SortOrder,
        columns: Vec<ColumnId>,
    },
    Aggregate {
        group_by: Vec<ColumnId>,
        fields: Vec<AggregateField>,
    },
    Bin {
        input_col: ColumnId,
        output_start: ColumnId,
        step: f64,
        columns: Vec<ColumnId>,
    },
    Stack {
        group_by: Vec<ColumnId>,
        offset: StackOffset,
        sort_by: Option<ColumnId>,
        sort_order: SortOrder,
        field: ColumnId,
        output_start: ColumnId,
        output_end: ColumnId,
        columns: Vec<ColumnId>,
    },
}

/// An authored transform specification.
#[derive(Clone, Debug, PartialEq)]
pub struct TransformSpec {
    kind: TransformSpecKind,
}

impl TransformSpec {
    /// Creates a filter transform.
    pub fn filter(predicate: Predicate, columns: Vec<ColumnId>) -> Self {
        Self {
            kind: TransformSpecKind::Filter { predicate, columns },
        }
    }

    /// Creates a sort transform.
    pub fn sort(by: ColumnId, order: SortOrder, columns: Vec<ColumnId>) -> Self {
        Self {
            kind: TransformSpecKind::Sort { by, order, columns },
        }
    }

    /// Creates an aggregate transform.
    pub fn aggregate(group_by: Vec<ColumnId>, fields: Vec<AggregateField>) -> Self {
        Self {
            kind: TransformSpecKind::Aggregate { group_by, fields },
        }
    }

    /// Creates a fixed-step bin transform.
    pub fn bin(
        input_col: ColumnId,
        output_start: ColumnId,
        step: f64,
        columns: Vec<ColumnId>,
    ) -> Self {
        Self {
            kind: TransformSpecKind::Bin {
                input_col,
                output_start,
                step,
                columns,
            },
        }
    }

    /// Creates a stack transform.
    #[allow(
        clippy::too_many_arguments,
        reason = "matches the authored stack parameters directly"
    )]
    pub fn stack(
        group_by: Vec<ColumnId>,
        offset: StackOffset,
        sort_by: Option<ColumnId>,
        sort_order: SortOrder,
        field: ColumnId,
        output_start: ColumnId,
        output_end: ColumnId,
        columns: Vec<ColumnId>,
    ) -> Self {
        Self {
            kind: TransformSpecKind::Stack {
                group_by,
                offset,
                sort_by,
                sort_order,
                field,
                output_start,
                output_end,
                columns,
            },
        }
    }
}

/// A single authored unit chart.
#[derive(Clone, Debug)]
pub struct UnitSpec {
    id_base: u64,
    derived_table_base: TableId,
    data: DataRef,
    transforms: Vec<TransformSpec>,
    mark: MarkDef,
    encoding: EncodingSet,
    width: f64,
    height: f64,
    title: Option<String>,
}

impl UnitSpec {
    /// Creates a new unit spec.
    pub fn new(id_base: u64, derived_table_base: TableId, data: DataRef, mark: MarkDef) -> Self {
        Self {
            id_base,
            derived_table_base,
            data,
            transforms: Vec::new(),
            mark,
            encoding: EncodingSet::new(),
            width: 220.0,
            height: 120.0,
            title: None,
        }
    }

    /// Sets the plot size used by the lowered chart.
    pub fn with_size(mut self, width: f64, height: f64) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Sets the authored chart title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Replaces the encoding set.
    pub fn with_encoding(mut self, encoding: EncodingSet) -> Self {
        self.encoding = encoding;
        self
    }

    /// Sets the x channel.
    pub fn with_x(mut self, x: ChannelDef) -> Self {
        self.encoding = self.encoding.with_x(x);
        self
    }

    /// Sets the x2 channel.
    pub fn with_x2(mut self, x2: ChannelDef) -> Self {
        self.encoding = self.encoding.with_x2(x2);
        self
    }

    /// Sets the y channel.
    pub fn with_y(mut self, y: ChannelDef) -> Self {
        self.encoding = self.encoding.with_y(y);
        self
    }

    /// Sets the y2 channel.
    pub fn with_y2(mut self, y2: ChannelDef) -> Self {
        self.encoding = self.encoding.with_y2(y2);
        self
    }

    /// Sets the color channel.
    pub fn with_color(mut self, color: ChannelDef) -> Self {
        self.encoding = self.encoding.with_color(color);
        self
    }

    /// Sets the text channel.
    pub fn with_text(mut self, text: ChannelDef) -> Self {
        self.encoding = self.encoding.with_text(text);
        self
    }

    /// Appends an authored transform.
    pub fn with_transform(mut self, transform: TransformSpec) -> Self {
        self.transforms.push(transform);
        self
    }

    /// Lowers the authored unit spec into a chart/transform/series plan.
    pub fn lower(&self, scene: &Scene) -> Result<LoweredUnit, LoweringError> {
        let DataRef::Table(input_table) = self.data;
        ensure_table_exists(scene, input_table)?;

        let x = self
            .encoding
            .x()
            .ok_or(LoweringError::MissingChannel("x"))?;
        let x2 = self.encoding.x2();
        let y = self
            .encoding
            .y()
            .ok_or(LoweringError::MissingChannel("y"))?;
        let y2 = self.encoding.y2();
        let color = self.encoding.color();
        if self.encoding.text().is_some() {
            return Err(LoweringError::Unsupported(
                "the experimental lowering slice does not render text channels yet",
            ));
        }
        if x.aggregate().is_some() {
            return Err(LoweringError::Unsupported(
                "aggregate on the x channel is not supported in the experimental lowering slice",
            ));
        }
        if let Some(x2) = x2
            && x2.aggregate().is_some()
        {
            return Err(LoweringError::Unsupported(
                "aggregate on the x2 channel is not supported in the experimental lowering slice",
            ));
        }
        if let Some(color) = color
            && color.aggregate().is_some()
        {
            return Err(LoweringError::Unsupported(
                "aggregate on the color channel is not supported in the experimental lowering slice",
            ));
        }
        if y.kind() != FieldKind::Quantitative {
            return Err(LoweringError::Unsupported(
                "the experimental lowering slice requires a quantitative y channel",
            ));
        }
        if let Some(y2) = y2
            && y2.aggregate().is_some()
        {
            return Err(LoweringError::Unsupported(
                "aggregate on the y2 channel is not supported in the experimental lowering slice",
            ));
        }
        if let Some(y2) = y2
            && y2.kind() != FieldKind::Quantitative
        {
            return Err(LoweringError::Unsupported(
                "the experimental lowering slice requires a quantitative y2 channel",
            ));
        }
        if color.is_some() && self.mark == MarkDef::Bar {
            return Err(LoweringError::Unsupported(
                "categorical color splitting is not supported for bar marks yet",
            ));
        }
        if (x2.is_some() || y2.is_some()) && self.mark != MarkDef::Area {
            return Err(LoweringError::Unsupported(
                "secondary position channels are currently only supported for area marks",
            ));
        }
        if x2.is_some() && y2.is_none() {
            return Err(LoweringError::Unsupported(
                "x2 currently requires y2 so the area can form an explicit lower edge",
            ));
        }

        match self.mark {
            MarkDef::Bar => {
                if !matches!(x.kind(), FieldKind::Ordinal | FieldKind::Nominal) {
                    return Err(LoweringError::Unsupported(
                        "bar lowering currently requires an ordinal or nominal x channel",
                    ));
                }
            }
            MarkDef::Line | MarkDef::Point | MarkDef::Area => {
                if !matches!(x.kind(), FieldKind::Quantitative | FieldKind::Temporal) {
                    return Err(LoweringError::Unsupported(
                        "line/point/area lowering currently requires a quantitative or temporal x channel",
                    ));
                }
            }
        }
        if let Some(x2) = x2 {
            if x2.kind() != x.kind() {
                return Err(LoweringError::Unsupported(
                    "x2 must use the same field kind as x in the experimental lowering slice",
                ));
            }
            if !matches!(x2.kind(), FieldKind::Quantitative | FieldKind::Temporal) {
                return Err(LoweringError::Unsupported(
                    "x2 currently requires a quantitative or temporal channel kind",
                ));
            }
        }

        let mut next_table = self.derived_table_base.0;
        let mut derived_tables = Vec::new();
        let mut base_program = Program::new();
        let mut current_table = input_table;

        for authored in &self.transforms {
            let output = alloc_table(&mut next_table);
            lower_authored_transform(&mut base_program, authored, current_table, output);
            derived_tables.push(output);
            current_table = output;
        }

        let mut lowered_y_field = y.field();
        if let Some(aggregate) = y.aggregate() {
            let output = alloc_table(&mut next_table);
            let output_col = ColumnId(next_derived_col(self));
            let mut group_by = vec![x.field()];
            if let Some(color) = color {
                push_unique_col(&mut group_by, color.field());
            }
            base_program.push(Transform::Aggregate {
                input: current_table,
                output,
                group_by,
                fields: vec![AggregateField {
                    op: aggregate,
                    input: y.field(),
                    output: output_col,
                }],
            });
            derived_tables.push(output);
            current_table = output;
            lowered_y_field = output_col;
        }

        let preview_frame = preview_output_frame(
            scene,
            &base_program,
            input_table,
            current_table,
            required_columns(x, x2, lowered_y_field, y2, color),
        )?;

        let mut program = if base_program.transforms().is_empty() {
            None
        } else {
            Some(base_program)
        };

        let mut series_layers = Vec::new();
        let mut legend_items = Vec::new();
        if let Some(color) = color {
            if !matches!(color.kind(), FieldKind::Ordinal | FieldKind::Nominal) {
                return Err(LoweringError::Unsupported(
                    "the experimental lowering slice only supports categorical color channels",
                ));
            }
            let series_values = distinct_values(&preview_frame, color.field());
            if series_values.is_empty() {
                return Err(LoweringError::Unsupported(
                    "color lowering requires at least one finite series value",
                ));
            }
            let fills = default_series_fills(series_values.len());
            let p = program.get_or_insert_with(Program::new);
            for (index, (value, fill)) in series_values.iter().copied().zip(fills).enumerate() {
                let output = alloc_table(&mut next_table);
                p.push(Transform::Filter {
                    input: current_table,
                    output,
                    predicate: Predicate {
                        col: color.field(),
                        op: vizir_transforms::CompareOp::Eq,
                        value,
                    },
                    columns: series_columns(x, x2, lowered_y_field, y2, Some(color)),
                });
                if matches!(self.mark, MarkDef::Line | MarkDef::Area) {
                    p.push(Transform::Sort {
                        input: output,
                        output,
                        by: x.field(),
                        order: SortOrder::Asc,
                        columns: series_columns(x, x2, lowered_y_field, y2, None),
                    });
                }
                derived_tables.push(output);
                let label = format_channel_value(value, color.kind());
                legend_items.push(LegendItem {
                    label,
                    fill: fill.clone(),
                });
                series_layers.push(match self.mark {
                    MarkDef::Bar => unreachable!("bar + color is rejected above"),
                    MarkDef::Line => SeriesLayer::Line(LineLayer {
                        id: MarkId::from_raw(self.id_base.wrapping_add(0x1_000 + index as u64)),
                        table: output,
                        x: x.field(),
                        y: lowered_y_field,
                        stroke: StrokeStyle::solid(fill, 2.0),
                    }),
                    MarkDef::Point => SeriesLayer::Point(PointLayer {
                        table: output,
                        x: x.field(),
                        y: lowered_y_field,
                        symbol: Symbol::Circle,
                        size: 6.0,
                        fill,
                    }),
                    MarkDef::Area => SeriesLayer::Area(AreaLayer {
                        id: MarkId::from_raw(self.id_base.wrapping_add(0x1_000 + index as u64)),
                        table: output,
                        x: x.field(),
                        x2: x2.map(ChannelDef::field),
                        y: lowered_y_field,
                        y2: y2.map(ChannelDef::field),
                        baseline: 0.0,
                        fill,
                    }),
                });
            }
        } else {
            series_layers.push(match self.mark {
                MarkDef::Bar => SeriesLayer::Bar(BarLayer {
                    table: current_table,
                    y: lowered_y_field,
                    baseline: 0.0,
                    fill: Brush::Solid(css::CORNFLOWER_BLUE),
                }),
                MarkDef::Line => SeriesLayer::Line(LineLayer {
                    id: MarkId::from_raw(self.id_base.wrapping_add(0x1_000)),
                    table: current_table,
                    x: x.field(),
                    y: lowered_y_field,
                    stroke: StrokeStyle::solid(css::BLACK, 2.0),
                }),
                MarkDef::Point => SeriesLayer::Point(PointLayer {
                    table: current_table,
                    x: x.field(),
                    y: lowered_y_field,
                    symbol: Symbol::Circle,
                    size: 6.0,
                    fill: Brush::Solid(css::TOMATO),
                }),
                MarkDef::Area => SeriesLayer::Area(AreaLayer {
                    id: MarkId::from_raw(self.id_base.wrapping_add(0x1_000)),
                    table: current_table,
                    x: x.field(),
                    x2: x2.map(ChannelDef::field),
                    y: lowered_y_field,
                    y2: y2.map(ChannelDef::field),
                    baseline: 0.0,
                    fill: Brush::Solid(css::CORNFLOWER_BLUE),
                }),
            });
        }

        let chart = build_chart_spec(
            self,
            &preview_frame,
            x,
            x2,
            lowered_y_field,
            y2.map(ChannelDef::field),
            y.title(),
            legend_items,
        )?;

        Ok(LoweredUnit {
            input_table,
            output_table: current_table,
            derived_tables,
            program,
            chart,
            series_layers,
        })
    }

    /// Lowers the authored spec and applies the derived tables to the scene.
    pub fn lower_into_scene(&self, scene: &mut Scene) -> Result<LoweredUnit, LoweringError> {
        let lowered = self.lower(scene)?;
        lowered.apply_to_scene(scene)?;
        Ok(lowered)
    }
}

/// Errors returned while lowering or executing an authored unit spec.
#[derive(Debug)]
pub enum LoweringError {
    /// The requested input or derived table does not exist in the scene.
    MissingTable(TableId),
    /// The referenced table exists, but has no data accessor.
    MissingTableData(TableId),
    /// Failed to extract a numeric frame from a table.
    FrameError {
        /// The table that failed extraction.
        table: TableId,
        /// The underlying frame-extraction error.
        err: TableFrameError,
    },
    /// A required channel was not authored.
    MissingChannel(&'static str),
    /// The authored shape is outside the current experimental lowering slice.
    Unsupported(&'static str),
    /// Failed to infer a numeric domain from the lowered data.
    MissingDomain {
        /// The affected field.
        field: ColumnId,
        /// The channel role that needed the domain.
        role: &'static str,
    },
    /// Failed while executing the lowered transform program.
    TransformExecution(SceneExecutionError),
}

impl From<SceneExecutionError> for LoweringError {
    fn from(value: SceneExecutionError) -> Self {
        Self::TransformExecution(value)
    }
}

/// A lowered unit chart plan.
#[derive(Clone, Debug)]
pub struct LoweredUnit {
    input_table: TableId,
    output_table: TableId,
    derived_tables: Vec<TableId>,
    program: Option<Program>,
    chart: ChartSpec,
    series_layers: Vec<SeriesLayer>,
}

impl LoweredUnit {
    /// Returns the original input table referenced by the authored unit spec.
    pub fn input_table(&self) -> TableId {
        self.input_table
    }

    /// Returns the base output table after authored transforms and aggregate lowering.
    pub fn output_table(&self) -> TableId {
        self.output_table
    }

    /// Returns every derived table id the lowered plan may write into the scene.
    pub fn derived_tables(&self) -> &[TableId] {
        &self.derived_tables
    }

    /// Returns the lowered chart specification.
    pub fn chart(&self) -> &ChartSpec {
        &self.chart
    }

    /// Returns the lowered transform program, if the unit spec needs derived tables.
    pub fn program(&self) -> Option<&Program> {
        self.program.as_ref()
    }

    /// Applies the lowered transform program to the scene.
    pub fn apply_to_scene(&self, scene: &mut Scene) -> Result<(), LoweringError> {
        if let Some(program) = &self.program {
            program.apply_to_scene(scene)?;
        }
        Ok(())
    }

    /// Produces the full mark list for the lowered chart.
    pub fn marks(
        &self,
        scene: &Scene,
        measurer: &impl TextMeasurer,
    ) -> Result<(ChartLayout, Vec<Mark>), LoweringError> {
        let layout = self.chart.layout(measurer);
        let mut marks = self.series_marks(scene, layout.data)?;
        marks.extend(self.chart.guide_marks(measurer, &layout));
        Ok((layout, marks))
    }

    /// Builds marks and runs a full scene tick.
    pub fn tick(
        &self,
        scene: &mut Scene,
        measurer: &impl TextMeasurer,
    ) -> Result<(ChartLayout, Vec<MarkDiff>), LoweringError> {
        let (layout, marks) = self.marks(scene, measurer)?;
        let diffs = scene.tick(marks);
        Ok((layout, diffs))
    }

    fn series_marks(&self, scene: &Scene, plot: Rect) -> Result<Vec<Mark>, LoweringError> {
        let mut out = Vec::new();
        for layer in &self.series_layers {
            out.extend(layer.marks(scene, &self.chart, plot)?);
        }
        Ok(out)
    }
}

#[derive(Clone, Debug)]
enum SeriesLayer {
    Bar(BarLayer),
    Line(LineLayer),
    Point(PointLayer),
    Area(AreaLayer),
}

impl SeriesLayer {
    fn marks(
        &self,
        scene: &Scene,
        chart: &ChartSpec,
        plot: Rect,
    ) -> Result<Vec<Mark>, LoweringError> {
        match self {
            Self::Bar(layer) => layer.marks(scene, chart, plot),
            Self::Line(layer) => layer.marks(scene, chart, plot),
            Self::Point(layer) => layer.marks(scene, chart, plot),
            Self::Area(layer) => layer.marks(scene, chart, plot),
        }
    }
}

#[derive(Clone, Debug)]
struct BarLayer {
    table: TableId,
    y: ColumnId,
    baseline: f64,
    fill: Brush,
}

impl BarLayer {
    fn marks(
        &self,
        scene: &Scene,
        chart: &ChartSpec,
        plot: Rect,
    ) -> Result<Vec<Mark>, LoweringError> {
        let table = scene
            .tables
            .get(&self.table)
            .ok_or(LoweringError::MissingTable(self.table))?;
        let row_keys = table.row_keys.clone();
        let band = chart
            .x_axis()
            .ok_or(LoweringError::MissingChannel("x"))?
            .scale_band(plot);
        let y_scale = chart
            .y_scale_continuous(plot)
            .ok_or(LoweringError::MissingChannel("y"))?;
        Ok(crate::BarMarkSpec::new(self.table, self.y, band, y_scale)
            .with_baseline(self.baseline)
            .with_fill(self.fill.clone())
            .marks(&row_keys))
    }
}

#[derive(Clone, Debug)]
struct LineLayer {
    id: MarkId,
    table: TableId,
    x: ColumnId,
    y: ColumnId,
    stroke: StrokeStyle,
}

impl LineLayer {
    fn marks(
        &self,
        _scene: &Scene,
        chart: &ChartSpec,
        plot: Rect,
    ) -> Result<Vec<Mark>, LoweringError> {
        let x_scale = chart
            .x_scale_continuous(plot)
            .ok_or(LoweringError::MissingChannel("x"))?;
        let y_scale = chart
            .y_scale_continuous(plot)
            .ok_or(LoweringError::MissingChannel("y"))?;
        Ok(
            crate::LineMarkSpec::new(self.id, self.table, self.x, self.y, x_scale, y_scale)
                .with_stroke(self.stroke.clone())
                .marks(),
        )
    }
}

#[derive(Clone, Debug)]
struct PointLayer {
    table: TableId,
    x: ColumnId,
    y: ColumnId,
    symbol: Symbol,
    size: f64,
    fill: Brush,
}

impl PointLayer {
    fn marks(
        &self,
        scene: &Scene,
        chart: &ChartSpec,
        plot: Rect,
    ) -> Result<Vec<Mark>, LoweringError> {
        let table = scene
            .tables
            .get(&self.table)
            .ok_or(LoweringError::MissingTable(self.table))?;
        let row_keys = table.row_keys.clone();
        let x_scale = chart
            .x_scale_continuous(plot)
            .ok_or(LoweringError::MissingChannel("x"))?;
        let y_scale = chart
            .y_scale_continuous(plot)
            .ok_or(LoweringError::MissingChannel("y"))?;
        Ok(
            PointMarkSpec::new(self.table, self.x, self.y, x_scale, y_scale)
                .with_symbol(self.symbol)
                .with_size(self.size)
                .with_fill(self.fill.clone())
                .marks(&row_keys),
        )
    }
}

#[derive(Clone, Debug)]
struct AreaLayer {
    id: MarkId,
    table: TableId,
    x: ColumnId,
    x2: Option<ColumnId>,
    y: ColumnId,
    y2: Option<ColumnId>,
    baseline: f64,
    fill: Brush,
}

impl AreaLayer {
    fn marks(
        &self,
        _scene: &Scene,
        chart: &ChartSpec,
        plot: Rect,
    ) -> Result<Vec<Mark>, LoweringError> {
        let x_scale = chart
            .x_scale_continuous(plot)
            .ok_or(LoweringError::MissingChannel("x"))?;
        let y_scale = chart
            .y_scale_continuous(plot)
            .ok_or(LoweringError::MissingChannel("y"))?;
        Ok(match (self.x2, self.y2) {
            (Some(x2), Some(y2)) => crate::RangeAreaMarkSpec::new(
                self.id.0, self.table, self.x, self.y, x2, y2, x_scale, y_scale,
            )
            .with_fill(self.fill.clone())
            .marks(),
            (None, Some(y2)) => crate::StackedAreaMarkSpec::new(
                self.id.0, self.table, self.x, y2, self.y, x_scale, y_scale,
            )
            .with_fill(self.fill.clone())
            .marks(),
            (None, None) => {
                crate::AreaMarkSpec::new(self.id.0, self.table, self.x, self.y, x_scale, y_scale)
                    .with_baseline(self.baseline)
                    .with_fill(self.fill.clone())
                    .marks()
            }
            (Some(_), None) => {
                return Err(LoweringError::Unsupported(
                    "x2 requires y2 before area marks can be rendered",
                ));
            }
        })
    }
}

fn build_chart_spec(
    spec: &UnitSpec,
    frame: &TableFrame,
    x: &ChannelDef,
    x2: Option<&ChannelDef>,
    y_field: ColumnId,
    y2_field: Option<ColumnId>,
    y_title: Option<&str>,
    legend_items: Vec<LegendItem>,
) -> Result<ChartSpec, LoweringError> {
    let title = spec.title.as_ref().map(|title| {
        TitleSpec::new(
            MarkId::from_raw(spec.id_base.wrapping_add(0x200)),
            title.clone(),
        )
        .with_font_size(12.0)
        .with_fill(css::BLACK)
    });

    let axis_bottom = build_x_axis(spec, frame, x, x2)?;
    let axis_left = build_y_axis(spec, frame, y_field, y2_field, y_title, spec.mark)?;
    let legend = if legend_items.is_empty() {
        None
    } else {
        Some((
            LegendSwatchesSpec::new(spec.id_base.wrapping_add(0x300), legend_items)
                .with_text_fill(css::BLACK),
            LegendPlacement {
                orient: LegendOrient::Right,
                ..LegendPlacement::default()
            },
        ))
    };

    Ok(ChartSpec {
        title,
        plot_size: Size {
            width: spec.width,
            height: spec.height,
        },
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
        legend,
    })
}

fn build_x_axis(
    spec: &UnitSpec,
    frame: &TableFrame,
    x: &ChannelDef,
    x2: Option<&ChannelDef>,
) -> Result<AxisSpec, LoweringError> {
    let mut axis = match x.kind() {
        FieldKind::Ordinal | FieldKind::Nominal => {
            let labels = category_labels(frame, x.field(), x.kind());
            AxisSpec::bottom(
                spec.id_base.wrapping_add(0x10_000),
                ScaleBandSpec::new(labels.len()).with_padding(0.2, 0.1),
            )
            .with_tick_count(labels.len().max(1))
            .with_tick_padding(4.0)
            .with_tick_formatter({
                let labels = labels.clone();
                move |v, _step| {
                    if labels.is_empty() {
                        return String::new();
                    }
                    let v = v
                        .round()
                        .clamp(0.0, (labels.len().saturating_sub(1)) as f64);
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "clamped to the label index range"
                    )]
                    let index = v as usize;
                    labels[index].clone()
                }
            })
        }
        FieldKind::Quantitative => {
            let domain = infer_frame_domain_pair(frame, x.field(), x2.map(ChannelDef::field), "x")?;
            AxisSpec::bottom(
                spec.id_base.wrapping_add(0x10_000),
                ScaleLinearSpec::new(expand_domain(domain)).with_nice(true),
            )
            .with_tick_count(6)
        }
        FieldKind::Temporal => {
            let domain = infer_frame_domain_pair(frame, x.field(), x2.map(ChannelDef::field), "x")?;
            AxisSpec::bottom(
                spec.id_base.wrapping_add(0x10_000),
                ScaleTimeSpec::new(expand_domain(domain)),
            )
            .with_tick_count(6)
        }
    };
    if let Some(title) = x.title() {
        axis = axis.with_title(title).with_title_offset(10.0);
    }
    Ok(axis)
}

fn build_y_axis(
    spec: &UnitSpec,
    frame: &TableFrame,
    y_field: ColumnId,
    y2_field: Option<ColumnId>,
    y_title: Option<&str>,
    mark: MarkDef,
) -> Result<AxisSpec, LoweringError> {
    let domain = infer_frame_domain_pair(frame, y_field, y2_field, "y")?;
    let domain = match mark {
        MarkDef::Bar => include_zero(expand_domain(domain)),
        MarkDef::Area if y2_field.is_none() => include_zero(expand_domain(domain)),
        MarkDef::Area | MarkDef::Line | MarkDef::Point => expand_domain(domain),
    };
    let mut axis = AxisSpec::left(
        spec.id_base.wrapping_add(0x11_000),
        ScaleLinearSpec::new(domain).with_nice(true),
    )
    .with_tick_count(6)
    .with_grid(GridStyle {
        stroke: StrokeStyle::solid(css::BLACK.with_alpha(40.0 / 255.0), 1.0),
    });
    if let Some(title) = y_title {
        axis = axis.with_title(title).with_title_offset(10.0);
    }
    Ok(axis)
}

fn ensure_table_exists(scene: &Scene, table: TableId) -> Result<(), LoweringError> {
    if scene.tables.contains_key(&table) {
        Ok(())
    } else {
        Err(LoweringError::MissingTable(table))
    }
}

fn lower_authored_transform(
    program: &mut Program,
    authored: &TransformSpec,
    input: TableId,
    output: TableId,
) {
    match &authored.kind {
        TransformSpecKind::Filter { predicate, columns } => program.push(Transform::Filter {
            input,
            output,
            predicate: predicate.clone(),
            columns: columns.clone(),
        }),
        TransformSpecKind::Sort { by, order, columns } => program.push(Transform::Sort {
            input,
            output,
            by: *by,
            order: *order,
            columns: columns.clone(),
        }),
        TransformSpecKind::Aggregate { group_by, fields } => program.push(Transform::Aggregate {
            input,
            output,
            group_by: group_by.clone(),
            fields: fields.clone(),
        }),
        TransformSpecKind::Bin {
            input_col,
            output_start,
            step,
            columns,
        } => program.push(Transform::Bin {
            input,
            output,
            input_col: *input_col,
            output_start: *output_start,
            step: *step,
            columns: columns.clone(),
        }),
        TransformSpecKind::Stack {
            group_by,
            offset,
            sort_by,
            sort_order,
            field,
            output_start,
            output_end,
            columns,
        } => program.push(Transform::Stack {
            input,
            output,
            group_by: group_by.clone(),
            offset: *offset,
            sort_by: *sort_by,
            sort_order: *sort_order,
            field: *field,
            output_start: *output_start,
            output_end: *output_end,
            columns: columns.clone(),
        }),
    }
}

fn preview_output_frame(
    scene: &Scene,
    program: &Program,
    input_table: TableId,
    output_table: TableId,
    columns: Vec<ColumnId>,
) -> Result<TableFrame, LoweringError> {
    if program.transforms().is_empty() {
        let table = scene
            .tables
            .get(&input_table)
            .ok_or(LoweringError::MissingTable(input_table))?;
        TableFrame::from_table(table, dedup_cols(columns)).map_err(|err| match err {
            TableFrameError::MissingData => LoweringError::MissingTableData(input_table),
            _ => LoweringError::FrameError {
                table: input_table,
                err,
            },
        })
    } else {
        let output = program.execute_on_scene(scene)?;
        output
            .tables
            .get(&output_table)
            .cloned()
            .ok_or(LoweringError::MissingTable(output_table))
    }
}

fn required_columns(
    x: &ChannelDef,
    x2: Option<&ChannelDef>,
    y_field: ColumnId,
    y2: Option<&ChannelDef>,
    color: Option<&ChannelDef>,
) -> Vec<ColumnId> {
    let mut out = vec![x.field(), y_field];
    if let Some(x2) = x2 {
        push_unique_col(&mut out, x2.field());
    }
    if let Some(y2) = y2 {
        push_unique_col(&mut out, y2.field());
    }
    if let Some(color) = color {
        push_unique_col(&mut out, color.field());
    }
    out
}

fn infer_frame_domain_pair(
    frame: &TableFrame,
    primary: ColumnId,
    secondary: Option<ColumnId>,
    role: &'static str,
) -> Result<(f64, f64), LoweringError> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for col in [Some(primary), secondary].into_iter().flatten() {
        for row in 0..frame.row_count() {
            let Some(v) = frame.f64(row, col) else {
                continue;
            };
            if !v.is_finite() {
                continue;
            }
            min = min.min(v);
            max = max.max(v);
        }
    }
    if min.is_finite() && max.is_finite() {
        Ok((min, max))
    } else {
        Err(LoweringError::MissingDomain {
            field: primary,
            role,
        })
    }
}

fn series_columns(
    x: &ChannelDef,
    x2: Option<&ChannelDef>,
    y_field: ColumnId,
    y2: Option<&ChannelDef>,
    color: Option<&ChannelDef>,
) -> Vec<ColumnId> {
    required_columns(x, x2, y_field, y2, color)
}

fn expand_domain((min, max): (f64, f64)) -> (f64, f64) {
    if min == max {
        if min == 0.0 {
            (-1.0, 1.0)
        } else {
            let pad = (min.abs() * 0.1).max(1.0);
            (min - pad, max + pad)
        }
    } else {
        (min, max)
    }
}

fn include_zero((min, max): (f64, f64)) -> (f64, f64) {
    (min.min(0.0), max.max(0.0))
}

fn category_labels(frame: &TableFrame, col: ColumnId, kind: FieldKind) -> Vec<String> {
    let mut labels = Vec::with_capacity(frame.row_count());
    for row in 0..frame.row_count() {
        let label = frame
            .f64(row, col)
            .map(|v| format_channel_value(v, kind))
            .unwrap_or_else(|| String::from("?"));
        labels.push(label);
    }
    labels
}

fn distinct_values(frame: &TableFrame, col: ColumnId) -> Vec<f64> {
    let mut values = Vec::new();
    for row in 0..frame.row_count() {
        let Some(v) = frame.f64(row, col) else {
            continue;
        };
        if !v.is_finite() {
            continue;
        }
        if values
            .iter()
            .all(|existing: &f64| existing.to_bits() != v.to_bits())
        {
            values.push(v);
        }
    }
    values
}

fn format_channel_value(v: f64, kind: FieldKind) -> String {
    if !v.is_finite() {
        return String::from("NaN");
    }
    match kind {
        FieldKind::Temporal => format_time_seconds(v, 1.0),
        FieldKind::Quantitative | FieldKind::Ordinal | FieldKind::Nominal => {
            let rounded = v.round();
            if (v - rounded).abs() <= 1e-9 {
                format!("{rounded:.0}")
            } else {
                format!("{v:.2}")
            }
        }
    }
}

fn default_series_fills(count: usize) -> Vec<Brush> {
    const PALETTE: [peniko::Color; 8] = [
        css::CORNFLOWER_BLUE,
        css::TOMATO,
        css::MEDIUM_SEA_GREEN,
        css::GOLDENROD,
        css::SLATE_BLUE,
        css::DARK_CYAN,
        css::CRIMSON,
        css::HOT_PINK,
    ];

    (0..count)
        .map(|index| Brush::Solid(PALETTE[index % PALETTE.len()]))
        .collect()
}

fn dedup_cols(mut cols: Vec<ColumnId>) -> Vec<ColumnId> {
    let mut out = Vec::with_capacity(cols.len());
    for col in cols.drain(..) {
        push_unique_col(&mut out, col);
    }
    out
}

fn push_unique_col(cols: &mut Vec<ColumnId>, col: ColumnId) {
    if cols.iter().all(|existing| *existing != col) {
        cols.push(col);
    }
}

fn next_derived_col(spec: &UnitSpec) -> u32 {
    let mut max_col = 0_u32;
    if let Some(x) = spec.encoding.x() {
        max_col = max_col.max(x.field().0);
    }
    if let Some(x2) = spec.encoding.x2() {
        max_col = max_col.max(x2.field().0);
    }
    if let Some(y) = spec.encoding.y() {
        max_col = max_col.max(y.field().0);
    }
    if let Some(y2) = spec.encoding.y2() {
        max_col = max_col.max(y2.field().0);
    }
    if let Some(color) = spec.encoding.color() {
        max_col = max_col.max(color.field().0);
    }
    if let Some(text) = spec.encoding.text() {
        max_col = max_col.max(text.field().0);
    }
    for transform in &spec.transforms {
        match &transform.kind {
            TransformSpecKind::Filter { predicate, columns } => {
                max_col = max_col.max(predicate.col.0);
                for col in columns {
                    max_col = max_col.max(col.0);
                }
            }
            TransformSpecKind::Sort { by, columns, .. } => {
                max_col = max_col.max(by.0);
                for col in columns {
                    max_col = max_col.max(col.0);
                }
            }
            TransformSpecKind::Aggregate { group_by, fields } => {
                for col in group_by {
                    max_col = max_col.max(col.0);
                }
                for field in fields {
                    max_col = max_col.max(field.input.0).max(field.output.0);
                }
            }
            TransformSpecKind::Bin {
                input_col,
                output_start,
                columns,
                ..
            } => {
                max_col = max_col.max(input_col.0).max(output_start.0);
                for col in columns {
                    max_col = max_col.max(col.0);
                }
            }
            TransformSpecKind::Stack {
                group_by,
                sort_by,
                field,
                output_start,
                output_end,
                columns,
                ..
            } => {
                max_col = max_col.max(field.0).max(output_start.0).max(output_end.0);
                if let Some(sort_by) = sort_by {
                    max_col = max_col.max(sort_by.0);
                }
                for col in group_by {
                    max_col = max_col.max(col.0);
                }
                for col in columns {
                    max_col = max_col.max(col.0);
                }
            }
        }
    }
    max_col.saturating_add(1)
}

fn alloc_table(next_table: &mut u32) -> TableId {
    let out = TableId(*next_table);
    *next_table = next_table.saturating_add(1);
    out
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::boxed::Box;
    use alloc::vec;

    use super::*;
    use crate::HeuristicTextMeasurer;
    use vizir_core::{MarkKind, Table, TableData};

    #[derive(Debug)]
    struct TwoCols {
        a: Vec<f64>,
        b: Vec<f64>,
    }

    impl TableData for TwoCols {
        fn row_count(&self) -> usize {
            self.a.len().min(self.b.len())
        }

        fn f64(&self, row: usize, col: ColumnId) -> Option<f64> {
            match col {
                ColumnId(0) => self.a.get(row).copied(),
                ColumnId(1) => self.b.get(row).copied(),
                _ => None,
            }
        }
    }

    #[derive(Debug)]
    struct ThreeCols {
        a: Vec<f64>,
        b: Vec<f64>,
        c: Vec<f64>,
    }

    impl TableData for ThreeCols {
        fn row_count(&self) -> usize {
            self.a.len().min(self.b.len()).min(self.c.len())
        }

        fn f64(&self, row: usize, col: ColumnId) -> Option<f64> {
            match col {
                ColumnId(0) => self.a.get(row).copied(),
                ColumnId(1) => self.b.get(row).copied(),
                ColumnId(2) => self.c.get(row).copied(),
                _ => None,
            }
        }
    }

    #[derive(Debug)]
    struct FourCols {
        a: Vec<f64>,
        b: Vec<f64>,
        c: Vec<f64>,
        d: Vec<f64>,
    }

    impl TableData for FourCols {
        fn row_count(&self) -> usize {
            self.a
                .len()
                .min(self.b.len())
                .min(self.c.len())
                .min(self.d.len())
        }

        fn f64(&self, row: usize, col: ColumnId) -> Option<f64> {
            match col {
                ColumnId(0) => self.a.get(row).copied(),
                ColumnId(1) => self.b.get(row).copied(),
                ColumnId(2) => self.c.get(row).copied(),
                ColumnId(3) => self.d.get(row).copied(),
                _ => None,
            }
        }
    }

    #[test]
    fn aggregate_bar_lowering_builds_program_and_bars() {
        let mut scene = Scene::new();
        let table_id = TableId(10);
        let mut table = Table::new(table_id);
        table.row_keys = (0..6_u64).collect();
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 0.0, 1.0, 1.0, 2.0, 2.0],
            b: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(0xAA00, TableId(100), DataRef::Table(table_id), MarkDef::Bar)
            .with_title("aggregate bar")
            .with_x(ChannelDef::ordinal(ColumnId(0)).with_title("category"))
            .with_y(
                ChannelDef::quantitative(ColumnId(1))
                    .with_aggregate(AggregateOp::Sum)
                    .with_title("sum(value)"),
            );

        let lowered = spec.lower(&scene).expect("lower aggregate bar");
        assert!(lowered.program().is_some());
        lowered.apply_to_scene(&mut scene).expect("apply program");
        assert_eq!(scene.tables[&lowered.output_table()].row_keys.len(), 3);

        let (_layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("marks");
        let rect_count = marks
            .iter()
            .filter(|mark| matches!(mark.encodings, vizir_core::MarkEncodings::Rect(_)))
            .count();
        assert_eq!(rect_count, 3);
    }

    #[test]
    fn point_color_lowering_creates_series_tables_and_legend() {
        let mut scene = Scene::new();
        let table_id = TableId(20);
        let mut table = Table::new(table_id);
        table.row_keys = (0..6_u64).collect();
        table.data = Some(Box::new(ThreeCols {
            a: vec![0.0, 1.0, 2.0, 0.5, 1.5, 2.5],
            b: vec![1.0, 2.0, 3.0, 1.5, 2.5, 3.5],
            c: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xBB00,
            TableId(200),
            DataRef::Table(table_id),
            MarkDef::Point,
        )
        .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("y"))
        .with_color(ChannelDef::nominal(ColumnId(2)).with_title("series"));

        let lowered = spec.lower(&scene).expect("lower point chart");
        assert!(lowered.program().is_some());
        assert_eq!(lowered.derived_tables().len(), 2);
        assert!(lowered.chart().legend.is_some());

        lowered.apply_to_scene(&mut scene).expect("apply program");
        let (_layout, diffs) = lowered
            .tick(&mut scene, &HeuristicTextMeasurer)
            .expect("tick point chart");
        assert!(diffs.iter().any(|diff| matches!(
            diff,
            MarkDiff::Enter {
                kind: MarkKind::Path,
                ..
            }
        )));
    }

    #[test]
    fn line_lowering_respects_sort_transform() {
        let mut scene = Scene::new();
        let table_id = TableId(30);
        let mut table = Table::new(table_id);
        table.row_keys = vec![10, 11, 12];
        table.data = Some(Box::new(TwoCols {
            a: vec![2.0, 0.0, 1.0],
            b: vec![20.0, 0.0, 10.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xCC00,
            TableId(300),
            DataRef::Table(table_id),
            MarkDef::Line,
        )
        .with_transform(TransformSpec::sort(
            ColumnId(0),
            SortOrder::Asc,
            vec![ColumnId(0), ColumnId(1)],
        ))
        .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("y"));

        let lowered = spec.lower(&scene).expect("lower line chart");
        lowered
            .apply_to_scene(&mut scene)
            .expect("apply line program");

        let sorted = scene
            .tables
            .get(&lowered.output_table())
            .expect("sorted table");
        let data = sorted.data.as_deref().expect("sorted data");
        assert_eq!(data.f64(0, ColumnId(0)), Some(0.0));
        assert_eq!(data.f64(1, ColumnId(0)), Some(1.0));
        assert_eq!(data.f64(2, ColumnId(0)), Some(2.0));

        let (_layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("line marks");
        assert!(
            marks
                .iter()
                .any(|mark| matches!(mark.encodings, vizir_core::MarkEncodings::Path(_)))
        );
    }

    #[test]
    fn area_lowering_emits_one_series_path() {
        let mut scene = Scene::new();
        let table_id = TableId(40);
        let mut table = Table::new(table_id);
        table.row_keys = vec![100, 101, 102];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![1.0, 3.0, 2.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xDD00,
            TableId(400),
            DataRef::Table(table_id),
            MarkDef::Area,
        )
        .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("y"));

        let lowered = spec.lower(&scene).expect("lower area chart");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("area series marks");
        assert_eq!(marks.len(), 1);
        assert!(matches!(
            marks[0].encodings,
            vizir_core::MarkEncodings::Path(_)
        ));
    }

    #[test]
    fn area_color_lowering_creates_series_paths_and_legend() {
        let mut scene = Scene::new();
        let table_id = TableId(50);
        let mut table = Table::new(table_id);
        table.row_keys = (0..6_u64).collect();
        table.data = Some(Box::new(ThreeCols {
            a: vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0],
            b: vec![1.0, 2.0, 1.5, 0.5, 1.0, 2.5],
            c: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xEE00,
            TableId(500),
            DataRef::Table(table_id),
            MarkDef::Area,
        )
        .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("y"))
        .with_color(ChannelDef::nominal(ColumnId(2)).with_title("series"));

        let lowered = spec.lower(&scene).expect("lower colored area chart");
        assert!(lowered.program().is_some());
        assert_eq!(lowered.derived_tables().len(), 2);
        assert!(lowered.chart().legend.is_some());

        lowered
            .apply_to_scene(&mut scene)
            .expect("apply colored area program");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("colored area series marks");
        assert_eq!(marks.len(), 2);
        assert!(
            marks
                .iter()
                .all(|mark| matches!(mark.encodings, vizir_core::MarkEncodings::Path(_)))
        );
    }

    #[test]
    fn ranged_area_lowering_uses_y2_without_forcing_zero() {
        let mut scene = Scene::new();
        let table_id = TableId(60);
        let mut table = Table::new(table_id);
        table.row_keys = vec![200, 201, 202];
        table.data = Some(Box::new(ThreeCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![5.0, 6.0, 7.0],
            c: vec![2.0, 3.0, 4.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xEF00,
            TableId(600),
            DataRef::Table(table_id),
            MarkDef::Area,
        )
        .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("hi"))
        .with_y2(ChannelDef::quantitative(ColumnId(2)).with_title("lo"));

        let lowered = spec.lower(&scene).expect("lower ranged area chart");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let y_scale = lowered
            .chart()
            .y_scale_continuous(layout.data)
            .expect("y scale");
        assert!(y_scale.domain_min() > 0.0);

        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("ranged area series marks");
        assert_eq!(marks.len(), 1);
        assert!(matches!(
            marks[0].encodings,
            vizir_core::MarkEncodings::Path(_)
        ));
    }

    #[test]
    fn ranged_area_lowering_uses_x2_and_y2_in_domains() {
        let mut scene = Scene::new();
        let table_id = TableId(70);
        let mut table = Table::new(table_id);
        table.row_keys = vec![300, 301, 302];
        table.data = Some(Box::new(FourCols {
            a: vec![1.0, 2.0, 3.0],
            b: vec![5.0, 6.0, 7.0],
            c: vec![0.0, 1.0, 2.0],
            d: vec![2.0, 3.0, 4.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xF000,
            TableId(700),
            DataRef::Table(table_id),
            MarkDef::Area,
        )
        .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("top"))
        .with_x2(ChannelDef::quantitative(ColumnId(2)).with_title("x2"))
        .with_y2(ChannelDef::quantitative(ColumnId(3)).with_title("bottom"));

        let lowered = spec.lower(&scene).expect("lower paired ranged area chart");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let x_scale = lowered
            .chart()
            .x_scale_continuous(layout.data)
            .expect("x scale");
        let y_scale = lowered
            .chart()
            .y_scale_continuous(layout.data)
            .expect("y scale");
        assert_eq!(x_scale.domain_min(), 0.0);
        assert_eq!(x_scale.domain_max(), 3.0);
        assert_eq!(y_scale.domain_min(), 2.0);
        assert_eq!(y_scale.domain_max(), 7.0);

        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("paired ranged area series marks");
        assert_eq!(marks.len(), 1);
        assert!(matches!(
            marks[0].encodings,
            vizir_core::MarkEncodings::Path(_)
        ));
    }
}
