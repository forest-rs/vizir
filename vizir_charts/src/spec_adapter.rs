// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Parser-facing adapters for the experimental authored spec seam.
//!
//! The types in [`crate::spec`] are good lowering targets, but they still speak in runtime-facing
//! identifiers such as [`vizir_core::ColumnId`]. This module adds a thin, schema-aware adapter
//! layer that accepts field names and resolves them into a [`crate::UnitSpec`].

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use peniko::Brush;
use vizir_core::{ColumnId, TableId};
use vizir_transforms::{
    AggregateField, AggregateOp, CalculateExpr, CalculateOp, CalculateOperand, CompareOp,
    LookupField, Predicate, SortOrder, StackOffset, WindowField, WindowOp,
};

use crate::{
    ChannelDef, DataRef, FacetSpec, FieldKind, LayerChildSpec, LayerSpec, MarkDef, StrokeStyle,
    TransformSpec, UnitSpec,
};

/// Context needed to adapt a parsed unit spec into a concrete [`UnitSpec`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdaptContext {
    /// Stable mark-id base for the adapted spec.
    pub id_base: u64,
    /// Base table id used for any derived tables written by the adapted spec.
    pub derived_table_base: TableId,
    /// Input data reference for the adapted spec.
    pub data: DataRef,
}

/// Errors returned while adapting a parsed spec into [`UnitSpec`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdaptError {
    /// A referenced field name was not present in the resolver or prior derived outputs.
    UnknownField {
        /// The unresolved field name.
        field: String,
        /// The role that was trying to resolve the field.
        role: &'static str,
    },
    /// A transform attempted to bind a derived output name that already referred to another field.
    DerivedFieldConflict {
        /// The conflicting derived field name.
        field: String,
    },
}

/// Resolves authored field names to concrete [`ColumnId`]s.
pub trait FieldResolver {
    /// Returns the column id for the given field name.
    fn resolve_column(&self, field: &str) -> Option<ColumnId>;

    /// Returns the maximum column id already present in the input schema, if any.
    fn max_column_id(&self) -> Option<ColumnId>;
}

/// One schema entry for [`SliceFieldResolver`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemaField<'a> {
    /// Authored field name.
    pub name: &'a str,
    /// Concrete column id in the input table.
    pub column: ColumnId,
}

/// A simple slice-backed [`FieldResolver`] for tests and small adapters.
#[derive(Clone, Copy, Debug)]
pub struct SliceFieldResolver<'a> {
    fields: &'a [SchemaField<'a>],
}

impl<'a> SliceFieldResolver<'a> {
    /// Creates a slice-backed field resolver.
    pub fn new(fields: &'a [SchemaField<'a>]) -> Self {
        Self { fields }
    }
}

impl FieldResolver for SliceFieldResolver<'_> {
    fn resolve_column(&self, field: &str) -> Option<ColumnId> {
        self.fields
            .iter()
            .find(|entry| entry.name == field)
            .map(|entry| entry.column)
    }

    fn max_column_id(&self) -> Option<ColumnId> {
        self.fields
            .iter()
            .map(|entry| entry.column)
            .max_by_key(|col| col.0)
    }
}

/// A parsed authored mark kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParsedMarkDef {
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

/// A parsed field kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParsedFieldKind {
    /// Continuous numeric data.
    Quantitative,
    /// Ordered categories.
    Ordinal,
    /// Unordered categories.
    Nominal,
    /// Time values represented as numeric seconds.
    Temporal,
}

/// A parsed channel definition that still refers to a field by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedChannelDef {
    field: String,
    kind: ParsedFieldKind,
    aggregate: Option<AggregateOp>,
    title: Option<String>,
}

impl ParsedChannelDef {
    /// Creates a quantitative channel over the given field.
    pub fn quantitative(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            kind: ParsedFieldKind::Quantitative,
            aggregate: None,
            title: None,
        }
    }

    /// Creates an ordinal channel over the given field.
    pub fn ordinal(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            kind: ParsedFieldKind::Ordinal,
            aggregate: None,
            title: None,
        }
    }

    /// Creates a nominal channel over the given field.
    pub fn nominal(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            kind: ParsedFieldKind::Nominal,
            aggregate: None,
            title: None,
        }
    }

    /// Creates a temporal channel over the given field.
    pub fn temporal(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            kind: ParsedFieldKind::Temporal,
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
}

/// Parsed encodings for one unit chart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedEncodingSet {
    x: Option<ParsedChannelDef>,
    x2: Option<ParsedChannelDef>,
    y: Option<ParsedChannelDef>,
    y2: Option<ParsedChannelDef>,
    color: Option<ParsedChannelDef>,
    size: Option<ParsedChannelDef>,
    shape: Option<ParsedChannelDef>,
    opacity: Option<ParsedChannelDef>,
    stroke: Option<ParsedChannelDef>,
    stroke_width: Option<ParsedChannelDef>,
    order: Option<ParsedChannelDef>,
    detail: Option<ParsedChannelDef>,
    text: Option<ParsedChannelDef>,
}

impl ParsedEncodingSet {
    /// Creates an empty parsed encoding set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the x channel.
    pub fn with_x(mut self, x: ParsedChannelDef) -> Self {
        self.x = Some(x);
        self
    }

    /// Sets the x2 channel.
    pub fn with_x2(mut self, x2: ParsedChannelDef) -> Self {
        self.x2 = Some(x2);
        self
    }

    /// Sets the y channel.
    pub fn with_y(mut self, y: ParsedChannelDef) -> Self {
        self.y = Some(y);
        self
    }

    /// Sets the y2 channel.
    pub fn with_y2(mut self, y2: ParsedChannelDef) -> Self {
        self.y2 = Some(y2);
        self
    }

    /// Sets the color channel.
    pub fn with_color(mut self, color: ParsedChannelDef) -> Self {
        self.color = Some(color);
        self
    }

    /// Sets the size channel.
    pub fn with_size_channel(mut self, size: ParsedChannelDef) -> Self {
        self.size = Some(size);
        self
    }

    /// Sets the shape channel.
    pub fn with_shape(mut self, shape: ParsedChannelDef) -> Self {
        self.shape = Some(shape);
        self
    }

    /// Sets the opacity channel.
    pub fn with_opacity(mut self, opacity: ParsedChannelDef) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// Sets the stroke channel.
    pub fn with_stroke(mut self, stroke: ParsedChannelDef) -> Self {
        self.stroke = Some(stroke);
        self
    }

    /// Sets the stroke width channel.
    pub fn with_stroke_width(mut self, stroke_width: ParsedChannelDef) -> Self {
        self.stroke_width = Some(stroke_width);
        self
    }

    /// Sets the order channel.
    pub fn with_order(mut self, order: ParsedChannelDef) -> Self {
        self.order = Some(order);
        self
    }

    /// Sets the detail channel.
    pub fn with_detail(mut self, detail: ParsedChannelDef) -> Self {
        self.detail = Some(detail);
        self
    }

    /// Sets the text channel.
    pub fn with_text(mut self, text: ParsedChannelDef) -> Self {
        self.text = Some(text);
        self
    }
}

/// A parsed aggregate field that still refers to field names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedAggregateField {
    /// Aggregate operation.
    pub op: AggregateOp,
    /// Input field name.
    pub field: String,
    /// Output field name.
    pub as_field: String,
}

impl ParsedAggregateField {
    /// Creates a parsed aggregate field.
    pub fn new(op: AggregateOp, field: impl Into<String>, as_field: impl Into<String>) -> Self {
        Self {
            op,
            field: field.into(),
            as_field: as_field.into(),
        }
    }
}

/// A parsed predicate that still refers to a field name.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedPredicate {
    /// Field name to read.
    pub field: String,
    /// Comparison operator.
    pub op: CompareOp,
    /// Right-hand constant value.
    pub value: f64,
}

impl ParsedPredicate {
    /// Creates a parsed predicate.
    pub fn new(field: impl Into<String>, op: CompareOp, value: f64) -> Self {
        Self {
            field: field.into(),
            op,
            value,
        }
    }
}

/// One operand in a parsed narrow calculate expression.
#[derive(Clone, Debug, PartialEq)]
pub enum ParsedCalculateOperand {
    /// Read from the named field.
    Field(String),
    /// Use a numeric literal.
    Constant(f64),
}

impl ParsedCalculateOperand {
    /// Creates a field operand.
    pub fn field(field: impl Into<String>) -> Self {
        Self::Field(field.into())
    }

    /// Creates a numeric literal operand.
    pub fn constant(value: f64) -> Self {
        Self::Constant(value)
    }
}

/// A parsed narrow arithmetic expression that still refers to field names.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedCalculateExpr {
    /// Left operand.
    pub left: ParsedCalculateOperand,
    /// Arithmetic operator.
    pub op: CalculateOp,
    /// Right operand.
    pub right: ParsedCalculateOperand,
}

impl ParsedCalculateExpr {
    /// Creates a parsed calculate expression.
    pub fn new(
        left: ParsedCalculateOperand,
        op: CalculateOp,
        right: ParsedCalculateOperand,
    ) -> Self {
        Self { left, op, right }
    }
}

/// One derived output field in a parsed narrow window transform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedWindowField {
    /// Window operation to compute.
    pub op: WindowOp,
    /// Output field name.
    pub as_field: String,
}

impl ParsedWindowField {
    /// Creates a parsed window field.
    pub fn new(op: WindowOp, as_field: impl Into<String>) -> Self {
        Self {
            op,
            as_field: as_field.into(),
        }
    }
}

/// One output field in a parsed narrow lookup transform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedLookupField {
    /// Field name to read from the lookup table.
    pub field: String,
    /// Output field name in the enriched result.
    pub as_field: String,
}

