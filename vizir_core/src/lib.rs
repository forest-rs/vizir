// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `vizir_core`: minimal incremental viz runtime (tables, signals, marks, diffs).
//!
//! This crate provides:
//! - versioned inputs ([`Table`]/[`Signal`])
//! - stable mark identity ([`MarkId`])
//! - explicit dependency tracking ([`InputRef`])
//! - incremental evaluation + diff output ([`MarkDiff`])
//! - per-kind mark payloads ([`MarkPayload`])
//!
//! It intentionally does NOT provide a full visualization grammar.
//!
//! Conceptually, a chart frontend can:
//! - store data in a [`Table`] (row keys + optional column access via [`TableData`])
//! - store interaction state in [`Signal`]s (zoom, selection, etc.)
//! - generate one [`Mark`] per row (bars/points/labels) with stable [`MarkId`]s
//! - call [`Scene::tick_table_rows`] and apply the resulting [`MarkDiff`] stream to a renderer.

#![no_std]

extern crate alloc;

use alloc::{boxed::Box, string::String, vec::Vec};
use core::any::Any;
use core::fmt;
use hashbrown::HashMap;
use hashbrown::hash_map::Entry;
use kurbo::{BezPath, Point, Rect, Shape};
use peniko::{Brush, Color};
use smallvec::SmallVec;

/// Monotonic version counter for inputs.
pub type Version = u64;

/// Stable identifier for a [`Table`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TableId(pub u32);

/// Stable identifier for a [`Signal`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SignalId(pub u32);

/// Stable identifier for a [`Mark`].
///
/// `MarkId`s must remain stable across frames for the same conceptual visual item; this is what
/// enables `Enter/Update/Exit` diffs and smooth transitions.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MarkId(pub u64);

impl MarkId {
    /// Create a mark id from a raw value.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Create a stable mark id for a row key within a table.
    ///
    /// This is a deterministic namespacing mix intended for stable identity across frames.
    pub fn for_row(table: TableId, row_key: u64) -> Self {
        // 64-bit mix based on golden ratio and some rotation; deterministic across runs.
        let table_ns = u64::from(table.0).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mixed =
            table_ns ^ row_key.rotate_left(17) ^ row_key.wrapping_mul(0xD6E8_FEB8_6659_FD93);
        Self(mixed)
    }
}

/// Stable identifier for a table column (placeholder).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ColId(pub u32);

/// The geometric "kind" of a mark, which determines how channels are interpreted.
///
/// `MarkKind` is derived from [`MarkEncodings`] (and is also echoed on [`MarkDiff`]) so downstream
/// renderers can interpret [`MarkPayload`] correctly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarkKind {
    /// An axis-aligned rectangle using [`RectChannels`].
    Rect,
    /// A text item positioned at a point.
    Text,
    /// A vector path.
    Path,
}

/// An input reference used for dependency tracking.
///
/// These references are declared on computed encodings (see [`Encoding::Compute`]) and determine
/// what becomes dirty when tables/signals change.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum InputRef {
    /// Reference an entire table (coarse dependency).
    ///
    /// Use this when an encoding depends on a whole table (e.g. a line path generated from all
    /// rows), rather than a single column.
    Table {
        /// The referenced table.
        table: TableId,
    },
    /// Reference a specific table column.
    ///
    /// Use this for “one mark per row” encodings where individual channels read per-row values.
    TableCol {
        /// The referenced table.
        table: TableId,
        /// The referenced column.
        col: ColId,
    },
    /// Reference a signal.
    Signal {
        /// The referenced signal.
        signal: SignalId,
    },
}

/// Minimal columnar table placeholder.
/// Replace with Arrow, your own column store, or a trait object later.
#[derive(Debug)]
pub struct Table {
    /// Stable identifier.
    pub id: TableId,
    /// Monotonic version counter.
    pub version: Version,
    /// Stable keys for each row.
    ///
    /// v1 is intentionally minimal: this is enough to derive stable [`MarkId`]s for
    /// row-driven mark sets without committing to a column representation yet.
    pub row_keys: Vec<u64>,

    /// Optional columnar access for encodings.
    pub data: Option<Box<dyn TableData>>,
}

impl Table {
    /// Create a new table with version `1`.
    pub fn new(id: TableId) -> Self {
        Self {
            id,
            version: 1,
            row_keys: Vec::new(),
            data: None,
        }
    }

    /// Increment the version counter.
    pub fn bump(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    /// Replace the table's row keys and bump its version.
    pub fn set_row_keys(&mut self, row_keys: Vec<u64>) {
        self.row_keys = row_keys;
        self.bump();
    }

    /// Set the table's data accessor and bump its version.
    pub fn set_data(&mut self, data: Option<Box<dyn TableData>>) {
        self.data = data;
        self.bump();
    }

    /// Return the number of rows.
    pub fn row_count(&self) -> usize {
        self.row_keys.len()
    }

    /// Return the stable key for a row index, if present.
    pub fn row_key(&self, row: usize) -> Option<u64> {
        self.row_keys.get(row).copied()
    }
}

/// Optional columnar access for table-driven mark encodings.
///
/// v0: only `f64` is supported because it's enough for basic charts (positions, sizes).
///
/// To use this, implement `TableData` on your column store and set [`Table::data`]. Computed mark
/// encodings can read values via [`EvalCtx::table_f64`].
pub trait TableData: fmt::Debug {
    /// Number of rows available via this accessor.
    fn row_count(&self) -> usize;

    /// Return a numeric value for a given row/column.
    fn f64(&self, row: usize, col: ColId) -> Option<f64>;
}

/// Type-erased access to a [`Signal`] for storage in a scene.
pub trait AnySignal: Any {
    /// Return the signal's stable identifier.
    fn id(&self) -> SignalId;
    /// Return the signal's version counter.
    fn version(&self) -> Version;
    /// Increment the version counter.
    fn bump(&mut self);
    /// Downcast support.
    fn as_any(&self) -> &dyn Any;
    /// Downcast support (mutable).
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// A versioned scalar value.
#[derive(Debug)]
pub struct Signal<T: Clone + 'static> {
    /// Stable identifier.
    pub id: SignalId,
    /// Monotonic version counter.
    pub version: Version,
    /// Current value.
    pub value: T,
}

impl<T: Clone + 'static> AnySignal for Signal<T> {
    fn id(&self) -> SignalId {
        self.id
    }
    fn version(&self) -> Version {
        self.version
    }
    fn bump(&mut self) {
        self.version = self.version.wrapping_add(1);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Errors returned by typed signal accessors on [`Scene`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SignalAccessError {
    /// A signal exists at the requested id, but its type does not match `T`.
    TypeMismatch,
}

type ComputeFn<T> = dyn Fn(&EvalCtx<'_>, MarkId) -> T + 'static;

/// A single encoding channel on a mark.
/// v1 supports either a constant or a computed function.
///
/// When using [`Encoding::Compute`], the `deps` list must include all inputs the closure reads.
/// Dependency tracking is explicit; missing deps means missing updates.
pub enum Encoding<T> {
    /// A constant value.
    Const(T),
    /// A computed value, with explicit dependencies.
    Compute {
        /// The inputs that may affect this encoding.
        deps: SmallVec<[InputRef; 4]>,
        /// Compute the value for a given mark.
        f: Box<ComputeFn<T>>,
    },
}

impl<T: fmt::Debug> fmt::Debug for Encoding<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Const(v) => f.debug_tuple("Const").field(v).finish(),
            Self::Compute { deps, .. } => f
                .debug_struct("Compute")
                .field("deps", deps)
                .field("f", &"<fn>")
                .finish(),
        }
    }
}

impl<T: Clone> Encoding<T> {
    fn deps(&self) -> SmallVec<[InputRef; 4]> {
        match self {
            Self::Const(_) => SmallVec::new(),
            Self::Compute { deps, .. } => deps.clone(),
        }
    }
}

fn deps4(deps: impl IntoIterator<Item = InputRef>) -> SmallVec<[InputRef; 4]> {
    let mut out = SmallVec::new();
    for dep in deps {
        out.push(dep);
    }
    out
}

/// Evaluated per-kind channels (payload) for a mark instance.
///
/// This is the “render-facing” data model: it is what downstream renderers consume, and it is what
/// appears in [`MarkDiff`] diffs (boxed) and cached on [`Mark`].
#[derive(Clone, Debug, PartialEq)]
pub enum MarkPayload {
    /// An axis-aligned rectangle.
    Rect(RectChannels),
    /// A text item positioned at a point.
    Text(TextChannels),
    /// A vector path.
    Path(PathChannels),
}

impl MarkPayload {
    /// Return the kind of this payload.
    pub fn kind(&self) -> MarkKind {
        match self {
            Self::Rect(_) => MarkKind::Rect,
            Self::Text(_) => MarkKind::Text,
            Self::Path(_) => MarkKind::Path,
        }
    }

