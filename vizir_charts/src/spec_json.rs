// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Narrow JSON parsing for the experimental authored spec seam.
//!
//! This module intentionally parses only the small authored slice that `vizir_charts` can lower
//! today. It is not a general Vega-Lite parser.

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use peniko::{Brush, Color};
use serde::Deserialize;
use serde_json::Value;
use vizir_transforms::{AggregateOp, CompareOp, SortOrder, StackOffset};

use crate::{
    ParsedAggregateField, ParsedChannelDef, ParsedEncodingSet, ParsedFieldKind,
    ParsedLayerChildSpec, ParsedLayerSpec, ParsedMarkDef, ParsedPredicate, ParsedTransformSpec,
    ParsedUnitSpec, StrokeStyle,
};

/// Errors returned while parsing a narrow JSON unit spec.
#[derive(Debug)]
pub enum JsonSpecError {
    /// The input could not be deserialized as JSON.
    Json(serde_json::Error),
    /// The JSON shape is syntactically valid, but outside the supported parser slice.
    Invalid(String),
}

impl core::fmt::Display for JsonSpecError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Json(err) => write!(f, "{err}"),
            Self::Invalid(message) => write!(f, "{message}"),
        }
    }
}

impl core::error::Error for JsonSpecError {}

impl From<serde_json::Error> for JsonSpecError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl ParsedUnitSpec {
    /// Parses a narrow JSON unit spec into a [`ParsedUnitSpec`].
    pub fn from_json_str(input: &str) -> Result<Self, JsonSpecError> {
        parse_unit_spec_json(input)
    }
}

impl ParsedLayerSpec {
    /// Parses a narrow JSON layer spec into a [`ParsedLayerSpec`].
    pub fn from_json_str(input: &str) -> Result<Self, JsonSpecError> {
        parse_layer_spec_json(input)
    }
}

/// Parses a narrow JSON unit spec into a [`ParsedUnitSpec`].
pub fn parse_unit_spec_json(input: &str) -> Result<ParsedUnitSpec, JsonSpecError> {
    let raw: JsonUnitSpec = serde_json::from_str(input)?;

    let mut spec = ParsedUnitSpec::new(parse_mark(raw.mark)?)
        .with_size(raw.width.unwrap_or(220.0), raw.height.unwrap_or(120.0));
    if let Some(title) = raw.title {
        spec = spec.with_title(title);
    }

    spec = spec.with_encoding(parse_encoding_set(raw.encoding)?);

    for transform in raw.transform {
        spec = spec.with_transform(parse_transform(transform)?);
    }

    Ok(spec)
}

