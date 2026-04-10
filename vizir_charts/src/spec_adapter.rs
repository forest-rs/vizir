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

use vizir_core::{ColumnId, TableId};
use vizir_transforms::{AggregateField, AggregateOp, CompareOp, Predicate, SortOrder, StackOffset};

use crate::{
    ChannelDef, DataRef, FieldKind, LayerChildSpec, LayerSpec, MarkDef, TransformSpec, UnitSpec,
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
pub struct ParsedLayerChildSpec {
    mark: ParsedMarkDef,
    transforms: Vec<ParsedTransformSpec>,
    encoding: ParsedEncodingSet,
}

impl ParsedLayerChildSpec {
    /// Creates a new parsed layer child for the given mark.
    pub fn new(mark: ParsedMarkDef) -> Self {
        Self {
            mark,
            transforms: Vec::new(),
            encoding: ParsedEncodingSet::new(),
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
    if let Some(order) = &child.encoding.order {
        out = out.with_order(adapt_channel(order, "layer child order", &mut fields)?);
    }
    if let Some(detail) = &child.encoding.detail {
        out = out.with_detail(adapt_channel(detail, "layer child detail", &mut fields)?);
    }
    if let Some(text) = &child.encoding.text {
        out = out.with_text(adapt_channel(text, "layer child text", &mut fields)?);
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
}