    /// Optional bounds hint for downstream damage calculation.
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Self::Rect(r) => Some(r.rect),
            // v1: text shaping/layout is downstream; bounds are not known here.
            Self::Text(_) => None,
            Self::Path(p) => Some(p.path.bounding_box()),
        }
    }
}

/// Evaluated channels for [`MarkKind::Rect`].
#[derive(Clone, Debug, PartialEq)]
pub struct RectChannels {
    /// Rectangle geometry in scene coordinates.
    pub rect: Rect,
    /// Fill paint.
    pub fill: Brush,
}

/// Evaluated channels for [`MarkKind::Text`].
#[derive(Clone, Debug, PartialEq)]
pub struct TextChannels {
    /// Anchor position in scene coordinates.
    pub pos: Point,
    /// Text content (unshaped).
    pub text: String,
    /// Font size in scene coordinates.
    pub font_size: f64,
    /// Text rotation angle in degrees, with positive angles rotating clockwise.
    ///
    /// This is consumed by downstream renderers (for example, SVG `transform="rotate(...)"`).
    /// In charting contexts, a left axis title is typically rendered with `-90` degrees.
    pub angle: f64,
    /// Horizontal text anchoring (how the glyphs align relative to [`TextChannels::pos`]).
    pub anchor: TextAnchor,
    /// Vertical alignment for text relative to [`TextChannels::pos`].
    pub baseline: TextBaseline,
    /// Fill paint.
    pub fill: Brush,
}

/// Horizontal anchoring for text.
///
/// This is typically set via [`MarkBuilder::text_anchor`], [`MarkBuilder::text_anchor_end`], or
/// [`MarkBuilder::text_anchor_middle`], and consumed by downstream renderers when placing text.
///
/// In SVG terms, this maps to the `text-anchor` attribute. In typical chart usage:
/// - y-axis tick labels use [`TextAnchor::End`] so the label’s right edge sits against the axis.
/// - x-axis tick labels use [`TextAnchor::Middle`] to center labels under ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextAnchor {
    /// Anchor at the start (left in LTR).
    Start,
    /// Anchor in the middle.
    Middle,
    /// Anchor at the end (right in LTR).
    End,
}

/// Vertical alignment for text.
///
/// This is typically set via [`MarkBuilder::text_baseline`] and consumed by downstream renderers.
///
/// In SVG terms, this maps to the `dominant-baseline` attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextBaseline {
    /// Baseline is centered on the anchor point.
    Middle,
    /// Baseline is the font’s alphabetic baseline.
    Alphabetic,
    /// Baseline is the font’s hanging baseline.
    Hanging,
    /// Baseline is the font’s ideographic baseline.
    Ideographic,
}

/// Evaluated channels for [`MarkKind::Path`].
#[derive(Clone, Debug, PartialEq)]
pub struct PathChannels {
    /// The vector path geometry.
    pub path: BezPath,
    /// Fill paint.
    pub fill: Brush,
    /// Stroke paint.
    pub stroke: Brush,
    /// Stroke width in scene coordinates.
    pub stroke_width: f64,
}

impl Default for RectChannels {
    fn default() -> Self {
        Self {
            rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            fill: Brush::Solid(Color::from_rgba8(0, 0, 0, 255)),
        }
    }
}

impl Default for TextChannels {
    fn default() -> Self {
        Self {
            pos: Point::new(0.0, 0.0),
            text: String::new(),
            font_size: 12.0,
            angle: 0.0,
            anchor: TextAnchor::Start,
            baseline: TextBaseline::Middle,
            fill: Brush::Solid(Color::from_rgba8(0, 0, 0, 255)),
        }
    }
}

impl Default for PathChannels {
    fn default() -> Self {
        Self {
            path: BezPath::new(),
            fill: Brush::Solid(Color::from_rgba8(0, 0, 0, 255)),
            stroke: Brush::default(),
            stroke_width: 0.0,
        }
    }
}

/// Declarative encodings for a mark.
///
/// These are the “bind-time” inputs: a higher layer (charts, DSL, Vega-like compiler) builds
/// `MarkEncodings`, and the core evaluates them incrementally to produce [`MarkPayload`] diffs.
#[derive(Debug)]
pub enum MarkEncodings {
    /// Encodings for [`MarkKind::Rect`].
    Rect(Box<RectEncodings>),
    /// Encodings for [`MarkKind::Text`].
    Text(Box<TextEncodings>),
    /// Encodings for [`MarkKind::Path`].
    Path(Box<PathEncodings>),
}

impl MarkEncodings {
    fn kind(&self) -> MarkKind {
        match self {
            Self::Rect(_) => MarkKind::Rect,
            Self::Text(_) => MarkKind::Text,
            Self::Path(_) => MarkKind::Path,
        }
    }