impl ParsedLookupField {
    /// Creates a parsed lookup field.
    pub fn new(field: impl Into<String>, as_field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            as_field: as_field.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ParsedTransformKind {
    Filter {
        predicate: ParsedPredicate,
        columns: Vec<String>,
    },
    Sort {
        by: String,
        order: SortOrder,
        columns: Vec<String>,
    },
    Calculate {
        expr: ParsedCalculateExpr,
        as_field: String,
        columns: Vec<String>,
    },
    JoinAggregate {
        group_by: Vec<String>,
        fields: Vec<ParsedAggregateField>,
        columns: Vec<String>,
    },
    Aggregate {
        group_by: Vec<String>,
        fields: Vec<ParsedAggregateField>,
    },
    Bin {
        field: String,
        as_start: String,
        step: f64,
        columns: Vec<String>,
    },
    Fold {
        fields: Vec<String>,
        as_key: String,
        as_value: String,
        columns: Vec<String>,
    },
    Lookup {
        from_table: TableId,
        key: String,
        from_key: String,
        fields: Vec<ParsedLookupField>,
        columns: Vec<String>,
    },
    Window {
        group_by: Vec<String>,
        sort_by: String,
        sort_order: SortOrder,
        fields: Vec<ParsedWindowField>,
        columns: Vec<String>,
    },
    Stack {
        group_by: Vec<String>,
        offset: StackOffset,
        sort_by: Option<String>,
        sort_order: SortOrder,
        field: String,
        as_start: String,
        as_end: String,
        columns: Vec<String>,
    },
}

/// A parsed transform specification that still refers to field names.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedTransformSpec {
    kind: ParsedTransformKind,
}

impl ParsedTransformSpec {
    /// Creates a parsed filter transform.
    pub fn filter(
        predicate: ParsedPredicate,
        columns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            kind: ParsedTransformKind::Filter {
                predicate,
                columns: collect_names(columns),
            },
        }
    }

    /// Creates a parsed sort transform.
    pub fn sort(
        by: impl Into<String>,
        order: SortOrder,
        columns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            kind: ParsedTransformKind::Sort {
                by: by.into(),
                order,
                columns: collect_names(columns),
            },
        }
    }

    /// Creates a parsed narrow arithmetic calculate transform.
    pub fn calculate(
        expr: ParsedCalculateExpr,
        as_field: impl Into<String>,
        columns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            kind: ParsedTransformKind::Calculate {
                expr,
                as_field: as_field.into(),
                columns: collect_names(columns),
            },
        }
    }

    /// Creates a parsed joinaggregate transform.
    pub fn joinaggregate(
        group_by: impl IntoIterator<Item = impl Into<String>>,
        fields: Vec<ParsedAggregateField>,
        columns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            kind: ParsedTransformKind::JoinAggregate {
                group_by: collect_names(group_by),
                fields,
                columns: collect_names(columns),
            },
        }
    }

    /// Creates a parsed aggregate transform.
    pub fn aggregate(
        group_by: impl IntoIterator<Item = impl Into<String>>,
        fields: Vec<ParsedAggregateField>,
    ) -> Self {
        Self {
            kind: ParsedTransformKind::Aggregate {
                group_by: collect_names(group_by),
                fields,
            },
        }
    }

    /// Creates a parsed narrow fold transform.
    pub fn fold(
        fields: impl IntoIterator<Item = impl Into<String>>,
        as_key: impl Into<String>,
        as_value: impl Into<String>,
        columns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            kind: ParsedTransformKind::Fold {
                fields: collect_names(fields),
                as_key: as_key.into(),
                as_value: as_value.into(),
                columns: collect_names(columns),
            },
        }
    }

    /// Creates a parsed narrow lookup transform.
    pub fn lookup(
        from_table: TableId,
        key: impl Into<String>,
        from_key: impl Into<String>,
        fields: Vec<ParsedLookupField>,
        columns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            kind: ParsedTransformKind::Lookup {
                from_table,
                key: key.into(),
                from_key: from_key.into(),
                fields,
                columns: collect_names(columns),
            },
        }
    }

    /// Creates a parsed narrow window transform.
    pub fn window(
        group_by: impl IntoIterator<Item = impl Into<String>>,
        sort_by: impl Into<String>,
        sort_order: SortOrder,
        fields: Vec<ParsedWindowField>,
        columns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            kind: ParsedTransformKind::Window {
                group_by: collect_names(group_by),
                sort_by: sort_by.into(),
                sort_order,
                fields,
                columns: collect_names(columns),
            },
        }
    }

    /// Creates a parsed fixed-step bin transform.
    pub fn bin(
        field: impl Into<String>,
        as_start: impl Into<String>,
        step: f64,
        columns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            kind: ParsedTransformKind::Bin {
                field: field.into(),
                as_start: as_start.into(),
                step,
                columns: collect_names(columns),
            },
        }
    }

    /// Creates a parsed stack transform.
    #[allow(
        clippy::too_many_arguments,
        reason = "matches the authored stack parameters directly"
    )]
    pub fn stack(
        group_by: impl IntoIterator<Item = impl Into<String>>,
        offset: StackOffset,
        sort_by: Option<impl Into<String>>,
        sort_order: SortOrder,
        field: impl Into<String>,
        as_start: impl Into<String>,
        as_end: impl Into<String>,
        columns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            kind: ParsedTransformKind::Stack {
                group_by: collect_names(group_by),
                offset,
                sort_by: sort_by.map(|field| field.into()),
                sort_order,
                field: field.into(),
                as_start: as_start.into(),
                as_end: as_end.into(),
                columns: collect_names(columns),
            },
        }
    }
}

/// A parsed unit chart that still refers to fields by name.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedUnitSpec {
    mark: ParsedMarkDef,
    transforms: Vec<ParsedTransformSpec>,
    encoding: ParsedEncodingSet,
    width: f64,
    height: f64,
    title: Option<String>,
}

impl ParsedUnitSpec {
    /// Creates a new parsed unit spec.
    pub fn new(mark: ParsedMarkDef) -> Self {
        Self {
            mark,
            transforms: Vec::new(),
            encoding: ParsedEncodingSet::new(),
            width: 220.0,
            height: 120.0,
            title: None,
        }
    }

    /// Sets the plot size used by the adapted chart.
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
    pub fn with_encoding(mut self, encoding: ParsedEncodingSet) -> Self {
        self.encoding = encoding;
        self
    }

    /// Sets the x channel.
    pub fn with_x(mut self, x: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_x(x);
        self
    }

    /// Sets the x2 channel.
    pub fn with_x2(mut self, x2: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_x2(x2);
        self
    }

    /// Sets the y channel.
    pub fn with_y(mut self, y: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_y(y);
        self
    }

    /// Sets the y2 channel.
    pub fn with_y2(mut self, y2: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_y2(y2);
        self
    }

    /// Sets the color channel.
    pub fn with_color(mut self, color: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_color(color);
        self
    }

    /// Sets the size channel.
    pub fn with_size_channel(mut self, size: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_size_channel(size);
        self
    }

    /// Sets the shape channel.
    pub fn with_shape(mut self, shape: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_shape(shape);
        self
    }

    /// Sets the opacity channel.
    pub fn with_opacity(mut self, opacity: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_opacity(opacity);
        self
    }

    /// Sets the stroke channel.
    pub fn with_stroke(mut self, stroke: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke(stroke);
        self
    }

    /// Sets the stroke width channel.
    pub fn with_stroke_width(mut self, stroke_width: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke_width(stroke_width);
        self
    }

    /// Sets the order channel.
    pub fn with_order(mut self, order: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_order(order);
        self
    }

    /// Sets the detail channel.
    pub fn with_detail(mut self, detail: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_detail(detail);
        self
    }

    /// Sets the text channel.
    pub fn with_text(mut self, text: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_text(text);
        self
    }

    /// Appends a parsed transform.
    pub fn with_transform(mut self, transform: ParsedTransformSpec) -> Self {
        self.transforms.push(transform);
        self
    }

    /// Adapts this parsed unit spec into a concrete [`UnitSpec`].
    pub fn adapt(
        &self,
        resolver: &impl FieldResolver,
        context: AdaptContext,
    ) -> Result<UnitSpec, AdaptError> {
        let mut fields = FieldBindings::new(resolver);

        let mut unit = UnitSpec::new(
            context.id_base,
            context.derived_table_base,
            context.data,
            adapt_mark(self.mark),
        )
        .with_size(self.width, self.height);
        if let Some(title) = &self.title {
            unit = unit.with_title(title.clone());
        }

        for transform in &self.transforms {
            unit = unit.with_transform(adapt_transform(transform, &mut fields)?);
        }

        if let Some(x) = &self.encoding.x {
            unit = unit.with_x(adapt_channel(x, "x", &mut fields)?);
        }
        if let Some(x2) = &self.encoding.x2 {
            unit = unit.with_x2(adapt_channel(x2, "x2", &mut fields)?);
        }
        if let Some(y) = &self.encoding.y {
            unit = unit.with_y(adapt_channel(y, "y", &mut fields)?);
        }
        if let Some(y2) = &self.encoding.y2 {
            unit = unit.with_y2(adapt_channel(y2, "y2", &mut fields)?);
        }
        if let Some(color) = &self.encoding.color {
            unit = unit.with_color(adapt_channel(color, "color", &mut fields)?);
        }
        if let Some(size) = &self.encoding.size {
            unit = unit.with_size_channel(adapt_channel(size, "size", &mut fields)?);
        }
        if let Some(shape) = &self.encoding.shape {
            unit = unit.with_shape(adapt_channel(shape, "shape", &mut fields)?);
        }
        if let Some(opacity) = &self.encoding.opacity {
            unit = unit.with_opacity(adapt_channel(opacity, "opacity", &mut fields)?);
        }
        if let Some(stroke) = &self.encoding.stroke {
            unit = unit.with_stroke(adapt_channel(stroke, "stroke", &mut fields)?);
        }
        if let Some(stroke_width) = &self.encoding.stroke_width {
            unit = unit.with_stroke_width(adapt_channel(stroke_width, "strokeWidth", &mut fields)?);
        }
        if let Some(order) = &self.encoding.order {
            unit = unit.with_order(adapt_channel(order, "order", &mut fields)?);
        }
        if let Some(detail) = &self.encoding.detail {
            unit = unit.with_detail(adapt_channel(detail, "detail", &mut fields)?);
        }
        if let Some(text) = &self.encoding.text {
            unit = unit.with_text(adapt_channel(text, "text", &mut fields)?);
        }

        Ok(unit)
    }
}

