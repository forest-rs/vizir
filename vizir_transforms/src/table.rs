// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Owned table representation used by the transform executor.

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;

use vizir_core::{ColId, Table, TableData, TableId};

/// Errors returned when building or using a [`TableFrame`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableFrameError {
    /// The requested column list is empty.
    EmptyColumns,
    /// The input table does not have a data accessor.
    MissingData,
}

/// An owned numeric table used as input/output of transform execution.
///
/// This is a deliberately small representation:
/// - stable `row_keys` (for downstream mark identity),
/// - a fixed set of numeric columns (`f64`).
#[derive(Debug, Clone)]
pub struct TableFrame {
    /// Stable keys for each row.
    pub row_keys: Vec<u64>,
    /// Column ids carried by this frame.
    pub columns: Vec<ColId>,
    /// Columnar numeric data, aligned to `columns`.
    pub data: Vec<Vec<f64>>,
}

impl TableFrame {
    /// Create an empty frame.
    pub fn new(columns: Vec<ColId>) -> Result<Self, TableFrameError> {
        if columns.is_empty() {
            return Err(TableFrameError::EmptyColumns);
        }
        Ok(Self {
            row_keys: Vec::new(),
            columns,
            data: Vec::new(),
        })
    }

    /// Extract a numeric frame from a `vizir_core` table.
    ///
    /// Missing values are represented as `NaN` in the output columns.
    pub fn from_table(table: &Table, columns: Vec<ColId>) -> Result<Self, TableFrameError> {
        if columns.is_empty() {
            return Err(TableFrameError::EmptyColumns);
        }
        let Some(data) = table.data.as_deref() else {
            return Err(TableFrameError::MissingData);
        };
        let n = table.row_keys.len();
        let mut cols = Vec::with_capacity(columns.len());
        for &col in &columns {
            let mut out = Vec::with_capacity(n);
            for row in 0..n {
                out.push(data.f64(row, col).unwrap_or(f64::NAN));
            }
            cols.push(out);
        }
        Ok(Self {
            row_keys: table.row_keys.clone(),
            columns,
            data: cols,
        })
    }

    /// Returns the number of rows.
    pub fn row_count(&self) -> usize {
        self.row_keys.len()
    }

    /// Returns a column index for a `ColId`, if present.
    pub fn column_index(&self, col: ColId) -> Option<usize> {
        self.columns.iter().position(|&c| c == col)
    }

    /// Gets a numeric value for a row/col if both exist.
    pub fn f64(&self, row: usize, col: ColId) -> Option<f64> {
        let ci = self.column_index(col)?;
        self.data.get(ci)?.get(row).copied()
    }

    /// Converts this frame into a `vizir_core::Table` with an owned `TableData` accessor.
    pub fn into_table(self, id: TableId) -> Table {
        Table {
            id,
            version: 1,
            row_keys: self.row_keys,
            data: Some(Box::new(FrameData {
                columns: self.columns,
                data: self.data,
            })),
        }
    }
}

#[derive(Debug)]
struct FrameData {
    columns: Vec<ColId>,
    data: Vec<Vec<f64>>,
}

impl TableData for FrameData {
    fn row_count(&self) -> usize {
        self.data.first().map_or(0, |c| c.len())
    }

    fn f64(&self, row: usize, col: ColId) -> Option<f64> {
        let idx = self.columns.iter().position(|&c| c == col)?;
        self.data.get(idx)?.get(row).copied()
    }
}