    fn deps(&self) -> SmallVec<[InputRef; 8]> {
        let mut out = SmallVec::new();
        match self {
            Self::Rect(e) => {
                let e = e.as_ref();
                out.extend(e.x.deps());
                out.extend(e.y.deps());
                out.extend(e.w.deps());
                out.extend(e.h.deps());
                out.extend(e.fill.deps());
            }
            Self::Text(e) => {
                let e = e.as_ref();
                out.extend(e.x.deps());
                out.extend(e.y.deps());
                out.extend(e.text.deps());
                out.extend(e.font_size.deps());
                out.extend(e.angle.deps());
                out.extend(e.anchor.deps());
                out.extend(e.baseline.deps());
                out.extend(e.fill.deps());
            }
            Self::Path(e) => {
                let e = e.as_ref();
                out.extend(e.path.deps());
                out.extend(e.fill.deps());
                out.extend(e.stroke.deps());
                out.extend(e.stroke_width.deps());
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

/// Encodings for [`MarkKind::Rect`].
#[derive(Debug)]
pub struct RectEncodings {
    /// X position.
    pub x: Encoding<f64>,
    /// Y position.
    pub y: Encoding<f64>,
    /// Width.
    pub w: Encoding<f64>,
    /// Height.
    pub h: Encoding<f64>,
    /// Fill paint.
    pub fill: Encoding<Brush>,
}

/// Encodings for [`MarkKind::Text`].
#[derive(Debug)]
pub struct TextEncodings {
    /// X position.
    pub x: Encoding<f64>,
    /// Y position.
    pub y: Encoding<f64>,
    /// Text content.
    pub text: Encoding<String>,
    /// Font size.
    pub font_size: Encoding<f64>,
    /// Text rotation angle in degrees (see [`TextChannels::angle`]).
    pub angle: Encoding<f64>,
    /// Horizontal text anchoring (see [`TextAnchor`]).
    pub anchor: Encoding<TextAnchor>,
    /// Vertical alignment for text (see [`TextBaseline`]).
    pub baseline: Encoding<TextBaseline>,
    /// Fill paint.
    pub fill: Encoding<Brush>,
}

/// Encodings for [`MarkKind::Path`].
#[derive(Debug)]
pub struct PathEncodings {
    /// Path geometry.
    pub path: Encoding<BezPath>,
    /// Fill paint.
    pub fill: Encoding<Brush>,
    /// Stroke paint.
    pub stroke: Encoding<Brush>,
    /// Stroke width.
    pub stroke_width: Encoding<f64>,
}

impl Default for RectEncodings {
    fn default() -> Self {
        Self {
            x: Encoding::Const(0.0),
            y: Encoding::Const(0.0),
            w: Encoding::Const(0.0),
            h: Encoding::Const(0.0),
            fill: Encoding::Const(Brush::Solid(Color::from_rgba8(0, 0, 0, 255))),
        }
    }
}

impl Default for TextEncodings {
    fn default() -> Self {
        Self {
            x: Encoding::Const(0.0),
            y: Encoding::Const(0.0),
            text: Encoding::Const(String::new()),
            font_size: Encoding::Const(12.0),
            angle: Encoding::Const(0.0),
            anchor: Encoding::Const(TextAnchor::Start),
            baseline: Encoding::Const(TextBaseline::Middle),
            fill: Encoding::Const(Brush::Solid(Color::from_rgba8(0, 0, 0, 255))),
        }
    }
}

impl Default for PathEncodings {
    fn default() -> Self {
        Self {
            path: Encoding::Const(BezPath::new()),
            fill: Encoding::Const(Brush::Solid(Color::from_rgba8(0, 0, 0, 255))),
            stroke: Encoding::Const(Brush::default()),
            stroke_width: Encoding::Const(0.0),
        }
    }
}

/// A stable-identity visual instance with declarative encodings.
///
/// After mutating any encoding, call [`Mark::rebuild_deps`] so incremental updates
/// can cheaply detect dirtiness.
#[derive(Debug)]
pub struct Mark {
    /// Stable identifier.
    pub id: MarkId,

    /// Z-ordering for rendering; higher values are drawn above lower values.
    pub z_index: i32,

    /// The geometric kind of this mark.
    pub kind: MarkKind,

    /// Encodings for this mark's kind.
    pub encodings: MarkEncodings,

    /// Flattened dependency summary for quick dirtiness checks.
    pub deps: SmallVec<[InputRef; 8]>,

    /// Cached evaluated channels for diffing/animation.
    pub cache: Option<MarkPayload>,

    /// Z-index used at the time of the last evaluation (for diffing/reordering).
    cached_z_index: i32,

    /// Last versions observed for inputs (simple per-mark tracking).
    pub last_seen: HashMap<InputRef, Version>,

    force_eval: bool,
}

impl Mark {
    /// Create a mark with constant/default encodings.
    pub fn new(id: MarkId) -> Self {
        let mut m = Self {
            id,
            z_index: 0,
            kind: MarkKind::Rect,
            encodings: MarkEncodings::Rect(Box::default()),
            deps: SmallVec::new(),
            cache: None,
            cached_z_index: 0,
            last_seen: HashMap::new(),
            force_eval: false,
        };
        m.rebuild_deps();
        m
    }

    /// Rebuild [`Mark::deps`] from per-encoding deps.
    pub fn rebuild_deps(&mut self) {
        self.kind = self.encodings.kind();
        self.deps = self.encodings.deps();
    }

    /// Start building a mark with chainable encoding setters.
    pub fn builder(id: MarkId) -> MarkBuilder {
        MarkBuilder {
            mark: Self::new(id),
        }
    }
}

/// A builder for [`Mark`] that rebuilds dependencies on `build()`.
#[derive(Debug)]
pub struct MarkBuilder {
    mark: Mark,
}

impl MarkBuilder {
    /// Set the mark z-index (rendering order).
    pub fn z_index(mut self, z_index: i32) -> Self {
        self.mark.z_index = z_index;
        self
    }

    /// Set the mark kind.
    pub fn kind(mut self, kind: MarkKind) -> Self {
        self.mark.kind = kind;
        self.mark.encodings = match kind {
            MarkKind::Rect => MarkEncodings::Rect(Box::default()),
            MarkKind::Text => MarkEncodings::Text(Box::default()),
            MarkKind::Path => MarkEncodings::Path(Box::default()),
        };
        self
    }

    /// Convenience for `MarkKind::Rect`.
    pub fn rect(self) -> Self {
        self.kind(MarkKind::Rect)
    }

    /// Convenience for `MarkKind::Text`.
    pub fn text(self) -> Self {
        self.kind(MarkKind::Text)
    }

    /// Convenience for `MarkKind::Path`.
    pub fn path(self) -> Self {
        self.kind(MarkKind::Path)
    }

    /// Set the `x` encoding to a constant value.
    pub fn x_const(mut self, v: f64) -> Self {
        match &mut self.mark.encodings {
            MarkEncodings::Rect(e) => e.as_mut().x = Encoding::Const(v),
            MarkEncodings::Text(e) => e.as_mut().x = Encoding::Const(v),
            MarkEncodings::Path(_) => {}
        }
        self
    }

    /// Set the `x` encoding to a computed value.
    pub fn x_compute(
        mut self,
        deps: impl IntoIterator<Item = InputRef>,
        f: impl Fn(&EvalCtx<'_>, MarkId) -> f64 + 'static,
    ) -> Self {
        match &mut self.mark.encodings {
            MarkEncodings::Rect(e) => {
                e.as_mut().x = Encoding::Compute {
                    deps: deps4(deps),
                    f: Box::new(f),
                };
            }
            MarkEncodings::Text(e) => {
                e.as_mut().x = Encoding::Compute {
                    deps: deps4(deps),
                    f: Box::new(f),
                };
            }
            MarkEncodings::Path(_) => {}
        }
        self
    }

    /// Set the `y` encoding to a constant value.
    pub fn y_const(mut self, v: f64) -> Self {
        match &mut self.mark.encodings {
            MarkEncodings::Rect(e) => e.as_mut().y = Encoding::Const(v),
            MarkEncodings::Text(e) => e.as_mut().y = Encoding::Const(v),
            MarkEncodings::Path(_) => {}
        }
        self
    }

    /// Set the `y` encoding to a computed value.
    pub fn y_compute(
        mut self,
        deps: impl IntoIterator<Item = InputRef>,
        f: impl Fn(&EvalCtx<'_>, MarkId) -> f64 + 'static,
    ) -> Self {
        match &mut self.mark.encodings {
            MarkEncodings::Rect(e) => {
                e.as_mut().y = Encoding::Compute {
                    deps: deps4(deps),
                    f: Box::new(f),
                };
            }
            MarkEncodings::Text(e) => {
                e.as_mut().y = Encoding::Compute {
                    deps: deps4(deps),
                    f: Box::new(f),
                };
            }
            MarkEncodings::Path(_) => {}
        }
        self
    }

    /// Set the `w` encoding to a constant value.
    pub fn w_const(mut self, v: f64) -> Self {
        if let MarkEncodings::Rect(e) = &mut self.mark.encodings {
            e.as_mut().w = Encoding::Const(v);
        }
        self
    }

    /// Set the `w` encoding to a computed value (rect marks only).
    pub fn w_compute(
        mut self,
        deps: impl IntoIterator<Item = InputRef>,
        f: impl Fn(&EvalCtx<'_>, MarkId) -> f64 + 'static,
    ) -> Self {
        if let MarkEncodings::Rect(e) = &mut self.mark.encodings {
            e.as_mut().w = Encoding::Compute {
                deps: deps4(deps),
                f: Box::new(f),
            };
        }
        self
    }

    /// Set the `h` encoding to a constant value.
    pub fn h_const(mut self, v: f64) -> Self {
        if let MarkEncodings::Rect(e) = &mut self.mark.encodings {
            e.as_mut().h = Encoding::Const(v);
        }
        self
    }

    /// Set the `h` encoding to a computed value (rect marks only).
    pub fn h_compute(
        mut self,
        deps: impl IntoIterator<Item = InputRef>,
        f: impl Fn(&EvalCtx<'_>, MarkId) -> f64 + 'static,
    ) -> Self {
        if let MarkEncodings::Rect(e) = &mut self.mark.encodings {
            e.as_mut().h = Encoding::Compute {
                deps: deps4(deps),
                f: Box::new(f),
            };
        }
        self
    }

    /// Set the `fill` encoding to a constant value.
    pub fn fill_const(mut self, v: Color) -> Self {
        let brush = Brush::Solid(v);
        match &mut self.mark.encodings {
            MarkEncodings::Rect(e) => e.as_mut().fill = Encoding::Const(brush),
            MarkEncodings::Text(e) => e.as_mut().fill = Encoding::Const(brush),
            MarkEncodings::Path(e) => e.as_mut().fill = Encoding::Const(brush),
        }
        self
    }

    /// Set the `fill` encoding to a constant brush.
    pub fn fill_brush_const(mut self, v: impl Into<Brush>) -> Self {
        let v = v.into();
        match &mut self.mark.encodings {
            MarkEncodings::Rect(e) => e.as_mut().fill = Encoding::Const(v),
            MarkEncodings::Text(e) => e.as_mut().fill = Encoding::Const(v),
            MarkEncodings::Path(e) => e.as_mut().fill = Encoding::Const(v),
        }
        self
    }

    /// Set the `fill` encoding to a computed value.
    pub fn fill_compute(
        mut self,
        deps: impl IntoIterator<Item = InputRef>,
        f: impl Fn(&EvalCtx<'_>, MarkId) -> Brush + 'static,
    ) -> Self {
        match &mut self.mark.encodings {
            MarkEncodings::Rect(e) => {
                e.as_mut().fill = Encoding::Compute {
                    deps: deps4(deps),
                    f: Box::new(f),
                };
            }
            MarkEncodings::Text(e) => {
                e.as_mut().fill = Encoding::Compute {
                    deps: deps4(deps),
                    f: Box::new(f),
                };
            }
            MarkEncodings::Path(e) => {
                e.as_mut().fill = Encoding::Compute {
                    deps: deps4(deps),
                    f: Box::new(f),
                };
            }
        }
        self
    }

    /// Set the `text` encoding to a constant value (text marks only).
    pub fn text_const(mut self, v: impl Into<String>) -> Self {
        if let MarkEncodings::Text(e) = &mut self.mark.encodings {
            e.as_mut().text = Encoding::Const(v.into());
        }
        self
    }

    /// Set the `text` encoding to a computed value (text marks only).
    pub fn text_compute(
        mut self,
        deps: impl IntoIterator<Item = InputRef>,
        f: impl Fn(&EvalCtx<'_>, MarkId) -> String + 'static,
    ) -> Self {
        if let MarkEncodings::Text(e) = &mut self.mark.encodings {
            e.as_mut().text = Encoding::Compute {
                deps: deps4(deps),
                f: Box::new(f),
            };
        }
        self
    }

    /// Set the `font_size` encoding to a constant value (text marks only).
    pub fn font_size_const(mut self, v: f64) -> Self {
        if let MarkEncodings::Text(e) = &mut self.mark.encodings {
            e.as_mut().font_size = Encoding::Const(v);
        }
        self
    }

    /// Set the `angle` encoding to a constant value in degrees (text marks only).
    pub fn angle_const(mut self, v: f64) -> Self {
        if let MarkEncodings::Text(e) = &mut self.mark.encodings {
            e.as_mut().angle = Encoding::Const(v);
        }
        self
    }

    /// Set the `angle` encoding to a computed value in degrees (text marks only).
    pub fn angle_compute(
        mut self,
        deps: impl IntoIterator<Item = InputRef>,
        f: impl Fn(&EvalCtx<'_>, MarkId) -> f64 + 'static,
    ) -> Self {
        if let MarkEncodings::Text(e) = &mut self.mark.encodings {
            e.as_mut().angle = Encoding::Compute {
                deps: deps4(deps),
                f: Box::new(f),
            };
        }
        self
    }

    /// Set the text anchor (text marks only).
    ///
    /// This controls how text aligns relative to the `(x, y)` position.
    pub fn text_anchor(mut self, anchor: TextAnchor) -> Self {
        if let MarkEncodings::Text(e) = &mut self.mark.encodings {
            e.as_mut().anchor = Encoding::Const(anchor);
        }
        self
    }

    /// Convenience for right-aligned text (`text-anchor="end"`).
    pub fn text_anchor_end(self) -> Self {
        self.text_anchor(TextAnchor::End)
    }

    /// Convenience for centered text (`text-anchor="middle"`).
    pub fn text_anchor_middle(self) -> Self {
        self.text_anchor(TextAnchor::Middle)
    }

    /// Set the text baseline (text marks only).
    ///
    /// This controls how text aligns vertically relative to the `(x, y)` position.
    pub fn text_baseline(mut self, baseline: TextBaseline) -> Self {
        if let MarkEncodings::Text(e) = &mut self.mark.encodings {
            e.as_mut().baseline = Encoding::Const(baseline);
        }
        self
    }

    /// Convenience for centered text (`dominant-baseline="middle"`).
    pub fn text_baseline_middle(self) -> Self {
        self.text_baseline(TextBaseline::Middle)
    }

    /// Convenience for alphabetic baseline text (`dominant-baseline="alphabetic"`).
    pub fn text_baseline_alphabetic(self) -> Self {
        self.text_baseline(TextBaseline::Alphabetic)
    }

    /// Set the `font_size` encoding to a computed value (text marks only).
    pub fn font_size_compute(
        mut self,
        deps: impl IntoIterator<Item = InputRef>,
        f: impl Fn(&EvalCtx<'_>, MarkId) -> f64 + 'static,
    ) -> Self {
        if let MarkEncodings::Text(e) = &mut self.mark.encodings {
            e.as_mut().font_size = Encoding::Compute {
                deps: deps4(deps),
                f: Box::new(f),
            };
        }
        self
    }

    /// Set the `path` encoding to a constant value (path marks only).
    pub fn path_const(mut self, v: BezPath) -> Self {
        if let MarkEncodings::Path(e) = &mut self.mark.encodings {
            e.as_mut().path = Encoding::Const(v);
        }
        self
    }

    /// Set the `path` encoding to a computed value (path marks only).
    pub fn path_compute(
        mut self,
        deps: impl IntoIterator<Item = InputRef>,
        f: impl Fn(&EvalCtx<'_>, MarkId) -> BezPath + 'static,
    ) -> Self {
        if let MarkEncodings::Path(e) = &mut self.mark.encodings {
            e.as_mut().path = Encoding::Compute {
                deps: deps4(deps),
                f: Box::new(f),
            };
        }
        self
    }

    /// Set the `stroke` encoding to a constant value (path marks only).
    pub fn stroke_const(mut self, v: Color) -> Self {
        if let MarkEncodings::Path(e) = &mut self.mark.encodings {
            e.as_mut().stroke = Encoding::Const(Brush::Solid(v));
        }
        self
    }

    /// Set the `stroke` encoding to a constant brush (path marks only).
    pub fn stroke_brush_const(mut self, v: impl Into<Brush>) -> Self {
        if let MarkEncodings::Path(e) = &mut self.mark.encodings {
            e.as_mut().stroke = Encoding::Const(v.into());
        }
        self
    }

    /// Set the `stroke_width` encoding to a constant value (path marks only).
    pub fn stroke_width_const(mut self, v: f64) -> Self {
        if let MarkEncodings::Path(e) = &mut self.mark.encodings {
            e.as_mut().stroke_width = Encoding::Const(v);
        }
        self
    }

    /// Finish building and rebuild dependencies.
    pub fn build(mut self) -> Mark {
        self.mark.rebuild_deps();
        self.mark
    }
}

/// Read-only access to scene inputs during encoding evaluation.
pub struct EvalCtx<'a> {
    /// Tables available during evaluation.
    pub tables: &'a HashMap<TableId, Table>,
    /// Signals available during evaluation.
    pub signals: &'a HashMap<SignalId, Box<dyn AnySignal>>,
}

impl fmt::Debug for EvalCtx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvalCtx")
            .field("tables_len", &self.tables.len())
            .field("signals_len", &self.signals.len())
            .finish()
    }
}

impl<'a> EvalCtx<'a> {
    /// Return the current version of a table, if present.
    pub fn table_version(&self, id: TableId) -> Option<Version> {
        self.tables.get(&id).map(|t| t.version)
    }
    /// Return the current version of a signal, if present.
    pub fn signal_version(&self, id: SignalId) -> Option<Version> {
        self.signals.get(&id).map(|s| s.version())
    }

    /// Downcast and clone a signal value, if present and of type `T`.
    pub fn signal<T: Clone + 'static>(&self, id: SignalId) -> Option<T> {
        let s = self.signals.get(&id)?;
        let typed = s.as_any().downcast_ref::<Signal<T>>()?;
        Some(typed.value.clone())
    }

    /// Read a numeric table value, if a table data accessor is present.
    pub fn table_f64(&self, table: TableId, row: usize, col: ColId) -> Option<f64> {
        let t = self.tables.get(&table)?;
        let data = t.data.as_deref()?;
        data.f64(row, col)
    }

    /// Return the current table row count.
    pub fn table_row_count(&self, table: TableId) -> Option<usize> {
        self.tables.get(&table).map(|t| t.row_keys.len())
    }
}

/// Mark-level diffs keyed by stable identity.
/// These are what Understory display/imaging layers consume.
///
/// Payloads are boxed to keep `MarkDiff` itself reasonably sized.
#[derive(Debug)]
pub enum MarkDiff {
    /// A mark is newly present.
    Enter {
        /// Stable identifier.
        id: MarkId,
        /// Z-ordering for rendering; higher values are drawn above lower values.
        z_index: i32,
        /// The mark kind.
        kind: MarkKind,
        /// Newly evaluated channels.
        new: Box<MarkPayload>,
        /// Optional bounds hint for downstream damage calculation.
        bounds: Option<Rect>,
    },
    /// A mark exists and some channels changed.
    Update {
        /// Stable identifier.
        id: MarkId,
        /// Previous z-index.
        old_z_index: i32,
        /// New z-index.
        new_z_index: i32,
        /// The mark kind.
        kind: MarkKind,
        /// Previously cached channels.
        old: Box<MarkPayload>,
        /// Newly evaluated channels.
        new: Box<MarkPayload>,
        /// Optional old bounds hint for downstream damage calculation.
        old_bounds: Option<Rect>,
        /// Optional new bounds hint for downstream damage calculation.
        new_bounds: Option<Rect>,
        /// Optional damage hint for downstream incremental rendering.
        ///
        /// When both old and new bounds are known, this is the union of the two. If either bound
        /// is unknown, this is `None` and downstream systems may choose a fallback strategy.
        damage: Option<Rect>,
    },
    /// A mark was removed.
    Exit {
        /// Stable identifier.
        id: MarkId,
        /// Z-ordering for rendering.
        z_index: i32,
        /// The mark kind.
        kind: MarkKind,
        /// Cached channels, if this mark was previously evaluated.
        old: Option<Box<MarkPayload>>,
        /// Optional bounds hint for downstream damage calculation.
        bounds: Option<Rect>,
    },
}

impl MarkDiff {
    /// Returns the best available damage rectangle for this diff.
    ///
    /// - `Enter`/`Exit`: returns the mark bounds hint, if present.
    /// - `Update`: returns the `damage` hint when present, otherwise `None`.
    pub fn damage(&self) -> Option<Rect> {
        match self {
            Self::Enter { bounds, .. } => *bounds,
            Self::Update { damage, .. } => *damage,
            Self::Exit { bounds, .. } => *bounds,
        }
    }
}

/// A mutable collection of tables, signals, and marks, with incremental evaluation.
pub struct Scene {
    /// Tables keyed by [`TableId`].
    pub tables: HashMap<TableId, Table>,
    /// Signals keyed by [`SignalId`].
    pub signals: HashMap<SignalId, Box<dyn AnySignal>>,
    /// Marks keyed by [`MarkId`].
    pub marks: HashMap<MarkId, Mark>,
    pending_removals: Vec<MarkId>,
}

impl fmt::Debug for Scene {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scene")
            .field("tables_len", &self.tables.len())
            .field("signals_len", &self.signals.len())
            .field("marks_len", &self.marks.len())
            .field("pending_removals_len", &self.pending_removals.len())
            .finish()
    }
}

impl Scene {
    /// Create an empty scene.
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
            signals: HashMap::new(),
            marks: HashMap::new(),
            pending_removals: Vec::new(),
        }
    }

    /// Queue a mark removal so it yields an `Exit` on the next `update()`.
    pub fn remove_mark(&mut self, id: MarkId) {
        if self.marks.contains_key(&id) {
            self.pending_removals.push(id);
        }
    }

    /// Insert or replace a signal with an initial version of `1`.
    pub fn insert_signal<T: Clone + 'static>(&mut self, id: SignalId, value: T) {
        self.signals.insert(
            id,
            Box::new(Signal {
                id,
                version: 1,
                value,
            }),
        );
    }

    /// Insert or replace a table with version `1`.
    pub fn insert_table(&mut self, table: Table) {
        self.tables.insert(table.id, table);
    }

    /// Replace a table's row keys and bump its version (inserting if missing).
    pub fn set_table_row_keys(&mut self, id: TableId, row_keys: Vec<u64>) {
        match self.tables.entry(id) {
            Entry::Occupied(mut e) => {
                e.get_mut().set_row_keys(row_keys);
            }
            Entry::Vacant(e) => {
                let mut table = Table::new(id);
                table.row_keys = row_keys;
                e.insert(table);
            }
        }
    }

    /// Set a table's data accessor and bump its version (inserting it if missing).
    pub fn set_table_data(&mut self, id: TableId, data: Option<Box<dyn TableData>>) {
        match self.tables.entry(id) {
            Entry::Occupied(mut e) => {
                e.get_mut().set_data(data);
            }
            Entry::Vacant(e) => {
                let mut table = Table::new(id);
                table.data = data;
                e.insert(table);
            }
        }
    }

    /// Set a signal value and bump its version (inserting it if missing).
    ///
    /// Returns `Err(TypeMismatch)` if a signal exists at `id` with a different type.
    pub fn set_signal<T: Clone + 'static>(
        &mut self,
        id: SignalId,
        value: T,
    ) -> Result<(), SignalAccessError> {
        let Some(signal) = self.signals.get_mut(&id) else {
            self.insert_signal(id, value);
            return Ok(());
        };

        let typed = signal
            .as_any_mut()
            .downcast_mut::<Signal<T>>()
            .ok_or(SignalAccessError::TypeMismatch)?;

        typed.bump();
        typed.value = value;
        Ok(())
    }

    /// Get an immutable reference to a signal value, if present and of type `T`.
    pub fn signal_ref<T: Clone + 'static>(
        &self,
        id: SignalId,
    ) -> Result<Option<&T>, SignalAccessError> {
        let Some(signal) = self.signals.get(&id) else {
            return Ok(None);
        };
        let typed = signal
            .as_any()
            .downcast_ref::<Signal<T>>()
            .ok_or(SignalAccessError::TypeMismatch)?;
        Ok(Some(&typed.value))
    }

    /// Get a mutable reference to a signal value, if present and of type `T`.
    pub fn signal_mut<T: Clone + 'static>(
        &mut self,
        id: SignalId,
    ) -> Result<Option<&mut T>, SignalAccessError> {
        let Some(signal) = self.signals.get_mut(&id) else {
            return Ok(None);
        };
        let typed = signal
            .as_any_mut()
            .downcast_mut::<Signal<T>>()
            .ok_or(SignalAccessError::TypeMismatch)?;
        Ok(Some(&mut typed.value))
    }

    /// Insert or replace a mark.
    pub fn upsert_mark(&mut self, mark: Mark) {
        self.marks.insert(mark.id, mark);
    }

    /// Replace the scene's mark set (structural reconciliation).
    ///
    /// The provided marks are treated as the complete "current frame" mark set:
    /// - Any existing mark ids not present in `marks` are removed and returned as `Exit` diffs.
    /// - Any mark whose [`MarkKind`] changes will also emit an `Exit` diff, and will re-enter on
    ///   the next evaluation.
    /// - Marks with ids that already exist will retain their cached evaluation state.
    ///
    /// This method does not evaluate marks; call [`Scene::update`] (or [`Scene::tick`]) to
    /// compute `Enter`/`Update` diffs.
    pub fn set_marks<I>(&mut self, marks: I) -> Vec<MarkDiff>
    where
        I: IntoIterator<Item = Mark>,
    {
        self.pending_removals.clear();

        let mut old_marks = core::mem::take(&mut self.marks);
        let mut exits = Vec::new();

        for mut mark in marks {
            mark.rebuild_deps();

            if let Some(old) = old_marks.remove(&mark.id) {
                if mark.kind != old.kind {
                    let bounds = old.cache.as_ref().and_then(MarkPayload::bounds);
                    exits.push(MarkDiff::Exit {
                        id: mark.id,
                        z_index: old.z_index,
                        kind: old.kind,
                        old: old.cache.map(Box::new),
                        bounds,
                    });
                } else {
                    mark.force_eval |= mark.deps != old.deps;
                    mark.cache = old.cache;
                    mark.cached_z_index = old.cached_z_index;
                    mark.last_seen = old.last_seen;
                }
            }

            self.marks.insert(mark.id, mark);
        }

        for (id, old) in old_marks {
            let bounds = old.cache.as_ref().and_then(MarkPayload::bounds);
            exits.push(MarkDiff::Exit {
                id,
                z_index: old.z_index,
                kind: old.kind,
                old: old.cache.map(Box::new),
                bounds,
            });
        }

        exits
    }

    /// Invalidate a mark so the next [`Scene::update`] recomputes all channels.
    pub fn invalidate_mark(&mut self, id: MarkId) -> bool {
        let Some(mark) = self.marks.get_mut(&id) else {
            return false;
        };
        mark.force_eval = true;
        true
    }

    /// Perform a full tick: reconcile marks and evaluate diffs.
    ///
    /// This is the most convenient API for “frame-based” callers: provide the full mark set for
    /// the current frame, and get back `Enter/Update/Exit` diffs.
    pub fn tick<I>(&mut self, marks: I) -> Vec<MarkDiff>
    where
        I: IntoIterator<Item = Mark>,
    {
        let mut diffs = self.set_marks(marks);
        diffs.extend(self.update());
        diffs
    }

    /// Build and tick a complete mark set derived from a table's row keys.
    ///
    /// This is a v1 convenience for "one mark per row" patterns. It derives stable
    /// `MarkId`s via [`MarkId::for_row`], reconciles the full set, and evaluates.
    ///
    /// If you need multiple marks per row or nested mark hierarchies, build marks
    /// explicitly and use [`Scene::tick`].
    pub fn tick_table_rows<F>(&mut self, table: TableId, mut build: F) -> Vec<MarkDiff>
    where
        F: FnMut(MarkId, u64, usize) -> Mark,
    {
        let Some(keys) = self.tables.get(&table).map(|t| t.row_keys.clone()) else {
            return self.tick(core::iter::empty());
        };

        self.tick(
            keys.into_iter()
                .enumerate()
                .map(|(i, k)| build(MarkId::for_row(table, k), k, i)),
        )
    }

    /// Evaluate incremental updates and produce mark diffs.
    ///
    /// Use this when the mark set is stable and only input versions changed (tables/signals).
    pub fn update(&mut self) -> Vec<MarkDiff> {
        let ctx = EvalCtx {
            tables: &self.tables,
            signals: &self.signals,
        };
        let mut diffs = Vec::new();

        for id in self.pending_removals.drain(..) {
            let removed = self.marks.remove(&id);
            let old = removed.as_ref().and_then(|m| m.cache.clone());
            let bounds = old.as_ref().and_then(MarkPayload::bounds);
            diffs.push(MarkDiff::Exit {
                id,
                z_index: removed.as_ref().map_or(0, |m| m.z_index),
                kind: removed.as_ref().map_or(MarkKind::Rect, |m| m.kind),
                old: old.map(Box::new),
                bounds,
            });
        }

        for mark in self.marks.values_mut() {
            let mut changed_inputs = SmallVec::<[InputRef; 8]>::new();

            for dep in mark.deps.iter().copied() {
                let v = match dep {
                    InputRef::Table { table } => ctx.table_version(table),
                    InputRef::TableCol { table, .. } => ctx.table_version(table),
                    InputRef::Signal { signal } => ctx.signal_version(signal),
                };
                let Some(v) = v else { continue };
                let prev = mark.last_seen.insert(dep, v).unwrap_or(0);
                if mark.cache.is_some() && v != prev {
                    changed_inputs.push(dep);
                }
            }

            if mark.cache.is_none() {
                let new = eval_payload(&mark.encodings, &ctx, mark.id);
                diffs.push(MarkDiff::Enter {
                    id: mark.id,
                    z_index: mark.z_index,
                    kind: mark.kind,
                    new: Box::new(new.clone()),
                    bounds: new.bounds(),
                });
                mark.cache = Some(new);
                mark.cached_z_index = mark.z_index;
                mark.force_eval = false;
                continue;
            }

            if mark.force_eval {
                let old_z_index = mark.cached_z_index;
                let new_z_index = mark.z_index;
                let old = mark.cache.as_ref().expect("checked above").clone();
                let new = eval_payload(&mark.encodings, &ctx, mark.id);

                if old != new || old_z_index != new_z_index {
                    let old_bounds = old.bounds();
                    let new_bounds = new.bounds();
                    let damage = union_bounds(old_bounds, new_bounds);
                    diffs.push(MarkDiff::Update {
                        id: mark.id,
                        old_z_index,
                        new_z_index,
                        kind: mark.kind,
                        old: Box::new(old.clone()),
                        new: Box::new(new.clone()),
                        old_bounds,
                        new_bounds,
                        damage,
                    });
                }

                mark.cache = Some(new);
                mark.cached_z_index = mark.z_index;
                mark.force_eval = false;
                continue;
            }

            if changed_inputs.is_empty() {
                if mark.cached_z_index != mark.z_index {
                    let old = mark.cache.as_ref().expect("checked above").clone();
                    let bounds = old.bounds();
                    diffs.push(MarkDiff::Update {
                        id: mark.id,
                        old_z_index: mark.cached_z_index,
                        new_z_index: mark.z_index,
                        kind: mark.kind,
                        old: Box::new(old.clone()),
                        new: Box::new(old.clone()),
                        old_bounds: bounds,
                        new_bounds: bounds,
                        damage: bounds,
                    });
                    mark.cached_z_index = mark.z_index;
                }
                continue;
            }

            let old_z_index = mark.cached_z_index;
            let new_z_index = mark.z_index;
            let old = mark.cache.as_ref().expect("checked above").clone();
            let mut new = old.clone();
            update_payload_incremental(&mark.encodings, &ctx, mark.id, &changed_inputs, &mut new);

            if old != new || old_z_index != new_z_index {
                let old_bounds = old.bounds();
                let new_bounds = new.bounds();
                let damage = union_bounds(old_bounds, new_bounds);
                diffs.push(MarkDiff::Update {
                    id: mark.id,
                    old_z_index,
                    new_z_index,
                    kind: mark.kind,
                    old: Box::new(old.clone()),
                    new: Box::new(new.clone()),
                    old_bounds,
                    new_bounds,
                    damage,
                });
            }
            mark.cache = Some(new);
            mark.cached_z_index = mark.z_index;
        }

        diffs
    }
}

fn union_bounds(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    let a = a?;
    let b = b?;
    Some(Rect::new(
        a.x0.min(b.x0),
        a.y0.min(b.y0),
        a.x1.max(b.x1),
        a.y1.max(b.y1),
    ))
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

fn encoding_needs_update<T>(enc: &Encoding<T>, changed_inputs: &SmallVec<[InputRef; 8]>) -> bool {
    match enc {
        Encoding::Const(_) => false,
        Encoding::Compute { deps, .. } => deps.iter().any(|d| changed_inputs.contains(d)),
    }
}

fn eval_value<T: Clone>(enc: &Encoding<T>, ctx: &EvalCtx<'_>, id: MarkId) -> T {
    match enc {
        Encoding::Const(v) => v.clone(),
        Encoding::Compute { f, .. } => (f)(ctx, id),
    }
}

fn eval_payload(encodings: &MarkEncodings, ctx: &EvalCtx<'_>, id: MarkId) -> MarkPayload {
    match encodings {
        MarkEncodings::Rect(e) => {
            let e = e.as_ref();
            let x = eval_value(&e.x, ctx, id);
            let y = eval_value(&e.y, ctx, id);
            let w = eval_value(&e.w, ctx, id);
            let h = eval_value(&e.h, ctx, id);
            let fill = eval_value(&e.fill, ctx, id);

            let x0 = x.min(x + w);
            let x1 = x.max(x + w);
            let y0 = y.min(y + h);
            let y1 = y.max(y + h);

            MarkPayload::Rect(RectChannels {
                rect: Rect::new(x0, y0, x1, y1),
                fill,
            })
        }
        MarkEncodings::Text(e) => {
            let e = e.as_ref();
            MarkPayload::Text(TextChannels {
                pos: Point::new(eval_value(&e.x, ctx, id), eval_value(&e.y, ctx, id)),
                text: eval_value(&e.text, ctx, id),
                font_size: eval_value(&e.font_size, ctx, id),
                angle: eval_value(&e.angle, ctx, id),
                anchor: eval_value(&e.anchor, ctx, id),
                baseline: eval_value(&e.baseline, ctx, id),
                fill: eval_value(&e.fill, ctx, id),
            })
        }
        MarkEncodings::Path(e) => {
            let e = e.as_ref();
            MarkPayload::Path(PathChannels {
                path: eval_value(&e.path, ctx, id),
                fill: eval_value(&e.fill, ctx, id),
                stroke: eval_value(&e.stroke, ctx, id),
                stroke_width: eval_value(&e.stroke_width, ctx, id),
            })
        }
    }
}

fn update_payload_incremental(
    encodings: &MarkEncodings,
    ctx: &EvalCtx<'_>,
    id: MarkId,
    changed_inputs: &SmallVec<[InputRef; 8]>,
    payload: &mut MarkPayload,
) {
    match encodings {
        MarkEncodings::Rect(e) => {
            let e = e.as_ref();
            let MarkPayload::Rect(p) = payload else {
                *payload = eval_payload(encodings, ctx, id);
                return;
            };
            let mut x = p.rect.x0;
            let mut y = p.rect.y0;
            let mut w = p.rect.width();
            let mut h = p.rect.height();

            let mut recompute_rect = false;
            if encoding_needs_update(&e.x, changed_inputs) {
                x = eval_value(&e.x, ctx, id);
                recompute_rect = true;
            }
            if encoding_needs_update(&e.y, changed_inputs) {
                y = eval_value(&e.y, ctx, id);
                recompute_rect = true;
            }
            if encoding_needs_update(&e.w, changed_inputs) {
                w = eval_value(&e.w, ctx, id);
                recompute_rect = true;
            }
            if encoding_needs_update(&e.h, changed_inputs) {
                h = eval_value(&e.h, ctx, id);
                recompute_rect = true;
            }
            if encoding_needs_update(&e.fill, changed_inputs) {
                p.fill = eval_value(&e.fill, ctx, id);
            }
            if recompute_rect {
                let x0 = x.min(x + w);
                let x1 = x.max(x + w);
                let y0 = y.min(y + h);
                let y1 = y.max(y + h);
                p.rect = Rect::new(x0, y0, x1, y1);
            }
        }
        MarkEncodings::Text(e) => {
            let e = e.as_ref();
            let MarkPayload::Text(p) = payload else {
                *payload = eval_payload(encodings, ctx, id);
                return;
            };
            if encoding_needs_update(&e.x, changed_inputs) {
                p.pos.x = eval_value(&e.x, ctx, id);
            }
            if encoding_needs_update(&e.y, changed_inputs) {
                p.pos.y = eval_value(&e.y, ctx, id);
            }
            if encoding_needs_update(&e.text, changed_inputs) {
                p.text = eval_value(&e.text, ctx, id);
            }
            if encoding_needs_update(&e.font_size, changed_inputs) {
                p.font_size = eval_value(&e.font_size, ctx, id);
            }
            if encoding_needs_update(&e.angle, changed_inputs) {
                p.angle = eval_value(&e.angle, ctx, id);
            }
            if encoding_needs_update(&e.anchor, changed_inputs) {
                p.anchor = eval_value(&e.anchor, ctx, id);
            }
            if encoding_needs_update(&e.baseline, changed_inputs) {
                p.baseline = eval_value(&e.baseline, ctx, id);
            }
            if encoding_needs_update(&e.fill, changed_inputs) {
                p.fill = eval_value(&e.fill, ctx, id);
            }
        }
        MarkEncodings::Path(e) => {
            let e = e.as_ref();
            let MarkPayload::Path(p) = payload else {
                *payload = eval_payload(encodings, ctx, id);
                return;
            };
            if encoding_needs_update(&e.path, changed_inputs) {
                p.path = eval_value(&e.path, ctx, id);
            }
            if encoding_needs_update(&e.fill, changed_inputs) {
                p.fill = eval_value(&e.fill, ctx, id);
            }
            if encoding_needs_update(&e.stroke, changed_inputs) {
                p.stroke = eval_value(&e.stroke, ctx, id);
            }
            if encoding_needs_update(&e.stroke_width, changed_inputs) {
                p.stroke_width = eval_value(&e.stroke_width, ctx, id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn enter_update_exit_smoke() {
        let mut scene = Scene::new();
        let zoom_id = SignalId(1);
        scene.insert_signal(zoom_id, 1.0_f32);

        let mark_id = MarkId(7);
        let mark = Mark::builder(mark_id)
            .x_compute([InputRef::Signal { signal: zoom_id }], move |ctx, _| {
                f64::from(ctx.signal::<f32>(zoom_id).unwrap_or(0.0))
            })
            .build();
        let diffs = scene.tick([mark]);

        assert!(matches!(
            &diffs[..],
            [MarkDiff::Enter { id, .. }] if *id == mark_id
        ));

        scene.set_signal(zoom_id, 2.0_f32).unwrap();
        let diffs = scene.update();
        let [MarkDiff::Update { id, old, new, .. }] = &diffs[..] else {
            panic!("expected a single update diff");
        };
        assert_eq!(*id, mark_id);
        let (MarkPayload::Rect(old), MarkPayload::Rect(new)) = (&**old, &**new) else {
            panic!("expected rect payloads");
        };
        assert_ne!(old.rect.x0, new.rect.x0);

        let diffs = scene.tick(core::iter::empty());
        assert!(matches!(
            &diffs[..],
            [MarkDiff::Exit { id, .. }] if *id == mark_id
        ));
    }

    #[test]
    fn only_recomputes_touched_encodings() {
        let mut scene = Scene::new();
        let a = SignalId(1);
        let b = SignalId(2);
        scene.insert_signal(a, 1.0_f32);
        scene.insert_signal(b, 10.0_f32);

        let mark_id = MarkId(1);
        let mark = Mark::builder(mark_id)
            .x_compute([InputRef::Signal { signal: a }], move |ctx, _| {
                f64::from(ctx.signal::<f32>(a).unwrap_or(0.0))
            })
            .y_compute([InputRef::Signal { signal: b }], move |ctx, _| {
                f64::from(ctx.signal::<f32>(b).unwrap_or(0.0))
            })
            .build();
        let _ = scene.tick([mark]);

        // Change `a` only; `y` should remain the same.
        scene.set_signal(a, 2.0_f32).unwrap();
        let diffs = scene.update();

        let [MarkDiff::Update { old, new, .. }] = &diffs[..] else {
            panic!("expected a single update diff");
        };
        let (MarkPayload::Rect(old), MarkPayload::Rect(new)) = (&**old, &**new) else {
            panic!("expected rect payloads");
        };
        assert_ne!(old.rect.x0, new.rect.x0);
        assert_eq!(old.rect.y0, new.rect.y0);
    }

    #[test]
    fn table_row_reconciliation_exit_enter() {
        let mut scene = Scene::new();
        let table_id = TableId(1);
        scene.set_table_row_keys(table_id, Vec::from([10_u64, 11_u64]));

        let diffs = scene.tick_table_rows(table_id, |id, row_key, _row| {
            Mark::builder(id).x_const(row_key as f64).build()
        });
        assert!(diffs.iter().any(|d| matches!(d, MarkDiff::Enter { .. })));

        // Remove one row key and add a new one: expect one exit and one enter.
        scene.set_table_row_keys(table_id, Vec::from([11_u64, 12_u64]));
        let diffs = scene.tick_table_rows(table_id, |id, row_key, _row| {
            Mark::builder(id).x_const(row_key as f64).build()
        });

        let enters = diffs
            .iter()
            .filter(|d| matches!(d, MarkDiff::Enter { .. }))
            .count();
        let exits = diffs
            .iter()
            .filter(|d| matches!(d, MarkDiff::Exit { .. }))
            .count();
        assert_eq!(enters, 1);
        assert_eq!(exits, 1);
    }

    #[test]
    fn text_updates_and_bounds_unknown() {
        let mut scene = Scene::new();
        let sig = SignalId(1);
        scene.insert_signal(sig, 1_u32);

        let mark_id = MarkId(1);
        let mark = Mark::builder(mark_id)
            .text()
            .x_const(0.0)
            .y_const(0.0)
            .text_compute([InputRef::Signal { signal: sig }], move |ctx, _| {
                let v = ctx.signal::<u32>(sig).unwrap_or(0);
                alloc::format!("v={v}")
            })
            .build();

        let diffs = scene.tick([mark]);
        let [MarkDiff::Enter { kind, bounds, .. }] = &diffs[..] else {
            panic!("expected enter");
        };
        assert_eq!(*kind, MarkKind::Text);
        assert_eq!(*bounds, None);

        scene.set_signal(sig, 2_u32).unwrap();
        let diffs = scene.update();
        let [MarkDiff::Update { old, new, .. }] = &diffs[..] else {
            panic!("expected update");
        };
        let (MarkPayload::Text(old), MarkPayload::Text(new)) = (&**old, &**new) else {
            panic!("expected text payloads");
        };
        assert_ne!(old.text, new.text);
    }

    #[test]
    fn path_enter_has_bounds() {
        let mut scene = Scene::new();
        let mark_id = MarkId(1);

        let mut triangle = BezPath::new();
        triangle.move_to((0.0, 0.0));
        triangle.line_to((10.0, 0.0));
        triangle.line_to((5.0, 10.0));
        triangle.close_path();

        let mark = Mark::builder(mark_id).path().path_const(triangle).build();
        let diffs = scene.tick([mark]);

        let [MarkDiff::Enter { kind, bounds, .. }] = &diffs[..] else {
            panic!("expected enter");
        };
        assert_eq!(*kind, MarkKind::Path);
        assert!(bounds.is_some());
    }

    #[test]
    fn set_signal_type_mismatch_does_not_bump_version() {
        let mut scene = Scene::new();
        let id = SignalId(1);

        scene.insert_signal(id, 1.0_f32);

        let before_version = scene.signals.get(&id).unwrap().version();

        // Wrong type: should error and should NOT bump version.
        let err = scene.set_signal::<u32>(id, 123_u32).unwrap_err();
        assert_eq!(err, SignalAccessError::TypeMismatch);

        let after_version = scene.signals.get(&id).unwrap().version();
        assert_eq!(before_version, after_version);

        // Also assert that the original value is unchanged.
        let sig_any = scene.signals.get(&id).unwrap();
        let typed = sig_any.as_any().downcast_ref::<Signal<f32>>().unwrap();
        assert_eq!(typed.value, 1.0_f32);
    }

    #[test]
    fn remove_mark_is_idempotent() {
        let mut scene = Scene::new();
        let id = MarkId(1);

        // Add a mark and evaluate it so it has cache.
        let _ = scene.tick([Mark::new(id)]);

        scene.remove_mark(id);

        // Removal should trigger an `Exit` diff.
        let diffs = scene.update();
        assert!(matches!(&diffs[..], [MarkDiff::Exit { id: got, .. }] if *got == id));

        // Removing again should now be a noop.
        scene.remove_mark(id);
        let diffs = scene.update();
        assert!(diffs.is_empty());
    }
}