/// A parsed shared-plot layer spec that still refers to fields by name.
#[derive(Clone, Debug, PartialEq)]
struct ParsedLayerChildStyle {
    fill: Option<Brush>,
    stroke: Option<StrokeStyle>,
    opacity: Option<f64>,
}

/// A parsed shared-plot layer spec that still refers to fields by name.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedLayerChildSpec {
    mark: ParsedMarkDef,
    transforms: Vec<ParsedTransformSpec>,
    encoding: ParsedEncodingSet,
    style: ParsedLayerChildStyle,
}

impl ParsedLayerChildSpec {
    /// Creates a new parsed layer child for the given mark.
    pub fn new(mark: ParsedMarkDef) -> Self {
        Self {
            mark,
            transforms: Vec::new(),
            encoding: ParsedEncodingSet::new(),
            style: ParsedLayerChildStyle {
                fill: None,
                stroke: None,
                opacity: None,
            },
        }
    }

    /// Appends a child-local transform.
    pub fn with_transform(mut self, transform: ParsedTransformSpec) -> Self {
        self.transforms.push(transform);
        self
    }

    /// Replaces the child override encoding set.
    pub fn with_encoding(mut self, encoding: ParsedEncodingSet) -> Self {
        self.encoding = encoding;
        self
    }

    /// Sets the child x override.
    pub fn with_x(mut self, x: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_x(x);
        self
    }

    /// Sets the child x2 override.
    pub fn with_x2(mut self, x2: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_x2(x2);
        self
    }

    /// Sets the child y override.
    pub fn with_y(mut self, y: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_y(y);
        self
    }

    /// Sets the child y2 override.
    pub fn with_y2(mut self, y2: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_y2(y2);
        self
    }

    /// Sets the child color override.
    pub fn with_color(mut self, color: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_color(color);
        self
    }

    /// Sets the child size override.
    pub fn with_size_channel(mut self, size: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_size_channel(size);
        self
    }

    /// Sets the child shape override.
    pub fn with_shape(mut self, shape: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_shape(shape);
        self
    }

    /// Sets the child opacity override.
    pub fn with_opacity(mut self, opacity: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_opacity(opacity);
        self
    }

    /// Sets the child stroke override.
    pub fn with_stroke(mut self, stroke: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke(stroke);
        self
    }

    /// Sets the child stroke width override.
    pub fn with_stroke_width(mut self, stroke_width: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke_width(stroke_width);
        self
    }

    /// Sets the child order override.
    pub fn with_order(mut self, order: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_order(order);
        self
    }

    /// Sets the child detail override.
    pub fn with_detail(mut self, detail: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_detail(detail);
        self
    }

    /// Sets the child text override.
    pub fn with_text(mut self, text: ParsedChannelDef) -> Self {
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
}

/// A parsed one-field facet spec that still refers to fields by name.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedFacetSpec {
    facet: ParsedChannelDef,
    unit: ParsedUnitSpec,
    title: Option<String>,
    columns: usize,
    spacing: f64,
}

impl ParsedFacetSpec {
    /// Creates a new parsed facet spec over a unit-shaped child mark.
    pub fn new(facet: ParsedChannelDef, mark: ParsedMarkDef) -> Self {
        Self {
            facet,
            unit: ParsedUnitSpec::new(mark),
            title: None,
            columns: 2,
            spacing: 24.0,
        }
    }

    /// Sets the outer facet title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the cell plot size.
    pub fn with_size(mut self, width: f64, height: f64) -> Self {
        self.unit = self.unit.with_size(width, height);
        self
    }

    /// Sets the number of facet columns.
    pub fn with_columns(mut self, columns: usize) -> Self {
        self.columns = columns.max(1);
        self
    }

    /// Sets the spacing between facet cells.
    pub fn with_spacing(mut self, spacing: f64) -> Self {
        self.spacing = spacing;
        self
    }

    /// Replaces the child encoding set.
    pub fn with_encoding(mut self, encoding: ParsedEncodingSet) -> Self {
        self.unit = self.unit.with_encoding(encoding);
        self
    }

    /// Sets the child x channel.
    pub fn with_x(mut self, x: ParsedChannelDef) -> Self {
        self.unit = self.unit.with_x(x);
        self
    }

    /// Sets the child x2 channel.
    pub fn with_x2(mut self, x2: ParsedChannelDef) -> Self {
        self.unit = self.unit.with_x2(x2);
        self
    }

    /// Sets the child y channel.
    pub fn with_y(mut self, y: ParsedChannelDef) -> Self {
        self.unit = self.unit.with_y(y);
        self
    }

    /// Sets the child y2 channel.
    pub fn with_y2(mut self, y2: ParsedChannelDef) -> Self {
        self.unit = self.unit.with_y2(y2);
        self
    }

    /// Sets the child color channel.
    pub fn with_color(mut self, color: ParsedChannelDef) -> Self {
        self.unit = self.unit.with_color(color);
        self
    }

    /// Sets the child size channel.
    pub fn with_size_channel(mut self, size: ParsedChannelDef) -> Self {
        self.unit = self.unit.with_size_channel(size);
        self
    }

    /// Sets the child shape channel.
    pub fn with_shape(mut self, shape: ParsedChannelDef) -> Self {
        self.unit = self.unit.with_shape(shape);
        self
    }

    /// Sets the child opacity channel.
    pub fn with_opacity(mut self, opacity: ParsedChannelDef) -> Self {
        self.unit = self.unit.with_opacity(opacity);
        self
    }

    /// Sets the child stroke channel.
    pub fn with_stroke(mut self, stroke: ParsedChannelDef) -> Self {
        self.unit = self.unit.with_stroke(stroke);
        self
    }

    /// Sets the child stroke-width channel.
    pub fn with_stroke_width(mut self, stroke_width: ParsedChannelDef) -> Self {
        self.unit = self.unit.with_stroke_width(stroke_width);
        self
    }

    /// Sets the child order channel.
    pub fn with_order(mut self, order: ParsedChannelDef) -> Self {
        self.unit = self.unit.with_order(order);
        self
    }

    /// Sets the child detail channel.
    pub fn with_detail(mut self, detail: ParsedChannelDef) -> Self {
        self.unit = self.unit.with_detail(detail);
        self
    }

    /// Sets the child text channel.
    pub fn with_text(mut self, text: ParsedChannelDef) -> Self {
        self.unit = self.unit.with_text(text);
        self
    }

    /// Appends a parsed transform to the child unit.
    pub fn with_transform(mut self, transform: ParsedTransformSpec) -> Self {
        self.unit = self.unit.with_transform(transform);
        self
    }

    /// Adapts this parsed facet spec into a concrete [`FacetSpec`].
    pub fn adapt(
        &self,
        resolver: &impl FieldResolver,
        context: AdaptContext,
    ) -> Result<FacetSpec, AdaptError> {
        let mut fields = FieldBindings::new(resolver);
        let facet = adapt_channel(&self.facet, "facet", &mut fields)?;
        let mut out = FacetSpec::new(
            context.id_base,
            context.derived_table_base,
            context.data,
            facet,
            adapt_mark(self.unit.mark),
        )
        .with_size(self.unit.width, self.unit.height)
        .with_columns(self.columns)
        .with_spacing(self.spacing);
        if let Some(title) = &self.title {
            out = out.with_title(title.clone());
        }
        for transform in &self.unit.transforms {
            out = out.with_transform(adapt_transform(transform, &mut fields)?);
        }
        if let Some(x) = &self.unit.encoding.x {
            out = out.with_x(adapt_channel(x, "x", &mut fields)?);
        }
        if let Some(x2) = &self.unit.encoding.x2 {
            out = out.with_x2(adapt_channel(x2, "x2", &mut fields)?);
        }
        if let Some(y) = &self.unit.encoding.y {
            out = out.with_y(adapt_channel(y, "y", &mut fields)?);
        }
        if let Some(y2) = &self.unit.encoding.y2 {
            out = out.with_y2(adapt_channel(y2, "y2", &mut fields)?);
        }
        if let Some(color) = &self.unit.encoding.color {
            out = out.with_color(adapt_channel(color, "color", &mut fields)?);
        }
        if let Some(size) = &self.unit.encoding.size {
            out = out.with_size_channel(adapt_channel(size, "size", &mut fields)?);
        }
        if let Some(shape) = &self.unit.encoding.shape {
            out = out.with_shape(adapt_channel(shape, "shape", &mut fields)?);
        }
        if let Some(opacity) = &self.unit.encoding.opacity {
            out = out.with_opacity(adapt_channel(opacity, "opacity", &mut fields)?);
        }
        if let Some(stroke) = &self.unit.encoding.stroke {
            out = out.with_stroke(adapt_channel(stroke, "stroke", &mut fields)?);
        }
        if let Some(stroke_width) = &self.unit.encoding.stroke_width {
            out = out.with_stroke_width(adapt_channel(stroke_width, "strokeWidth", &mut fields)?);
        }
        if let Some(order) = &self.unit.encoding.order {
            out = out.with_order(adapt_channel(order, "order", &mut fields)?);
        }
        if let Some(detail) = &self.unit.encoding.detail {
            out = out.with_detail(adapt_channel(detail, "detail", &mut fields)?);
        }
        if let Some(text) = &self.unit.encoding.text {
            out = out.with_text(adapt_channel(text, "text", &mut fields)?);
        }
        Ok(out)
    }
}

/// A parsed shared-plot layer spec that still refers to fields by name.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedLayerSpec {
    children: Vec<ParsedLayerChildSpec>,
    transforms: Vec<ParsedTransformSpec>,
    encoding: ParsedEncodingSet,
    width: f64,
    height: f64,
    title: Option<String>,
}

impl Default for ParsedLayerSpec {
    fn default() -> Self {
        Self::new()
    }
}

