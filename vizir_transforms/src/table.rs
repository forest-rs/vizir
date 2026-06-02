// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Owned table representation used by the transform executor.

use alloc::boxed::Box;
use alloc::vec::Vec;

use vizir_core::{
    ColumnId, ColumnSchema, ColumnType, F64ColumnRef, Table, TableData, TableId, TableSchema,
};

/// Errors returned when building or using a [`TableFrame`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableFrameError {
    /// The requested column list is empty.
    EmptyColumns,
    /// The input table does not have a data accessor.
    MissingData,
}

impl core::fmt::Display for TableFrameError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyColumns => write!(f, "table frame extraction requires at least one column"),
            Self::MissingData => write!(f, "table has no data accessor"),
        }
    }
}

impl core::error::Error for TableFrameError {}

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
    pub columns: Vec<ColumnId>,
    /// Columnar numeric data, aligned to `columns`.
    pub data: Vec<Vec<f64>>,
}

impl TableFrame {
    /// Create an empty frame.
    pub fn new(columns: Vec<ColumnId>) -> Result<Self, TableFrameError> {
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
    pub fn from_table(table: &Table, columns: Vec<ColumnId>) -> Result<Self, TableFrameError> {
        if columns.is_empty() {
            return Err(TableFrameError::EmptyColumns);
        }
        let Some(data) = table.data.as_deref() else {
            return Err(TableFrameError::MissingData);
        };
        let n = table.row_keys.len();
        let mut cols = Vec::with_capacity(columns.len());
        for &col in &columns {
            if let Some(values) = data.f64_column(col) {
                let values = values.as_slice();
                let len = values.len().min(n);
                let mut out = Vec::with_capacity(n);
                out.extend_from_slice(&values[..len]);
                out.resize(n, f64::NAN);
                cols.push(out);
            } else {
                let mut out = Vec::with_capacity(n);
                for row in 0..n {
                    out.push(data.get_f64(row, col).unwrap_or(f64::NAN));
                }
                cols.push(out);
            }
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

    /// Returns a column index for a `ColumnId`, if present.
    pub fn column_index(&self, col: ColumnId) -> Option<usize> {
        self.columns.iter().position(|&c| c == col)
    }

    /// Gets a numeric value for a row/col if both exist.
    pub fn get_f64(&self, row: usize, col: ColumnId) -> Option<f64> {
        let ci = self.column_index(col)?;
        self.data.get(ci)?.get(row).copied()
    }

    /// Converts this frame into a `vizir_core::Table` with an owned `TableData` accessor.
    pub fn into_table(self, id: TableId) -> Table {
        let mut table = Table::new(id);
        table.row_keys = self.row_keys;
        table.schema = numeric_schema(self.columns.iter().copied());
        table.data = Some(Box::new(FrameData {
            columns: self.columns,
            data: self.data,
        }));
        table
    }
}

fn numeric_schema(columns: impl IntoIterator<Item = ColumnId>) -> TableSchema {
    TableSchema::from_columns(
        columns
            .into_iter()
            .map(|id| ColumnSchema::new(id, ColumnType::F64)),
    )
}

#[derive(Debug)]
struct FrameData {
    columns: Vec<ColumnId>,
    data: Vec<Vec<f64>>,
}

impl TableData for FrameData {
    fn row_count(&self) -> usize {
        self.data.first().map_or(0, |c| c.len())
    }

    fn column_type(&self, col: ColumnId) -> Option<ColumnType> {
        self.columns.contains(&col).then_some(ColumnType::F64)
    }

    fn get_f64(&self, row: usize, col: ColumnId) -> Option<f64> {
        let idx = self.columns.iter().position(|&c| c == col)?;
        self.data.get(idx)?.get(row).copied()
    }

    fn f64_column(&self, col: ColumnId) -> Option<F64ColumnRef<'_>> {
        let idx = self.columns.iter().position(|&c| c == col)?;
        Some(F64ColumnRef::Slice(self.data.get(idx)?))
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[derive(Debug)]
    struct ViewOnlyData {
        values: Vec<f64>,
    }

    impl TableData for ViewOnlyData {
        fn row_count(&self) -> usize {
            self.values.len()
        }

        fn column_type(&self, col: ColumnId) -> Option<ColumnType> {
            (col == ColumnId(0)).then_some(ColumnType::F64)
        }

        fn f64_column(&self, col: ColumnId) -> Option<F64ColumnRef<'_>> {
            (col == ColumnId(0)).then_some(F64ColumnRef::Slice(&self.values))
        }
    }

    #[test]
    fn from_table_uses_bulk_f64_column_view() {
        let mut table = Table::new(TableId(1));
        table.row_keys = vec![10, 11, 12];
        table.data = Some(Box::new(ViewOnlyData {
            values: vec![1.0, 2.0],
        }));

        let frame = TableFrame::from_table(&table, vec![ColumnId(0)]).unwrap();

        assert_eq!(frame.row_keys, vec![10, 11, 12]);
        assert_eq!(frame.columns, vec![ColumnId(0)]);
        assert_eq!(frame.data.len(), 1);
        assert_eq!(frame.data[0][..2], [1.0, 2.0]);
        assert!(frame.data[0][2].is_nan());
    }

    #[test]
    fn into_table_exposes_bulk_f64_column_view() {
        let frame = TableFrame {
            row_keys: vec![10, 11],
            columns: vec![ColumnId(0)],
            data: vec![vec![1.0, 2.0]],
        };
        let table = frame.into_table(TableId(1));

        assert_eq!(
            table.f64_column(ColumnId(0)).map(F64ColumnRef::as_slice),
            Some(&[1.0, 2.0][..])
        );
    }
}
