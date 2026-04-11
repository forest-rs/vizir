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
//! - `bar`, `line`, `point`, `area`, `rule`, and `text` marks,
//! - `x`, `x2`, `y`, `y2`, `color`, `size`, `shape`, `opacity`, `stroke`, `strokeWidth`, `order`, `detail`, and `text` channels,
//! - optional chart titles.
//!
//! It is not a JSON parser and not a full Vega/Vega-Lite implementation.

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use kurbo::{Affine, BezPath, Rect};
use peniko::Brush;
use peniko::color::palette::css;
use vizir_core::{
    ColumnId, Encoding, InputRef, Mark, MarkDiff, MarkEncodings, MarkId, PathEncodings,
    RectEncodings, Scene, TableId, TextAnchor, TextBaseline, TextEncodings,
};
use vizir_transforms::{
    AggregateField, AggregateOp, CalculateExpr, CalculateOperand, Predicate, Program,
    SceneExecutionError, SortOrder, StackOffset, TableFrame, TableFrameError, Transform,
    WindowField,
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
    /// A full-span threshold line from either x or y.
    Rule,
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
    opacity: Option<ChannelDef>,
    stroke: Option<ChannelDef>,
    stroke_width: Option<ChannelDef>,
    order: Option<ChannelDef>,
    detail: Option<ChannelDef>,
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

    /// Sets the opacity channel.
    pub fn with_opacity(mut self, opacity: ChannelDef) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// Sets the stroke channel.
    pub fn with_stroke(mut self, stroke: ChannelDef) -> Self {
        self.stroke = Some(stroke);
        self
    }

    /// Sets the stroke width channel.
    pub fn with_stroke_width(mut self, stroke_width: ChannelDef) -> Self {
        self.stroke_width = Some(stroke_width);
        self
    }

    /// Sets the order channel.
    pub fn with_order(mut self, order: ChannelDef) -> Self {
        self.order = Some(order);
        self
    }

    /// Sets the detail channel.
    pub fn with_detail(mut self, detail: ChannelDef) -> Self {
        self.detail = Some(detail);
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

    fn opacity(&self) -> Option<&ChannelDef> {
        self.opacity.as_ref()
    }

    fn stroke(&self) -> Option<&ChannelDef> {
        self.stroke.as_ref()
    }

    fn stroke_width(&self) -> Option<&ChannelDef> {
        self.stroke_width.as_ref()
    }

    fn order(&self) -> Option<&ChannelDef> {
        self.order.as_ref()
    }

    fn detail(&self) -> Option<&ChannelDef> {
        self.detail.as_ref()
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
    Calculate {
        expr: CalculateExpr,
        output_col: ColumnId,
        columns: Vec<ColumnId>,
    },
    JoinAggregate {
        group_by: Vec<ColumnId>,
        fields: Vec<AggregateField>,
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
    Fold {
        fields: Vec<ColumnId>,
        output_key: ColumnId,
        output_value: ColumnId,
        columns: Vec<ColumnId>,
    },
    Window {
        group_by: Vec<ColumnId>,
        sort_by: ColumnId,
        sort_order: SortOrder,
        fields: Vec<WindowField>,
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

    /// Creates a narrow arithmetic calculate transform.
    pub fn calculate(expr: CalculateExpr, output_col: ColumnId, columns: Vec<ColumnId>) -> Self {
        Self {
            kind: TransformSpecKind::Calculate {
                expr,
                output_col,
                columns,
            },
        }
    }

    /// Creates a joinaggregate transform that writes grouped aggregates back per row.
    pub fn joinaggregate(
        group_by: Vec<ColumnId>,
        fields: Vec<AggregateField>,
        columns: Vec<ColumnId>,
    ) -> Self {
        Self {
            kind: TransformSpecKind::JoinAggregate {
                group_by,
                fields,
                columns,
            },
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

    /// Creates a narrow numeric fold transform.
    pub fn fold(
        fields: Vec<ColumnId>,
        output_key: ColumnId,
        output_value: ColumnId,
        columns: Vec<ColumnId>,
    ) -> Self {
        Self {
            kind: TransformSpecKind::Fold {
                fields,
                output_key,
                output_value,
                columns,
            },
        }
    }

    /// Creates a narrow window transform.
    pub fn window(
        group_by: Vec<ColumnId>,
        sort_by: ColumnId,
        sort_order: SortOrder,
        fields: Vec<WindowField>,
        columns: Vec<ColumnId>,
    ) -> Self {
        Self {
            kind: TransformSpecKind::Window {
                group_by,
                sort_by,
                sort_order,
                fields,
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

    /// Sets the opacity channel.
    pub fn with_opacity(mut self, opacity: ChannelDef) -> Self {
        self.encoding = self.encoding.with_opacity(opacity);
        self
    }

    /// Sets the stroke channel.
    pub fn with_stroke(mut self, stroke: ChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke(stroke);
        self
    }

    /// Sets the stroke width channel.
    pub fn with_stroke_width(mut self, stroke_width: ChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke_width(stroke_width);
        self
    }

    /// Sets the order channel.
    pub fn with_order(mut self, order: ChannelDef) -> Self {
        self.encoding = self.encoding.with_order(order);
        self
    }

    /// Sets the detail channel.
    pub fn with_detail(mut self, detail: ChannelDef) -> Self {
        self.encoding = self.encoding.with_detail(detail);
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
        if self.mark == MarkDef::Rule {
            return self.lower_rule(scene, input_table);
        }

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
        let opacity = self.encoding.opacity();
        let stroke = self.encoding.stroke();
        let stroke_width = self.encoding.stroke_width();
        let order = self.encoding.order();
        let detail = self.encoding.detail();
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
        if let Some(opacity) = opacity
            && opacity.aggregate().is_some()
        {
            return Err(LoweringError::Unsupported(
                "aggregate on the opacity channel is not supported in the experimental lowering slice",
            ));
        }
        if let Some(stroke) = stroke
            && stroke.aggregate().is_some()
        {
            return Err(LoweringError::Unsupported(
                "aggregate on the stroke channel is not supported in the experimental lowering slice",
            ));
        }
        if let Some(stroke_width) = stroke_width
            && stroke_width.aggregate().is_some()
        {
            return Err(LoweringError::Unsupported(
                "aggregate on the strokeWidth channel is not supported in the experimental lowering slice",
            ));
        }
        if let Some(order) = order
            && order.aggregate().is_some()
        {
            return Err(LoweringError::Unsupported(
                "aggregate on the order channel is not supported in the experimental lowering slice",
            ));
        }
        if let Some(detail) = detail
            && detail.aggregate().is_some()
        {
            return Err(LoweringError::Unsupported(
                "aggregate on the detail channel is not supported in the experimental lowering slice",
            ));
        }
        if color.is_some() && self.mark == MarkDef::Text {
            return Err(LoweringError::Unsupported(
                "categorical color splitting is not supported for text marks yet",
            ));
        }
        if x2.is_some() && self.mark != MarkDef::Area {
            return Err(LoweringError::Unsupported(
                "x2 is currently only supported for area marks",
            ));
        }
        if y2.is_some() && !matches!(self.mark, MarkDef::Area | MarkDef::Bar) {
            return Err(LoweringError::Unsupported(
                "y2 is currently only supported for area and bar marks",
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
        if let Some(opacity) = opacity
            && opacity.kind() != FieldKind::Quantitative
        {
            return Err(LoweringError::Unsupported(
                "opacity currently requires a quantitative channel",
            ));
        }
        if let Some(stroke) = stroke
            && !matches!(stroke.kind(), FieldKind::Ordinal | FieldKind::Nominal)
        {
            return Err(LoweringError::Unsupported(
                "point stroke currently requires an ordinal or nominal channel",
            ));
        }
        if let Some(stroke_width) = stroke_width
            && stroke_width.kind() != FieldKind::Quantitative
        {
            return Err(LoweringError::Unsupported(
                "point strokeWidth currently requires a quantitative channel",
            ));
        }
        if opacity.is_some() && matches!(self.mark, MarkDef::Line | MarkDef::Area) {
            return Err(LoweringError::Unsupported(
                "opacity is currently only supported for bar, point, and text marks",
            ));
        }
        if (stroke.is_some() || stroke_width.is_some()) && self.mark != MarkDef::Point {
            return Err(LoweringError::Unsupported(
                "stroke and strokeWidth are currently only supported for point marks",
            ));
        }
        if let Some(detail) = detail
            && !matches!(detail.kind(), FieldKind::Ordinal | FieldKind::Nominal)
        {
            return Err(LoweringError::Unsupported(
                "detail currently requires an ordinal or nominal channel",
            ));
        }
        if color.is_some() && detail.is_some() {
            return Err(LoweringError::Unsupported(
                "color and detail cannot be combined in the current lowering slice",
            ));
        }
        if order.is_some() && !matches!(self.mark, MarkDef::Line | MarkDef::Area) {
            return Err(LoweringError::Unsupported(
                "order is currently only supported for line and area marks",
            ));
        }
        if detail.is_some() && !matches!(self.mark, MarkDef::Line | MarkDef::Area) {
            return Err(LoweringError::Unsupported(
                "detail is currently only supported for line and area marks",
            ));
        }
        if y.aggregate().is_some() && order.is_some() {
            return Err(LoweringError::Unsupported(
                "order is not yet supported with aggregated y channels",
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
            MarkDef::Rule => unreachable!("rule lowering is handled before generic validation"),
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
            if let Some(detail) = detail {
                push_unique_col(&mut group_by, detail.field());
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
            required_columns(
                x,
                x2,
                lowered_y_field,
                y2,
                color,
                size,
                shape,
                opacity,
                stroke,
                stroke_width,
                order,
                detail,
                text,
            ),
        )?;

        let point_size_domain = size
            .map(|size| infer_frame_domain_pair(&preview_frame, size.field(), None, "size"))
            .transpose()?;
        let opacity_domain = opacity
            .map(|opacity| {
                infer_frame_domain_pair(&preview_frame, opacity.field(), None, "opacity")
            })
            .transpose()?;
        let point_stroke_map = stroke
            .map(|stroke| build_brush_map(&preview_frame, stroke.field()))
            .unwrap_or_default();
        let point_stroke_width_domain = stroke_width
            .map(|stroke_width| {
                infer_frame_domain_pair(&preview_frame, stroke_width.field(), None, "strokeWidth")
            })
            .transpose()?;
        let point_shape_map = shape
            .map(|shape| build_shape_map(&preview_frame, shape.field()))
            .unwrap_or_default();
        let bar_category_index_map = if self.mark == MarkDef::Bar {
            build_category_index_map(&preview_frame, x.field())
        } else {
            Vec::new()
        };

        let mut program = if base_program.transforms().is_empty() {
            None
        } else {
            Some(base_program)
        };

        let sort_col = order.map_or_else(|| x.field(), ChannelDef::field);
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
                        opacity,
                        stroke,
                        stroke_width,
                        order,
                        None,
                        text,
                    ),
                });
                if matches!(self.mark, MarkDef::Line | MarkDef::Area) {
                    p.push(Transform::Sort {
                        input: output,
                        output,
                        by: sort_col,
                        order: SortOrder::Asc,
                        columns: series_columns(
                            x,
                            x2,
                            lowered_y_field,
                            y2,
                            None,
                            size,
                            shape,
                            opacity,
                            stroke,
                            stroke_width,
                            order,
                            None,
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
                    MarkDef::Bar => SeriesLayer::Bar(BarLayer {
                        id_base: self.id_base.wrapping_add(0x1_000 + index as u64),
                        table: output,
                        x: x.field(),
                        y: lowered_y_field,
                        y2: y2.map(ChannelDef::field),
                        category_index_map: bar_category_index_map.clone(),
                        group_index: if y2.is_some() { 0 } else { index },
                        group_count: if y2.is_some() { 1 } else { series_values.len() },
                        baseline: 0.0,
                        fill,
                        opacity: opacity.map(ChannelDef::field),
                        opacity_domain,
                    }),
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
                        opacity: opacity.map(ChannelDef::field),
                        opacity_domain,
                        constant_stroke: Brush::Solid(css::BLACK),
                        has_constant_stroke_style: false,
                        stroke: stroke.map(ChannelDef::field),
                        stroke_map: point_stroke_map.clone(),
                        default_stroke_width: 1.5,
                        stroke_width: stroke_width.map(ChannelDef::field),
                        stroke_width_domain: point_stroke_width_domain,
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
                        stroke: None,
                    }),
                    MarkDef::Rule => unreachable!("rule lowering is handled by lower_rule"),
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
                        opacity: opacity.map(ChannelDef::field),
                        opacity_domain,
                    }),
                });
            }
        } else if let Some(detail) = detail {
            let series_values = distinct_values(&preview_frame, detail.field());
            if series_values.is_empty() {
                return Err(LoweringError::Unsupported(
                    "detail lowering requires at least one finite series value",
                ));
            }
            let p = program.get_or_insert_with(Program::new);
            for (index, value) in series_values.iter().copied().enumerate() {
                let output = alloc_table(&mut next_table);
                p.push(Transform::Filter {
                    input: current_table,
                    output,
                    predicate: Predicate {
                        col: detail.field(),
                        op: vizir_transforms::CompareOp::Eq,
                        value,
                    },
                    columns: series_columns(
                        x,
                        x2,
                        lowered_y_field,
                        y2,
                        None,
                        size,
                        shape,
                        opacity,
                        stroke,
                        stroke_width,
                        order,
                        Some(detail),
                        text,
                    ),
                });
                p.push(Transform::Sort {
                    input: output,
                    output,
                    by: sort_col,
                    order: SortOrder::Asc,
                    columns: series_columns(
                        x,
                        x2,
                        lowered_y_field,
                        y2,
                        None,
                        size,
                        shape,
                        opacity,
                        stroke,
                        stroke_width,
                        order,
                        None,
                        text,
                    ),
                });
                derived_tables.push(output);
                series_layers.push(match self.mark {
                    MarkDef::Line => SeriesLayer::Line(LineLayer {
                        id: MarkId::from_raw(self.id_base.wrapping_add(0x1_000 + index as u64)),
                        table: output,
                        x: x.field(),
                        y: lowered_y_field,
                        stroke: StrokeStyle::solid(css::BLACK, 2.0),
                    }),
                    MarkDef::Area => SeriesLayer::Area(AreaLayer {
                        id: MarkId::from_raw(self.id_base.wrapping_add(0x1_000 + index as u64)),
                        table: output,
                        x: x.field(),
                        x2: x2.map(ChannelDef::field),
                        y: lowered_y_field,
                        y2: y2.map(ChannelDef::field),
                        baseline: 0.0,
                        fill: Brush::Solid(css::CORNFLOWER_BLUE),
                        stroke: None,
                    }),
                    MarkDef::Bar | MarkDef::Point | MarkDef::Rule | MarkDef::Text => {
                        unreachable!("detail is validated for line/area marks only")
                    }
                });
            }
        } else {
            if matches!(self.mark, MarkDef::Line | MarkDef::Area) && order.is_some() {
                let output = alloc_table(&mut next_table);
                let p = program.get_or_insert_with(Program::new);
                p.push(Transform::Sort {
                    input: current_table,
                    output,
                    by: sort_col,
                    order: SortOrder::Asc,
                    columns: series_columns(
                        x,
                        x2,
                        lowered_y_field,
                        y2,
                        None,
                        size,
                        shape,
                        opacity,
                        stroke,
                        stroke_width,
                        order,
                        None,
                        text,
                    ),
                });
                derived_tables.push(output);
                current_table = output;
            }
            series_layers.push(match self.mark {
                MarkDef::Bar => SeriesLayer::Bar(BarLayer {
                    id_base: self.id_base.wrapping_add(0x1_000),
                    table: current_table,
                    x: x.field(),
                    y: lowered_y_field,
                    y2: y2.map(ChannelDef::field),
                    category_index_map: bar_category_index_map,
                    group_index: 0,
                    group_count: 1,
                    baseline: 0.0,
                    fill: Brush::Solid(css::CORNFLOWER_BLUE),
                    opacity: opacity.map(ChannelDef::field),
                    opacity_domain,
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
                    opacity: opacity.map(ChannelDef::field),
                    opacity_domain,
                    constant_stroke: Brush::Solid(css::BLACK),
                    has_constant_stroke_style: false,
                    stroke: stroke.map(ChannelDef::field),
                    stroke_map: point_stroke_map,
                    default_stroke_width: 1.5,
                    stroke_width: stroke_width.map(ChannelDef::field),
                    stroke_width_domain: point_stroke_width_domain,
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
                    stroke: None,
                }),
                MarkDef::Rule => unreachable!("rule lowering is handled by lower_rule"),
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
                    opacity: opacity.map(ChannelDef::field),
                    opacity_domain,
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

    fn lower_rule(
        &self,
        scene: &Scene,
        input_table: TableId,
    ) -> Result<LoweredUnit, LoweringError> {
        let x = self.encoding.x();
        let y = self.encoding.y();
        let x2 = self.encoding.x2();
        let y2 = self.encoding.y2();
        let color = self.encoding.color();
        let size = self.encoding.size();
        let shape = self.encoding.shape();
        let opacity = self.encoding.opacity();
        let stroke = self.encoding.stroke();
        let stroke_width = self.encoding.stroke_width();
        let order = self.encoding.order();
        let detail = self.encoding.detail();
        let text = self.encoding.text();

        if x.is_some() == y.is_some() {
            return Err(LoweringError::Unsupported(
                "rule lowering requires exactly one of x or y",
            ));
        }
        if x2.is_some()
            || y2.is_some()
            || color.is_some()
            || size.is_some()
            || shape.is_some()
            || opacity.is_some()
            || stroke.is_some()
            || stroke_width.is_some()
            || order.is_some()
            || detail.is_some()
            || text.is_some()
        {
            return Err(LoweringError::Unsupported(
                "rule lowering currently supports only a single x or y channel",
            ));
        }
        if let Some(x) = x
            && x.aggregate().is_some()
        {
            return Err(LoweringError::Unsupported(
                "aggregate on the x channel is not supported for rule marks",
            ));
        }
        if let Some(y) = y {
            if y.aggregate().is_some() {
                return Err(LoweringError::Unsupported(
                    "aggregate on the y channel is not supported for rule marks; use an aggregate transform instead",
                ));
            }
            if y.kind() != FieldKind::Quantitative {
                return Err(LoweringError::Unsupported(
                    "horizontal rule lowering requires a quantitative y channel",
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

        let preview_frame = preview_output_frame(
            scene,
            &base_program,
            input_table,
            current_table,
            dedup_cols(match (x, y) {
                (Some(x), None) => vec![x.field()],
                (None, Some(y)) => vec![y.field()],
                _ => unreachable!("validated exactly one rule channel"),
            }),
        )?;

        let chart = build_rule_chart_spec(self, &preview_frame, x, y)?;
        let series_layers = vec![match (x, y) {
            (Some(x), None) => SeriesLayer::Rule(RuleLayer {
                id_base: self.id_base.wrapping_add(0x1_000),
                table: current_table,
                orientation: RuleOrientation::Vertical {
                    x: x.field(),
                    kind: x.kind(),
                },
                stroke: StrokeStyle::solid(css::BLACK, 1.0),
            }),
            (None, Some(y)) => SeriesLayer::Rule(RuleLayer {
                id_base: self.id_base.wrapping_add(0x1_000),
                table: current_table,
                orientation: RuleOrientation::Horizontal { y: y.field() },
                stroke: StrokeStyle::solid(css::BLACK, 1.0),
            }),
            _ => unreachable!("validated exactly one rule channel"),
        }];

        Ok(LoweredUnit {
            input_table,
            output_table: current_table,
            derived_tables,
            program: if base_program.transforms().is_empty() {
                None
            } else {
                Some(base_program)
            },
            chart,
            series_layers,
        })
    }
}

/// A narrow one-field faceted chart.
///
/// This reuses the authored unit-chart seam and partitions one input table by a categorical
/// `facet` channel. Each facet cell lowers the same unit chart against its filtered table.
#[derive(Clone, Debug)]
pub struct FacetSpec {
    id_base: u64,
    derived_table_base: TableId,
    data: DataRef,
    facet: ChannelDef,
    transforms: Vec<TransformSpec>,
    mark: MarkDef,
    encoding: EncodingSet,
    width: f64,
    height: f64,
    title: Option<String>,
    columns: usize,
    spacing: f64,
}

impl FacetSpec {
    /// Creates a new one-field facet spec.
    pub fn new(
        id_base: u64,
        derived_table_base: TableId,
        data: DataRef,
        facet: ChannelDef,
        mark: MarkDef,
    ) -> Self {
        Self {
            id_base,
            derived_table_base,
            data,
            facet,
            transforms: Vec::new(),
            mark,
            encoding: EncodingSet::new(),
            width: 220.0,
            height: 120.0,
            title: None,
            columns: 2,
            spacing: 24.0,
        }
    }

    /// Sets the plot size used by each facet cell.
    pub fn with_size(mut self, width: f64, height: f64) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Sets the authored facet title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the number of facet columns in the rendered grid.
    pub fn with_columns(mut self, columns: usize) -> Self {
        self.columns = columns.max(1);
        self
    }

    /// Sets the spacing between facet cells.
    pub fn with_spacing(mut self, spacing: f64) -> Self {
        self.spacing = spacing;
        self
    }

    /// Replaces the facet cell encoding set.
    pub fn with_encoding(mut self, encoding: EncodingSet) -> Self {
        self.encoding = encoding;
        self
    }

    /// Sets the x channel for each facet cell.
    pub fn with_x(mut self, x: ChannelDef) -> Self {
        self.encoding = self.encoding.with_x(x);
        self
    }

    /// Sets the x2 channel for each facet cell.
    pub fn with_x2(mut self, x2: ChannelDef) -> Self {
        self.encoding = self.encoding.with_x2(x2);
        self
    }

    /// Sets the y channel for each facet cell.
    pub fn with_y(mut self, y: ChannelDef) -> Self {
        self.encoding = self.encoding.with_y(y);
        self
    }

    /// Sets the y2 channel for each facet cell.
    pub fn with_y2(mut self, y2: ChannelDef) -> Self {
        self.encoding = self.encoding.with_y2(y2);
        self
    }

    /// Sets the color channel for each facet cell.
    pub fn with_color(mut self, color: ChannelDef) -> Self {
        self.encoding = self.encoding.with_color(color);
        self
    }

    /// Sets the size channel for each facet cell.
    pub fn with_size_channel(mut self, size: ChannelDef) -> Self {
        self.encoding = self.encoding.with_size(size);
        self
    }

    /// Sets the shape channel for each facet cell.
    pub fn with_shape(mut self, shape: ChannelDef) -> Self {
        self.encoding = self.encoding.with_shape(shape);
        self
    }

    /// Sets the opacity channel for each facet cell.
    pub fn with_opacity(mut self, opacity: ChannelDef) -> Self {
        self.encoding = self.encoding.with_opacity(opacity);
        self
    }

    /// Sets the stroke channel for each facet cell.
    pub fn with_stroke(mut self, stroke: ChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke(stroke);
        self
    }

    /// Sets the stroke-width channel for each facet cell.
    pub fn with_stroke_width(mut self, stroke_width: ChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke_width(stroke_width);
        self
    }

    /// Sets the order channel for each facet cell.
    pub fn with_order(mut self, order: ChannelDef) -> Self {
        self.encoding = self.encoding.with_order(order);
        self
    }

    /// Sets the detail channel for each facet cell.
    pub fn with_detail(mut self, detail: ChannelDef) -> Self {
        self.encoding = self.encoding.with_detail(detail);
        self
    }

    /// Sets the text channel for each facet cell.
    pub fn with_text(mut self, text: ChannelDef) -> Self {
        self.encoding = self.encoding.with_text(text);
        self
    }

    /// Appends an authored transform for each facet cell.
    pub fn with_transform(mut self, transform: TransformSpec) -> Self {
        self.transforms.push(transform);
        self
    }

    /// Lowers the authored facet spec into a faceted chart/transform plan.
    pub fn lower(&self, scene: &Scene) -> Result<LoweredFacet, LoweringError> {
        let DataRef::Table(input_table) = self.data;
        ensure_table_exists(scene, input_table)?;

        if self.facet.aggregate().is_some() {
            return Err(LoweringError::Unsupported(
                "facet channels do not support aggregate in the experimental lowering slice",
            ));
        }
        if !matches!(self.facet.kind(), FieldKind::Ordinal | FieldKind::Nominal) {
            return Err(LoweringError::Unsupported(
                "facet lowering currently requires an ordinal or nominal channel",
            ));
        }

        let input = scene
            .tables
            .get(&input_table)
            .ok_or(LoweringError::MissingTable(input_table))?;
        let frame =
            TableFrame::from_table(input, facet_preview_columns(self)).map_err(
                |err| match err {
                    TableFrameError::MissingData => LoweringError::MissingTableData(input_table),
                    _ => LoweringError::FrameError {
                        table: input_table,
                        err,
                    },
                },
            )?;
        let facet_values = distinct_values(&frame, self.facet.field());
        if facet_values.is_empty() {
            return Err(LoweringError::Unsupported(
                "facet lowering requires at least one finite facet value",
            ));
        }

        let mut cells = Vec::new();
        let mut derived_tables = Vec::new();
        let mut program = None;
        let filter_columns = facet_filter_columns(self);
        for (index, value) in facet_values.iter().copied().enumerate() {
            let filtered_table = facet_cell_input_table(self.derived_table_base, index);
            let mut filter_program = Program::new();
            filter_program.push(Transform::Filter {
                input: input_table,
                output: filtered_table,
                predicate: Predicate {
                    col: self.facet.field(),
                    op: vizir_transforms::CompareOp::Eq,
                    value,
                },
                columns: filter_columns.clone(),
            });

            let output = filter_program.execute_on_scene(scene)?;
            let filtered = output
                .tables
                .get(&filtered_table)
                .cloned()
                .ok_or(LoweringError::MissingTable(filtered_table))?;
            let mut filtered_scene = Scene::new();
            filtered_scene.insert_table(filtered.into_table(filtered_table));

            let label = facet_cell_label(self.facet.clone(), value);
            let mut lowered = self
                .facet_cell_unit(filtered_table, index, &label)
                .lower(&filtered_scene)?;
            lowered.chart.legend = None;

            derived_tables.push(filtered_table);
            extend_unique_tables(&mut derived_tables, &lowered.derived_tables);
            merge_programs(&mut program, Some(filter_program));
            merge_programs(&mut program, lowered.program.clone());
            cells.push(LoweredFacetCell { label, lowered });
        }

        Ok(LoweredFacet {
            id_base: self.id_base,
            input_table,
            derived_tables,
            program,
            cells,
            title: self.title.clone(),
            columns: self.columns.max(1),
            spacing: self.spacing.max(0.0),
        })
    }

    /// Lowers the authored facet spec and applies the derived tables to the scene.
    pub fn lower_into_scene(&self, scene: &mut Scene) -> Result<LoweredFacet, LoweringError> {
        let lowered = self.lower(scene)?;
        lowered.apply_to_scene(scene)?;
        Ok(lowered)
    }

    fn facet_cell_unit(&self, input_table: TableId, index: usize, title: &str) -> UnitSpec {
        UnitSpec {
            id_base: facet_cell_id_base(self.id_base, index),
            derived_table_base: facet_cell_derived_table_base(self.derived_table_base, index),
            data: DataRef::Table(input_table),
            transforms: self.transforms.clone(),
            mark: self.mark,
            encoding: self.encoding.clone(),
            width: self.width,
            height: self.height,
            title: Some(String::from(title)),
        }
    }
}

/// One unit-shaped child entry in a narrow shared-plot layer spec.
///
/// A child always contributes exactly one mark kind plus its own transform and encoding block.
/// Shared layer lowering still keeps one plot shell, so children may not redefine shared-domain
/// channels incompatibly with the base child.
#[derive(Clone, Debug, PartialEq)]
struct LayerChildStyle {
    fill: Option<Brush>,
    stroke: Option<StrokeStyle>,
    opacity: Option<f64>,
}

/// One unit-shaped child entry in a narrow shared-plot layer spec.
///
/// A child always contributes exactly one mark kind plus its own transform and encoding block.
/// Shared layer lowering still keeps one plot shell, so children may not redefine shared-domain
/// channels incompatibly with the base child.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerChildSpec {
    mark: MarkDef,
    transforms: Vec<TransformSpec>,
    encoding: EncodingSet,
    style: LayerChildStyle,
}

impl LayerChildSpec {
    /// Creates a new layer child for the given mark.
    pub fn new(mark: MarkDef) -> Self {
        Self {
            mark,
            transforms: Vec::new(),
            encoding: EncodingSet::new(),
            style: LayerChildStyle {
                fill: None,
                stroke: None,
                opacity: None,
            },
        }
    }

    /// Appends a child-local transform.
    pub fn with_transform(mut self, transform: TransformSpec) -> Self {
        self.transforms.push(transform);
        self
    }

    /// Replaces the child unit encoding set.
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

    /// Sets the opacity channel.
    pub fn with_opacity(mut self, opacity: ChannelDef) -> Self {
        self.encoding = self.encoding.with_opacity(opacity);
        self
    }

    /// Sets the child stroke override.
    pub fn with_stroke(mut self, stroke: ChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke(stroke);
        self
    }

    /// Sets the child stroke width override.
    pub fn with_stroke_width(mut self, stroke_width: ChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke_width(stroke_width);
        self
    }

    /// Sets the child order override.
    pub fn with_order(mut self, order: ChannelDef) -> Self {
        self.encoding = self.encoding.with_order(order);
        self
    }

    /// Sets the child detail override.
    pub fn with_detail(mut self, detail: ChannelDef) -> Self {
        self.encoding = self.encoding.with_detail(detail);
        self
    }

    /// Sets the child text override.
    pub fn with_text(mut self, text: ChannelDef) -> Self {
        self.encoding = self.encoding.with_text(text);
        self
    }

    /// Sets a constant child fill style.
    pub fn with_fill_style(mut self, fill: impl Into<Brush>) -> Self {
        self.style.fill = Some(fill.into());
        self
    }

    /// Sets a constant child stroke style.
    pub fn with_stroke_style(mut self, stroke: StrokeStyle) -> Self {
        self.style.stroke = Some(stroke);
        self
    }

    /// Sets a constant child opacity multiplier.
    pub fn with_opacity_value(mut self, opacity: f64) -> Self {
        self.style.opacity = Some(opacity);
        self
    }

    fn mark(&self) -> MarkDef {
        self.mark
    }
}

/// A narrow shared-plot layered chart.
///
/// Every child mark shares the same data source and chart shell. Shared transforms run for every
/// child, and each child may contribute unit-shaped mark, transform, and encoding content. The
/// base child still owns the shared x/y shell, so later children may not fork the shared x or
/// legend/color story. This is an intentionally small composition slice for common overlays such
/// as line + point.
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

    /// Sets the opacity channel.
    pub fn with_opacity(mut self, opacity: ChannelDef) -> Self {
        self.encoding = self.encoding.with_opacity(opacity);
        self
    }

    /// Sets the stroke channel.
    pub fn with_stroke(mut self, stroke: ChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke(stroke);
        self
    }

    /// Sets the stroke width channel.
    pub fn with_stroke_width(mut self, stroke_width: ChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke_width(stroke_width);
        self
    }

    /// Sets the order channel.
    pub fn with_order(mut self, order: ChannelDef) -> Self {
        self.encoding = self.encoding.with_order(order);
        self
    }

    /// Sets the detail channel.
    pub fn with_detail(mut self, detail: ChannelDef) -> Self {
        self.encoding = self.encoding.with_detail(detail);
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

    /// Appends a shared-plot child with unit-shaped mark/transform/encoding content.
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
        let base_unit = self.layer_child_unit(base_index, None);
        validate_layer_child_literal_style(
            base_unit.mark,
            &base_unit.encoding,
            &self.children[base_index].style,
        )?;
        let mut base_lowered = base_unit.lower(scene)?;
        apply_layer_child_style(
            &mut base_lowered.series_layers,
            &self.children[base_index].style,
        )?;
        let base_defaults = inherited_layer_encoding_defaults(
            &self.encoding,
            &merge_layer_encoding(
                &self.encoding,
                &self.children[base_index].encoding,
                self.children[base_index].mark(),
            ),
        );

        let mut series_layers = base_lowered.series_layers.clone();
        let mut derived_tables = base_lowered.derived_tables.clone();
        let mut combined_program = base_lowered.program.clone();
        for index in 0..self.children.len() {
            if index == base_index {
                continue;
            }
            validate_layer_child_shared_channels(&base_defaults, &self.children[index].encoding)?;
            let child = self.layer_child_unit(index, Some(&base_defaults));
            validate_layer_child_literal_style(
                child.mark,
                &child.encoding,
                &self.children[index].style,
            )?;
            let mut lowered = child.lower(scene)?;
            apply_layer_child_style(&mut lowered.series_layers, &self.children[index].style)?;
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

    fn layer_child_unit(&self, index: usize, inherited_defaults: Option<&EncodingSet>) -> UnitSpec {
        let child = &self.children[index];
        UnitSpec {
            id_base: child_layer_id_base(self.id_base, index),
            derived_table_base: child_layer_table_base(self.derived_table_base, index),
            data: self.data,
            transforms: layer_child_transforms(&self.transforms, &child.transforms),
            mark: child.mark(),
            encoding: merge_layer_encoding(
                inherited_defaults.unwrap_or(&self.encoding),
                &child.encoding,
                child.mark(),
            ),
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
struct LoweredFacetCell {
    label: String,
    lowered: LoweredUnit,
}

/// Layout output for a faceted chart grid.
#[derive(Clone, Debug, PartialEq)]
pub struct FacetLayout {
    /// Outer faceted view bounds.
    pub view: Rect,
    /// Reserved rectangle for the facet title, when present.
    pub title_top: Option<Rect>,
    /// Slot rectangle for each facet cell in row-major order.
    pub cells: Vec<Rect>,
}

/// A lowered faceted chart plan.
#[derive(Clone, Debug)]
pub struct LoweredFacet {
    id_base: u64,
    input_table: TableId,
    derived_tables: Vec<TableId>,
    program: Option<Program>,
    cells: Vec<LoweredFacetCell>,
    title: Option<String>,
    columns: usize,
    spacing: f64,
}

impl LoweredFacet {
    /// Returns the original input table referenced by the authored facet spec.
    pub fn input_table(&self) -> TableId {
        self.input_table
    }

    /// Returns every derived table id the lowered plan may write into the scene.
    pub fn derived_tables(&self) -> &[TableId] {
        &self.derived_tables
    }

    /// Returns the lowered transform program, if the facet spec needs derived tables.
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

    /// Produces the full translated mark list for the faceted chart.
    pub fn marks(
        &self,
        scene: &Scene,
        measurer: &impl TextMeasurer,
    ) -> Result<(FacetLayout, Vec<Mark>), LoweringError> {
        let mut slot_width: f64 = 0.0;
        let mut slot_height: f64 = 0.0;
        let mut cell_payloads = Vec::with_capacity(self.cells.len());
        for cell in &self.cells {
            let (layout, marks) = cell.lowered.marks(scene, measurer)?;
            slot_width = slot_width.max(layout.view.width());
            slot_height = slot_height.max(layout.view.height());
            cell_payloads.push((layout, marks));
        }

        let title_spec = self
            .title
            .as_ref()
            .map(|title| TitleSpec::new(MarkId::from_raw(self.id_base), title.clone()));
        let title_height = title_spec
            .as_ref()
            .map_or(0.0, |title| title.measure(measurer));
        let title_gap = if title_spec.is_some() {
            self.spacing
        } else {
            0.0
        };
        let columns = self.columns.max(1);
        let rows = cell_payloads.len().div_ceil(columns);
        let view_width = if cell_payloads.is_empty() {
            0.0
        } else {
            columns as f64 * slot_width + (columns.saturating_sub(1)) as f64 * self.spacing
        };
        let grid_height = if cell_payloads.is_empty() {
            0.0
        } else {
            rows as f64 * slot_height + (rows.saturating_sub(1)) as f64 * self.spacing
        };
        let view_height = title_height + title_gap + grid_height;
        let view = Rect::new(0.0, 0.0, view_width, view_height);
        let title_top = title_spec
            .as_ref()
            .map(|_| Rect::new(0.0, 0.0, view_width, title_height));

        let mut out = Vec::new();
        if let (Some(title), Some(title_rect)) = (title_spec.as_ref(), title_top) {
            out.extend(title.marks(measurer, title_rect));
        }

        let mut cells = Vec::with_capacity(cell_payloads.len());
        for (index, (layout, marks)) in cell_payloads.into_iter().enumerate() {
            let col = index % columns;
            let row = index / columns;
            let x = col as f64 * (slot_width + self.spacing);
            let y = title_height + title_gap + row as f64 * (slot_height + self.spacing);
            let cell_rect = Rect::new(x, y, x + layout.view.width(), y + layout.view.height());
            let dx = x - layout.view.x0;
            let dy = y - layout.view.y0;
            out.extend(marks.into_iter().map(|mark| translate_mark(mark, dx, dy)));
            cells.push(cell_rect);
        }

        Ok((
            FacetLayout {
                view,
                title_top,
                cells,
            },
            out,
        ))
    }

    /// Builds translated marks and runs a full scene tick.
    pub fn tick(
        &self,
        scene: &mut Scene,
        measurer: &impl TextMeasurer,
    ) -> Result<(FacetLayout, Vec<MarkDiff>), LoweringError> {
        let (layout, marks) = self.marks(scene, measurer)?;
        let diffs = scene.tick(marks);
        Ok((layout, diffs))
    }

    /// Returns the facet cell labels in row-major order.
    pub fn cell_labels(&self) -> Vec<&str> {
        self.cells.iter().map(|cell| cell.label.as_str()).collect()
    }
}

#[derive(Clone, Debug)]
enum SeriesLayer {
    Bar(BarLayer),
    Line(LineLayer),
    Point(PointLayer),
    Area(AreaLayer),
    Rule(RuleLayer),
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
            Self::Rule(layer) => layer.marks(scene, chart, plot),
            Self::Text(layer) => layer.marks(scene, chart, plot),
        }
    }
}

#[derive(Clone, Debug)]
struct BarLayer {
    id_base: u64,
    table: TableId,
    x: ColumnId,
    y: ColumnId,
    y2: Option<ColumnId>,
    category_index_map: Vec<(u64, usize)>,
    group_index: usize,
    group_count: usize,
    baseline: f64,
    fill: Brush,
    opacity: Option<ColumnId>,
    opacity_domain: Option<(f64, f64)>,
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
        let x_col = self.x;
        let y_col = self.y;
        let y2_col = self.y2;
        let category_index_map = self.category_index_map.clone();
        let group_index = self.group_index;
        let group_count = self.group_count;
        let baseline = self.baseline;
        let fill = self.fill.clone();
        let opacity_col = self.opacity;
        let opacity_domain = self.opacity_domain;
        let band = chart
            .x_axis()
            .ok_or(LoweringError::MissingChannel("x"))?
            .scale_band(plot);
        let (group_offset, group_width) =
            grouped_bar_slot_geometry(band.band_width(), group_index, group_count);
        let y_scale = chart
            .y_scale_continuous(plot)
            .ok_or(LoweringError::MissingChannel("y"))?;
        Ok(row_keys
            .iter()
            .copied()
            .enumerate()
            .map(|(row, row_key)| {
                let category_index_map = category_index_map.clone();
                let mark = Mark::builder(layer_row_mark_id(id_base, row_key))
                    .rect()
                    .z_index(crate::z_order::SERIES_FILL)
                    .x_compute(
                        [InputRef::TableCol {
                            table: table_id,
                            col: x_col,
                        }],
                        move |ctx, _| {
                            let value = ctx.table_f64(table_id, row, x_col).unwrap_or(f64::NAN);
                            band.x(category_index_for_value(value, &category_index_map))
                                + group_offset
                        },
                    )
                    .w_const(group_width);
                let mut mark = if let Some(y2_col) = y2_col {
                    mark.y_compute(
                        [
                            InputRef::TableCol {
                                table: table_id,
                                col: y_col,
                            },
                            InputRef::TableCol {
                                table: table_id,
                                col: y2_col,
                            },
                        ],
                        move |ctx, _| {
                            let top = ctx.table_f64(table_id, row, y_col).unwrap_or(baseline);
                            let bottom = ctx.table_f64(table_id, row, y2_col).unwrap_or(baseline);
                            y_scale.map(top.max(bottom))
                        },
                    )
                    .h_compute(
                        [
                            InputRef::TableCol {
                                table: table_id,
                                col: y_col,
                            },
                            InputRef::TableCol {
                                table: table_id,
                                col: y2_col,
                            },
                        ],
                        move |ctx, _| {
                            let top = ctx.table_f64(table_id, row, y_col).unwrap_or(baseline);
                            let bottom = ctx.table_f64(table_id, row, y2_col).unwrap_or(baseline);
                            (y_scale.map(top) - y_scale.map(bottom)).abs()
                        },
                    )
                } else {
                    let y0 = y_scale.map(baseline);
                    mark.y_compute(
                        [InputRef::TableCol {
                            table: table_id,
                            col: y_col,
                        }],
                        move |ctx, _| {
                            let v = ctx.table_f64(table_id, row, y_col).unwrap_or(baseline);
                            y_scale.map(v).min(y0)
                        },
                    )
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
                };
                mark = if let Some(opacity_col) = opacity_col {
                    let fill = fill.clone();
                    mark.fill_compute(
                        [InputRef::TableCol {
                            table: table_id,
                            col: opacity_col,
                        }],
                        move |ctx, _| {
                            brush_with_opacity(
                                &fill,
                                opacity_for_value(
                                    ctx.table_f64(table_id, row, opacity_col).unwrap_or(1.0),
                                    opacity_domain,
                                    1.0,
                                ),
                            )
                        },
                    )
                } else {
                    mark.fill_brush_const(fill.clone())
                };
                mark.build()
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
    opacity: Option<ColumnId>,
    opacity_domain: Option<(f64, f64)>,
    constant_stroke: Brush,
    has_constant_stroke_style: bool,
    stroke: Option<ColumnId>,
    stroke_map: Vec<(u64, Brush)>,
    default_stroke_width: f64,
    stroke_width: Option<ColumnId>,
    stroke_width_domain: Option<(f64, f64)>,
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
        let opacity_col = self.opacity;
        let opacity_domain = self.opacity_domain;
        let constant_stroke = self.constant_stroke.clone();
        let has_constant_stroke_style = self.has_constant_stroke_style;
        let stroke_col = self.stroke;
        let stroke_map = self.stroke_map.clone();
        let default_stroke_width = self.default_stroke_width;
        let stroke_width_col = self.stroke_width;
        let stroke_width_domain = self.stroke_width_domain;
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
                let stroke = stroke_col
                    .and_then(|col| table.data.as_deref().and_then(|data| data.f64(row, col)))
                    .map(|value| {
                        brush_for_series_value(value, &stroke_map, constant_stroke.clone())
                    })
                    .unwrap_or_else(|| constant_stroke.clone());
                let stroke_width = stroke_width_col
                    .and_then(|col| table.data.as_deref().and_then(|data| data.f64(row, col)))
                    .map(|value| {
                        stroke_width_for_value(value, stroke_width_domain, default_stroke_width)
                    })
                    .unwrap_or(default_stroke_width);
                let has_stroke_style =
                    has_constant_stroke_style || stroke_col.is_some() || stroke_width_col.is_some();

                if symbol == Symbol::Square && !has_stroke_style {
                    let mut mark = Mark::builder(layer_row_mark_id(id_base, row_key))
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
                        .h_const(size);
                    mark = if let Some(opacity_col) = opacity_col {
                        let fill = fill.clone();
                        mark.fill_compute(
                            [InputRef::TableCol {
                                table: table_id,
                                col: opacity_col,
                            }],
                            move |ctx, _| {
                                brush_with_opacity(
                                    &fill,
                                    opacity_for_value(
                                        ctx.table_f64(table_id, row, opacity_col).unwrap_or(1.0),
                                        opacity_domain,
                                        1.0,
                                    ),
                                )
                            },
                        )
                    } else {
                        mark.fill_brush_const(fill.clone())
                    };
                    mark.build()
                } else {
                    let mut mark = Mark::builder(layer_row_mark_id(id_base, row_key))
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
                        );
                    mark = if let Some(opacity_col) = opacity_col {
                        let fill = fill.clone();
                        mark.fill_compute(
                            [InputRef::TableCol {
                                table: table_id,
                                col: opacity_col,
                            }],
                            move |ctx, _| {
                                brush_with_opacity(
                                    &fill,
                                    opacity_for_value(
                                        ctx.table_f64(table_id, row, opacity_col).unwrap_or(1.0),
                                        opacity_domain,
                                        1.0,
                                    ),
                                )
                            },
                        )
                    } else {
                        mark.fill_brush_const(fill.clone())
                    };
                    mark = if let Some(stroke_col) = stroke_col {
                        let stroke_map = stroke_map.clone();
                        let default_stroke = constant_stroke.clone();
                        mark.stroke_compute(
                            [InputRef::TableCol {
                                table: table_id,
                                col: stroke_col,
                            }],
                            move |ctx, _| {
                                let value =
                                    ctx.table_f64(table_id, row, stroke_col).unwrap_or(f64::NAN);
                                brush_for_series_value(value, &stroke_map, default_stroke.clone())
                            },
                        )
                    } else {
                        mark.stroke_brush_const(stroke.clone())
                    };
                    mark = if let Some(stroke_width_col) = stroke_width_col {
                        mark.stroke_width_compute(
                            [InputRef::TableCol {
                                table: table_id,
                                col: stroke_width_col,
                            }],
                            move |ctx, _| {
                                let value = ctx
                                    .table_f64(table_id, row, stroke_width_col)
                                    .unwrap_or(default_stroke_width);
                                stroke_width_for_value(
                                    value,
                                    stroke_width_domain,
                                    default_stroke_width,
                                )
                            },
                        )
                    } else if has_stroke_style {
                        mark.stroke_width_const(stroke_width)
                    } else {
                        mark.stroke_width_const(0.0)
                    };
                    mark.build()
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
    stroke: Option<StrokeStyle>,
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
            (Some(x2), Some(y2)) => {
                let mut spec = crate::RangeAreaMarkSpec::new(
                    self.id.0, self.table, self.x, self.y, x2, y2, x_scale, y_scale,
                )
                .with_fill(self.fill.clone());
                if let Some(stroke) = self.stroke.clone() {
                    spec = spec.with_stroke(stroke);
                }
                spec.marks()
            }
            (None, Some(y2)) => {
                let mut spec = crate::StackedAreaMarkSpec::new(
                    self.id.0, self.table, self.x, y2, self.y, x_scale, y_scale,
                )
                .with_fill(self.fill.clone());
                if let Some(stroke) = self.stroke.clone() {
                    spec = spec.with_stroke(stroke);
                }
                spec.marks()
            }
            (None, None) => {
                let mut spec = crate::AreaMarkSpec::new(
                    self.id.0, self.table, self.x, self.y, x_scale, y_scale,
                )
                .with_baseline(self.baseline)
                .with_fill(self.fill.clone());
                if let Some(stroke) = self.stroke.clone() {
                    spec = spec.with_stroke(stroke);
                }
                spec.marks()
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
enum RuleOrientation {
    Vertical { x: ColumnId, kind: FieldKind },
    Horizontal { y: ColumnId },
}

#[derive(Clone, Debug)]
struct RuleLayer {
    id_base: u64,
    table: TableId,
    orientation: RuleOrientation,
    stroke: StrokeStyle,
}

impl RuleLayer {
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
        let stroke = self.stroke.clone();

        Ok(match self.orientation {
            RuleOrientation::Vertical { x, kind } => match kind {
                FieldKind::Ordinal | FieldKind::Nominal => {
                    let band = chart
                        .x_axis()
                        .ok_or(LoweringError::MissingChannel("x"))?
                        .scale_band(plot);
                    row_keys
                        .iter()
                        .copied()
                        .enumerate()
                        .map(|(row, row_key)| {
                            crate::RuleMarkSpec::vertical(
                                layer_row_mark_id(id_base, row_key),
                                band.x(row) + 0.5 * band.band_width(),
                                plot.y0,
                                plot.y1,
                            )
                            .with_stroke(stroke.brush.clone(), stroke.stroke_width)
                            .mark()
                        })
                        .collect()
                }
                FieldKind::Quantitative | FieldKind::Temporal => {
                    let x_scale = chart
                        .x_scale_continuous(plot)
                        .ok_or(LoweringError::MissingChannel("x"))?;
                    row_keys
                        .iter()
                        .copied()
                        .enumerate()
                        .map(|(row, row_key)| {
                            crate::RuleMarkSpec::vertical(
                                layer_row_mark_id(id_base, row_key),
                                x_scale.map(
                                    table
                                        .data
                                        .as_deref()
                                        .and_then(|data| data.f64(row, x))
                                        .unwrap_or(0.0),
                                ),
                                plot.y0,
                                plot.y1,
                            )
                            .with_stroke(stroke.brush.clone(), stroke.stroke_width)
                            .mark()
                        })
                        .collect()
                }
            },
            RuleOrientation::Horizontal { y } => {
                let y_scale = chart
                    .y_scale_continuous(plot)
                    .ok_or(LoweringError::MissingChannel("y"))?;
                row_keys
                    .iter()
                    .copied()
                    .enumerate()
                    .map(|(row, row_key)| {
                        crate::RuleMarkSpec::horizontal(
                            layer_row_mark_id(id_base, row_key),
                            y_scale.map(
                                table
                                    .data
                                    .as_deref()
                                    .and_then(|data| data.f64(row, y))
                                    .unwrap_or(0.0),
                            ),
                            plot.x0,
                            plot.x1,
                        )
                        .with_stroke(stroke.brush.clone(), stroke.stroke_width)
                        .mark()
                    })
                    .collect()
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
    opacity: Option<ColumnId>,
    opacity_domain: Option<(f64, f64)>,
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
        let opacity_col = self.opacity;
        let opacity_domain = self.opacity_domain;
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
                        let mut mark = Mark::builder(layer_row_mark_id(id_base, row_key))
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
                            .text_anchor(TextAnchor::Middle)
                            .text_baseline(TextBaseline::Ideographic);
                        mark = if let Some(opacity_col) = opacity_col {
                            let fill = fill.clone();
                            mark.fill_compute(
                                [InputRef::TableCol {
                                    table: table_id,
                                    col: opacity_col,
                                }],
                                move |ctx, _| {
                                    brush_with_opacity(
                                        &fill,
                                        opacity_for_value(
                                            ctx.table_f64(table_id, row, opacity_col)
                                                .unwrap_or(1.0),
                                            opacity_domain,
                                            1.0,
                                        ),
                                    )
                                },
                            )
                        } else {
                            mark.fill_brush_const(fill.clone())
                        };
                        mark.build()
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
                        let mut mark = Mark::builder(layer_row_mark_id(id_base, row_key))
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
                            .text_anchor(TextAnchor::Middle)
                            .text_baseline(TextBaseline::Ideographic);
                        mark = if let Some(opacity_col) = opacity_col {
                            let fill = fill.clone();
                            mark.fill_compute(
                                [InputRef::TableCol {
                                    table: table_id,
                                    col: opacity_col,
                                }],
                                move |ctx, _| {
                                    brush_with_opacity(
                                        &fill,
                                        opacity_for_value(
                                            ctx.table_f64(table_id, row, opacity_col)
                                                .unwrap_or(1.0),
                                            opacity_domain,
                                            1.0,
                                        ),
                                    )
                                },
                            )
                        } else {
                            mark.fill_brush_const(fill.clone())
                        };
                        mark.build()
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

fn build_rule_chart_spec(
    spec: &UnitSpec,
    frame: &TableFrame,
    x: Option<&ChannelDef>,
    y: Option<&ChannelDef>,
) -> Result<ChartSpec, LoweringError> {
    let title = spec.title.as_ref().map(|title| {
        TitleSpec::new(
            MarkId::from_raw(spec.id_base.wrapping_add(0x200)),
            title.clone(),
        )
        .with_font_size(12.0)
        .with_fill(css::BLACK)
    });

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
        axis_left: y
            .map(|y| build_y_axis(spec, frame, y.field(), None, y.title(), MarkDef::Rule))
            .transpose()?,
        axis_right: None,
        axis_top: None,
        axis_bottom: x.map(|x| build_x_axis(spec, frame, x, None)).transpose()?,
        legend: None,
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
        MarkDef::Area | MarkDef::Line | MarkDef::Point | MarkDef::Rule | MarkDef::Text => {
            expand_domain(domain)
        }
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
        TransformSpecKind::Calculate {
            expr,
            output_col,
            columns,
        } => program.push(Transform::Calculate {
            input,
            output,
            expr: *expr,
            output_col: *output_col,
            columns: columns.clone(),
        }),
        TransformSpecKind::JoinAggregate {
            group_by,
            fields,
            columns,
        } => program.push(Transform::JoinAggregate {
            input,
            output,
            group_by: group_by.clone(),
            fields: fields.clone(),
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
        TransformSpecKind::Fold {
            fields,
            output_key,
            output_value,
            columns,
        } => program.push(Transform::Fold {
            input,
            output,
            fields: fields.clone(),
            output_key: *output_key,
            output_value: *output_value,
            columns: columns.clone(),
        }),
        TransformSpecKind::Window {
            group_by,
            sort_by,
            sort_order,
            fields,
            columns,
        } => program.push(Transform::Window {
            input,
            output,
            group_by: group_by.clone(),
            sort_by: *sort_by,
            sort_order: *sort_order,
            fields: fields.clone(),
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
    opacity: Option<&ChannelDef>,
    stroke: Option<&ChannelDef>,
    stroke_width: Option<&ChannelDef>,
    order: Option<&ChannelDef>,
    detail: Option<&ChannelDef>,
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
    if let Some(opacity) = opacity {
        push_unique_col(&mut out, opacity.field());
    }
    if let Some(stroke) = stroke {
        push_unique_col(&mut out, stroke.field());
    }
    if let Some(stroke_width) = stroke_width {
        push_unique_col(&mut out, stroke_width.field());
    }
    if let Some(order) = order {
        push_unique_col(&mut out, order.field());
    }
    if let Some(detail) = detail {
        push_unique_col(&mut out, detail.field());
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
    opacity: Option<&ChannelDef>,
    stroke: Option<&ChannelDef>,
    stroke_width: Option<&ChannelDef>,
    order: Option<&ChannelDef>,
    detail: Option<&ChannelDef>,
    text: Option<&ChannelDef>,
) -> Vec<ColumnId> {
    required_columns(
        x,
        x2,
        y_field,
        y2,
        color,
        size,
        shape,
        opacity,
        stroke,
        stroke_width,
        order,
        detail,
        text,
    )
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
    distinct_values(frame, col)
        .into_iter()
        .map(|v| format_channel_value(v, kind))
        .collect()
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

fn build_category_index_map(frame: &TableFrame, col: ColumnId) -> Vec<(u64, usize)> {
    distinct_values(frame, col)
        .into_iter()
        .enumerate()
        .map(|(index, value)| (value.to_bits(), index))
        .collect()
}

fn category_index_for_value(value: f64, index_map: &[(u64, usize)]) -> usize {
    index_map
        .iter()
        .find_map(|(bits, index)| (*bits == value.to_bits()).then_some(*index))
        .unwrap_or(0)
}

fn grouped_bar_slot_geometry(
    band_width: f64,
    group_index: usize,
    group_count: usize,
) -> (f64, f64) {
    if group_count <= 1 {
        return (0.0, band_width);
    }
    let outer_gap = band_width * 0.1;
    let slot_gap = outer_gap / (group_count as f64 + 1.0);
    let slot_width = (band_width - outer_gap) / group_count as f64;
    let x = slot_gap + group_index as f64 * slot_width;
    (x, slot_width)
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

fn build_brush_map(frame: &TableFrame, col: ColumnId) -> Vec<(u64, Brush)> {
    default_series_fills(distinct_values(frame, col).len())
        .into_iter()
        .zip(distinct_values(frame, col))
        .map(|(brush, value)| (value.to_bits(), brush))
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

fn brush_for_series_value(value: f64, brush_map: &[(u64, Brush)], default: Brush) -> Brush {
    if !value.is_finite() {
        return default;
    }
    brush_map
        .iter()
        .find(|(bits, _)| *bits == value.to_bits())
        .map_or(default, |(_, brush)| brush.clone())
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

fn opacity_for_value(value: f64, domain: Option<(f64, f64)>, default: f64) -> f64 {
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
    0.2 + t * 0.8
}

fn stroke_width_for_value(value: f64, domain: Option<(f64, f64)>, default: f64) -> f64 {
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
    0.5 + t * 3.5
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "peniko alpha is f32 and opacity is clamped to the [0, 1] range first"
)]
fn brush_with_opacity(brush: &Brush, opacity: f64) -> Brush {
    let opacity = opacity.clamp(0.0, 1.0);
    match brush {
        Brush::Solid(color) => Brush::Solid(color.with_alpha(opacity as f32)),
        _ => brush.clone(),
    }
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

fn facet_preview_columns(spec: &FacetSpec) -> Vec<ColumnId> {
    dedup_cols(vec![spec.facet.field()])
}

fn facet_filter_columns(spec: &FacetSpec) -> Vec<ColumnId> {
    let mut out = vec![spec.facet.field()];
    if let Some(x) = spec.encoding.x() {
        push_unique_col(&mut out, x.field());
    }
    if let Some(x2) = spec.encoding.x2() {
        push_unique_col(&mut out, x2.field());
    }
    if let Some(y) = spec.encoding.y() {
        push_unique_col(&mut out, y.field());
    }
    if let Some(y2) = spec.encoding.y2() {
        push_unique_col(&mut out, y2.field());
    }
    if let Some(color) = spec.encoding.color() {
        push_unique_col(&mut out, color.field());
    }
    if let Some(size) = spec.encoding.size() {
        push_unique_col(&mut out, size.field());
    }
    if let Some(shape) = spec.encoding.shape() {
        push_unique_col(&mut out, shape.field());
    }
    if let Some(opacity) = spec.encoding.opacity() {
        push_unique_col(&mut out, opacity.field());
    }
    if let Some(stroke) = spec.encoding.stroke() {
        push_unique_col(&mut out, stroke.field());
    }
    if let Some(stroke_width) = spec.encoding.stroke_width() {
        push_unique_col(&mut out, stroke_width.field());
    }
    if let Some(order) = spec.encoding.order() {
        push_unique_col(&mut out, order.field());
    }
    if let Some(detail) = spec.encoding.detail() {
        push_unique_col(&mut out, detail.field());
    }
    if let Some(text) = spec.encoding.text() {
        push_unique_col(&mut out, text.field());
    }
    for transform in &spec.transforms {
        extend_transform_input_columns(&mut out, transform);
    }
    dedup_cols(out)
}

fn extend_transform_input_columns(out: &mut Vec<ColumnId>, transform: &TransformSpec) {
    match &transform.kind {
        TransformSpecKind::Filter { predicate, columns } => {
            push_unique_col(out, predicate.col);
            for &col in columns {
                push_unique_col(out, col);
            }
        }
        TransformSpecKind::Sort { by, columns, .. } => {
            push_unique_col(out, *by);
            for &col in columns {
                push_unique_col(out, col);
            }
        }
        TransformSpecKind::Calculate {
            expr,
            output_col: _,
            columns,
        } => {
            for operand in [expr.left, expr.right] {
                if let CalculateOperand::Column(col) = operand {
                    push_unique_col(out, col);
                }
            }
            for &col in columns {
                push_unique_col(out, col);
            }
        }
        TransformSpecKind::JoinAggregate {
            group_by,
            fields,
            columns,
        } => {
            for &col in group_by {
                push_unique_col(out, col);
            }
            for field in fields {
                push_unique_col(out, field.input);
            }
            for &col in columns {
                push_unique_col(out, col);
            }
        }
        TransformSpecKind::Aggregate { group_by, fields } => {
            for &col in group_by {
                push_unique_col(out, col);
            }
            for field in fields {
                push_unique_col(out, field.input);
            }
        }
        TransformSpecKind::Bin {
            input_col, columns, ..
        } => {
            push_unique_col(out, *input_col);
            for &col in columns {
                push_unique_col(out, col);
            }
        }
        TransformSpecKind::Fold {
            fields, columns, ..
        } => {
            for &col in fields {
                push_unique_col(out, col);
            }
            for &col in columns {
                push_unique_col(out, col);
            }
        }
        TransformSpecKind::Window {
            group_by,
            sort_by,
            fields: _,
            columns,
            ..
        } => {
            for &col in group_by {
                push_unique_col(out, col);
            }
            push_unique_col(out, *sort_by);
            for &col in columns {
                push_unique_col(out, col);
            }
        }
        TransformSpecKind::Stack {
            group_by,
            sort_by,
            field,
            columns,
            ..
        } => {
            for &col in group_by {
                push_unique_col(out, col);
            }
            if let Some(sort_by) = sort_by {
                push_unique_col(out, *sort_by);
            }
            push_unique_col(out, *field);
            for &col in columns {
                push_unique_col(out, col);
            }
        }
    }
}

fn facet_cell_label(facet: ChannelDef, value: f64) -> String {
    let value = format_channel_value(value, facet.kind());
    if let Some(title) = facet.title() {
        format!("{title}: {value}")
    } else {
        value
    }
}

fn facet_cell_id_base(id_base: u64, index: usize) -> u64 {
    id_base.wrapping_add((index as u64).wrapping_mul(0x10_0000))
}

fn facet_cell_input_table(base: TableId, index: usize) -> TableId {
    let index = u32::try_from(index).unwrap_or(u32::MAX);
    TableId(base.0.wrapping_add(index.wrapping_mul(0x100)))
}

fn facet_cell_derived_table_base(base: TableId, index: usize) -> TableId {
    let input = facet_cell_input_table(base, index);
    TableId(input.0.wrapping_add(1))
}

fn translate_mark(mut mark: Mark, dx: f64, dy: f64) -> Mark {
    mark.encodings = match mark.encodings {
        MarkEncodings::Rect(enc) => MarkEncodings::Rect(Box::new(RectEncodings {
            x: translate_scalar_encoding(enc.x, dx),
            y: translate_scalar_encoding(enc.y, dy),
            w: enc.w,
            h: enc.h,
            fill: enc.fill,
        })),
        MarkEncodings::Text(enc) => MarkEncodings::Text(Box::new(TextEncodings {
            x: translate_scalar_encoding(enc.x, dx),
            y: translate_scalar_encoding(enc.y, dy),
            text: enc.text,
            font_size: enc.font_size,
            angle: enc.angle,
            anchor: enc.anchor,
            baseline: enc.baseline,
            fill: enc.fill,
        })),
        MarkEncodings::Path(enc) => MarkEncodings::Path(Box::new(PathEncodings {
            path: translate_path_encoding(enc.path, dx, dy),
            fill: enc.fill,
            stroke: enc.stroke,
            stroke_width: enc.stroke_width,
        })),
    };
    mark.cache = None;
    mark.rebuild_deps();
    mark
}

fn translate_scalar_encoding(encoding: Encoding<f64>, delta: f64) -> Encoding<f64> {
    match encoding {
        Encoding::Const(v) => Encoding::Const(v + delta),
        Encoding::Compute { deps, f } => Encoding::Compute {
            deps,
            f: Box::new(move |ctx, id| f(ctx, id) + delta),
        },
    }
}

fn translate_path_encoding(encoding: Encoding<BezPath>, dx: f64, dy: f64) -> Encoding<BezPath> {
    match encoding {
        Encoding::Const(path) => Encoding::Const(translate_path(path, dx, dy)),
        Encoding::Compute { deps, f } => Encoding::Compute {
            deps,
            f: Box::new(move |ctx, id| translate_path(f(ctx, id), dx, dy)),
        },
    }
}

fn translate_path(mut path: BezPath, dx: f64, dy: f64) -> BezPath {
    path.apply_affine(Affine::translate((dx, dy)));
    path
}

fn merge_layer_encoding(
    shared: &EncodingSet,
    overrides: &EncodingSet,
    mark: MarkDef,
) -> EncodingSet {
    if mark == MarkDef::Rule {
        return EncodingSet {
            x: overrides.x.clone(),
            x2: None,
            y: overrides.y.clone(),
            y2: None,
            color: None,
            size: None,
            shape: None,
            opacity: overrides.opacity.clone(),
            stroke: overrides.stroke.clone(),
            stroke_width: overrides.stroke_width.clone(),
            order: None,
            detail: None,
            text: None,
        };
    }
    let mut out = EncodingSet {
        x: overrides.x.clone().or_else(|| shared.x.clone()),
        x2: overrides.x2.clone().or_else(|| shared.x2.clone()),
        y: overrides.y.clone().or_else(|| shared.y.clone()),
        y2: overrides.y2.clone().or_else(|| shared.y2.clone()),
        color: overrides.color.clone().or_else(|| shared.color.clone()),
        size: overrides.size.clone().or_else(|| shared.size.clone()),
        shape: overrides.shape.clone().or_else(|| shared.shape.clone()),
        opacity: overrides.opacity.clone().or_else(|| shared.opacity.clone()),
        stroke: overrides.stroke.clone().or_else(|| shared.stroke.clone()),
        stroke_width: overrides
            .stroke_width
            .clone()
            .or_else(|| shared.stroke_width.clone()),
        order: overrides.order.clone().or_else(|| shared.order.clone()),
        detail: overrides.detail.clone().or_else(|| shared.detail.clone()),
        text: overrides.text.clone().or_else(|| shared.text.clone()),
    };
    if mark != MarkDef::Area {
        out.x2 = None;
        out.y2 = None;
    }
    out
}

fn validate_layer_child_shared_channels(
    base: &EncodingSet,
    child: &EncodingSet,
) -> Result<(), LoweringError> {
    if let Some(x) = child.x()
        && base
            .x()
            .is_none_or(|base_x| !equivalent_shared_channel(base_x, x))
    {
        return Err(LoweringError::Unsupported(
            "layer children must share the same x channel as the base child",
        ));
    }
    if let Some(x2) = child.x2() {
        let Some(base_x2) = base.x2() else {
            return Err(LoweringError::Unsupported(
                "layer children cannot introduce x2 when the base child has no shared x2 channel",
            ));
        };
        if !equivalent_shared_channel(base_x2, x2) {
            return Err(LoweringError::Unsupported(
                "layer children must share the same x2 channel as the base child",
            ));
        }
    }
    if let Some(color) = child.color() {
        let Some(base_color) = base.color() else {
            return Err(LoweringError::Unsupported(
                "layer children cannot introduce a child-local color channel in the shared-legend slice",
            ));
        };
        if !equivalent_shared_channel(base_color, color) {
            return Err(LoweringError::Unsupported(
                "layer children must share the same color channel as the base child",
            ));
        }
    }
    Ok(())
}

fn validate_layer_child_literal_style(
    mark: MarkDef,
    encoding: &EncodingSet,
    style: &LayerChildStyle,
) -> Result<(), LoweringError> {
    if style.fill.is_some() && encoding.color().is_some() {
        return Err(LoweringError::Unsupported(
            "layer children cannot combine literal fill style with a color channel",
        ));
    }
    if style.fill.is_some() && mark == MarkDef::Rule {
        return Err(LoweringError::Unsupported(
            "literal fill style is not supported on rule layer children",
        ));
    }
    if style.opacity.is_some() && encoding.opacity().is_some() {
        return Err(LoweringError::Unsupported(
            "layer children cannot combine literal opacity with an opacity channel",
        ));
    }
    if style.stroke.is_some() && encoding.stroke().is_some() {
        return Err(LoweringError::Unsupported(
            "layer children cannot combine literal stroke style with a stroke channel",
        ));
    }
    if style.stroke.is_some() && matches!(mark, MarkDef::Bar | MarkDef::Text) {
        return Err(LoweringError::Unsupported(
            "literal stroke style is only supported on line, point, and area layer children",
        ));
    }
    Ok(())
}

fn apply_layer_child_style(
    layers: &mut [SeriesLayer],
    style: &LayerChildStyle,
) -> Result<(), LoweringError> {
    for layer in layers {
        match layer {
            SeriesLayer::Bar(bar) => {
                if let Some(fill) = &style.fill {
                    bar.fill = fill.clone();
                }
                if let Some(opacity) = style.opacity {
                    bar.fill = brush_with_opacity(&bar.fill, opacity);
                }
                if style.stroke.is_some() {
                    return Err(LoweringError::Unsupported(
                        "literal stroke style is not supported on bar layer children",
                    ));
                }
            }
            SeriesLayer::Line(line) => {
                if style.fill.is_some() {
                    return Err(LoweringError::Unsupported(
                        "literal fill style is not supported on line layer children",
                    ));
                }
                if let Some(stroke) = &style.stroke {
                    line.stroke = stroke.clone();
                }
                if let Some(opacity) = style.opacity {
                    line.stroke.brush = brush_with_opacity(&line.stroke.brush, opacity);
                }
            }
            SeriesLayer::Point(point) => {
                if let Some(fill) = &style.fill {
                    point.fill = fill.clone();
                }
                if let Some(stroke) = &style.stroke {
                    point.constant_stroke = stroke.brush.clone();
                    point.default_stroke_width = stroke.stroke_width;
                    point.has_constant_stroke_style = true;
                }
                if let Some(opacity) = style.opacity {
                    point.fill = brush_with_opacity(&point.fill, opacity);
                }
            }
            SeriesLayer::Area(area) => {
                if let Some(fill) = &style.fill {
                    area.fill = fill.clone();
                }
                if let Some(stroke) = &style.stroke {
                    area.stroke = Some(stroke.clone());
                }
                if let Some(opacity) = style.opacity {
                    area.fill = brush_with_opacity(&area.fill, opacity);
                    if let Some(stroke) = &mut area.stroke {
                        stroke.brush = brush_with_opacity(&stroke.brush, opacity);
                    }
                }
            }
            SeriesLayer::Rule(rule) => {
                if style.fill.is_some() {
                    return Err(LoweringError::Unsupported(
                        "literal fill style is not supported on rule layer children",
                    ));
                }
                if let Some(stroke) = &style.stroke {
                    rule.stroke = stroke.clone();
                }
                if let Some(opacity) = style.opacity {
                    rule.stroke.brush = brush_with_opacity(&rule.stroke.brush, opacity);
                }
            }
            SeriesLayer::Text(text) => {
                if let Some(fill) = &style.fill {
                    text.fill = fill.clone();
                }
                if let Some(opacity) = style.opacity {
                    text.fill = brush_with_opacity(&text.fill, opacity);
                }
                if style.stroke.is_some() {
                    return Err(LoweringError::Unsupported(
                        "literal stroke style is not supported on text layer children",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn equivalent_shared_channel(a: &ChannelDef, b: &ChannelDef) -> bool {
    a.field == b.field && a.kind == b.kind && a.aggregate == b.aggregate
}

fn inherited_layer_encoding_defaults(
    shared: &EncodingSet,
    base_child: &EncodingSet,
) -> EncodingSet {
    let mut out = shared.clone();
    if out.x.is_none() {
        out.x = base_child.x.clone();
    }
    if out.x2.is_none() {
        out.x2 = base_child.x2.clone();
    }
    if out.y.is_none() {
        out.y = base_child.y.clone();
    }
    if out.y2.is_none() {
        out.y2 = base_child.y2.clone();
    }
    if out.color.is_none() {
        out.color = base_child.color.clone();
    }
    if out.order.is_none() {
        out.order = base_child.order.clone();
    }
    if out.detail.is_none() {
        out.detail = base_child.detail.clone();
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
    if let Some(index) = children
        .iter()
        .position(|child| child.mark() == MarkDef::Line)
    {
        return index;
    }
    if let Some(index) = children
        .iter()
        .position(|child| child.mark() == MarkDef::Point)
    {
        return index;
    }
    if let Some(index) = children
        .iter()
        .position(|child| child.mark() == MarkDef::Text)
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
    if let Some(size) = spec.encoding.size() {
        max_col = max_col.max(size.field().0);
    }
    if let Some(shape) = spec.encoding.shape() {
        max_col = max_col.max(shape.field().0);
    }
    if let Some(opacity) = spec.encoding.opacity() {
        max_col = max_col.max(opacity.field().0);
    }
    if let Some(stroke) = spec.encoding.stroke() {
        max_col = max_col.max(stroke.field().0);
    }
    if let Some(stroke_width) = spec.encoding.stroke_width() {
        max_col = max_col.max(stroke_width.field().0);
    }
    if let Some(order) = spec.encoding.order() {
        max_col = max_col.max(order.field().0);
    }
    if let Some(detail) = spec.encoding.detail() {
        max_col = max_col.max(detail.field().0);
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
            TransformSpecKind::Calculate {
                expr,
                output_col,
                columns,
            } => {
                max_col = max_col.max(output_col.0);
                for operand in [expr.left, expr.right] {
                    if let CalculateOperand::Column(col) = operand {
                        max_col = max_col.max(col.0);
                    }
                }
                for col in columns {
                    max_col = max_col.max(col.0);
                }
            }
            TransformSpecKind::JoinAggregate {
                group_by,
                fields,
                columns,
            } => {
                for col in group_by {
                    max_col = max_col.max(col.0);
                }
                for field in fields {
                    max_col = max_col.max(field.input.0).max(field.output.0);
                }
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
            TransformSpecKind::Fold {
                fields,
                output_key,
                output_value,
                columns,
            } => {
                max_col = max_col.max(output_key.0).max(output_value.0);
                for col in fields {
                    max_col = max_col.max(col.0);
                }
                for col in columns {
                    max_col = max_col.max(col.0);
                }
            }
            TransformSpecKind::Window {
                group_by,
                sort_by,
                fields,
                columns,
                ..
            } => {
                max_col = max_col.max(sort_by.0);
                for col in group_by {
                    max_col = max_col.max(col.0);
                }
                for field in fields {
                    max_col = max_col.max(field.output.0);
                }
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
    use vizir_transforms::WindowOp;

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
            .filter(|mark| matches!(mark.encodings, MarkEncodings::Rect(_)))
            .count();
        assert_eq!(rect_count, 3);
    }

    #[test]
    fn calculate_point_lowering_derives_alias_column_before_mark_building() {
        let mut scene = Scene::new();
        let table_id = TableId(1010);
        let mut table = Table::new(table_id);
        table.row_keys = vec![1, 2, 3];
        table.data = Some(Box::new(ThreeCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![2.0, 4.0, 6.0],
            c: vec![0.5, 1.0, 1.5],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xAA80,
            TableId(1011),
            DataRef::Table(table_id),
            MarkDef::Point,
        )
        .with_transform(TransformSpec::calculate(
            CalculateExpr {
                left: CalculateOperand::Column(ColumnId(1)),
                op: vizir_transforms::CalculateOp::Add,
                right: CalculateOperand::Column(ColumnId(2)),
            },
            ColumnId(10),
            vec![ColumnId(0), ColumnId(1), ColumnId(2)],
        ))
        .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
        .with_y(ChannelDef::quantitative(ColumnId(10)).with_title("base + delta"));

        let lowered = spec
            .lower_into_scene(&mut scene)
            .expect("lower calculated points");
        let calculated = scene
            .tables
            .get(&lowered.output_table())
            .expect("calculated output table");
        let data = calculated.data.as_deref().expect("calculated table data");
        assert_eq!(data.f64(0, ColumnId(10)), Some(2.5));
        assert_eq!(data.f64(1, ColumnId(10)), Some(5.0));
        assert_eq!(data.f64(2, ColumnId(10)), Some(7.5));

        let (_layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("point marks");
        let point_like = marks
            .iter()
            .filter(|mark| matches!(mark.kind, MarkKind::Path | MarkKind::Rect))
            .count();
        assert!(point_like >= 3);
    }

    #[test]
    fn joinaggregate_point_lowering_derives_group_means_per_row() {
        let mut scene = Scene::new();
        let table_id = TableId(1012);
        let mut table = Table::new(table_id);
        table.row_keys = vec![10, 11, 12, 13, 14, 15];
        table.data = Some(Box::new(ThreeCols {
            a: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            b: vec![2.0, 4.0, 6.0, 3.0, 5.0, 7.0],
            c: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xAA90,
            TableId(1013),
            DataRef::Table(table_id),
            MarkDef::Point,
        )
        .with_transform(TransformSpec::joinaggregate(
            vec![ColumnId(2)],
            vec![AggregateField {
                op: AggregateOp::Mean,
                input: ColumnId(1),
                output: ColumnId(10),
            }],
            vec![ColumnId(0), ColumnId(1), ColumnId(2)],
        ))
        .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
        .with_y(ChannelDef::quantitative(ColumnId(10)).with_title("mean(value)"))
        .with_color(ChannelDef::nominal(ColumnId(2)).with_title("series"));

        let lowered = spec
            .lower_into_scene(&mut scene)
            .expect("lower joinaggregate points");
        let joined = scene
            .tables
            .get(&lowered.output_table())
            .expect("joinaggregate output table");
        let data = joined.data.as_deref().expect("joinaggregate table data");
        assert_eq!(data.f64(0, ColumnId(10)), Some(4.0));
        assert_eq!(data.f64(2, ColumnId(10)), Some(4.0));
        assert_eq!(data.f64(3, ColumnId(10)), Some(5.0));
        assert_eq!(data.f64(5, ColumnId(10)), Some(5.0));
    }

    #[test]
    fn fold_bar_lowering_expands_wide_rows_into_grouped_series() {
        let mut scene = Scene::new();
        let table_id = TableId(1016);
        let mut table = Table::new(table_id);
        table.row_keys = vec![30, 31];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0],
            b: vec![2.0, 4.0],
            c: vec![3.0, 5.0],
            d: vec![4.0, 6.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xAAB0,
            TableId(1017),
            DataRef::Table(table_id),
            MarkDef::Bar,
        )
        .with_transform(TransformSpec::fold(
            vec![ColumnId(1), ColumnId(2), ColumnId(3)],
            ColumnId(10),
            ColumnId(11),
            vec![ColumnId(0)],
        ))
        .with_x(ChannelDef::ordinal(ColumnId(0)).with_title("category"))
        .with_y(ChannelDef::quantitative(ColumnId(11)).with_title("value"))
        .with_color(ChannelDef::nominal(ColumnId(10)).with_title("measure slot"));

        let lowered = spec
            .lower_into_scene(&mut scene)
            .expect("lower folded bars");
        let folded = scene
            .tables
            .get(&lowered.output_table())
            .expect("folded output table");
        assert_eq!(folded.row_keys.len(), 6);
        assert!(lowered.chart().legend.is_some());
    }

    #[test]
    fn window_line_lowering_derives_rank_column_before_series_building() {
        let mut scene = Scene::new();
        let table_id = TableId(1014);
        let mut table = Table::new(table_id);
        table.row_keys = vec![20, 21, 22, 23, 24, 25];
        table.data = Some(Box::new(ThreeCols {
            a: vec![9.0, 5.0, 1.0, 8.0, 4.0, 2.0],
            b: vec![9.0, 5.0, 1.0, 8.0, 4.0, 2.0],
            c: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xAAA0,
            TableId(1015),
            DataRef::Table(table_id),
            MarkDef::Line,
        )
        .with_transform(TransformSpec::window(
            vec![ColumnId(2)],
            ColumnId(1),
            SortOrder::Desc,
            vec![WindowField {
                op: WindowOp::Rank,
                output: ColumnId(10),
            }],
            vec![ColumnId(1), ColumnId(2)],
        ))
        .with_transform(TransformSpec::sort(
            ColumnId(10),
            SortOrder::Asc,
            vec![ColumnId(1), ColumnId(2), ColumnId(10)],
        ))
        .with_x(ChannelDef::quantitative(ColumnId(10)).with_title("rank"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("value"))
        .with_color(ChannelDef::nominal(ColumnId(2)).with_title("series"));

        let lowered = spec
            .lower_into_scene(&mut scene)
            .expect("lower ranked line");
        let ranked = scene
            .tables
            .get(&lowered.output_table())
            .expect("window output table");
        let data = ranked.data.as_deref().expect("window table data");
        let ranks = (0..data.row_count())
            .map(|row| data.f64(row, ColumnId(10)).expect("rank value"))
            .collect::<Vec<_>>();
        assert_eq!(ranks, vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0]);
    }

    #[test]
    fn grouped_bar_lowering_places_color_series_in_distinct_category_slots() {
        let mut scene = Scene::new();
        let table_id = TableId(11);
        let mut table = Table::new(table_id);
        table.row_keys = vec![10, 11, 12, 13, 14];
        table.data = Some(Box::new(ThreeCols {
            a: vec![0.0, 1.0, 2.0, 1.0, 2.0],
            b: vec![2.0, 3.0, 4.0, 5.0, 6.0],
            c: vec![0.0, 0.0, 0.0, 1.0, 1.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(0xAB00, TableId(110), DataRef::Table(table_id), MarkDef::Bar)
            .with_x(ChannelDef::ordinal(ColumnId(0)).with_title("category"))
            .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("value"))
            .with_color(ChannelDef::nominal(ColumnId(2)).with_title("series"));

        let lowered = spec
            .lower_into_scene(&mut scene)
            .expect("lower grouped bars");
        assert!(lowered.chart().legend.is_some());

        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("grouped bar marks");
        let diffs = scene.tick(marks);
        let xs = diffs
            .into_iter()
            .filter_map(|diff| match diff {
                MarkDiff::Enter { new, .. } => match *new {
                    vizir_core::MarkPayload::Rect(channels) => Some(channels.rect.x0),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(xs.len(), 5);

        let mut unique = xs;
        unique.sort_by(f64::total_cmp);
        unique.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        assert_eq!(unique.len(), 5);
    }

    #[test]
    fn stacked_bar_lowering_keeps_color_series_in_shared_category_slots() {
        let mut scene = Scene::new();
        let table_id = TableId(12);
        let mut table = Table::new(table_id);
        table.row_keys = vec![20, 21, 22, 23, 24];
        table.data = Some(Box::new(ThreeCols {
            a: vec![0.0, 1.0, 2.0, 1.0, 2.0],
            b: vec![2.0, 3.0, 4.0, 5.0, 6.0],
            c: vec![0.0, 0.0, 0.0, 1.0, 1.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(0xAC00, TableId(120), DataRef::Table(table_id), MarkDef::Bar)
            .with_transform(TransformSpec::stack(
                vec![ColumnId(0)],
                StackOffset::Zero,
                Some(ColumnId(2)),
                SortOrder::Asc,
                ColumnId(1),
                ColumnId(10),
                ColumnId(11),
                vec![ColumnId(0), ColumnId(1), ColumnId(2)],
            ))
            .with_x(ChannelDef::ordinal(ColumnId(0)).with_title("category"))
            .with_y(ChannelDef::quantitative(ColumnId(11)).with_title("top"))
            .with_y2(ChannelDef::quantitative(ColumnId(10)).with_title("bottom"))
            .with_color(ChannelDef::nominal(ColumnId(2)).with_title("series"));

        let lowered = spec
            .lower_into_scene(&mut scene)
            .expect("lower stacked bars");
        assert!(lowered.chart().legend.is_some());

        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("stacked bar marks");
        let diffs = scene.tick(marks);
        let xs = diffs
            .into_iter()
            .filter_map(|diff| match diff {
                MarkDiff::Enter { new, .. } => match *new {
                    vizir_core::MarkPayload::Rect(channels) => Some(channels.rect.x0),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(xs.len(), 5);

        let mut unique = xs;
        unique.sort_by(f64::total_cmp);
        unique.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn facet_lowering_partitions_rows_into_multiple_cells() {
        let mut scene = Scene::new();
        let table_id = TableId(13);
        let mut table = Table::new(table_id);
        table.row_keys = vec![30, 31, 32, 33, 34, 35];
        table.data = Some(Box::new(ThreeCols {
            a: vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0],
            b: vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
            c: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        }));
        scene.insert_table(table);

        let spec = FacetSpec::new(
            0xAD00,
            TableId(130),
            DataRef::Table(table_id),
            ChannelDef::nominal(ColumnId(2)).with_title("series"),
            MarkDef::Bar,
        )
        .with_title("Facet Demo")
        .with_x(ChannelDef::ordinal(ColumnId(0)).with_title("category"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("value"));

        let lowered = spec.lower_into_scene(&mut scene).expect("lower facet spec");
        assert_eq!(lowered.cell_labels(), vec!["series: 0", "series: 1"]);

        let (layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("facet marks");
        assert_eq!(layout.cells.len(), 2);
        assert!(layout.title_top.is_some());
        assert!(
            marks
                .iter()
                .any(|mark| matches!(mark.encodings, MarkEncodings::Text(_)))
        );
    }

    #[test]
    fn facet_lowering_rejects_quantitative_facet_channels() {
        let mut scene = Scene::new();
        let table_id = TableId(14);
        let mut table = Table::new(table_id);
        table.row_keys = vec![40, 41];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0],
            b: vec![2.0, 4.0],
        }));
        scene.insert_table(table);

        let spec = FacetSpec::new(
            0xAE00,
            TableId(140),
            DataRef::Table(table_id),
            ChannelDef::quantitative(ColumnId(0)),
            MarkDef::Line,
        )
        .with_x(ChannelDef::quantitative(ColumnId(0)))
        .with_y(ChannelDef::quantitative(ColumnId(1)));

        let err = spec
            .lower(&scene)
            .expect_err("quantitative facet should fail");
        assert!(
            matches!(err, LoweringError::Unsupported(message) if message.contains("ordinal or nominal"))
        );
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
    fn opacity_mapping_uses_visual_alpha_range() {
        assert_eq!(opacity_for_value(f64::NAN, Some((1.0, 5.0)), 1.0), 1.0);
        assert!((opacity_for_value(1.0, Some((1.0, 5.0)), 1.0) - 0.2).abs() < 1e-9);
        assert!((opacity_for_value(5.0, Some((1.0, 5.0)), 1.0) - 1.0).abs() < 1e-9);
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
    fn point_opacity_lowering_applies_alpha_to_fills() {
        let mut scene = Scene::new();
        let table_id = TableId(22);
        let mut table = Table::new(table_id);
        table.row_keys = vec![24, 25];
        table.data = Some(Box::new(ThreeCols {
            a: vec![0.0, 1.0],
            b: vec![1.0, 2.0],
            c: vec![0.0, 10.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xBC80,
            TableId(220),
            DataRef::Table(table_id),
            MarkDef::Point,
        )
        .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("y"))
        .with_opacity(ChannelDef::quantitative(ColumnId(2)).with_title("opacity"));

        let lowered = spec.lower(&scene).expect("lower opacity points");
        let (_layout, diffs) = lowered
            .tick(&mut scene, &HeuristicTextMeasurer)
            .expect("tick points");
        let fills = diffs
            .into_iter()
            .filter_map(|diff| match diff {
                MarkDiff::Enter { new, .. } => match *new {
                    vizir_core::MarkPayload::Path(channels) => Some(channels.fill),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(fills.contains(&Brush::Solid(css::TOMATO.with_alpha(0.2))));
        assert!(fills.contains(&Brush::Solid(css::TOMATO.with_alpha(1.0))));
    }

    #[test]
    fn point_stroke_and_width_lowering_apply_path_styles() {
        let mut scene = Scene::new();
        let table_id = TableId(23);
        let mut table = Table::new(table_id);
        table.row_keys = vec![30, 31];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0],
            b: vec![1.0, 2.0],
            c: vec![0.0, 1.0],
            d: vec![1.0, 5.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xBD00,
            TableId(230),
            DataRef::Table(table_id),
            MarkDef::Point,
        )
        .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("y"))
        .with_stroke(ChannelDef::nominal(ColumnId(2)).with_title("series"))
        .with_stroke_width(ChannelDef::quantitative(ColumnId(3)).with_title("weight"));

        let lowered = spec.lower(&scene).expect("lower stroked points");
        let (_layout, diffs) = lowered
            .tick(&mut scene, &HeuristicTextMeasurer)
            .expect("tick stroked points");
        let mut strokes = Vec::new();
        let mut widths = Vec::new();
        for diff in diffs {
            let MarkDiff::Enter { new, .. } = diff else {
                continue;
            };
            let vizir_core::MarkPayload::Path(channels) = *new else {
                continue;
            };
            strokes.push(channels.stroke);
            widths.push(channels.stroke_width);
        }
        assert!(strokes.contains(&Brush::Solid(css::CORNFLOWER_BLUE)));
        assert!(strokes.contains(&Brush::Solid(css::TOMATO)));
        assert!(widths.iter().any(|width| (*width - 0.5).abs() < 1e-9));
        assert!(widths.iter().any(|width| (*width - 4.0).abs() < 1e-9));
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
                .any(|mark| matches!(mark.encodings, MarkEncodings::Path(_)))
        );
    }

    #[test]
    fn line_lowering_rejects_opacity_channel() {
        let mut scene = Scene::new();
        let table_id = TableId(31);
        let mut table = Table::new(table_id);
        table.row_keys = vec![13, 14];
        table.data = Some(Box::new(ThreeCols {
            a: vec![0.0, 1.0],
            b: vec![2.0, 3.0],
            c: vec![0.3, 0.8],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xCC80,
            TableId(310),
            DataRef::Table(table_id),
            MarkDef::Line,
        )
        .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("y"))
        .with_opacity(ChannelDef::quantitative(ColumnId(2)).with_title("opacity"));

        let err = spec.lower(&scene).expect_err("line opacity should fail");
        assert!(matches!(
            err,
            LoweringError::Unsupported(message)
                if message.contains("opacity")
        ));
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
        assert!(matches!(marks[0].encodings, MarkEncodings::Path(_)));
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
    fn horizontal_rule_lowering_emits_full_width_marks() {
        let mut scene = Scene::new();
        let table_id = TableId(42);
        let mut table = Table::new(table_id);
        table.row_keys = vec![310, 311];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0],
            b: vec![2.0, 4.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xDE00,
            TableId(420),
            DataRef::Table(table_id),
            MarkDef::Rule,
        )
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("threshold"));

        let lowered = spec.lower(&scene).expect("lower horizontal rule");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("rule marks");
        assert_eq!(marks.len(), 2);
        assert!(marks.iter().all(|mark| mark.kind == MarkKind::Path));
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
                .all(|mark| matches!(mark.encodings, MarkEncodings::Path(_)))
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
        assert!(matches!(marks[0].encodings, MarkEncodings::Path(_)));
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
        assert!(matches!(marks[0].encodings, MarkEncodings::Path(_)));
    }

    #[test]
    fn line_detail_lowering_splits_series_and_sorts_by_order() {
        let mut scene = Scene::new();
        let table_id = TableId(75);
        let mut table = Table::new(table_id);
        table.row_keys = vec![900, 901, 902, 903];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 0.0, 1.0],
            b: vec![10.0, 20.0, 30.0, 40.0],
            c: vec![2.0, 1.0, 2.0, 1.0],
            d: vec![0.0, 0.0, 1.0, 1.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xF080,
            TableId(750),
            DataRef::Table(table_id),
            MarkDef::Line,
        )
        .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
        .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("y"))
        .with_order(ChannelDef::quantitative(ColumnId(2)).with_title("step"))
        .with_detail(ChannelDef::nominal(ColumnId(3)).with_title("series"));

        let lowered = spec.lower(&scene).expect("lower ordered detailed line");
        assert_eq!(lowered.derived_tables().len(), 2);

        lowered
            .apply_to_scene(&mut scene)
            .expect("apply ordered detailed line");
        let first = scene
            .tables
            .get(&lowered.derived_tables()[0])
            .expect("first detailed series");
        let second = scene
            .tables
            .get(&lowered.derived_tables()[1])
            .expect("second detailed series");
        assert_eq!(first.row_keys, vec![901, 900]);
        assert_eq!(second.row_keys, vec![903, 902]);

        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("ordered detailed marks");
        assert_eq!(marks.len(), 2);
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
                .all(|mark| matches!(mark.encodings, MarkEncodings::Path(_)))
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
    fn layered_rule_child_can_draw_an_aggregated_mean_threshold() {
        let mut scene = Scene::new();
        let table_id = TableId(822);
        let mut table = Table::new(table_id);
        table.row_keys = vec![610, 611, 612, 613];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0, 2.0, 3.0],
            b: vec![1.0, 2.0, 3.0, 4.0],
        }));
        scene.insert_table(table);

        let spec = LayerSpec::new(0xF1A8, TableId(821), DataRef::Table(table_id))
            .with_child(
                LayerChildSpec::new(MarkDef::Line)
                    .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
                    .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("value")),
            )
            .with_child(
                LayerChildSpec::new(MarkDef::Rule)
                    .with_transform(TransformSpec::aggregate(
                        vec![],
                        vec![AggregateField {
                            op: AggregateOp::Mean,
                            input: ColumnId(1),
                            output: ColumnId(2),
                        }],
                    ))
                    .with_y(ChannelDef::quantitative(ColumnId(2)).with_title("mean"))
                    .with_stroke_style(StrokeStyle::solid(css::TOMATO, 2.0)),
            );

        let lowered = spec
            .lower_into_scene(&mut scene)
            .expect("lower layered mean rule");
        assert_eq!(lowered.derived_tables().len(), 1);

        let (_layout, diffs) = lowered
            .tick(&mut scene, &HeuristicTextMeasurer)
            .expect("tick layered mean rule");
        let mut strokes = Vec::new();
        let mut widths = Vec::new();
        for diff in diffs {
            let MarkDiff::Enter { new, .. } = diff else {
                continue;
            };
            let vizir_core::MarkPayload::Path(channels) = *new else {
                continue;
            };
            strokes.push(channels.stroke);
            widths.push(channels.stroke_width);
        }
        assert!(strokes.contains(&Brush::Solid(css::TOMATO)));
        assert!(widths.iter().any(|width| (*width - 2.0).abs() < 1e-9));
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
    fn layer_children_inherit_base_child_positional_defaults() {
        let mut scene = Scene::new();
        let table_id = TableId(85);
        let mut table = Table::new(table_id);
        table.row_keys = vec![850, 851, 852];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![4.0, 5.0, 6.0],
            c: vec![1.0, 2.0, 3.0],
            d: vec![2.0, 3.0, 4.0],
        }));
        scene.insert_table(table);

        let spec = LayerSpec::new(0xF1D0, TableId(850), DataRef::Table(table_id))
            .with_child(
                LayerChildSpec::new(MarkDef::Area)
                    .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
                    .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("high"))
                    .with_y2(ChannelDef::quantitative(ColumnId(2)).with_title("low")),
            )
            .with_child(
                LayerChildSpec::new(MarkDef::Line)
                    .with_y(ChannelDef::quantitative(ColumnId(3)).with_title("line")),
            );

        let lowered = spec
            .lower(&scene)
            .expect("lower layer with base-child defaults");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("layer marks");
        assert_eq!(marks.len(), 2);
    }

    #[test]
    fn layer_children_can_be_fully_specified_unit_entries() {
        let mut scene = Scene::new();
        let table_id = TableId(86);
        let mut table = Table::new(table_id);
        table.row_keys = vec![860, 861, 862];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![3.0, 4.0, 5.0],
        }));
        scene.insert_table(table);

        let spec = LayerSpec::new(0xF1E0, TableId(860), DataRef::Table(table_id))
            .with_child(
                LayerChildSpec::new(MarkDef::Line)
                    .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
                    .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("value")),
            )
            .with_child(
                LayerChildSpec::new(MarkDef::Point)
                    .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
                    .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("value")),
            );

        let lowered = spec
            .lower(&scene)
            .expect("lower layer with fully specified child units");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let marks = lowered
            .series_marks(&scene, layout.data)
            .expect("layer marks");
        assert_eq!(marks.len(), 4);
    }

    #[test]
    fn layer_child_literal_styles_apply_to_rendered_marks() {
        let mut scene = Scene::new();
        let table_id = TableId(861);
        let mut table = Table::new(table_id);
        table.row_keys = vec![8600, 8601, 8602];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![4.0, 5.0, 6.0],
            c: vec![1.0, 2.0, 3.0],
            d: vec![2.0, 3.0, 4.0],
        }));
        scene.insert_table(table);

        let spec = LayerSpec::new(0xF1E8, TableId(861), DataRef::Table(table_id))
            .with_child(
                LayerChildSpec::new(MarkDef::Area)
                    .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
                    .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("high"))
                    .with_y2(ChannelDef::quantitative(ColumnId(2)).with_title("low"))
                    .with_fill_style(css::CORNFLOWER_BLUE)
                    .with_stroke_style(StrokeStyle::solid(css::CORNFLOWER_BLUE, 1.0))
                    .with_opacity_value(0.25),
            )
            .with_child(
                LayerChildSpec::new(MarkDef::Line)
                    .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
                    .with_y(ChannelDef::quantitative(ColumnId(3)).with_title("line"))
                    .with_stroke_style(StrokeStyle::solid(css::BLACK, 2.5)),
            );

        let (_layout, diffs) = spec
            .lower_into_scene(&mut scene)
            .expect("lower styled layer")
            .tick(&mut scene, &HeuristicTextMeasurer)
            .expect("tick styled layer");

        let mut fills = Vec::new();
        let mut strokes = Vec::new();
        let mut widths = Vec::new();
        for diff in diffs {
            let MarkDiff::Enter { new, .. } = diff else {
                continue;
            };
            let vizir_core::MarkPayload::Path(channels) = *new else {
                continue;
            };
            fills.push(channels.fill);
            strokes.push(channels.stroke);
            widths.push(channels.stroke_width);
        }
        assert!(fills.contains(&Brush::Solid(css::CORNFLOWER_BLUE.with_alpha(0.25))));
        assert!(strokes.contains(&Brush::Solid(css::BLACK)));
        assert!(widths.iter().any(|width| (*width - 2.5).abs() < 1e-9));
    }

    #[test]
    fn layer_lowering_rejects_conflicting_child_x_channel() {
        let mut scene = Scene::new();
        let table_id = TableId(87);
        let mut table = Table::new(table_id);
        table.row_keys = vec![870, 871, 872];
        table.data = Some(Box::new(ThreeCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![3.0, 4.0, 5.0],
            c: vec![10.0, 11.0, 12.0],
        }));
        scene.insert_table(table);

        let spec = LayerSpec::new(0xF1F0, TableId(870), DataRef::Table(table_id))
            .with_child(
                LayerChildSpec::new(MarkDef::Line)
                    .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
                    .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("value")),
            )
            .with_child(
                LayerChildSpec::new(MarkDef::Point)
                    .with_x(ChannelDef::quantitative(ColumnId(2)).with_title("other x"))
                    .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("value")),
            );

        let err = spec
            .lower(&scene)
            .expect_err("conflicting child x should fail");
        assert!(matches!(
            err,
            LoweringError::Unsupported(message)
                if message.contains("same x channel")
        ));
    }

    #[test]
    fn layer_lowering_rejects_child_local_color_channel() {
        let mut scene = Scene::new();
        let table_id = TableId(88);
        let mut table = Table::new(table_id);
        table.row_keys = vec![880, 881];
        table.data = Some(Box::new(ThreeCols {
            a: vec![0.0, 1.0],
            b: vec![3.0, 4.0],
            c: vec![0.0, 1.0],
        }));
        scene.insert_table(table);

        let spec = LayerSpec::new(0xF200, TableId(880), DataRef::Table(table_id))
            .with_child(
                LayerChildSpec::new(MarkDef::Line)
                    .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
                    .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("value")),
            )
            .with_child(
                LayerChildSpec::new(MarkDef::Point)
                    .with_x(ChannelDef::quantitative(ColumnId(0)).with_title("x"))
                    .with_y(ChannelDef::quantitative(ColumnId(1)).with_title("value"))
                    .with_color(ChannelDef::nominal(ColumnId(2)).with_title("series")),
            );

        let err = spec
            .lower(&scene)
            .expect_err("child-local color should fail");
        assert!(matches!(
            err,
            LoweringError::Unsupported(message)
                if message.contains("child-local color")
        ));
    }

    #[test]
    fn unit_lowering_rejects_color_and_detail_together() {
        let mut scene = Scene::new();
        let table_id = TableId(89);
        let mut table = Table::new(table_id);
        table.row_keys = vec![890, 891];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0],
            b: vec![3.0, 4.0],
            c: vec![0.0, 1.0],
            d: vec![0.0, 1.0],
        }));
        scene.insert_table(table);

        let spec = UnitSpec::new(
            0xF210,
            TableId(890),
            DataRef::Table(table_id),
            MarkDef::Line,
        )
        .with_x(ChannelDef::quantitative(ColumnId(0)))
        .with_y(ChannelDef::quantitative(ColumnId(1)))
        .with_color(ChannelDef::nominal(ColumnId(2)))
        .with_detail(ChannelDef::nominal(ColumnId(3)));

        let err = spec.lower(&scene).expect_err("color + detail should fail");
        assert!(matches!(
            err,
            LoweringError::Unsupported(message)
                if message.contains("color and detail")
        ));
    }

    #[test]
    fn layer_lowering_rejects_bar_with_non_bar_marks() {
        let scene = Scene::new();
        let spec = LayerSpec::new(0xF220, TableId(900), DataRef::Table(TableId(1)))
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