impl ParsedLayerSpec {
    /// Creates a new empty parsed layer spec.
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            transforms: Vec::new(),
            encoding: ParsedEncodingSet::new(),
            width: 220.0,
            height: 120.0,
            title: None,
        }
    }

    /// Sets the plot size used by the adapted chart.
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
    pub fn with_encoding(mut self, encoding: ParsedEncodingSet) -> Self {
        self.encoding = encoding;
        self
    }

    /// Sets the x channel.
    pub fn with_x(mut self, x: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_x(x);
        self
    }

    /// Sets the x2 channel.
    pub fn with_x2(mut self, x2: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_x2(x2);
        self
    }

    /// Sets the y channel.
    pub fn with_y(mut self, y: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_y(y);
        self
    }

    /// Sets the y2 channel.
    pub fn with_y2(mut self, y2: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_y2(y2);
        self
    }

    /// Sets the color channel.
    pub fn with_color(mut self, color: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_color(color);
        self
    }

    /// Sets the opacity channel.
    pub fn with_opacity(mut self, opacity: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_opacity(opacity);
        self
    }

    /// Sets the stroke channel.
    pub fn with_stroke(mut self, stroke: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke(stroke);
        self
    }

    /// Sets the stroke width channel.
    pub fn with_stroke_width(mut self, stroke_width: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_stroke_width(stroke_width);
        self
    }

    /// Sets the order channel.
    pub fn with_order(mut self, order: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_order(order);
        self
    }

    /// Sets the detail channel.
    pub fn with_detail(mut self, detail: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_detail(detail);
        self
    }

    /// Sets the text channel.
    pub fn with_text(mut self, text: ParsedChannelDef) -> Self {
        self.encoding = self.encoding.with_text(text);
        self
    }

    /// Appends a parsed transform.
    pub fn with_transform(mut self, transform: ParsedTransformSpec) -> Self {
        self.transforms.push(transform);
        self
    }

    /// Appends a parsed mark layer.
    pub fn with_mark(mut self, mark: ParsedMarkDef) -> Self {
        self.children.push(ParsedLayerChildSpec::new(mark));
        self
    }

    /// Appends a parsed child with mark-specific encoding overrides.
    pub fn with_child(mut self, child: ParsedLayerChildSpec) -> Self {
        self.children.push(child);
        self
    }

    /// Adapts this parsed layer spec into a concrete [`LayerSpec`].
    pub fn adapt(
        &self,
        resolver: &impl FieldResolver,
        context: AdaptContext,
    ) -> Result<LayerSpec, AdaptError> {
        let mut fields = FieldBindings::new(resolver);
        let mut layer = LayerSpec::new(context.id_base, context.derived_table_base, context.data)
            .with_size(self.width, self.height);
        if let Some(title) = &self.title {
            layer = layer.with_title(title.clone());
        }

        for transform in &self.transforms {
            layer = layer.with_transform(adapt_transform(transform, &mut fields)?);
        }

        if let Some(x) = &self.encoding.x {
            layer = layer.with_x(adapt_channel(x, "x", &mut fields)?);
        }
        if let Some(x2) = &self.encoding.x2 {
            layer = layer.with_x2(adapt_channel(x2, "x2", &mut fields)?);
        }
        if let Some(y) = &self.encoding.y {
            layer = layer.with_y(adapt_channel(y, "y", &mut fields)?);
        }
        if let Some(y2) = &self.encoding.y2 {
            layer = layer.with_y2(adapt_channel(y2, "y2", &mut fields)?);
        }
        if let Some(color) = &self.encoding.color {
            layer = layer.with_color(adapt_channel(color, "color", &mut fields)?);
        }
        if let Some(size) = &self.encoding.size {
            layer = layer.with_size_channel(adapt_channel(size, "size", &mut fields)?);
        }
        if let Some(shape) = &self.encoding.shape {
            layer = layer.with_shape(adapt_channel(shape, "shape", &mut fields)?);
        }
        if let Some(opacity) = &self.encoding.opacity {
            layer = layer.with_opacity(adapt_channel(opacity, "opacity", &mut fields)?);
        }
        if let Some(stroke) = &self.encoding.stroke {
            layer = layer.with_stroke(adapt_channel(stroke, "stroke", &mut fields)?);
        }
        if let Some(stroke_width) = &self.encoding.stroke_width {
            layer =
                layer.with_stroke_width(adapt_channel(stroke_width, "strokeWidth", &mut fields)?);
        }
        if let Some(order) = &self.encoding.order {
            layer = layer.with_order(adapt_channel(order, "order", &mut fields)?);
        }
        if let Some(detail) = &self.encoding.detail {
            layer = layer.with_detail(adapt_channel(detail, "detail", &mut fields)?);
        }
        if let Some(text) = &self.encoding.text {
            layer = layer.with_text(adapt_channel(text, "text", &mut fields)?);
        }
        for child in &self.children {
            layer = layer.with_child(adapt_layer_child(child, &fields)?);
        }

        Ok(layer)
    }
}

fn collect_names(names: impl IntoIterator<Item = impl Into<String>>) -> Vec<String> {
    names.into_iter().map(|name| name.into()).collect()
}

fn adapt_mark(mark: ParsedMarkDef) -> MarkDef {
    match mark {
        ParsedMarkDef::Bar => MarkDef::Bar,
        ParsedMarkDef::Line => MarkDef::Line,
        ParsedMarkDef::Point => MarkDef::Point,
        ParsedMarkDef::Area => MarkDef::Area,
        ParsedMarkDef::Rule => MarkDef::Rule,
        ParsedMarkDef::Text => MarkDef::Text,
    }
}

fn adapt_field_kind(kind: ParsedFieldKind) -> FieldKind {
    match kind {
        ParsedFieldKind::Quantitative => FieldKind::Quantitative,
        ParsedFieldKind::Ordinal => FieldKind::Ordinal,
        ParsedFieldKind::Nominal => FieldKind::Nominal,
        ParsedFieldKind::Temporal => FieldKind::Temporal,
    }
}

fn adapt_channel(
    channel: &ParsedChannelDef,
    role: &'static str,
    fields: &mut FieldBindings<'_>,
) -> Result<ChannelDef, AdaptError> {
    let column = fields.resolve_input(&channel.field, role)?;
    let mut out = match adapt_field_kind(channel.kind) {
        FieldKind::Quantitative => ChannelDef::quantitative(column),
        FieldKind::Ordinal => ChannelDef::ordinal(column),
        FieldKind::Nominal => ChannelDef::nominal(column),
        FieldKind::Temporal => ChannelDef::temporal(column),
    };
    if let Some(aggregate) = channel.aggregate {
        out = out.with_aggregate(aggregate);
    }
    if let Some(title) = &channel.title {
        out = out.with_title(title.clone());
    }
    Ok(out)
}

fn adapt_layer_child(
    child: &ParsedLayerChildSpec,
    base_fields: &FieldBindings<'_>,
) -> Result<LayerChildSpec, AdaptError> {
    let mut fields = base_fields.clone();
    let mut out = LayerChildSpec::new(adapt_mark(child.mark));
    for transform in &child.transforms {
        out = out.with_transform(adapt_transform(transform, &mut fields)?);
    }
    if let Some(x) = &child.encoding.x {
        out = out.with_x(adapt_channel(x, "layer child x", &mut fields)?);
    }
    if let Some(x2) = &child.encoding.x2 {
        out = out.with_x2(adapt_channel(x2, "layer child x2", &mut fields)?);
    }
    if let Some(y) = &child.encoding.y {
        out = out.with_y(adapt_channel(y, "layer child y", &mut fields)?);
    }
    if let Some(y2) = &child.encoding.y2 {
        out = out.with_y2(adapt_channel(y2, "layer child y2", &mut fields)?);
    }
    if let Some(color) = &child.encoding.color {
        out = out.with_color(adapt_channel(color, "layer child color", &mut fields)?);
    }
    if let Some(size) = &child.encoding.size {
        out = out.with_size_channel(adapt_channel(size, "layer child size", &mut fields)?);
    }
    if let Some(shape) = &child.encoding.shape {
        out = out.with_shape(adapt_channel(shape, "layer child shape", &mut fields)?);
    }
    if let Some(opacity) = &child.encoding.opacity {
        out = out.with_opacity(adapt_channel(opacity, "layer child opacity", &mut fields)?);
    }
    if let Some(stroke) = &child.encoding.stroke {
        out = out.with_stroke(adapt_channel(stroke, "layer child stroke", &mut fields)?);
    }
    if let Some(stroke_width) = &child.encoding.stroke_width {
        out = out.with_stroke_width(adapt_channel(
            stroke_width,
            "layer child strokeWidth",
            &mut fields,
        )?);
    }
    if let Some(order) = &child.encoding.order {
        out = out.with_order(adapt_channel(order, "layer child order", &mut fields)?);
    }
    if let Some(detail) = &child.encoding.detail {
        out = out.with_detail(adapt_channel(detail, "layer child detail", &mut fields)?);
    }
    if let Some(text) = &child.encoding.text {
        out = out.with_text(adapt_channel(text, "layer child text", &mut fields)?);
    }
    if let Some(fill) = &child.style.fill {
        out = out.with_fill_style(fill.clone());
    }
    if let Some(stroke) = &child.style.stroke {
        out = out.with_stroke_style(stroke.clone());
    }
    if let Some(opacity) = child.style.opacity {
        out = out.with_opacity_value(opacity);
    }
    Ok(out)
}