/// Parses a narrow JSON shared-plot layer spec into a [`ParsedLayerSpec`].
pub fn parse_layer_spec_json(input: &str) -> Result<ParsedLayerSpec, JsonSpecError> {
    let raw: JsonLayerSpec = serde_json::from_str(input)?;

    let mut spec =
        ParsedLayerSpec::new().with_size(raw.width.unwrap_or(220.0), raw.height.unwrap_or(120.0));
    if let Some(title) = raw.title {
        spec = spec.with_title(title);
    }

    spec = spec.with_encoding(parse_encoding_set(raw.encoding)?);

    for transform in raw.transform {
        spec = spec.with_transform(parse_transform(transform)?);
    }
    for layer in raw.layer {
        let mut child = ParsedLayerChildSpec::new(parse_mark(layer.mark)?)
            .with_encoding(parse_encoding_set(layer.encoding.unwrap_or_default())?);
        for transform in layer.transform {
            child = child.with_transform(parse_transform(transform)?);
        }
        if let Some(style) = layer.style {
            if let Some(fill) = style.fill {
                child = child.with_fill_style(parse_brush(&fill)?);
            }
            if let Some(stroke) = style.stroke {
                child = child.with_stroke_style(StrokeStyle::solid(
                    parse_color(&stroke)?,
                    style.stroke_width.unwrap_or(1.0),
                ));
            }
            if let Some(opacity) = style.opacity {
                child = child.with_opacity_value(opacity);
            }
        }
        spec = spec.with_child(child);
    }

    Ok(spec)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonUnitSpec {
    mark: JsonMark,
    #[serde(default)]
    encoding: JsonEncoding,
    #[serde(default)]
    transform: Vec<Value>,
    width: Option<f64>,
    height: Option<f64>,
    title: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonLayerSpec {
    layer: Vec<JsonLayerEntry>,
    #[serde(default)]
    encoding: JsonEncoding,
    #[serde(default)]
    transform: Vec<Value>,
    width: Option<f64>,
    height: Option<f64>,
    title: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonLayerEntry {
    mark: JsonMark,
    #[serde(default)]
    transform: Vec<Value>,
    encoding: Option<JsonEncoding>,
    style: Option<JsonLayerStyle>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonLayerStyle {
    fill: Option<String>,
    stroke: Option<String>,
    #[serde(rename = "strokeWidth")]
    stroke_width: Option<f64>,
    opacity: Option<f64>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonEncoding {
    x: Option<JsonChannel>,
    x2: Option<JsonChannel>,
    y: Option<JsonChannel>,
    y2: Option<JsonChannel>,
    color: Option<JsonChannel>,
    size: Option<JsonChannel>,
    shape: Option<JsonChannel>,
    opacity: Option<JsonChannel>,
    stroke: Option<JsonChannel>,
    #[serde(rename = "strokeWidth")]
    stroke_width: Option<JsonChannel>,
    order: Option<JsonChannel>,
    detail: Option<JsonChannel>,
    text: Option<JsonChannel>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonChannel {
    field: String,
    #[serde(rename = "type")]
    kind: String,
    aggregate: Option<String>,
    title: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum JsonMark {
    Name(String),
    Object {
        #[serde(rename = "type")]
        kind: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonPredicate {
    field: String,
    op: String,
    value: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonFilterTransform {
    filter: JsonPredicate,
    #[serde(default)]
    columns: Vec<String>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonSortField {
    field: String,
    order: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonSortTransform {
    sort: JsonSortField,
    #[serde(default)]
    columns: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonAggregateField {
    op: String,
    field: String,
    #[serde(rename = "as")]
    as_field: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonAggregateTransform {
    aggregate: Vec<JsonAggregateField>,
    #[serde(default)]
    groupby: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonBinTransform {
    bin: JsonBinBody,
    #[serde(default)]
    columns: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonBinBody {
    field: String,
    #[serde(rename = "as")]
    as_field: String,
    step: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonStackTransform {
    stack: String,
    #[serde(default)]
    groupby: Vec<String>,
    sort: Option<JsonSortField>,
    offset: Option<String>,
    #[serde(rename = "as")]
    as_fields: [String; 2],
    #[serde(default)]
    columns: Vec<String>,
}

fn parse_mark(mark: JsonMark) -> Result<ParsedMarkDef, JsonSpecError> {
    let kind = match mark {
        JsonMark::Name(kind) | JsonMark::Object { kind } => kind,
    };
    match kind.as_str() {
        "bar" => Ok(ParsedMarkDef::Bar),
        "line" => Ok(ParsedMarkDef::Line),
        "point" => Ok(ParsedMarkDef::Point),
        "area" => Ok(ParsedMarkDef::Area),
        "rule" => Ok(ParsedMarkDef::Rule),
        "text" => Ok(ParsedMarkDef::Text),
        _ => Err(JsonSpecError::Invalid(format!(
            "unsupported mark type `{kind}`"
        ))),
    }
}

fn parse_channel(channel: JsonChannel) -> Result<ParsedChannelDef, JsonSpecError> {
    let kind = parse_field_kind(&channel.kind)?;
    let mut out = match kind {
        ParsedFieldKind::Quantitative => ParsedChannelDef::quantitative(channel.field),
        ParsedFieldKind::Ordinal => ParsedChannelDef::ordinal(channel.field),
        ParsedFieldKind::Nominal => ParsedChannelDef::nominal(channel.field),
        ParsedFieldKind::Temporal => ParsedChannelDef::temporal(channel.field),
    };
    if let Some(aggregate) = channel.aggregate {
        out = out.with_aggregate(parse_aggregate_op(&aggregate)?);
    }
    if let Some(title) = channel.title {
        out = out.with_title(title);
    }
    Ok(out)
}

fn parse_encoding_set(encoding: JsonEncoding) -> Result<ParsedEncodingSet, JsonSpecError> {
    let mut out = ParsedEncodingSet::new();
    if let Some(x) = encoding.x {
        out = out.with_x(parse_channel(x)?);
    }
    if let Some(x2) = encoding.x2 {
        out = out.with_x2(parse_channel(x2)?);
    }
    if let Some(y) = encoding.y {
        out = out.with_y(parse_channel(y)?);
    }
    if let Some(y2) = encoding.y2 {
        out = out.with_y2(parse_channel(y2)?);
    }
    if let Some(color) = encoding.color {
        out = out.with_color(parse_channel(color)?);
    }
    if let Some(size) = encoding.size {
        out = out.with_size_channel(parse_channel(size)?);
    }
    if let Some(shape) = encoding.shape {
        out = out.with_shape(parse_channel(shape)?);
    }
    if let Some(opacity) = encoding.opacity {
        out = out.with_opacity(parse_channel(opacity)?);
    }
    if let Some(stroke) = encoding.stroke {
        out = out.with_stroke(parse_channel(stroke)?);
    }
    if let Some(stroke_width) = encoding.stroke_width {
        out = out.with_stroke_width(parse_channel(stroke_width)?);
    }
    if let Some(order) = encoding.order {
        out = out.with_order(parse_channel(order)?);
    }
    if let Some(detail) = encoding.detail {
        out = out.with_detail(parse_channel(detail)?);
    }
    if let Some(text) = encoding.text {
        out = out.with_text(parse_channel(text)?);
    }
    Ok(out)
}

fn parse_transform(value: Value) -> Result<ParsedTransformSpec, JsonSpecError> {
    let Some(object) = value.as_object() else {
        return Err(JsonSpecError::Invalid(String::from(
            "transform entries must be JSON objects",
        )));
    };

    if object.contains_key("filter") {
        let raw: JsonFilterTransform = serde_json::from_value(value)?;
        if raw.columns.is_empty() {
            return Err(JsonSpecError::Invalid(String::from(
                "filter transforms currently require a `columns` array",
            )));
        }
        return Ok(ParsedTransformSpec::filter(
            ParsedPredicate::new(
                raw.filter.field,
                parse_compare_op(&raw.filter.op)?,
                raw.filter.value,
            ),
            raw.columns,
        ));
    }

    if object.contains_key("sort") {
        let raw: JsonSortTransform = serde_json::from_value(value)?;
        if raw.columns.is_empty() {
            return Err(JsonSpecError::Invalid(String::from(
                "sort transforms currently require a `columns` array",
            )));
        }
        return Ok(ParsedTransformSpec::sort(
            raw.sort.field,
            parse_sort_order(raw.sort.order.as_deref().unwrap_or("ascending"))?,
            raw.columns,
        ));
    }

    if object.contains_key("aggregate") {
        let raw: JsonAggregateTransform = serde_json::from_value(value)?;
        let fields = raw
            .aggregate
            .into_iter()
            .map(|field| {
                Ok(ParsedAggregateField::new(
                    parse_aggregate_op(&field.op)?,
                    field.field,
                    field.as_field,
                ))
            })
            .collect::<Result<Vec<_>, JsonSpecError>>()?;
        return Ok(ParsedTransformSpec::aggregate(raw.groupby, fields));
    }

    if object.contains_key("bin") {
        let raw: JsonBinTransform = serde_json::from_value(value)?;
        if raw.columns.is_empty() {
            return Err(JsonSpecError::Invalid(String::from(
                "bin transforms currently require a `columns` array",
            )));
        }
        return Ok(ParsedTransformSpec::bin(
            raw.bin.field,
            raw.bin.as_field,
            raw.bin.step,
            raw.columns,
        ));
    }

    if object.contains_key("stack") {
        let raw: JsonStackTransform = serde_json::from_value(value)?;
        if raw.columns.is_empty() {
            return Err(JsonSpecError::Invalid(String::from(
                "stack transforms currently require a `columns` array",
            )));
        }
        return Ok(ParsedTransformSpec::stack(
            raw.groupby,
            parse_stack_offset(raw.offset.as_deref().unwrap_or("zero"))?,
            raw.sort.clone().map(|sort| sort.field),
            parse_sort_order(
                raw.sort
                    .as_ref()
                    .and_then(|sort| sort.order.as_deref())
                    .unwrap_or("ascending"),
            )?,
            raw.stack,
            raw.as_fields[0].clone(),
            raw.as_fields[1].clone(),
            raw.columns,
        ));
    }

    Err(JsonSpecError::Invalid(String::from(
        "unsupported transform object",
    )))
}

fn parse_brush(raw: &str) -> Result<Brush, JsonSpecError> {
    Ok(Brush::Solid(parse_color(raw)?))
}

fn parse_color(raw: &str) -> Result<Color, JsonSpecError> {
    let raw = raw.strip_prefix('#').ok_or_else(|| {
        JsonSpecError::Invalid(format!(
            "literal style colors currently require #RRGGBB or #RRGGBBAA hex strings, got {raw}"
        ))
    })?;
    let bytes = match raw.len() {
        6 => {
            let r = u8::from_str_radix(&raw[0..2], 16)
                .map_err(|_| JsonSpecError::Invalid(format!("invalid hex color #{raw}")))?;
            let g = u8::from_str_radix(&raw[2..4], 16)
                .map_err(|_| JsonSpecError::Invalid(format!("invalid hex color #{raw}")))?;
            let b = u8::from_str_radix(&raw[4..6], 16)
                .map_err(|_| JsonSpecError::Invalid(format!("invalid hex color #{raw}")))?;
            [r, g, b, 255]
        }
        8 => {
            let r = u8::from_str_radix(&raw[0..2], 16)
                .map_err(|_| JsonSpecError::Invalid(format!("invalid hex color #{raw}")))?;
            let g = u8::from_str_radix(&raw[2..4], 16)
                .map_err(|_| JsonSpecError::Invalid(format!("invalid hex color #{raw}")))?;
            let b = u8::from_str_radix(&raw[4..6], 16)
                .map_err(|_| JsonSpecError::Invalid(format!("invalid hex color #{raw}")))?;
            let a = u8::from_str_radix(&raw[6..8], 16)
                .map_err(|_| JsonSpecError::Invalid(format!("invalid hex color #{raw}")))?;
            [r, g, b, a]
        }
        _ => {
            return Err(JsonSpecError::Invalid(format!(
                "literal style colors currently require #RRGGBB or #RRGGBBAA hex strings, got #{raw}"
            )));
        }
    };
    Ok(Color::from_rgba8(bytes[0], bytes[1], bytes[2], bytes[3]))
}

fn parse_field_kind(kind: &str) -> Result<ParsedFieldKind, JsonSpecError> {
    match kind {
        "quantitative" | "Q" | "q" => Ok(ParsedFieldKind::Quantitative),
        "ordinal" | "O" | "o" => Ok(ParsedFieldKind::Ordinal),
        "nominal" | "N" | "n" => Ok(ParsedFieldKind::Nominal),
        "temporal" | "T" | "t" => Ok(ParsedFieldKind::Temporal),
        _ => Err(JsonSpecError::Invalid(format!(
            "unsupported channel type `{kind}`"
        ))),
    }
}

fn parse_aggregate_op(op: &str) -> Result<AggregateOp, JsonSpecError> {
    match op {
        "count" => Ok(AggregateOp::Count),
        "sum" => Ok(AggregateOp::Sum),
        "min" => Ok(AggregateOp::Min),
        "max" => Ok(AggregateOp::Max),
        "mean" | "average" => Ok(AggregateOp::Mean),
        _ => Err(JsonSpecError::Invalid(format!(
            "unsupported aggregate op `{op}`"
        ))),
    }
}

fn parse_compare_op(op: &str) -> Result<CompareOp, JsonSpecError> {
    match op {
        "lt" | "<" => Ok(CompareOp::Lt),
        "le" | "lte" | "<=" => Ok(CompareOp::Le),
        "gt" | ">" => Ok(CompareOp::Gt),
        "ge" | "gte" | ">=" => Ok(CompareOp::Ge),
        "eq" | "==" => Ok(CompareOp::Eq),
        "ne" | "!=" => Ok(CompareOp::Ne),
        _ => Err(JsonSpecError::Invalid(format!(
            "unsupported filter op `{op}`"
        ))),
    }
}

fn parse_sort_order(order: &str) -> Result<SortOrder, JsonSpecError> {
    match order {
        "asc" | "ascending" => Ok(SortOrder::Asc),
        "desc" | "descending" => Ok(SortOrder::Desc),
        _ => Err(JsonSpecError::Invalid(format!(
            "unsupported sort order `{order}`"
        ))),
    }
}

fn parse_stack_offset(offset: &str) -> Result<StackOffset, JsonSpecError> {
    match offset {
        "zero" => Ok(StackOffset::Zero),
        "center" => Ok(StackOffset::Center),
        "wiggle" => Ok(StackOffset::Wiggle),
        "normalize" => Ok(StackOffset::Normalize),
        _ => Err(JsonSpecError::Invalid(format!(
            "unsupported stack offset `{offset}`"
        ))),
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::{
        AdaptContext, DataRef, ParsedMarkDef, ParsedUnitSpec, SchemaField, SliceFieldResolver,
    };
    use vizir_core::{ColumnId, TableId};
    use vizir_transforms::AggregateOp;

    #[test]
    fn parses_channel_aggregate_bar_spec() {
        let spec = parse_unit_spec_json(
            r#"{
                "mark": "bar",
                "title": "Totals",
                "width": 240.0,
                "height": 120.0,
                "encoding": {
                    "x": { "field": "category", "type": "ordinal", "title": "category" },
                    "y": { "field": "value", "type": "quantitative", "aggregate": "sum", "title": "sum(value)" }
                }
            }"#,
        )
        .expect("parse bar spec");

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "category",
                column: ColumnId(0),
            },
            SchemaField {
                name: "value",
                column: ColumnId(1),
            },
        ]);
        let _unit = spec
            .adapt(
                &resolver,
                AdaptContext {
                    id_base: 0xA0_000,
                    derived_table_base: TableId(100),
                    data: DataRef::Table(TableId(1)),
                },
            )
            .expect("adapt parsed bar spec");
    }

    #[test]
    fn parses_transform_aliases_into_parsed_spec() {
        let spec = parse_unit_spec_json(
            r#"{
                "mark": {"type": "bar"},
                "transform": [
                    {
                        "aggregate": [{ "op": "sum", "field": "value", "as": "sum_value" }],
                        "groupby": ["category"]
                    }
                ],
                "encoding": {
                    "x": { "field": "category", "type": "ordinal" },
                    "y": { "field": "sum_value", "type": "quantitative" }
                }
            }"#,
        )
        .expect("parse aggregate transform spec");

        let _ = spec;
    }

    #[test]
    fn parses_ranged_area_secondary_channels() {
        let spec = ParsedUnitSpec::from_json_str(
            r#"{
                "mark": "area",
                "encoding": {
                    "x": { "field": "x", "type": "quantitative" },
                    "y": { "field": "y", "type": "quantitative" },
                    "x2": { "field": "x2", "type": "quantitative" },
                    "y2": { "field": "y2", "type": "quantitative" }
                }
            }"#,
        )
        .expect("parse ranged area");

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
                name: "x2",
                column: ColumnId(2),
            },
            SchemaField {
                name: "y2",
                column: ColumnId(3),
            },
        ]);
        let unit = spec
            .adapt(
                &resolver,
                AdaptContext {
                    id_base: 0xB0_000,
                    derived_table_base: TableId(200),
                    data: DataRef::Table(TableId(2)),
                },
            )
            .expect("adapt ranged area");
        let _ = unit;
    }

    #[test]
    fn rejects_unknown_mark() {
        let err = parse_unit_spec_json(r#"{ "mark": "tick" }"#).expect_err("tick should fail");
        assert!(
            matches!(err, JsonSpecError::Invalid(message) if message.contains("unsupported mark"))
        );
    }

    #[test]
    fn rejects_filter_without_columns() {
        let err = parse_unit_spec_json(
            r#"{
                "mark": "point",
                "transform": [
                    { "filter": { "field": "x", "op": "ge", "value": 1.0 } }
                ]
            }"#,
        )
        .expect_err("filter without columns should fail");
        assert!(matches!(err, JsonSpecError::Invalid(message) if message.contains("columns")));
    }

    #[test]
    fn parses_supported_aggregate_alias_name() {
        let spec = parse_unit_spec_json(
            r#"{
                "mark": "bar",
                "encoding": {
                    "x": { "field": "category", "type": "O" },
                    "y": { "field": "value", "type": "Q", "aggregate": "sum" }
                }
            }"#,
        )
        .expect("parse short type aliases");

        let _ = spec;
        let _sum = AggregateOp::Sum;
        let _bar = ParsedMarkDef::Bar;
    }

    #[test]
    fn parses_text_mark_spec() {
        let spec = parse_unit_spec_json(include_str!("../../fixtures/specs/unit_text.json"))
            .expect("parse text mark spec");

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "x",
                column: ColumnId(0),
            },
            SchemaField {
                name: "y",
                column: ColumnId(1),
            },
        ]);
        let _unit = spec
            .adapt(
                &resolver,
                AdaptContext {
                    id_base: 0xC1_000,
                    derived_table_base: TableId(301),
                    data: DataRef::Table(TableId(4)),
                },
            )
            .expect("adapt parsed text mark");
    }

    #[test]
    fn parses_rule_mark_spec() {
        let spec = parse_unit_spec_json(
            r#"{
                "mark": "rule",
                "encoding": {
                    "y": { "field": "threshold", "type": "quantitative", "title": "threshold" }
                }
            }"#,
        )
        .expect("parse rule mark spec");

        let resolver = SliceFieldResolver::new(&[SchemaField {
            name: "threshold",
            column: ColumnId(1),
        }]);
        let _unit = spec
            .adapt(
                &resolver,
                AdaptContext {
                    id_base: 0xC1_050,
                    derived_table_base: TableId(301),
                    data: DataRef::Table(TableId(4)),
                },
            )
            .expect("adapt parsed rule mark");
    }

    #[test]
    fn parses_point_shape_size_fixture() {
        let spec = parse_unit_spec_json(include_str!(
            "../../fixtures/specs/unit_point_shape_size.json"
        ))
        .expect("parse point shape/size spec");

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
                name: "magnitude",
                column: ColumnId(2),
            },
            SchemaField {
                name: "series",
                column: ColumnId(3),
            },
        ]);
        let _unit = spec
            .adapt(
                &resolver,
                AdaptContext {
                    id_base: 0xC1_100,
                    derived_table_base: TableId(304),
                    data: DataRef::Table(TableId(6)),
                },
            )
            .expect("adapt parsed point shape/size spec");
    }

    #[test]
    fn parses_point_opacity_fixture() {
        let spec =
            parse_unit_spec_json(include_str!("../../fixtures/specs/unit_point_opacity.json"))
                .expect("parse point opacity spec");

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
        let _unit = spec
            .adapt(
                &resolver,
                AdaptContext {
                    id_base: 0xC1_140,
                    derived_table_base: TableId(305),
                    data: DataRef::Table(TableId(7)),
                },
            )
            .expect("adapt parsed point opacity spec");
    }

    #[test]
    fn parses_point_stroke_width_fixture() {
        let spec = parse_unit_spec_json(include_str!(
            "../../fixtures/specs/unit_point_stroke_width.json"
        ))
        .expect("parse point stroke/width spec");

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
        let _unit = spec
            .adapt(
                &resolver,
                AdaptContext {
                    id_base: 0xC1_160,
                    derived_table_base: TableId(306),
                    data: DataRef::Table(TableId(8)),
                },
            )
            .expect("adapt parsed point stroke/width spec");
    }

    #[test]
    fn parses_line_order_detail_fixture() {
        let spec = parse_unit_spec_json(include_str!(
            "../../fixtures/specs/unit_line_order_detail.json"
        ))
        .expect("parse line order/detail spec");

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
        let _unit = spec
            .adapt(
                &resolver,
                AdaptContext {
                    id_base: 0xC1_180,
                    derived_table_base: TableId(305),
                    data: DataRef::Table(TableId(7)),
                },
            )
            .expect("adapt parsed order/detail spec");
    }

    #[test]
    fn parses_shared_plot_layer_spec() {
        let spec = parse_layer_spec_json(include_str!("../../fixtures/specs/layer_area_line.json"))
            .expect("parse layer spec");

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "x",
                column: ColumnId(0),
            },
            SchemaField {
                name: "area_top",
                column: ColumnId(1),
            },
            SchemaField {
                name: "line_y",
                column: ColumnId(2),
            },
        ]);
        let _layer = spec
            .adapt(
                &resolver,
                AdaptContext {
                    id_base: 0xC0_000,
                    derived_table_base: TableId(300),
                    data: DataRef::Table(TableId(3)),
                },
            )
            .expect("adapt parsed layer");
    }

    #[test]
    fn parses_bar_text_layer_fixture() {
        let spec = parse_layer_spec_json(include_str!("../../fixtures/specs/layer_bar_text.json"))
            .expect("parse bar + text layer");

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "category",
                column: ColumnId(0),
            },
            SchemaField {
                name: "value",
                column: ColumnId(1),
            },
        ]);
        let _layer = spec
            .adapt(
                &resolver,
                AdaptContext {
                    id_base: 0xC2_000,
                    derived_table_base: TableId(302),
                    data: DataRef::Table(TableId(5)),
                },
            )
            .expect("adapt parsed bar + text layer");
    }

    #[test]
    fn parses_layer_base_child_defaults_fixture() {
        let spec = parse_layer_spec_json(include_str!(
            "../../fixtures/specs/layer_base_child_defaults.json"
        ))
        .expect("parse base-child defaults layer");

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
        let _layer = spec
            .adapt(
                &resolver,
                AdaptContext {
                    id_base: 0xC2_100,
                    derived_table_base: TableId(306),
                    data: DataRef::Table(TableId(8)),
                },
            )
            .expect("adapt parsed base-child defaults layer");
    }

    #[test]
    fn parses_nested_child_units_fixture() {
        let spec =
            parse_layer_spec_json(include_str!("../../fixtures/specs/layer_nested_units.json"))
                .expect("parse nested child units layer");

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
        let _layer = spec
            .adapt(
                &resolver,
                AdaptContext {
                    id_base: 0xC2_200,
                    derived_table_base: TableId(307),
                    data: DataRef::Table(TableId(9)),
                },
            )
            .expect("adapt parsed nested child units layer");
    }

    #[test]
    fn parses_line_rule_layer_fixture() {
        let spec = parse_layer_spec_json(include_str!("../../fixtures/specs/layer_line_rule.json"))
            .expect("parse line + rule layer");

        let resolver = SliceFieldResolver::new(&[
            SchemaField {
                name: "x",
                column: ColumnId(0),
            },
            SchemaField {
                name: "y",
                column: ColumnId(1),
            },
        ]);
        let _layer = spec
            .adapt(
                &resolver,
                AdaptContext {
                    id_base: 0xC2_300,
                    derived_table_base: TableId(308),
                    data: DataRef::Table(TableId(10)),
                },
            )
            .expect("adapt parsed line + rule layer");
    }
}
