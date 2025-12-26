// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Vega-ish table transforms.
//!
//! This crate provides:
//! - a small transform IR that models `TableId -> TableId` operators, and
//! - a full-recompute executor suitable as a first “transform foundations” landing.
//!
//! The executor is intentionally simple:
//! - it preserves upstream `row_keys` as stable identity for per-row marks, and
//! - it only supports numeric (`f64`) columns for now.

#![no_std]

extern crate alloc;

mod program;
mod scene;
mod table;
mod transform;

pub use program::{ExecutionError, Program, ProgramOutput};
pub use scene::SceneExecutionError;
pub use table::{TableFrame, TableFrameError};
pub use transform::{
    AggregateField, AggregateOp, CompareOp, Predicate, SortOrder, StackOffset, Transform,
};