fn adapt_transform(
    transform: &ParsedTransformSpec,
    fields: &mut FieldBindings<'_>,
) -> Result<TransformSpec, AdaptError> {
    match &transform.kind {
        ParsedTransformKind::Filter { predicate, columns } => Ok(TransformSpec::filter(
            Predicate {
                col: fields.resolve_input(&predicate.field, "filter predicate")?,
                op: predicate.op,
                value: predicate.value,
            },
            resolve_columns(fields, columns, "filter carry-through")?,
        )),
        ParsedTransformKind::Sort { by, order, columns } => Ok(TransformSpec::sort(
            fields.resolve_input(by, "sort key")?,
            *order,
            resolve_columns(fields, columns, "sort carry-through")?,
        )),
        ParsedTransformKind::Calculate {
            expr,
            as_field,
            columns,
        } => Ok(TransformSpec::calculate(
            CalculateExpr {
                left: resolve_calculate_operand(fields, &expr.left)?,
                op: expr.op,
                right: resolve_calculate_operand(fields, &expr.right)?,
            },
            fields.allocate_output(as_field)?,
            resolve_columns(fields, columns, "calculate carry-through")?,
        )),
        ParsedTransformKind::JoinAggregate {
            group_by,
            fields: agg,
            columns,
        } => Ok(TransformSpec::joinaggregate(
            resolve_columns(fields, group_by, "joinaggregate group_by")?,
            agg.iter()
                .map(|field| {
                    Ok(AggregateField {
                        op: field.op,
                        input: fields.resolve_input(&field.field, "joinaggregate input")?,
                        output: fields.allocate_output(&field.as_field)?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            resolve_columns(fields, columns, "joinaggregate carry-through")?,
        )),
        ParsedTransformKind::Aggregate {
            group_by,
            fields: agg,
        } => Ok(TransformSpec::aggregate(
            resolve_columns(fields, group_by, "aggregate group_by")?,
            agg.iter()
                .map(|field| {
                    Ok(AggregateField {
                        op: field.op,
                        input: fields.resolve_input(&field.field, "aggregate input")?,
                        output: fields.allocate_output(&field.as_field)?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        )),
        ParsedTransformKind::Bin {
            field,
            as_start,
            step,
            columns,
        } => Ok(TransformSpec::bin(
            fields.resolve_input(field, "bin input")?,
            fields.allocate_output(as_start)?,
            *step,
            resolve_columns(fields, columns, "bin carry-through")?,
        )),
        ParsedTransformKind::Fold {
            fields: folded_fields,
            as_key,
            as_value,
            columns,
        } => Ok(TransformSpec::fold(
            resolve_columns(fields, folded_fields, "fold fields")?,
            fields.allocate_output(as_key)?,
            fields.allocate_output(as_value)?,
            resolve_columns(fields, columns, "fold carry-through")?,
        )),
        ParsedTransformKind::Lookup {
            from_table,
            key,
            from_key,
            fields: lookup_fields,
            columns,
        } => Ok(TransformSpec::lookup(
            *from_table,
            fields.resolve_input(key, "lookup key")?,
            fields.resolve_input(from_key, "lookup from")?,
            lookup_fields
                .iter()
                .map(|field| {
                    Ok(LookupField {
                        input: fields.resolve_input(&field.field, "lookup field")?,
                        output: fields.allocate_output(&field.as_field)?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            resolve_columns(fields, columns, "lookup carry-through")?,
        )),
        ParsedTransformKind::Window {
            group_by,
            sort_by,
            sort_order,
            fields: window_fields,
            columns,
        } => Ok(TransformSpec::window(
            resolve_columns(fields, group_by, "window group_by")?,
            fields.resolve_input(sort_by, "window sort")?,
            *sort_order,
            window_fields
                .iter()
                .map(|field| {
                    Ok(WindowField {
                        op: field.op,
                        output: fields.allocate_output(&field.as_field)?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
            resolve_columns(fields, columns, "window carry-through")?,
        )),
        ParsedTransformKind::Stack {
            group_by,
            offset,
            sort_by,
            sort_order,
            field,
            as_start,
            as_end,
            columns,
        } => Ok(TransformSpec::stack(
            resolve_columns(fields, group_by, "stack group_by")?,
            *offset,
            sort_by
                .as_ref()
                .map(|field| fields.resolve_input(field, "stack sort"))
                .transpose()?,
            *sort_order,
            fields.resolve_input(field, "stack field")?,
            fields.allocate_output(as_start)?,
            fields.allocate_output(as_end)?,
            resolve_columns(fields, columns, "stack carry-through")?,
        )),
    }
}

fn resolve_columns(
    fields: &mut FieldBindings<'_>,
    columns: &[String],
    role: &'static str,
) -> Result<Vec<ColumnId>, AdaptError> {
    columns
        .iter()
        .map(|field| fields.resolve_input(field, role))
        .collect()
}

fn resolve_calculate_operand(
    fields: &mut FieldBindings<'_>,
    operand: &ParsedCalculateOperand,
) -> Result<CalculateOperand, AdaptError> {
    match operand {
        ParsedCalculateOperand::Field(field) => Ok(CalculateOperand::Column(
            fields.resolve_input(field, "calculate operand")?,
        )),
        ParsedCalculateOperand::Constant(value) => Ok(CalculateOperand::Constant(*value)),
    }
}

#[derive(Clone)]
struct FieldBindings<'a> {
    resolver: &'a dyn FieldResolver,
    derived: Vec<(String, ColumnId)>,
    next_column: u32,
}

impl<'a> FieldBindings<'a> {
    fn new(resolver: &'a dyn FieldResolver) -> Self {
        let next_column = resolver
            .max_column_id()
            .map_or(0, |column| column.0.saturating_add(1));
        Self {
            resolver,
            derived: Vec::new(),
            next_column,
        }
    }

    fn resolve_input(&mut self, field: &str, role: &'static str) -> Result<ColumnId, AdaptError> {
        if let Some((_, column)) = self.derived.iter().find(|(name, _)| name == field) {
            return Ok(*column);
        }
        self.resolver
            .resolve_column(field)
            .ok_or_else(|| AdaptError::UnknownField {
                field: field.to_string(),
                role,
            })
    }

    fn allocate_output(&mut self, field: &str) -> Result<ColumnId, AdaptError> {
        if self.derived.iter().any(|(name, _)| name == field)
            || self.resolver.resolve_column(field).is_some()
        {
            return Err(AdaptError::DerivedFieldConflict {
                field: field.to_string(),
            });
        }
        let column = ColumnId(self.next_column);
        self.next_column = self.next_column.saturating_add(1);
        self.derived.push((field.to_string(), column));
        Ok(column)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::boxed::Box;
    use alloc::vec;

    use super::*;
    use crate::{HeuristicTextMeasurer, LoweringError};
    use vizir_core::{Scene, Table, TableData};

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

    fn resolver() -> SliceFieldResolver<'static> {
        SliceFieldResolver::new(&[
            SchemaField {
                name: "category",
                column: ColumnId(0),
            },
            SchemaField {
                name: "value",
                column: ColumnId(1),
            },
            SchemaField {
                name: "x",
                column: ColumnId(0),
            },
            SchemaField {
                name: "y",
                column: ColumnId(1),
            },
            SchemaField {
                name: "x2",
                column: ColumnId(2),
            },
            SchemaField {
                name: "y2",
                column: ColumnId(3),
            },
            SchemaField {
                name: "series",
                column: ColumnId(2),
            },
        ])
    }

    fn context(table: TableId) -> AdaptContext {
        AdaptContext {
            id_base: 0xA0_000,
            derived_table_base: TableId(table.0 + 100),
            data: DataRef::Table(table),
        }
    }

    #[test]
    fn parsed_aggregate_alias_adapts_and_lowers() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Bar)
            .with_transform(ParsedTransformSpec::aggregate(
                ["category"],
                vec![ParsedAggregateField::new(
                    AggregateOp::Sum,
                    "value",
                    "sum_value",
                )],
            ))
            .with_x(ParsedChannelDef::ordinal("category").with_title("category"))
            .with_y(ParsedChannelDef::quantitative("sum_value").with_title("sum(value)"));

        let mut scene = Scene::new();
        let table_id = TableId(10);
        let mut table = Table::new(table_id);
        table.row_keys = (0..6_u64).collect();
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 0.0, 1.0, 1.0, 2.0, 2.0],
            b: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver(), context(table_id))
            .expect("adapt parsed aggregate spec");
        let lowered = unit.lower(&scene).expect("lower adapted spec");
        assert!(lowered.program().is_some());
        lowered
            .apply_to_scene(&mut scene)
            .expect("apply adapted program");
        assert_eq!(scene.tables[&lowered.output_table()].row_keys.len(), 3);
    }

    #[test]
    fn parsed_sort_transform_resolves_names() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Line)
            .with_transform(ParsedTransformSpec::sort("x", SortOrder::Asc, ["x", "y"]))
            .with_x(ParsedChannelDef::quantitative("x").with_title("x"))
            .with_y(ParsedChannelDef::quantitative("y").with_title("y"));

        let mut scene = Scene::new();
        let table_id = TableId(20);
        let mut table = Table::new(table_id);
        table.row_keys = vec![10, 11, 12];
        table.data = Some(Box::new(TwoCols {
            a: vec![2.0, 0.0, 1.0],
            b: vec![20.0, 0.0, 10.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver(), context(table_id))
            .expect("adapt parsed line spec");
        let lowered = unit
            .lower_into_scene(&mut scene)
            .expect("lower sorted line");
        let sorted = scene
            .tables
            .get(&lowered.output_table())
            .expect("sorted output");
        let data = sorted.data.as_deref().expect("sorted data");
        assert_eq!(data.f64(0, ColumnId(0)), Some(0.0));
        assert_eq!(data.f64(1, ColumnId(0)), Some(1.0));
        assert_eq!(data.f64(2, ColumnId(0)), Some(2.0));
    }

    #[test]
    fn parsed_calculate_transform_allocates_alias_and_lowers() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Point)
            .with_transform(ParsedTransformSpec::calculate(
                ParsedCalculateExpr::new(
                    ParsedCalculateOperand::field("base"),
                    CalculateOp::Add,
                    ParsedCalculateOperand::field("delta"),
                ),
                "total",
                ["x", "base", "delta"],
            ))
            .with_x(ParsedChannelDef::quantitative("x"))
            .with_y(ParsedChannelDef::quantitative("total"));

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "x",
                column: ColumnId(0),
            },
            SchemaField {
                name: "base",
                column: ColumnId(1),
            },
            SchemaField {
                name: "delta",
                column: ColumnId(2),
            },
        ]);

        let mut scene = Scene::new();
        let table_id = TableId(201);
        let mut table = Table::new(table_id);
        table.row_keys = vec![210, 211, 212];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![2.0, 4.0, 6.0],
            c: vec![0.5, 1.0, 1.5],
            d: vec![0.0, 0.0, 0.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver, context(table_id))
            .expect("adapt calculate spec");
        let lowered = unit
            .lower_into_scene(&mut scene)
            .expect("lower calculate spec");
        let calculated = scene
            .tables
            .get(&lowered.output_table())
            .expect("calculated output");
        let data = calculated.data.as_deref().expect("calculated data");
        assert_eq!(data.f64(0, ColumnId(3)), Some(2.5));
        assert_eq!(data.f64(1, ColumnId(3)), Some(5.0));
        assert_eq!(data.f64(2, ColumnId(3)), Some(7.5));
    }

    #[test]
    fn parsed_joinaggregate_transform_allocates_alias_and_lowers() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Point)
            .with_transform(ParsedTransformSpec::joinaggregate(
                ["series"],
                vec![ParsedAggregateField::new(
                    AggregateOp::Mean,
                    "value",
                    "mean_value",
                )],
                ["x", "value", "series"],
            ))
            .with_x(ParsedChannelDef::quantitative("x"))
            .with_y(ParsedChannelDef::quantitative("mean_value"))
            .with_color(ParsedChannelDef::nominal("series"));

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "x",
                column: ColumnId(0),
            },
            SchemaField {
                name: "value",
                column: ColumnId(1),
            },
            SchemaField {
                name: "series",
                column: ColumnId(2),
            },
        ]);

        let mut scene = Scene::new();
        let table_id = TableId(202);
        let mut table = Table::new(table_id);
        table.row_keys = vec![220, 221, 222, 223];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0, 3.0],
            b: vec![2.0, 4.0, 3.0, 5.0],
            c: vec![0.0, 0.0, 1.0, 1.0],
            d: vec![0.0, 0.0, 0.0, 0.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver, context(table_id))
            .expect("adapt joinaggregate spec");
        let lowered = unit
            .lower_into_scene(&mut scene)
            .expect("lower joinaggregate spec");
        let joined = scene
            .tables
            .get(&lowered.output_table())
            .expect("joinaggregate output");
        let data = joined.data.as_deref().expect("joinaggregate data");
        assert_eq!(data.f64(0, ColumnId(3)), Some(3.0));
        assert_eq!(data.f64(1, ColumnId(3)), Some(3.0));
        assert_eq!(data.f64(2, ColumnId(3)), Some(4.0));
        assert_eq!(data.f64(3, ColumnId(3)), Some(4.0));
    }

    #[test]
    fn parsed_fold_transform_allocates_slot_and_value_aliases() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Bar)
            .with_transform(ParsedTransformSpec::fold(
                ["q1", "q2", "q3"],
                "measure_slot",
                "measure_value",
                ["category"],
            ))
            .with_x(ParsedChannelDef::ordinal("category"))
            .with_y(ParsedChannelDef::quantitative("measure_value"))
            .with_color(ParsedChannelDef::nominal("measure_slot"));

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "category",
                column: ColumnId(0),
            },
            SchemaField {
                name: "q1",
                column: ColumnId(1),
            },
            SchemaField {
                name: "q2",
                column: ColumnId(2),
            },
            SchemaField {
                name: "q3",
                column: ColumnId(3),
            },
        ]);

        let mut scene = Scene::new();
        let table_id = TableId(204);
        let mut table = Table::new(table_id);
        table.row_keys = vec![240, 241];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0],
            b: vec![2.0, 4.0],
            c: vec![3.0, 5.0],
            d: vec![4.0, 6.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver, context(table_id))
            .expect("adapt fold spec");
        let lowered = unit.lower_into_scene(&mut scene).expect("lower fold spec");
        let folded = scene
            .tables
            .get(&lowered.output_table())
            .expect("fold output");
        assert_eq!(folded.row_keys.len(), 6);
    }

    #[test]
    fn parsed_lookup_transform_enriches_from_secondary_table() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Bar)
            .with_transform(ParsedTransformSpec::lookup(
                TableId(205),
                "category",
                "lookup_category",
                vec![ParsedLookupField::new("lookup_value", "value")],
                ["category"],
            ))
            .with_x(ParsedChannelDef::ordinal("category"))
            .with_y(ParsedChannelDef::quantitative("value"));

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "category",
                column: ColumnId(0),
            },
            SchemaField {
                name: "lookup_category",
                column: ColumnId(0),
            },
            SchemaField {
                name: "lookup_value",
                column: ColumnId(1),
            },
        ]);

        let mut scene = Scene::new();
        let input_table = TableId(204);
        let mut input = Table::new(input_table);
        input.row_keys = vec![250, 251, 252];
        input.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![0.0, 0.0, 0.0],
        }));
        scene.insert_table(input);

        let mut lookup = Table::new(TableId(205));
        lookup.row_keys = vec![260, 261];
        lookup.data = Some(Box::new(TwoCols {
            a: vec![0.0, 2.0],
            b: vec![4.0, 6.0],
        }));
        scene.insert_table(lookup);

        let unit = parsed
            .adapt(&resolver, context(input_table))
            .expect("adapt lookup spec");
        let lowered = unit
            .lower_into_scene(&mut scene)
            .expect("lower lookup spec");
        let enriched = scene
            .tables
            .get(&lowered.output_table())
            .expect("lookup output");
        let data = enriched.data.as_deref().expect("lookup data");
        assert_eq!(data.f64(0, ColumnId(2)), Some(4.0));
        assert!(data.f64(1, ColumnId(2)).expect("lookup miss").is_nan());
        assert_eq!(data.f64(2, ColumnId(2)), Some(6.0));
    }

    #[test]
    fn parsed_window_transform_allocates_rank_alias_and_lowers() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Line)
            .with_transform(ParsedTransformSpec::window(
                ["series"],
                "value",
                SortOrder::Desc,
                vec![ParsedWindowField::new(WindowOp::Rank, "value_rank")],
                ["value", "series"],
            ))
            .with_transform(ParsedTransformSpec::sort(
                "value_rank",
                SortOrder::Asc,
                ["value", "series", "value_rank"],
            ))
            .with_x(ParsedChannelDef::quantitative("value_rank"))
            .with_y(ParsedChannelDef::quantitative("value"))
            .with_color(ParsedChannelDef::nominal("series"));

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "value",
                column: ColumnId(0),
            },
            SchemaField {
                name: "series",
                column: ColumnId(1),
            },
        ]);

        let mut scene = Scene::new();
        let table_id = TableId(203);
        let mut table = Table::new(table_id);
        table.row_keys = vec![230, 231, 232, 233, 234, 235];
        table.data = Some(Box::new(TwoCols {
            a: vec![9.0, 5.0, 1.0, 8.0, 4.0, 2.0],
            b: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver, context(table_id))
            .expect("adapt window spec");
        let lowered = unit
            .lower_into_scene(&mut scene)
            .expect("lower window spec");
        let ranked = scene
            .tables
            .get(&lowered.output_table())
            .expect("window output");
        let data = ranked.data.as_deref().expect("window data");
        let ranks = (0..data.row_count())
            .map(|row| data.f64(row, ColumnId(2)).expect("rank value"))
            .collect::<Vec<_>>();
        assert_eq!(ranks, vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0]);
    }

    #[test]
    fn parsed_ranged_area_adapts_secondary_channels() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Area)
            .with_x(ParsedChannelDef::quantitative("x"))
            .with_y(ParsedChannelDef::quantitative("y"))
            .with_x2(ParsedChannelDef::quantitative("x2"))
            .with_y2(ParsedChannelDef::quantitative("y2"));

        let mut scene = Scene::new();
        let table_id = TableId(30);
        let mut table = Table::new(table_id);
        table.row_keys = vec![100, 101, 102];
        table.data = Some(Box::new(FourCols {
            a: vec![1.0, 2.0, 3.0],
            b: vec![5.0, 6.0, 7.0],
            c: vec![0.0, 1.0, 2.0],
            d: vec![2.0, 3.0, 4.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver(), context(table_id))
            .expect("adapt ranged area");
        let lowered = unit.lower(&scene).expect("lower ranged area");
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
        let (_layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("area marks");
        assert!(
            marks
                .iter()
                .any(|mark| matches!(mark.encodings, vizir_core::MarkEncodings::Path(_)))
        );
    }

    #[test]
    fn parsed_text_mark_adapts_and_lowers() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Text)
            .with_x(ParsedChannelDef::ordinal("x"))
            .with_y(ParsedChannelDef::quantitative("y"))
            .with_text(ParsedChannelDef::quantitative("y"));

        let mut scene = Scene::new();
        let table_id = TableId(31);
        let mut table = Table::new(table_id);
        table.row_keys = vec![110, 111, 112];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![5.0, 4.0, 6.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver(), context(table_id))
            .expect("adapt text mark");
        let lowered = unit.lower(&scene).expect("lower text mark");
        let (_layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("text marks");
        assert!(
            marks
                .iter()
                .any(|mark| mark.kind == vizir_core::MarkKind::Text)
        );
    }

    #[test]
    fn parsed_rule_mark_adapts_and_lowers() {
        let parsed =
            ParsedUnitSpec::new(ParsedMarkDef::Rule).with_y(ParsedChannelDef::quantitative("y"));

        let mut scene = Scene::new();
        let table_id = TableId(311);
        let mut table = Table::new(table_id);
        table.row_keys = vec![115, 116];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0],
            b: vec![2.0, 4.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver(), context(table_id))
            .expect("adapt rule mark");
        let lowered = unit.lower(&scene).expect("lower rule mark");
        let (_layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("rule marks");
        let path_count = marks
            .iter()
            .filter(|mark| mark.kind == vizir_core::MarkKind::Path)
            .count();
        assert!(path_count >= 2);
    }

    #[test]
    fn parsed_point_shape_size_adapts_and_lowers() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Point)
            .with_x(ParsedChannelDef::quantitative("x"))
            .with_y(ParsedChannelDef::quantitative("y"))
            .with_size_channel(ParsedChannelDef::quantitative("size"))
            .with_shape(ParsedChannelDef::nominal("shape"));

        let mut scene = Scene::new();
        let table_id = TableId(32);
        let mut table = Table::new(table_id);
        table.row_keys = vec![120, 121, 122, 123];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0, 3.0],
            b: vec![1.0, 2.0, 3.0, 2.5],
            c: vec![1.0, 4.0, 2.0, 7.0],
            d: vec![0.0, 1.0, 2.0, 3.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(
                &SliceFieldResolver::new(&[
                    SchemaField {
                        name: "x",
                        column: ColumnId(0),
                    },
                    SchemaField {
                        name: "y",
                        column: ColumnId(1),
                    },
                    SchemaField {
                        name: "size",
                        column: ColumnId(2),
                    },
                    SchemaField {
                        name: "shape",
                        column: ColumnId(3),
                    },
                ]),
                context(table_id),
            )
            .expect("adapt point shape/size");
        let lowered = unit.lower(&scene).expect("lower point shape/size");
        let (_layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("point marks");
        assert!(marks.len() >= 4);
        assert!(
            marks
                .iter()
                .any(|mark| mark.kind == vizir_core::MarkKind::Rect)
        );
        assert!(
            marks
                .iter()
                .any(|mark| mark.kind == vizir_core::MarkKind::Path)
        );
    }

    #[test]
    fn parsed_point_opacity_adapts_and_lowers() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Point)
            .with_x(ParsedChannelDef::quantitative("x"))
            .with_y(ParsedChannelDef::quantitative("y"))
            .with_opacity(ParsedChannelDef::quantitative("opacity"));

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "x",
                column: ColumnId(0),
            },
            SchemaField {
                name: "y",
                column: ColumnId(1),
            },
            SchemaField {
                name: "opacity",
                column: ColumnId(2),
            },
        ]);

        let mut scene = Scene::new();
        let table_id = TableId(39);
        let mut table = Table::new(table_id);
        table.row_keys = vec![90, 91];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0],
            b: vec![1.0, 2.0],
            c: vec![0.0, 10.0],
            d: vec![0.0, 0.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver, context(table_id))
            .expect("adapt point opacity");
        let lowered = unit.lower(&scene).expect("lower point opacity");
        assert_eq!(lowered.output_table(), table_id);
    }

    #[test]
    fn parsed_point_stroke_width_adapts_and_lowers() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Point)
            .with_x(ParsedChannelDef::quantitative("x"))
            .with_y(ParsedChannelDef::quantitative("y"))
            .with_stroke(ParsedChannelDef::nominal("series"))
            .with_stroke_width(ParsedChannelDef::quantitative("weight"));

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "x",
                column: ColumnId(0),
            },
            SchemaField {
                name: "y",
                column: ColumnId(1),
            },
            SchemaField {
                name: "series",
                column: ColumnId(2),
            },
            SchemaField {
                name: "weight",
                column: ColumnId(3),
            },
        ]);

        let mut scene = Scene::new();
        let table_id = TableId(38);
        let mut table = Table::new(table_id);
        table.row_keys = vec![80, 81];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0],
            b: vec![1.0, 2.0],
            c: vec![0.0, 1.0],
            d: vec![1.0, 5.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver, context(table_id))
            .expect("adapt point stroke/width");
        let lowered = unit.lower(&scene).expect("lower point stroke/width");
        assert_eq!(lowered.output_table(), table_id);
    }

    #[test]
    fn unknown_field_returns_structured_error() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Point)
            .with_x(ParsedChannelDef::quantitative("missing"))
            .with_y(ParsedChannelDef::quantitative("y"));

        let err = parsed
            .adapt(&resolver(), context(TableId(40)))
            .expect_err("missing field should fail");
        assert_eq!(
            err,
            AdaptError::UnknownField {
                field: String::from("missing"),
                role: "x",
            }
        );
    }

    #[test]
    fn derived_alias_conflict_is_rejected() {
        let parsed =
            ParsedUnitSpec::new(ParsedMarkDef::Bar).with_transform(ParsedTransformSpec::aggregate(
                ["category"],
                vec![ParsedAggregateField::new(
                    AggregateOp::Sum,
                    "value",
                    "value",
                )],
            ));

        let err = parsed
            .adapt(&resolver(), context(TableId(50)))
            .expect_err("derived alias conflict should fail");
        assert_eq!(
            err,
            AdaptError::DerivedFieldConflict {
                field: String::from("value"),
            }
        );
    }

    #[test]
    fn adapted_unknown_field_fails_before_lowering() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Area)
            .with_x(ParsedChannelDef::quantitative("x"))
            .with_y(ParsedChannelDef::quantitative("sum_value"));

        let unit = parsed.adapt(&resolver(), context(TableId(60)));
        assert!(matches!(
            unit,
            Err(AdaptError::UnknownField { role: "y", .. })
        ));

        let _ = LoweringError::MissingChannel("y");
    }

    #[test]
    fn parsed_layer_adapts_and_lowers_shared_marks() {
        let parsed = ParsedLayerSpec::new()
            .with_title("line + point")
            .with_mark(ParsedMarkDef::Line)
            .with_mark(ParsedMarkDef::Point)
            .with_x(ParsedChannelDef::quantitative("x"))
            .with_y(ParsedChannelDef::quantitative("y"));

        let mut scene = Scene::new();
        let table_id = TableId(70);
        let mut table = Table::new(table_id);
        table.row_keys = vec![10, 11, 12];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![1.0, 2.0, 3.0],
        }));
        scene.insert_table(table);

        let layer = parsed
            .adapt(&resolver(), context(table_id))
            .expect("adapt parsed layer");
        let lowered = layer.lower(&scene).expect("lower parsed layer");
        let (_layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("layer marks");
        assert!(marks.len() >= 4);
    }

    #[test]
    fn parsed_grouped_bar_adapts_and_lowers() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Bar)
            .with_x(ParsedChannelDef::ordinal("category"))
            .with_y(ParsedChannelDef::quantitative("value"))
            .with_color(ParsedChannelDef::nominal("series"));

        let mut scene = Scene::new();
        let table_id = TableId(321);
        let mut table = Table::new(table_id);
        table.row_keys = vec![124, 125, 126, 127, 128];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0, 1.0, 2.0],
            b: vec![2.0, 3.0, 4.0, 5.0, 6.0],
            c: vec![0.0, 0.0, 0.0, 1.0, 1.0],
            d: vec![0.0, 0.0, 0.0, 0.0, 0.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver(), context(table_id))
            .expect("adapt grouped bar");
        let lowered = unit
            .lower_into_scene(&mut scene)
            .expect("lower grouped bar");
        let (_layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("grouped bar marks");
        assert!(
            marks
                .iter()
                .any(|mark| mark.kind == vizir_core::MarkKind::Rect)
        );
    }

    #[test]
    fn parsed_stacked_bar_adapts_and_lowers() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Bar)
            .with_transform(ParsedTransformSpec::stack(
                ["category"],
                StackOffset::Zero,
                Some("series"),
                SortOrder::Asc,
                "value",
                "y0",
                "y1",
                ["category", "value", "series"],
            ))
            .with_x(ParsedChannelDef::ordinal("category"))
            .with_y(ParsedChannelDef::quantitative("y1"))
            .with_y2(ParsedChannelDef::quantitative("y0"))
            .with_color(ParsedChannelDef::nominal("series"));

        let mut scene = Scene::new();
        let table_id = TableId(322);
        let mut table = Table::new(table_id);
        table.row_keys = vec![130, 131, 132, 133, 134];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0, 1.0, 2.0],
            b: vec![2.0, 3.0, 4.0, 5.0, 6.0],
            c: vec![0.0, 0.0, 0.0, 1.0, 1.0],
            d: vec![0.0, 0.0, 0.0, 0.0, 0.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver(), context(table_id))
            .expect("adapt stacked bar");
        let lowered = unit
            .lower_into_scene(&mut scene)
            .expect("lower stacked bar");
        let (_layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("stacked bar marks");
        assert!(
            marks
                .iter()
                .any(|mark| mark.kind == vizir_core::MarkKind::Rect)
        );
        assert!(lowered.chart().legend.is_some());
    }

    #[test]
    fn parsed_facet_adapts_and_lowers() {
        let parsed = ParsedFacetSpec::new(ParsedChannelDef::nominal("series"), ParsedMarkDef::Bar)
            .with_title("Faceted Bars")
            .with_x(ParsedChannelDef::ordinal("category"))
            .with_y(ParsedChannelDef::quantitative("value"));

        let mut scene = Scene::new();
        let table_id = TableId(323);
        let mut table = Table::new(table_id);
        table.row_keys = vec![140, 141, 142, 143, 144, 145];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0],
            b: vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
            c: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            d: vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }));
        scene.insert_table(table);

        let facet = parsed
            .adapt(&resolver(), context(table_id))
            .expect("adapt facet");
        let lowered = facet.lower_into_scene(&mut scene).expect("lower facet");
        let (layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("facet marks");
        assert_eq!(layout.cells.len(), 2);
        assert!(
            marks
                .iter()
                .any(|mark| mark.kind == vizir_core::MarkKind::Rect)
        );
    }

    #[test]
    fn parsed_layer_child_overrides_adapt() {
        let parsed = ParsedLayerSpec::new()
            .with_x(ParsedChannelDef::quantitative("x"))
            .with_child(
                ParsedLayerChildSpec::new(ParsedMarkDef::Area)
                    .with_y(ParsedChannelDef::quantitative("y"))
                    .with_y2(ParsedChannelDef::quantitative("y2")),
            )
            .with_child(
                ParsedLayerChildSpec::new(ParsedMarkDef::Line)
                    .with_y(ParsedChannelDef::quantitative("y2")),
            );

        let mut scene = Scene::new();
        let table_id = TableId(71);
        let mut table = Table::new(table_id);
        table.row_keys = vec![20, 21, 22];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![3.0, 4.0, 5.0],
            c: vec![1.0, 2.0, 2.5],
            d: vec![0.0, 0.0, 0.0],
        }));
        scene.insert_table(table);

        let layer = parsed
            .adapt(&resolver(), context(table_id))
            .expect("adapt overridden layer");
        let lowered = layer.lower(&scene).expect("lower overridden layer");
        let (_layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("layer marks");
        assert!(marks.len() >= 2);
    }

    #[test]
    fn parsed_layer_child_transforms_adapt_and_execute() {
        let parsed = ParsedLayerSpec::new()
            .with_x(ParsedChannelDef::quantitative("x"))
            .with_y(ParsedChannelDef::quantitative("y"))
            .with_mark(ParsedMarkDef::Line)
            .with_child(
                ParsedLayerChildSpec::new(ParsedMarkDef::Point).with_transform(
                    ParsedTransformSpec::filter(
                        ParsedPredicate {
                            field: String::from("x"),
                            op: CompareOp::Ge,
                            value: 1.0,
                        },
                        ["x", "y"],
                    ),
                ),
            );

        let mut scene = Scene::new();
        let table_id = TableId(72);
        let mut table = Table::new(table_id);
        table.row_keys = vec![30, 31, 32];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![4.0, 5.0, 6.0],
        }));
        scene.insert_table(table);

        let layer = parsed
            .adapt(&resolver(), context(table_id))
            .expect("adapt transformed layer");
        let lowered = layer
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
        let filtered = scene
            .tables
            .get(&lowered.derived_tables()[0])
            .expect("filtered child table");
        assert_eq!(filtered.row_keys, vec![31, 32]);
    }

    #[test]
    fn parsed_layer_rule_child_adapts_and_lowers() {
        let parsed = ParsedLayerSpec::new()
            .with_child(
                ParsedLayerChildSpec::new(ParsedMarkDef::Line)
                    .with_x(ParsedChannelDef::quantitative("x"))
                    .with_y(ParsedChannelDef::quantitative("y")),
            )
            .with_child(
                ParsedLayerChildSpec::new(ParsedMarkDef::Rule)
                    .with_transform(ParsedTransformSpec::aggregate(
                        Vec::<String>::new(),
                        vec![ParsedAggregateField::new(AggregateOp::Mean, "y", "mean_y")],
                    ))
                    .with_y(ParsedChannelDef::quantitative("mean_y"))
                    .with_stroke_style(StrokeStyle::solid(
                        peniko::color::palette::css::TOMATO,
                        2.0,
                    )),
            );

        let mut scene = Scene::new();
        let table_id = TableId(721);
        let mut table = Table::new(table_id);
        table.row_keys = vec![34, 35, 36, 37];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0, 2.0, 3.0],
            b: vec![1.0, 2.0, 3.0, 4.0],
        }));
        scene.insert_table(table);

        let layer = parsed
            .adapt(&resolver(), context(table_id))
            .expect("adapt layered rule");
        let lowered = layer.lower(&scene).expect("lower layered rule");
        assert_eq!(lowered.derived_tables().len(), 1);
    }

    #[test]
    fn parsed_line_order_detail_adapts_and_lowers() {
        let parsed = ParsedUnitSpec::new(ParsedMarkDef::Line)
            .with_x(ParsedChannelDef::quantitative("x"))
            .with_y(ParsedChannelDef::quantitative("y"))
            .with_order(ParsedChannelDef::quantitative("step"))
            .with_detail(ParsedChannelDef::nominal("series"));

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "x",
                column: ColumnId(0),
            },
            SchemaField {
                name: "y",
                column: ColumnId(1),
            },
            SchemaField {
                name: "step",
                column: ColumnId(2),
            },
            SchemaField {
                name: "series",
                column: ColumnId(3),
            },
        ]);

        let mut scene = Scene::new();
        let table_id = TableId(73);
        let mut table = Table::new(table_id);
        table.row_keys = vec![40, 41, 42, 43];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 0.0, 1.0],
            b: vec![10.0, 20.0, 30.0, 40.0],
            c: vec![2.0, 1.0, 2.0, 1.0],
            d: vec![0.0, 0.0, 1.0, 1.0],
        }));
        scene.insert_table(table);

        let unit = parsed
            .adapt(&resolver, context(table_id))
            .expect("adapt ordered detailed line");
        let lowered = unit.lower(&scene).expect("lower ordered detailed line");
        assert_eq!(lowered.derived_tables().len(), 2);
    }

    #[test]
    fn parsed_layer_children_inherit_base_child_defaults() {
        let parsed = ParsedLayerSpec::new()
            .with_child(
                ParsedLayerChildSpec::new(ParsedMarkDef::Area)
                    .with_x(ParsedChannelDef::quantitative("x"))
                    .with_y(ParsedChannelDef::quantitative("high"))
                    .with_y2(ParsedChannelDef::quantitative("low")),
            )
            .with_child(
                ParsedLayerChildSpec::new(ParsedMarkDef::Line)
                    .with_y(ParsedChannelDef::quantitative("line")),
            );

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "x",
                column: ColumnId(0),
            },
            SchemaField {
                name: "high",
                column: ColumnId(1),
            },
            SchemaField {
                name: "low",
                column: ColumnId(2),
            },
            SchemaField {
                name: "line",
                column: ColumnId(3),
            },
        ]);

        let mut scene = Scene::new();
        let table_id = TableId(74);
        let mut table = Table::new(table_id);
        table.row_keys = vec![50, 51, 52];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![4.0, 5.0, 6.0],
            c: vec![1.0, 2.0, 3.0],
            d: vec![2.0, 3.0, 4.0],
        }));
        scene.insert_table(table);

        let layer = parsed
            .adapt(&resolver, context(table_id))
            .expect("adapt base-child defaults");
        let lowered = layer.lower(&scene).expect("lower base-child defaults");
        let layout = lowered.chart().layout(&HeuristicTextMeasurer);
        let y_scale = lowered
            .chart()
            .y_scale_continuous(layout.data)
            .expect("y scale");
        assert_eq!(y_scale.domain_min(), 1.0);
        assert_eq!(y_scale.domain_max(), 6.0);
    }

    #[test]
    fn parsed_layer_children_can_be_fully_specified_units() {
        let parsed = ParsedLayerSpec::new()
            .with_child(
                ParsedLayerChildSpec::new(ParsedMarkDef::Line)
                    .with_x(ParsedChannelDef::quantitative("x"))
                    .with_y(ParsedChannelDef::quantitative("y")),
            )
            .with_child(
                ParsedLayerChildSpec::new(ParsedMarkDef::Point)
                    .with_x(ParsedChannelDef::quantitative("x"))
                    .with_y(ParsedChannelDef::quantitative("y")),
            );

        let mut scene = Scene::new();
        let table_id = TableId(75);
        let mut table = Table::new(table_id);
        table.row_keys = vec![60, 61, 62];
        table.data = Some(Box::new(TwoCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![3.0, 4.0, 5.0],
        }));
        scene.insert_table(table);

        let layer = parsed
            .adapt(&resolver(), context(table_id))
            .expect("adapt fully specified child units");
        let lowered = layer
            .lower(&scene)
            .expect("lower fully specified child units");
        let (_layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("layer marks");
        assert!(marks.len() >= 4);
    }

    #[test]
    fn parsed_layer_child_literal_styles_adapt_and_lower() {
        let parsed = ParsedLayerSpec::new()
            .with_child(
                ParsedLayerChildSpec::new(ParsedMarkDef::Area)
                    .with_x(ParsedChannelDef::quantitative("x"))
                    .with_y(ParsedChannelDef::quantitative("high"))
                    .with_y2(ParsedChannelDef::quantitative("low"))
                    .with_fill_style(Brush::Solid(peniko::color::palette::css::CORNFLOWER_BLUE))
                    .with_stroke_style(StrokeStyle::solid(
                        peniko::color::palette::css::CORNFLOWER_BLUE,
                        1.0,
                    ))
                    .with_opacity_value(0.25),
            )
            .with_child(
                ParsedLayerChildSpec::new(ParsedMarkDef::Line)
                    .with_x(ParsedChannelDef::quantitative("x"))
                    .with_y(ParsedChannelDef::quantitative("line"))
                    .with_stroke_style(StrokeStyle::solid(peniko::color::palette::css::BLACK, 2.5)),
            );

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "x",
                column: ColumnId(0),
            },
            SchemaField {
                name: "high",
                column: ColumnId(1),
            },
            SchemaField {
                name: "low",
                column: ColumnId(2),
            },
            SchemaField {
                name: "line",
                column: ColumnId(3),
            },
        ]);

        let mut scene = Scene::new();
        let table_id = TableId(751);
        let mut table = Table::new(table_id);
        table.row_keys = vec![80, 81, 82];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![4.0, 5.0, 6.0],
            c: vec![1.0, 2.0, 3.0],
            d: vec![2.0, 3.0, 4.0],
        }));
        scene.insert_table(table);

        let layer = parsed
            .adapt(&resolver, context(table_id))
            .expect("adapt styled child units");
        let lowered = layer.lower(&scene).expect("lower styled child units");
        let (_layout, marks) = lowered
            .marks(&scene, &HeuristicTextMeasurer)
            .expect("layer marks");
        assert!(marks.len() >= 2);
    }

    #[test]
    fn parsed_layer_child_conflicting_x_is_rejected_during_lowering() {
        let parsed = ParsedLayerSpec::new()
            .with_child(
                ParsedLayerChildSpec::new(ParsedMarkDef::Line)
                    .with_x(ParsedChannelDef::quantitative("x"))
                    .with_y(ParsedChannelDef::quantitative("y")),
            )
            .with_child(
                ParsedLayerChildSpec::new(ParsedMarkDef::Point)
                    .with_x(ParsedChannelDef::quantitative("step"))
                    .with_y(ParsedChannelDef::quantitative("y")),
            );

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "x",
                column: ColumnId(0),
            },
            SchemaField {
                name: "y",
                column: ColumnId(1),
            },
            SchemaField {
                name: "step",
                column: ColumnId(2),
            },
        ]);

        let mut scene = Scene::new();
        let table_id = TableId(76);
        let mut table = Table::new(table_id);
        table.row_keys = vec![70, 71, 72];
        table.data = Some(Box::new(FourCols {
            a: vec![0.0, 1.0, 2.0],
            b: vec![3.0, 4.0, 5.0],
            c: vec![10.0, 11.0, 12.0],
            d: vec![0.0, 0.0, 0.0],
        }));
        scene.insert_table(table);

        let layer = parsed
            .adapt(&resolver, context(table_id))
            .expect("adapt conflicting child layer");
        let err = layer
            .lower(&scene)
            .expect_err("conflicting child x should fail");
        assert!(matches!(
            err,
            LoweringError::Unsupported(message)
                if message.contains("same x channel")
        ));
    }
}
