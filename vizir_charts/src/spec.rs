// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Experimental authored-spec and lowering support.
//!
//! This module is the first compilation seam between a Vega-Lite-like authored chart description
//! and the existing `vizir_core` / `vizir_transforms` / `vizir_charts` runtime pieces.
//!
//! The supported slice is intentionally small:
//! - one unit chart or a narrow shared-plot layer spec,
//! - one input table already present in a [`vizir_core::Scene`],
//! - a small transform subset,
//! - `bar`, `line`, `point`, `area`, and `text` marks,
//! - `x`, `x2`, `y`, `y2`, `color`, `size`, `shape`, and `text` channels,
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
use vizir_core::{
    ColumnId, InputRef, Mark, MarkDiff, MarkId, Scene, TableId, TextAnchor, TextBaseline,
};
use vizir_transforms::{
    AggregateField, AggregateOp, Predicate, Program, SceneExecutionError, SortOrder, StackOffset,
    TableFrame, TableFrameError, Transform,
};

#[cfg(not(feature = "std"))]
use crate::float::FloatExt;

use crate::{
    AxisSpec, ChartLayout, ChartLayoutSpec, ChartSpec, GridStyle, LegendItem, LegendOrient,
    LegendPlacement, LegendSwatchesSpec, ScaleBandSpec, ScaleLinearSpec, ScaleTimeSpec, Size,
    StrokeStyle, Symbol, TextMeasurer, TitleSpec, format_time_seconds,
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
    /// A text label positioned by x/y and formatted from a numeric field.
    Text,
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
    size: Option<ChannelDef>,
    shape: Option<ChannelDef>,
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

    /// Sets the size channel.
    pub fn with_size(mut self, size: ChannelDef) -> Self {
        self.size = Some(size);
        self
    }

    /// Sets the shape channel.
    pub fn with_shape(mut self, shape: ChannelDef) -> Self {
        self.shape = Some(shape);
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

    fn size(&self) -> Option<&ChannelDef> {
        self.size.as_ref()
    }

    fn shape(&self) -> Option<&ChannelDef> {
        self.shape.as_ref()
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

    /// Sets the size channel.
    pub fn with_size_channel(mut self, size: ChannelDef) -> Self {
        self.encoding = self.encoding.with_size(size);
        self
    }

    /// Sets the shape channel.
    pub fn with_shape(mut self, shape: ChannelDef) -> Self {
        self.encoding = self.encoding.with_shape(shape);
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
        let size = self.encoding.size();
        let shape = self.encoding.shape();
        let text = self.encoding.text();
        if self.mark != MarkDef::Text && text.is_some() {
            return Err(LoweringError::Unsupported(
                "text channels currently only lower on text marks",
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
        if let Some(text) = text
            && text.aggregate().is_some()
        {
            return Err(LoweringError::Unsupported(
                "aggregate on the text channel is not supported in the experimental lowering slice",
            ));
        }
        if let Some(size) = size
            && size.aggregate().is_some()
        {
            return Err(LoweringError::Unsupported(
                "aggregate on the size channel is not supported in the experimental lowering slice",
            ));
        }
        if let Some(shape) = shape
            && shape.aggregate().is_some()
        {
            return Err(LoweringError::Unsupported(
                "aggregate on the shape channel is not supported in the experimental lowering slice",
            ));
        }
        if color.is_some() && self.mark == MarkDef::Bar {
            return Err(LoweringError::Unsupported(
                "categorical color splitting is not supported for bar marks yet",
            ));
        }
        if color.is_some() && self.mark == MarkDef::Text {
            return Err(LoweringError::Unsupported(
                "categorical color splitting is not supported for text marks yet",
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
        if (size.is_some() || shape.is_some()) && self.mark != MarkDef::Point {
            return Err(LoweringError::Unsupported(
                "size and shape channels are currently only supported for point marks",
            ));
        }
        if let Some(size) = size
            && size.kind() != FieldKind::Quantitative
        {
            return Err(LoweringError::Unsupported(
                "point size currently requires a quantitative channel",
            ));
        }
        if let Some(shape) = shape
            && shape.kind() == FieldKind::Temporal
        {
            return Err(LoweringError::Unsupported(
                "point shape does not support temporal channels in the experimental lowering slice",
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
            MarkDef::Text => {
                if text.is_none() {
                    return Err(LoweringError::MissingChannel("text"));
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
            required_columns(x, x2, lowered_y_field, y2, color, size, shape, text),
        )?;

        let point_size_domain = size
            .map(|size| infer_frame_domain_pair(&preview_frame, size.field(), None, "size"))
            .transpose()?;
        let point_shape_map = shape
            .map(|shape| build_shape_map(&preview_frame, shape.field()))
            .unwrap_or_default();

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
                    columns: series_columns(
                        x,
                        x2,
                        lowered_y_field,
                        y2,
                        Some(color),
                        size,
                        shape,
                        text,
                    ),
                });
                if matches!(self.mark, MarkDef::Line | MarkDef::Area) {
                    p.push(Transform::Sort {
                        input: output,
                        output,
                        by: x.field(),
                        order: SortOrder::Asc,
                        columns: series_columns(
                            x,
                            x2,
                            lowered_y_field,
                            y2,
                            None,
                            size,
                            shape,
                            text,
                        ),
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
                        id_base: self.id_base.wrapping_add(0x1_000 + index as u64),
                        table: output,
                        x: x.field(),
                        y: lowered_y_field,
                        default_symbol: Symbol::Circle,
                        shape: shape.map(ChannelDef::field),
                        shape_map: point_shape_map.clone(),
                        default_size: 6.0,
                        size: size.map(ChannelDef::field),
                        size_domain: point_size_domain,
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
                    MarkDef::Text => SeriesLayer::Text(TextLayer {
                        id_base: self.id_base.wrapping_add(0x2_000 + index as u64),
                        table: output,
                        x: x.field(),
                        x_kind: x.kind(),
                        y: lowered_y_field,
                        text: text
                            .expect("text mark validates text channel before color lowering")
                            .field(),
                        text_kind: text
                            .expect("text mark validates text channel before color lowering")
                            .kind(),
                        fill,
                    }),
                });
            }
        } else {
            series_layers.push(match self.mark {
                MarkDef::Bar => SeriesLayer::Bar(BarLayer {
                    id_base: self.id_base.wrapping_add(0x1_000),
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
                    id_base: self.id_base.wrapping_add(0x1_100),
                    table: current_table,
                    x: x.field(),
                    y: lowered_y_field,
                    default_symbol: Symbol::Circle,
                    shape: shape.map(ChannelDef::field),
                    shape_map: point_shape_map,
                    default_size: 6.0,
                    size: size.map(ChannelDef::field),
                    size_domain: point_size_domain,
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
                MarkDef::Text => SeriesLayer::Text(TextLayer {
                    id_base: self.id_base.wrapping_add(0x1_200),
                    table: current_table,
                    x: x.field(),
                    x_kind: x.kind(),
                    y: lowered_y_field,
                    text: text
                        .expect("text mark validates text channel before series lowering")
                        .field(),
                    text_kind: text
                        .expect("text mark validates text channel before series lowering")
                        .kind(),
                    fill: Brush::Solid(css::BLACK),
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

/// One child entry in a narrow shared-plot layer spec.
///
/// A child always contributes exactly one mark kind and may override selected channels from the
/// parent [`LayerSpec`].
#[derive(Clone, Debug, PartialEq)]
pub struct LayerChildSpec {
    mark: MarkDef,
    transforms: Vec<TransformSpec>,
    encoding: EncodingSet,
}

impl LayerChildSpec {
    /// Creates a new layer child for the given mark.
    pub fn new(mark: MarkDef) -> Self {
        Self {
            mark,
            transforms: Vec::new(),
            encoding: EncodingSet::new(),
        }
    }

    /// Appends a child-local transform.
    pub fn with_transform(mut self, transform: TransformSpec) -> Self {
        self.transforms.push(transform);
        self
    }

    /// Replaces the child override encoding set.
    pub fn with_encoding(mut self, encoding: EncodingSet) -> Self {
        self.encoding = encoding;
        self
    }

    /// Sets the child x override.
    pub fn with_x(mut self, x: ChannelDef) -> Self {
        self.encoding = self.encoding.with_x(x);
        self
    }

    /// Sets the child x2 override.
    pub fn with_x2(mut self, x2: ChannelDef) -> Self {
        self.encoding = self.encoding.with_x2(x2);
        self
    }

    /// Sets the child y override.
    pub fn with_y(mut self, y: ChannelDef) -> Self {
        self.encoding = self.encoding.with_y(y);
        self
    }

    /// Sets the child y2 override.
    pub fn with_y2(mut self, y2: ChannelDef) -> Self {
        self.encoding = self.encoding.with_y2(y2);
        self
    }

    /// Sets the child color override.
    pub fn with_color(mut self, color: ChannelDef) -> Self {
        self.encoding = self.encoding.with_color(color);
        self
    }

    /// Sets the child size override.
    pub fn with_size_channel(mut self, size: ChannelDef) -> Self {
        self.encoding = self.encoding.with_size(size);
        self
    }

    /// Sets the child shape override.
    pub fn with_shape(mut self, shape: ChannelDef) -> Self {
        self.encoding = self.encoding.with_shape(shape);
        self
    }

    /// Sets the child text override.
    pub fn with_text(mut self, text: ChannelDef) -> Self {
        self.encoding = self.encoding.with_text(text);
        self
    }

    fn mark(&self) -> MarkDef {
        self.mark
    }
}

/// A narrow shared-plot layered chart.
///
/// Every child mark shares the same data source and chart shell. Shared transforms run for every
/// child, and each child may append its own transform chain and selected channel overrides. This
/// is an intentionally small composition slice for common overlays such as line + point.
#[derive(Clone, Debug)]
pub struct LayerSpec {
    id_base: u64,
    derived_table_base: TableId,
    data: DataRef,
    transforms: Vec<TransformSpec>,
    encoding: EncodingSet,
    children: Vec<LayerChildSpec>,
    width: f64,
    height: f64,
    title: Option<String>,
}

impl LayerSpec {
    /// Creates a new empty layer spec.
    pub fn new(id_base: u64, derived_table_base: TableId, data: DataRef) -> Self {
        Self {
            id_base,
            derived_table_base,
            data,
            transforms: Vec::new(),
            encoding: EncodingSet::new(),
            children: Vec::new(),
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

    /// Sets the size channel.
    pub fn with_size_channel(mut self, size: ChannelDef) -> Self {
        self.encoding = self.encoding.with_size(size);
        self
    }

    /// Sets the shape channel.
    pub fn with_shape(mut self, shape: ChannelDef) -> Self {
        self.encoding = self.encoding.with_shape(shape);
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

    /// Appends a shared-plot mark layer.
    pub fn with_mark(mut self, mark: MarkDef) -> Self {
        self.children.push(LayerChildSpec::new(mark));
        self
    }

    /// Appends a shared-plot child with mark-specific encoding overrides.
    pub fn with_child(mut self, child: LayerChildSpec) -> Self {
        self.children.push(child);
        self
    }

    /// Lowers the authored layer spec into a shared chart/transform/series plan.
    pub fn lower(&self, scene: &Scene) -> Result<LoweredLayer, LoweringError> {
        if self.children.is_empty() {
            return Err(LoweringError::Unsupported(
                "layer lowering requires at least one mark",
            ));
        }
        if self
            .children
            .iter()
            .any(|child| child.mark() == MarkDef::Bar)
            && self
                .children
                .iter()
                .any(|child| !matches!(child.mark(), MarkDef::Bar | MarkDef::Text))
        {
            return Err(LoweringError::Unsupported(
                "bar layering is only supported with bar/text marks in the experimental slice",
            ));
        }

        let base_index = base_layer_mark_index(&self.children);
        let base_unit = self.layer_child_unit(base_index);
        let base_lowered = base_unit.lower(scene)?;

        let mut series_layers = base_lowered.series_layers.clone();
        let mut derived_tables = base_lowered.derived_tables.clone();
        let mut combined_program = base_lowered.program.clone();
        for index in 0..self.children.len() {
            if index == base_index {
                continue;
            }
            let child = self.layer_child_unit(index);
            let lowered = child.lower(scene)?;
            series_layers.extend(lowered.series_layers);
            extend_unique_tables(&mut derived_tables, &lowered.derived_tables);
            merge_programs(&mut combined_program, lowered.program);
        }

        Ok(LoweredLayer {
            input_table: base_lowered.input_table,
            output_table: base_lowered.output_table,
            derived_tables,
            program: combined_program,
            chart: base_lowered.chart,
            series_layers,
        })
    }

    /// Lowers the authored layer spec and applies the derived tables to the scene.
    pub fn lower_into_scene(&self, scene: &mut Scene) -> Result<LoweredLayer, LoweringError> {
        let lowered = self.lower(scene)?;
        lowered.apply_to_scene(scene)?;
        Ok(lowered)
    }

    fn layer_child_unit(&self, index: usize) -> UnitSpec {
        let child = &self.children[index];
        UnitSpec {
            id_base: child_layer_id_base(self.id_base, index),
            derived_table_base: child_layer_table_base(self.derived_table_base, index),
            data: self.data,
            transforms: layer_child_transforms(&self.transforms, &child.transforms),
            mark: child.mark(),
            encoding: merge_layer_encoding(&self.encoding, &child.encoding, child.mark()),
            width: self.width,
            height: self.height,
            title: self.title.clone(),
        }
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

/// A lowered shared-plot layered chart plan.
#[derive(Clone, Debug)]
pub struct LoweredLayer {
    input_table: TableId,
    output_table: TableId,
    derived_tables: Vec<TableId>,
    program: Option<Program>,
    chart: ChartSpec,
    series_layers: Vec<SeriesLayer>,
}

impl LoweredLayer {
    /// Returns the original input table referenced by the authored layer spec.
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

    /// Returns the lowered transform program, if the layer spec needs derived tables.
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
    Text(TextLayer),
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
            Self::Text(layer) => layer.marks(scene, chart, plot),
        }
    }
}

#[derive(Clone, Debug)]
struct BarLayer {
    id_base: u64,
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
        let id_base = self.id_base;
        let table_id = self.table;
        let y_col = self.y;
        let baseline = self.baseline;
        let fill = self.fill.clone();
        let band = chart
            .x_axis()
            .ok_or(LoweringError::MissingChannel("x"))?
            .scale_band(plot);
        let y_scale = chart
            .y_scale_continuous(plot)
            .ok_or(LoweringError::MissingChannel("y"))?;
        Ok(row_keys
            .iter()
            .copied()
            .enumerate()
            .map(|(row, row_key)| {
                let y0 = y_scale.map(baseline);
                Mark::builder(layer_row_mark_id(id_base, row_key))
                    .rect()
                    .z_index(crate::z_order::SERIES_FILL)
                    .x_const(band.x(row))
                    .y_compute(
                        [InputRef::TableCol {
                            table: table_id,
                            col: y_col,
                        }],
                        move |ctx, _| {
                            let v = ctx.table_f64(table_id, row, y_col).unwrap_or(baseline);
                            y_scale.map(v).min(y0)
                        },
                    )
                    .w_const(band.band_width())
                    .h_compute(
                        [InputRef::TableCol {
                            table: table_id,
                            col: y_col,
                        }],
                        move |ctx, _| {
                            let v = ctx.table_f64(table_id, row, y_col).unwrap_or(baseline);
                            (y_scale.map(v) - y0).abs()
                        },
                    )
                    .fill_brush_const(fill.clone())
                    .build()
            })
            .collect())
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
    id_base: u64,
    table: TableId,
    x: ColumnId,
    y: ColumnId,
    default_symbol: Symbol,
    shape: Option<ColumnId>,
    shape_map: Vec<(u64, Symbol)>,
    default_size: f64,
    size: Option<ColumnId>,
    size_domain: Option<(f64, f64)>,
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
        let id_base = self.id_base;
        let table_id = self.table;
        let x_col = self.x;
        let y_col = self.y;
        let default_size = self.default_size;
        let size_col = self.size;
        let size_domain = self.size_domain;
        let default_symbol = self.default_symbol;
        let shape_col = self.shape;
        let shape_map = self.shape_map.clone();
        let fill = self.fill.clone();
        let x_scale = chart
            .x_scale_continuous(plot)
            .ok_or(LoweringError::MissingChannel("x"))?;
        let y_scale = chart
            .y_scale_continuous(plot)
            .ok_or(LoweringError::MissingChannel("y"))?;
        Ok(row_keys
            .iter()
            .copied()
            .enumerate()
            .map(|(row, row_key)| {
                let size = size_col
                    .and_then(|col| table.data.as_deref().and_then(|data| data.f64(row, col)))
                    .map(|value| point_size_for_value(value, size_domain, default_size))
                    .unwrap_or(default_size);
                let symbol = shape_col
                    .and_then(|col| table.data.as_deref().and_then(|data| data.f64(row, col)))
                    .map(|value| symbol_for_shape_value(value, &shape_map, default_symbol))
                    .unwrap_or(default_symbol);

                if symbol == Symbol::Square {
                    Mark::builder(layer_row_mark_id(id_base, row_key))
                        .rect()
                        .z_index(crate::z_order::SERIES_POINTS)
                        .x_compute(
                            [InputRef::TableCol {
                                table: table_id,
                                col: x_col,
                            }],
                            move |ctx, _| {
                                x_scale.map(ctx.table_f64(table_id, row, x_col).unwrap_or(0.0))
                                    - size / 2.0
                            },
                        )
                        .y_compute(
                            [InputRef::TableCol {
                                table: table_id,
                                col: y_col,
                            }],
                            move |ctx, _| {
                                y_scale.map(ctx.table_f64(table_id, row, y_col).unwrap_or(0.0))
                                    - size / 2.0
                            },
                        )
                        .w_const(size)
                        .h_const(size)
                        .fill_brush_const(fill.clone())
                        .build()
                } else {
                    Mark::builder(layer_row_mark_id(id_base, row_key))
                        .path()
                        .z_index(crate::z_order::SERIES_POINTS)
                        .path_compute(
                            [
                                InputRef::TableCol {
                                    table: table_id,
                                    col: x_col,
                                },
                                InputRef::TableCol {
                                    table: table_id,
                                    col: y_col,
                                },
                            ],
                            move |ctx, _| {
                                let x =
                                    x_scale.map(ctx.table_f64(table_id, row, x_col).unwrap_or(0.0));
                                let y =
                                    y_scale.map(ctx.table_f64(table_id, row, y_col).unwrap_or(0.0));
                                symbol.path(x, y, size)
                            },
                        )
                        .fill_brush_const(fill.clone())
                        .stroke_width_const(0.0)
                        .build()
                }
            })
            .collect())
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

#[derive(Clone, Debug)]
struct TextLayer {
    id_base: u64,
    table: TableId,
    x: ColumnId,
    x_kind: FieldKind,
    y: ColumnId,
    text: ColumnId,
    text_kind: FieldKind,
    fill: Brush,
}

impl TextLayer {
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
        let id_base = self.id_base;
        let table_id = self.table;
        let x_col = self.x;
        let y_col = self.y;
        let text_col = self.text;
        let text_kind = self.text_kind;
        let fill = self.fill.clone();
        let y_scale = chart
            .y_scale_continuous(plot)
            .ok_or(LoweringError::MissingChannel("y"))?;

        Ok(match self.x_kind {
            FieldKind::Quantitative | FieldKind::Temporal => {
                let x_scale = chart
                    .x_scale_continuous(plot)
                    .ok_or(LoweringError::MissingChannel("x"))?;
                row_keys
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(row, row_key)| {
                        Mark::builder(layer_row_mark_id(id_base, row_key))
                            .text()
                            .z_index(crate::z_order::SERIES_LABELS)
                            .x_compute(
                                [InputRef::TableCol {
                                    table: table_id,
                                    col: x_col,
                                }],
                                move |ctx, _| {
                                    x_scale.map(ctx.table_f64(table_id, row, x_col).unwrap_or(0.0))
                                },
                            )
                            .y_compute(
                                [InputRef::TableCol {
                                    table: table_id,
                                    col: y_col,
                                }],
                                move |ctx, _| {
                                    y_scale.map(ctx.table_f64(table_id, row, y_col).unwrap_or(0.0))
                                        - 4.0
                                },
                            )
                            .text_compute(
                                [InputRef::TableCol {
                                    table: table_id,
                                    col: text_col,
                                }],
                                move |ctx, _| {
                                    format_channel_value(
                                        ctx.table_f64(table_id, row, text_col).unwrap_or(f64::NAN),
                                        text_kind,
                                    )
                                },
                            )
                            .font_size_const(10.0)
                            .fill_brush_const(fill.clone())
                            .text_anchor(TextAnchor::Middle)
                            .text_baseline(TextBaseline::Ideographic)
                            .build()
                    })
                    .collect()
            }
            FieldKind::Ordinal | FieldKind::Nominal => {
                let band = chart
                    .x_axis()
                    .ok_or(LoweringError::MissingChannel("x"))?
                    .scale_band(plot);
                let band_width = band.band_width();
                row_keys
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(row, row_key)| {
                        Mark::builder(layer_row_mark_id(id_base, row_key))
                            .text()
                            .z_index(crate::z_order::SERIES_LABELS)
                            .x_const(band.x(row) + 0.5 * band_width)
                            .y_compute(
                                [InputRef::TableCol {
                                    table: table_id,
                                    col: y_col,
                                }],
                                move |ctx, _| {
                                    y_scale.map(ctx.table_f64(table_id, row, y_col).unwrap_or(0.0))
                                        - 4.0
                                },
                            )
                            .text_compute(
                                [InputRef::TableCol {
                                    table: table_id,
                                    col: text_col,
                                }],
                                move |ctx, _| {
                                    format_channel_value(
                                        ctx.table_f64(table_id, row, text_col).unwrap_or(f64::NAN),
                                        text_kind,
                                    )
                                },
                            )
                            .font_size_const(10.0)
                            .fill_brush_const(fill.clone())
                            .text_anchor(TextAnchor::Middle)
                            .text_baseline(TextBaseline::Ideographic)
                            .build()
                    })
                    .collect()
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
        MarkDef::Area | MarkDef::Line | MarkDef::Point | MarkDef::Text => expand_domain(domain),
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
    size: Option<&ChannelDef>,
    shape: Option<&ChannelDef>,
    text: Option<&ChannelDef>,
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
    if let Some(size) = size {
        push_unique_col(&mut out, size.field());
    }
    if let Some(shape) = shape {
        push_unique_col(&mut out, shape.field());
    }
    if let Some(text) = text {
        push_unique_col(&mut out, text.field());
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
    size: Option<&ChannelDef>,
    shape: Option<&ChannelDef>,
    text: Option<&ChannelDef>,
) -> Vec<ColumnId> {
    required_columns(x, x2, y_field, y2, color, size, shape, text)
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

fn build_shape_map(frame: &TableFrame, col: ColumnId) -> Vec<(u64, Symbol)> {
    const PALETTE: [Symbol; 4] = [
        Symbol::Circle,
        Symbol::Square,
        Symbol::Diamond,
        Symbol::Triangle,
    ];
    distinct_values(frame, col)
        .into_iter()
        .enumerate()
        .map(|(index, value)| (value.to_bits(), PALETTE[index % PALETTE.len()]))
        .collect()
}

fn symbol_for_shape_value(value: f64, shape_map: &[(u64, Symbol)], default: Symbol) -> Symbol {
    if !value.is_finite() {
        return default;
    }
    shape_map
        .iter()
        .find(|(bits, _)| *bits == value.to_bits())
        .map_or(default, |(_, symbol)| *symbol)
}

fn point_size_for_value(value: f64, domain: Option<(f64, f64)>, default: f64) -> f64 {
    let Some((min, max)) = domain else {
        return default;
    };
    if !value.is_finite() {
        return default;
    }
    let (min, max) = expand_domain((min, max));
    if min >= max {
        return default;
    }
    let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
    4.0 + t * 8.0
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

fn merge_layer_encoding(
    shared: &EncodingSet,
    overrides: &EncodingSet,
    mark: MarkDef,
) -> EncodingSet {
    let mut out = EncodingSet {
        x: overrides.x.clone().or_else(|| shared.x.clone()),
        x2: overrides.x2.clone().or_else(|| shared.x2.clone()),
        y: overrides.y.clone().or_else(|| shared.y.clone()),
        y2: overrides.y2.clone().or_else(|| shared.y2.clone()),
        color: overrides.color.clone().or_else(|| shared.color.clone()),
        size: overrides.size.clone().or_else(|| shared.size.clone()),
        shape: overrides.shape.clone().or_else(|| shared.shape.clone()),
        text: overrides.text.clone().or_else(|| shared.text.clone()),
    };
    if mark != MarkDef::Area {
        out.x2 = None;
        out.y2 = None;
    }
    out
}

fn layer_child_transforms(shared: &[TransformSpec], child: &[TransformSpec]) -> Vec<TransformSpec> {
    let mut out = Vec::with_capacity(shared.len() + child.len());
    out.extend(shared.iter().cloned());
    out.extend(child.iter().cloned());
    out
}

fn child_layer_id_base(id_base: u64, index: usize) -> u64 {
    id_base.wrapping_add((index as u64).wrapping_mul(0x100_000))
}

fn child_layer_table_base(base: TableId, index: usize) -> TableId {
    let index = u32::try_from(index).unwrap_or(u32::MAX);
    TableId(base.0.wrapping_add(index.wrapping_mul(0x100)))
}

fn base_layer_mark_index(children: &[LayerChildSpec]) -> usize {
    if let Some(index) = children
        .iter()
        .position(|child| child.mark() == MarkDef::Bar)
    {
        return index;
    }
    if let Some(index) = children
        .iter()
        .position(|child| child.mark() == MarkDef::Area)
    {
        return index;
    }
    0
}

fn extend_unique_tables(out: &mut Vec<TableId>, tables: &[TableId]) {
    for table in tables {
        if out.iter().all(|existing| *existing != *table) {
            out.push(*table);
        }
    }
}

fn merge_programs(out: &mut Option<Program>, program: Option<Program>) {
    let Some(program) = program else {
        return;
    };
    let dst = out.get_or_insert_with(Program::new);
    for transform in program.transforms().iter().cloned() {
        dst.push(transform);
    }
}

fn layer_row_mark_id(id_base: u64, row_key: u64) -> MarkId {
    MarkId::from_raw(
        id_base ^ row_key.rotate_left(17) ^ row_key.wrapping_mul(0xD6E8_FEB8_6659_FD93),
    )
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
    fn point_size_mapping_uses_visual_range() {
        assert_eq!(point_size_for_value(f64::NAN, Some((1.0, 5.0)), 6.0), 6.0);
        assert!((point_size_for_value(1.0, Some((1.0, 5.0)), 6.0) - 4.0).abs() < 1e-9);
        assert!((point_size_for_value(5.0, Some((1.0, 5.0)), 6.0) - 12.0).abs() < 1e-9);
    }

    #[test]
    fn point_shape_size_lowering_emits_mixed_symbols() {
        let mut scene = Scene::new();
        let table_id = TableId(21);
        let mut table = Table::new(table_id);
        table.row_keys = vec![20, 21, 22, 23];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0, 3.0],
            b: vec![1.0, 2.0, 3.0, 2.5],
            c: vec![1.0, 4.0, 2.0, 7.0],
            d: vec![0.0, 1.0, 2.0, 3.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xBC00,
            TableId(210),
            DataRef::Table(table_id),
            MarkDef::Point,
        )
        .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("y"))
        .with_size_channel(ChannelDef::quantitative(ColumnId(2)).with_title("size"))
        .with_shape(ChannelDef::nominal(ColumnId(3)).with_title("shape"));

        let lowered = spec.lower(&scene).expect("lower point shape/size");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("point marks");
        assert_eq!(marks.len(), 4);
        assert!(marks.iter().any(|mark| mark.kind == MarkKind::Rect));
        assert!(marks.iter().any(|mark| mark.kind == MarkKind::Path));
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
    fn text_mark_lowering_formats_numeric_labels() {
        let mut scene = Scene::new();
        let table_id = TableId(41);
        let mut table = Table::new(table_id);
        table.row_keys = vec![301, 302, 303];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![3.0, 5.0, 4.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xDD80,
            TableId(410),
            DataRef::Table(table_id),
            MarkDef::Text,
        )
        .with_x(ChannelDef::ordinal(ColumnId(0)).with_title("category"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("value"))
        .with_text(ChannelDef::quantitative(ColumnId(1)));

        let lowered = spec.lower(&scene).expect("lower text mark");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("text marks");
        assert_eq!(marks.len(), 3);
        assert!(marks.iter().all(|mark| mark.kind == MarkKind::Text));
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

    #[test]
    fn layer_lowering_combines_line_and_point_marks() {
        let mut scene = Scene::new();
        let table_id = TableId(80);
        let mut table = Table::new(table_id);
        table.row_keys = vec![400, 401, 402];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![1.0, 2.0, 3.0],
        }));
        scene.insert_table(table);

        let spec = LayerSpec::new(0xF100, TableId(800), DataRef::Table(table_id))
            .with_title("line + point")
            .with_mark(MarkDef::Line)
            .with_mark(MarkDef::Point)
            .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
            .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("y"));

        let lowered = spec.lower(&scene).expect("lower layered chart");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("layered series marks");
        assert_eq!(marks.len(), 4);
    }

    #[test]
    fn layer_child_overrides_can_change_y_channels_per_mark() {
        let mut scene = Scene::new();
        let table_id = TableId(81);
        let mut table = Table::new(table_id);
        table.row_keys = vec![500, 501, 502];
        table.data = Some(Box::new(ThreeCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![3.0, 4.0, 5.0],
            c: vec![1.0, 2.0, 2.5],
        }));
        scene.insert_table(table);

        let spec = LayerSpec::new(0xF180, TableId(810), DataRef::Table(table_id))
            .with_title("area + line")
            .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
            .with_child(
                LayerChildSpec::new(MarkDef::Area)
                    .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("top"))
                    .with_y2(ChannelDef::quantitative(ColumnId(2))),
            )
            .with_child(
                LayerChildSpec::new(MarkDef::Line)
                    .with_y(ChannelDef::quantitative(ColumnId(2)).with_title("line")),
            );

        let lowered = spec.lower(&scene).expect("lower overridden layer");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("overridden layer marks");
        assert_eq!(marks.len(), 2);
        assert!(
            marks
                .iter()
                .all(|mark| matches!(mark.encodings, vizir_core::MarkEncodings::Path(_)))
        );
    }

    #[test]
    fn layer_child_transforms_merge_into_combined_program() {
        let mut scene = Scene::new();
        let table_id = TableId(82);
        let mut table = Table::new(table_id);
        table.row_keys = vec![600, 601, 602];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![3.0, 4.0, 5.0],
        }));
        scene.insert_table(table);

        let spec = LayerSpec::new(0xF1A0, TableId(820), DataRef::Table(table_id))
            .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
            .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("y"))
            .with_mark(MarkDef::Line)
            .with_child(
                LayerChildSpec::new(MarkDef::Point).with_transform(TransformSpec::filter(
                    Predicate {
                        col: ColumnId(0),
                        op: vizir_transforms::CompareOp::Ge,
                        value: 1.0,
                    },
                    vec![ColumnId(0), ColumnId(1)],
                )),
            );

        let lowered = spec
            .lower_into_scene(&mut scene)
            .expect("lower transformed layer");
        assert_eq!(
            lowered
                .program()
                .expect("combined program")
                .transforms()
                .len(),
            1
        );
        assert_eq!(lowered.derived_tables().len(), 1);

        let filtered = scene
            .tables
            .get(&lowered.derived_tables()[0])
            .expect("filtered child table");
        assert_eq!(filtered.row_keys, vec![601, 602]);
    }

    #[test]
    fn bar_text_layering_is_supported() {
        let mut scene = Scene::new();
        let table_id = TableId(83);
        let mut table = Table::new(table_id);
        table.row_keys = vec![700, 701, 702];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![3.0, 4.0, 2.0],
        }));
        scene.insert_table(table);

        let spec = LayerSpec::new(0xF1B0, TableId(830), DataRef::Table(table_id))
            .with_x(ChannelDef::ordinal(ColumnId(0)).with_title("category"))
            .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("value"))
            .with_mark(MarkDef::Bar)
            .with_child(
                LayerChildSpec::new(MarkDef::Text).with_text(ChannelDef::quantitative(ColumnId(1))),
            );

        let lowered = spec.lower(&scene).expect("lower bar + text layer");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("bar + text marks");
        assert!(marks.iter().any(|mark| mark.kind == MarkKind::Rect));
        assert!(marks.iter().any(|mark| mark.kind == MarkKind::Text));
    }

    #[test]
    fn layered_domains_follow_the_base_child() {
        let mut scene = Scene::new();
        let table_id = TableId(84);
        let mut table = Table::new(table_id);
        table.row_keys = vec![800, 801, 802];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![3.0, 4.0, 5.0],
            c: vec![1.0, 1.5, 2.0],
            d: vec![6.0, 7.0, 8.0],
        }));
        scene.insert_table(table);

        let spec = LayerSpec::new(0xF1C0, TableId(840), DataRef::Table(table_id))
            .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
            .with_child(
                LayerChildSpec::new(MarkDef::Area)
                    .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("band"))
                    .with_y2(ChannelDef::quantitative(ColumnId(2))),
            )
            .with_child(
                LayerChildSpec::new(MarkDef::Line)
                    .with_y(ChannelDef::quantitative(ColumnId(3)).with_title("line")),
            );

        let lowered = spec.lower(&scene).expect("lower layered domains");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let y_scale = lowered
            .chart()
            .y_scale_continuous(layout.data)
            .expect("y scale");
        assert_eq!(y_scale.domain_min(), 1.0);
        assert_eq!(y_scale.domain_max(), 5.0);
    }

    #[test]
    fn layer_lowering_rejects_bar_with_non_bar_marks() {
        let scene = Scene::new();
        let spec = LayerSpec::new(0xF200, TableId(900), DataRef::Table(TableId(1)))
            .with_mark(MarkDef::Bar)
            .with_mark(MarkDef::Line)
            .with_x(ChannelDef::ordinal(ColumnId(0)))
            .with_y(ChannelDef::quantitative(ColumnId(1)));

        let err = spec.lower(&scene).expect_err("mixed bar layer should fail");
        assert!(matches!(
            err,
            LoweringError::Unsupported(message)
                if message.contains("bar layering")
        ));
    }
}
