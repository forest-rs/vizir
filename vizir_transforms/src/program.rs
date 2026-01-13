// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Transform program and executor.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use hashbrown::HashMap;
use vizir_core::{ColId, TableId};

use crate::table::TableFrame;
use crate::transform::{AggregateOp, SortOrder, StackOffset, Transform};

/// Errors returned when executing a transform [`Program`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionError {
    /// The referenced input table does not exist.
    MissingInput(TableId),
    /// A transform requires a column that is not present in the input frame.
    MissingColumn {
        /// The table that was missing a required column.
        table: TableId,
        /// The missing column id.
        col: ColId,
    },
    /// A transform is invalid (e.g. empty column set).
    InvalidTransform,
    /// A transform variant is present in the IR but not implemented by this executor.
    Unimplemented(&'static str),
}

/// Outputs of executing a program.
#[derive(Debug, Default)]
pub struct ProgramOutput {
    /// Output tables produced by transforms, keyed by their `TableId`.
    pub tables: HashMap<TableId, TableFrame>,
}

/// A sequence of table transforms.
#[derive(Debug, Default, Clone)]
pub struct Program {
    transforms: Vec<Transform>,
}

impl Program {
    /// Creates an empty program.
    pub fn new() -> Self {
        Self {
            transforms: Vec::new(),
        }
    }

    /// Adds a transform to the end of the program.
    pub fn push(&mut self, t: Transform) {
        self.transforms.push(t);
    }

    /// Returns the transform list.
    pub fn transforms(&self) -> &[Transform] {
        &self.transforms
    }

    /// Execute the program using the provided input tables.
    ///
    /// The program is executed in order, and outputs can be referenced by later transforms.
    pub fn execute(
        &self,
        inputs: &HashMap<TableId, TableFrame>,
    ) -> Result<ProgramOutput, ExecutionError> {
        let mut out = ProgramOutput::default();

        for t in &self.transforms {
            match t {
                Transform::Filter {
                    input,
                    output,
                    predicate,
                    columns,
                } => {
                    let frame = get_frame(*input, inputs, &out.tables)?;
                    if columns.is_empty() {
                        return Err(ExecutionError::InvalidTransform);
                    }
                    require_columns(*input, frame, columns)?;
                    require_columns(*input, frame, core::slice::from_ref(&predicate.col))?;

                    let mut row_indices = Vec::new();
                    for row in 0..frame.row_count() {
                        let v = frame.f64(row, predicate.col).unwrap_or(f64::NAN);
                        if predicate.eval(v) {
                            row_indices.push(row);
                        }
                    }

                    let mut new_row_keys = Vec::with_capacity(row_indices.len());
                    for &r in &row_indices {
                        new_row_keys.push(frame.row_keys[r]);
                    }

                    let mut new_data = Vec::with_capacity(columns.len());
                    for &col in columns {
                        let ci = frame.column_index(col).expect("validated");
                        let src = &frame.data[ci];
                        let mut dst = Vec::with_capacity(row_indices.len());
                        for &r in &row_indices {
                            dst.push(src[r]);
                        }
                        new_data.push(dst);
                    }

                    out.tables.insert(
                        *output,
                        TableFrame {
                            row_keys: new_row_keys,
                            columns: columns.clone(),
                            data: new_data,
                        },
                    );
                }
                Transform::Project {
                    input,
                    output,
                    columns,
                } => {
                    let frame = get_frame(*input, inputs, &out.tables)?;
                    if columns.is_empty() {
                        return Err(ExecutionError::InvalidTransform);
                    }
                    require_columns(*input, frame, columns)?;

                    let n = frame.row_count();
                    let mut new_data = Vec::with_capacity(columns.len());
                    for &col in columns {
                        let ci = frame.column_index(col).expect("validated");
                        new_data.push(frame.data[ci].clone());
                    }

                    out.tables.insert(
                        *output,
                        TableFrame {
                            row_keys: frame.row_keys.clone(),
                            columns: columns.clone(),
                            data: new_data,
                        },
                    );
                    debug_assert_eq!(
                        out.tables[output].row_count(),
                        n,
                        "project should preserve row count"
                    );
                }
                Transform::Sort {
                    input,
                    output,
                    by,
                    order,
                    columns,
                } => {
                    let frame = get_frame(*input, inputs, &out.tables)?;
                    if columns.is_empty() {
                        return Err(ExecutionError::InvalidTransform);
                    }
                    require_columns(*input, frame, columns)?;
                    require_columns(*input, frame, core::slice::from_ref(by))?;

                    let by_idx = frame.column_index(*by).expect("validated");
                    let by_col = &frame.data[by_idx];

                    let mut idx: Vec<usize> = (0..frame.row_count()).collect();
                    idx.sort_by(|&a, &b| {
                        let av = by_col[a];
                        let bv = by_col[b];
                        let ord = cmp_f64_bits(av.to_bits(), bv.to_bits());
                        let ord = match order {
                            SortOrder::Asc => ord,
                            SortOrder::Desc => ord.reverse(),
                        };
                        ord.then_with(|| frame.row_keys[a].cmp(&frame.row_keys[b]))
                    });

                    let mut new_row_keys = Vec::with_capacity(idx.len());
                    for &r in &idx {
                        new_row_keys.push(frame.row_keys[r]);
                    }

                    let mut new_data = Vec::with_capacity(columns.len());
                    for &col in columns {
                        let ci = frame.column_index(col).expect("validated");
                        let src = &frame.data[ci];
                        let mut dst = Vec::with_capacity(idx.len());
                        for &r in &idx {
                            dst.push(src[r]);
                        }
                        new_data.push(dst);
                    }

                    out.tables.insert(
                        *output,
                        TableFrame {
                            row_keys: new_row_keys,
                            columns: columns.clone(),
                            data: new_data,
                        },
                    );
                }
                Transform::Aggregate {
                    input,
                    output,
                    group_by,
                    fields,
                } => {
                    let frame = get_frame(*input, inputs, &out.tables)?;
                    if fields.is_empty() {
                        return Err(ExecutionError::InvalidTransform);
                    }
                    if !group_by.is_empty() {
                        require_columns(*input, frame, group_by)?;
                    }
                    for f in fields {
                        require_columns(*input, frame, core::slice::from_ref(&f.input))?;
                    }

                    #[derive(Debug)]
                    struct Acc {
                        group_vals: Vec<f64>,
                        sum: Vec<f64>,
                        count: Vec<u64>,
                        min: Vec<f64>,
                        max: Vec<f64>,
                    }

                    let mut groups: HashMap<Vec<u64>, usize> = HashMap::new();
                    let mut order: Vec<Vec<u64>> = Vec::new();
                    let mut accs: Vec<Acc> = Vec::new();

                    for row in 0..frame.row_count() {
                        let mut key: Vec<u64> = Vec::with_capacity(group_by.len());
                        let mut group_vals: Vec<f64> = Vec::with_capacity(group_by.len());
                        for &c in group_by {
                            let v = frame.f64(row, c).unwrap_or(f64::NAN);
                            key.push(v.to_bits());
                            group_vals.push(v);
                        }

                        let idx = match groups.get(&key).copied() {
                            Some(i) => i,
                            None => {
                                let i = accs.len();
                                order.push(key.clone());
                                groups.insert(key, i);
                                accs.push(Acc {
                                    group_vals,
                                    sum: vec![0.0; fields.len()],
                                    count: vec![0; fields.len()],
                                    min: vec![f64::INFINITY; fields.len()],
                                    max: vec![f64::NEG_INFINITY; fields.len()],
                                });
                                i
                            }
                        };

                        let acc = &mut accs[idx];
                        for (fi, f) in fields.iter().enumerate() {
                            let v = frame.f64(row, f.input).unwrap_or(f64::NAN);
                            match f.op {
                                AggregateOp::Count => {
                                    acc.count[fi] = acc.count[fi].saturating_add(1);
                                }
                                AggregateOp::Sum | AggregateOp::Mean => {
                                    if v.is_finite() {
                                        acc.sum[fi] += v;
                                        acc.count[fi] = acc.count[fi].saturating_add(1);
                                    }
                                }
                                AggregateOp::Min => {
                                    if v.is_finite() {
                                        acc.min[fi] = acc.min[fi].min(v);
                                    }
                                }
                                AggregateOp::Max => {
                                    if v.is_finite() {
                                        acc.max[fi] = acc.max[fi].max(v);
                                    }
                                }
                            }
                        }
                    }

                    // Build output columns: group_by then each field.output.
                    let mut columns = Vec::with_capacity(group_by.len() + fields.len());
                    columns.extend(group_by.iter().copied());
                    columns.extend(fields.iter().map(|f| f.output));

                    let mut data: Vec<Vec<f64>> = vec![Vec::new(); columns.len()];
                    let mut row_keys: Vec<u64> = Vec::with_capacity(order.len());

                    for (gi, key) in order.iter().enumerate() {
                        row_keys.push(hash_group_key(key));
                        let acc = &accs[gi];

                        // Group-by columns.
                        for (i, v) in acc.group_vals.iter().copied().enumerate() {
                            data[i].push(v);
                        }

                        // Aggregate columns.
                        for (fi, f) in fields.iter().enumerate() {
                            let v = match f.op {
                                AggregateOp::Count => acc.count[fi] as f64,
                                AggregateOp::Sum => acc.sum[fi],
                                AggregateOp::Min => {
                                    if acc.min[fi].is_finite() {
                                        acc.min[fi]
                                    } else {
                                        f64::NAN
                                    }
                                }
                                AggregateOp::Max => {
                                    if acc.max[fi].is_finite() {
                                        acc.max[fi]
                                    } else {
                                        f64::NAN
                                    }
                                }
                                AggregateOp::Mean => {
                                    let c = acc.count[fi];
                                    if c == 0 {
                                        f64::NAN
                                    } else {
                                        acc.sum[fi] / c as f64
                                    }
                                }
                            };
                            data[group_by.len() + fi].push(v);
                        }
                    }

                    out.tables.insert(
                        *output,
                        TableFrame {
                            row_keys,
                            columns,
                            data,
                        },
                    );
                }
                Transform::Bin {
                    input,
                    output,
                    input_col,
                    output_start,
                    step,
                    columns,
                } => {
                    let frame = get_frame(*input, inputs, &out.tables)?;
                    if !step.is_finite() || *step <= 0.0 {
                        return Err(ExecutionError::InvalidTransform);
                    }
                    if columns.contains(output_start) {
                        return Err(ExecutionError::InvalidTransform);
                    }

                    require_columns(*input, frame, columns)?;
                    require_columns(*input, frame, core::slice::from_ref(input_col))?;

                    let mut out_columns = Vec::with_capacity(columns.len() + 1);
                    out_columns.extend(columns.iter().copied());
                    out_columns.push(*output_start);

                    let mut out_data: Vec<Vec<f64>> = Vec::with_capacity(out_columns.len());
                    for &col in columns {
                        let ci = frame.column_index(col).expect("validated");
                        out_data.push(frame.data[ci].clone());
                    }

                    // Bin values are anchored at 0 and floored to multiples of `step`.
                    let in_idx = frame.column_index(*input_col).expect("validated");
                    let in_col = &frame.data[in_idx];
                    let mut binned = Vec::with_capacity(frame.row_count());
                    for &v in in_col {
                        let bin0 = if v.is_finite() {
                            let q = floor_f64(v / *step);
                            q * *step
                        } else {
                            f64::NAN
                        };
                        binned.push(bin0);
                    }
                    out_data.push(binned);

                    out.tables.insert(
                        *output,
                        TableFrame {
                            row_keys: frame.row_keys.clone(),
                            columns: out_columns,
                            data: out_data,
                        },
                    );
                }
                Transform::Stack {
                    input,
                    output,
                    group_by,
                    offset,
                    sort_by,
                    sort_order,
                    field,
                    output_start,
                    output_end,
                    columns,
                } => {
                    let frame = get_frame(*input, inputs, &out.tables)?;
                    if columns.is_empty() {
                        return Err(ExecutionError::InvalidTransform);
                    }
                    if columns.contains(output_start) || columns.contains(output_end) {
                        return Err(ExecutionError::InvalidTransform);
                    }
                    if output_start == output_end {
                        return Err(ExecutionError::InvalidTransform);
                    }

                    require_columns(*input, frame, columns)?;
                    if !group_by.is_empty() {
                        require_columns(*input, frame, group_by)?;
                    }
                    if let Some(sort_by) = sort_by {
                        require_columns(*input, frame, core::slice::from_ref(sort_by))?;
                    }
                    require_columns(*input, frame, core::slice::from_ref(field))?;

                    let mut out_columns = Vec::with_capacity(columns.len() + 2);
                    out_columns.extend(columns.iter().copied());
                    out_columns.push(*output_start);
                    out_columns.push(*output_end);

                    let mut out_data: Vec<Vec<f64>> = Vec::with_capacity(out_columns.len());
                    for &col in columns {
                        let ci = frame.column_index(col).expect("validated");
                        out_data.push(frame.data[ci].clone());
                    }

                    let field_idx = frame.column_index(*field).expect("validated");
                    let field_col = &frame.data[field_idx];

                    // Group rows. We'll compute y0/y1 in a group-local order, but store results in
                    // original row order.
                    let mut groups: HashMap<Vec<u64>, Vec<usize>> = HashMap::new();
                    for row in 0..frame.row_count() {
                        let mut key: Vec<u64> = Vec::with_capacity(group_by.len());
                        for &c in group_by {
                            let v = frame.f64(row, c).unwrap_or(f64::NAN);
                            key.push(v.to_bits());
                        }
                        groups.entry(key).or_default().push(row);
                    }

                    let mut y0: Vec<f64> = vec![f64::NAN; frame.row_count()];
                    let mut y1: Vec<f64> = vec![f64::NAN; frame.row_count()];

                    let sort_by_idx = sort_by.and_then(|c| frame.column_index(c));

                    if matches!(offset, StackOffset::Wiggle) {
                        // v0 wiggle offset: compute a baseline across ordered groups (typically x
                        // slices), then stack within each group in series order.
                        //
                        // This needs:
                        // - an ordered grouping key (so we can walk group slices in order)
                        // - a stable series order (`sort_by`) across all groups
                        let sort_by_idx = sort_by_idx.ok_or(ExecutionError::InvalidTransform)?;
                        if group_by.is_empty() {
                            return Err(ExecutionError::InvalidTransform);
                        }

                        // Group ordering: lexicographic by group_by columns.
                        let mut group_keys: Vec<Vec<u64>> = groups.keys().cloned().collect();
                        group_keys.sort_by(|a, b| cmp_key_bits(a, b));

                        // Series ordering: unique `sort_by` values across the whole table.
                        let mut series_bits: Vec<u64> = Vec::new();
                        let mut series_seen: HashMap<u64, ()> = HashMap::new();
                        let sort_col = &frame.data[sort_by_idx];
                        for &v in sort_col.iter() {
                            let bits = v.to_bits();
                            if series_seen.insert(bits, ()).is_none() {
                                series_bits.push(bits);
                            }
                        }
                        series_bits.sort_by(|a, b| cmp_f64_bits(*a, *b));
                        if matches!(sort_order, SortOrder::Desc) {
                            series_bits.reverse();
                        }
                        if series_bits.is_empty() {
                            return Err(ExecutionError::InvalidTransform);
                        }

                        let mut series_index: HashMap<u64, usize> = HashMap::new();
                        for (i, &b) in series_bits.iter().enumerate() {
                            series_index.insert(b, i);
                        }

                        let n_series = series_bits.len();
                        let n_groups = group_keys.len();
                        let mut values_by_group: Vec<Vec<f64>> = Vec::with_capacity(n_groups);
                        let mut row_by_group: Vec<Vec<Option<usize>>> =
                            Vec::with_capacity(n_groups);

                        for key in group_keys.iter() {
                            let rows = groups.get(key).ok_or(ExecutionError::InvalidTransform)?;
                            let mut values: Vec<f64> = vec![0.0; n_series];
                            let mut row_for: Vec<Option<usize>> = vec![None; n_series];
                            for &row in rows.iter() {
                                let bits = sort_col[row].to_bits();
                                let si = *series_index
                                    .get(&bits)
                                    .ok_or(ExecutionError::InvalidTransform)?;
                                if row_for[si].replace(row).is_some() {
                                    // Multiple rows for the same (group, series) slot.
                                    return Err(ExecutionError::InvalidTransform);
                                }
                                let v = field_col[row];
                                if v.is_finite() {
                                    values[si] = v.abs();
                                }
                            }
                            values_by_group.push(values);
                            row_by_group.push(row_for);
                        }

                        // D3/Vega wiggle baseline:
                        // y[0] = 0
                        // y[i] = y[i-1] - (sum_j v[i][j] * (v[i][j] - v[i-1][j]) / 2) / sum_j v[i][j]
                        let mut baseline: Vec<f64> = vec![0.0; n_groups];
                        for i in 1..n_groups {
                            let mut sum = 0.0_f64;
                            for &v in values_by_group[i].iter() {
                                sum += v;
                            }
                            if sum != 0.0 {
                                let mut k = 0.0_f64;
                                for (&v, &v_prev) in
                                    values_by_group[i].iter().zip(values_by_group[i - 1].iter())
                                {
                                    k += v * (v - v_prev) * 0.5;
                                }
                                baseline[i] = baseline[i - 1] - k / sum;
                            } else {
                                baseline[i] = baseline[i - 1];
                            }
                        }

                        for (gi, row_for) in row_by_group.iter().enumerate() {
                            let mut last = baseline[gi];
                            for &row_opt in row_for.iter() {
                                if let Some(row) = row_opt {
                                    let v = field_col[row];
                                    if !v.is_finite() {
                                        y0[row] = f64::NAN;
                                        y1[row] = f64::NAN;
                                        continue;
                                    }
                                    y0[row] = last;
                                    last += v.abs();
                                    y1[row] = last;
                                }
                            }
                        }
                    } else {
                        // Vega `center` needs the maximum group sum (using absolute values).
                        let mut max_abs_sum = 0.0_f64;
                        if matches!(offset, StackOffset::Center) {
                            for rows in groups.values() {
                                let mut s = 0.0_f64;
                                for &row in rows {
                                    let v = field_col[row];
                                    if v.is_finite() {
                                        s += v.abs();
                                    }
                                }
                                max_abs_sum = max_abs_sum.max(s);
                            }
                        }

                        for rows in groups.values_mut() {
                            if let Some(sort_by_idx) = sort_by_idx {
                                let col = &frame.data[sort_by_idx];
                                rows.sort_by(|&a, &b| {
                                    let av = col[a];
                                    let bv = col[b];
                                    let ord = cmp_f64_bits(av.to_bits(), bv.to_bits());
                                    let ord = match sort_order {
                                        SortOrder::Asc => ord,
                                        SortOrder::Desc => ord.reverse(),
                                    };
                                    ord.then_with(|| frame.row_keys[a].cmp(&frame.row_keys[b]))
                                });
                            }

                            match offset {
                                StackOffset::Zero => {
                                    let mut last_pos = 0.0_f64;
                                    let mut last_neg = 0.0_f64;
                                    for &row in rows.iter() {
                                        let v = field_col[row];
                                        if !v.is_finite() {
                                            y0[row] = f64::NAN;
                                            y1[row] = f64::NAN;
                                            continue;
                                        }
                                        if v < 0.0 {
                                            y0[row] = last_neg;
                                            last_neg += v;
                                            y1[row] = last_neg;
                                        } else {
                                            y0[row] = last_pos;
                                            last_pos += v;
                                            y1[row] = last_pos;
                                        }
                                    }
                                }
                                StackOffset::Wiggle => {
                                    unreachable!("wiggle offset handled in the early branch");
                                }
                                StackOffset::Center => {
                                    let mut abs_sum = 0.0_f64;
                                    for &row in rows.iter() {
                                        let v = field_col[row];
                                        if v.is_finite() {
                                            abs_sum += v.abs();
                                        }
                                    }
                                    let mut last = (max_abs_sum - abs_sum) * 0.5;
                                    for &row in rows.iter() {
                                        let v = field_col[row];
                                        if !v.is_finite() {
                                            y0[row] = f64::NAN;
                                            y1[row] = f64::NAN;
                                            continue;
                                        }
                                        y0[row] = last;
                                        last += v.abs();
                                        y1[row] = last;
                                    }
                                }
                                StackOffset::Normalize => {
                                    let mut abs_sum = 0.0_f64;
                                    for &row in rows.iter() {
                                        let v = field_col[row];
                                        if v.is_finite() {
                                            abs_sum += v.abs();
                                        }
                                    }
                                    if abs_sum == 0.0 {
                                        for &row in rows.iter() {
                                            let v = field_col[row];
                                            if v.is_finite() {
                                                y0[row] = 0.0;
                                                y1[row] = 0.0;
                                            } else {
                                                y0[row] = f64::NAN;
                                                y1[row] = f64::NAN;
                                            }
                                        }
                                    } else {
                                        let scale = 1.0 / abs_sum;
                                        let mut last = 0.0_f64;
                                        let mut v = 0.0_f64;
                                        for &row in rows.iter() {
                                            let x = field_col[row];
                                            if !x.is_finite() {
                                                y0[row] = f64::NAN;
                                                y1[row] = f64::NAN;
                                                continue;
                                            }
                                            y0[row] = last;
                                            v += x.abs();
                                            last = scale * v;
                                            y1[row] = last;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    out_data.push(y0);
                    out_data.push(y1);

                    out.tables.insert(
                        *output,
                        TableFrame {
                            row_keys: frame.row_keys.clone(),
                            columns: out_columns,
                            data: out_data,
                        },
                    );
                }
            }
        }

        Ok(out)
    }
}

fn hash_group_key(bits: &[u64]) -> u64 {
    // FNV-1a 64-bit: deterministic and cheap.
    let mut h = 0xcbf29ce484222325_u64;
    for &x in bits {
        h ^= x;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn cmp_f64_bits(a: u64, b: u64) -> core::cmp::Ordering {
    let a = f64::from_bits(a);
    let b = f64::from_bits(b);
    match a.partial_cmp(&b) {
        Some(ord) => ord,
        None => {
            // Sort NaNs last for deterministic ordering.
            if a.is_nan() && !b.is_nan() {
                core::cmp::Ordering::Greater
            } else if !a.is_nan() && b.is_nan() {
                core::cmp::Ordering::Less
            } else {
                core::cmp::Ordering::Equal
            }
        }
    }
}

fn cmp_key_bits(a: &[u64], b: &[u64]) -> core::cmp::Ordering {
    let n = a.len().min(b.len());
    for i in 0..n {
        let ord = cmp_f64_bits(a[i], b[i]);
        if ord != core::cmp::Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}

fn get_frame<'a>(
    id: TableId,
    inputs: &'a HashMap<TableId, TableFrame>,
    outputs: &'a HashMap<TableId, TableFrame>,
) -> Result<&'a TableFrame, ExecutionError> {
    outputs
        .get(&id)
        .or_else(|| inputs.get(&id))
        .ok_or(ExecutionError::MissingInput(id))
}

fn require_columns(
    table: TableId,
    frame: &TableFrame,
    cols: &[ColId],
) -> Result<(), ExecutionError> {
    for &c in cols {
        if frame.column_index(c).is_none() {
            return Err(ExecutionError::MissingColumn { table, col: c });
        }
    }
    Ok(())
}

#[cfg(not(any(feature = "std", feature = "libm")))]
compile_error!(
    "vizir_transforms requires either the `std` or `libm` feature for floating-point math"
);

fn floor_f64(x: f64) -> f64 {
    #[cfg(feature = "std")]
    {
        x.floor()
    }
    #[cfg(all(not(feature = "std"), feature = "libm"))]
    {
        libm::floor(x)
    }
    #[cfg(all(not(feature = "std"), not(feature = "libm")))]
    {
        let _ = x;
        unreachable!("compile_error should have prevented this configuration");
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::vec;

    use super::*;
    use crate::transform::{AggregateField, AggregateOp, CompareOp, Predicate};

    fn frame() -> TableFrame {
        TableFrame {
            row_keys: vec![10, 11, 12, 13],
            columns: vec![ColId(0), ColId(1)],
            data: vec![vec![1.0, 2.0, 3.0, 4.0], vec![10.0, 9.0, 8.0, 7.0]],
        }
    }

    #[test]
    fn filter_preserves_row_keys() {
        let mut p = Program::new();
        p.push(Transform::Filter {
            input: TableId(1),
            output: TableId(2),
            predicate: Predicate {
                col: ColId(0),
                op: CompareOp::Ge,
                value: 3.0,
            },
            columns: vec![ColId(0), ColId(1)],
        });
        let inputs: HashMap<_, _> = [(TableId(1), frame())].into_iter().collect();
        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();
        assert_eq!(t.row_keys, vec![12, 13]);
        assert_eq!(t.data[0], vec![3.0, 4.0]);
    }

    #[test]
    fn project_selects_columns() {
        let mut p = Program::new();
        p.push(Transform::Project {
            input: TableId(1),
            output: TableId(2),
            columns: vec![ColId(1)],
        });
        let inputs: HashMap<_, _> = [(TableId(1), frame())].into_iter().collect();
        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();
        assert_eq!(t.columns, vec![ColId(1)]);
        assert_eq!(t.data.len(), 1);
        assert_eq!(t.data[0], vec![10.0, 9.0, 8.0, 7.0]);
    }

    #[test]
    fn sort_reorders_rows_and_preserves_keys() {
        let mut p = Program::new();
        p.push(Transform::Sort {
            input: TableId(1),
            output: TableId(2),
            by: ColId(1),
            order: SortOrder::Asc,
            columns: vec![ColId(0), ColId(1)],
        });
        let inputs: HashMap<_, _> = [(TableId(1), frame())].into_iter().collect();
        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();
        // Column1 is [10, 9, 8, 7] so ascending sort yields rows 7,8,9,10 (keys 13,12,11,10).
        assert_eq!(t.row_keys, vec![13, 12, 11, 10]);
        assert_eq!(t.data[1], vec![7.0, 8.0, 9.0, 10.0]);
    }

    #[test]
    fn aggregate_groups_by_key_and_sums() {
        let mut p = Program::new();
        p.push(Transform::Aggregate {
            input: TableId(1),
            output: TableId(2),
            group_by: vec![ColId(0)],
            fields: vec![AggregateField {
                op: AggregateOp::Sum,
                input: ColId(1),
                output: ColId(2),
            }],
        });

        let inputs: HashMap<_, _> = [(
            TableId(1),
            TableFrame {
                row_keys: vec![1, 2, 3, 4, 5],
                columns: vec![ColId(0), ColId(1)],
                data: vec![
                    vec![0.0, 0.0, 1.0, 1.0, 1.0], // group key
                    vec![1.0, 2.0, 3.0, 4.0, 5.0], // values
                ],
            },
        )]
        .into_iter()
        .collect();

        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();
        assert_eq!(t.columns, vec![ColId(0), ColId(2)]);
        assert_eq!(t.data.len(), 2);
        assert_eq!(t.data[0], vec![0.0, 1.0]);
        assert_eq!(t.data[1], vec![3.0, 12.0]);
    }

    #[test]
    fn bin_floors_to_step_multiples() {
        let mut p = Program::new();
        p.push(Transform::Bin {
            input: TableId(1),
            output: TableId(2),
            input_col: ColId(0),
            output_start: ColId(2),
            step: 2.0,
            columns: vec![ColId(0)],
        });

        let inputs: HashMap<_, _> = [(
            TableId(1),
            TableFrame {
                row_keys: vec![1, 2, 3, 4],
                columns: vec![ColId(0)],
                data: vec![vec![3.7, 6.2, 5.9, 8.0]],
            },
        )]
        .into_iter()
        .collect();

        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();
        assert_eq!(t.columns, vec![ColId(0), ColId(2)]);
        assert_eq!(t.data[0], vec![3.7, 6.2, 5.9, 8.0]);
        assert_eq!(t.data[1], vec![2.0, 6.0, 4.0, 8.0]);
        assert_eq!(t.row_keys, vec![1, 2, 3, 4]);
    }

    #[test]
    fn stack_accumulates_positive_and_negative_separately() {
        let mut p = Program::new();
        p.push(Transform::Stack {
            input: TableId(1),
            output: TableId(2),
            group_by: vec![ColId(0)],
            offset: StackOffset::Zero,
            sort_by: None,
            sort_order: SortOrder::Asc,
            field: ColId(2),
            output_start: ColId(3),
            output_end: ColId(4),
            columns: vec![ColId(0), ColId(1), ColId(2)],
        });

        let inputs: HashMap<_, _> = [(
            TableId(1),
            TableFrame {
                row_keys: vec![10, 11, 12, 13, 14],
                columns: vec![ColId(0), ColId(1), ColId(2)],
                data: vec![
                    vec![0.0, 0.0, 0.0, 1.0, 1.0],  // group key
                    vec![0.0, 1.0, 2.0, 0.0, 1.0],  // "series" id (not used by stack)
                    vec![1.0, 2.0, -1.0, 3.0, 4.0], // value
                ],
            },
        )]
        .into_iter()
        .collect();

        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();
        assert_eq!(
            t.columns,
            vec![ColId(0), ColId(1), ColId(2), ColId(3), ColId(4)]
        );

        // Group 0.0:
        // row0 v=1 => [0,1]
        // row1 v=2 => [1,3]
        // row2 v=-1 => [0,-1] (negative stack, Vega convention)
        assert_eq!(t.data[3][0], 0.0);
        assert_eq!(t.data[4][0], 1.0);
        assert_eq!(t.data[3][1], 1.0);
        assert_eq!(t.data[4][1], 3.0);
        assert_eq!(t.data[3][2], 0.0);
        assert_eq!(t.data[4][2], -1.0);

        // Group 1.0:
        // row3 v=3 => [0,3]
        // row4 v=4 => [3,7]
        assert_eq!(t.data[3][3], 0.0);
        assert_eq!(t.data[4][3], 3.0);
        assert_eq!(t.data[3][4], 3.0);
        assert_eq!(t.data[4][4], 7.0);

        assert_eq!(t.row_keys, vec![10, 11, 12, 13, 14]);
    }

    #[test]
    fn stack_with_empty_groupby_is_global_stack() {
        let mut p = Program::new();
        p.push(Transform::Stack {
            input: TableId(1),
            output: TableId(2),
            group_by: vec![],
            offset: StackOffset::Zero,
            sort_by: None,
            sort_order: SortOrder::Asc,
            field: ColId(0),
            output_start: ColId(1),
            output_end: ColId(2),
            columns: vec![ColId(0)],
        });

        let inputs: HashMap<_, _> = [(
            TableId(1),
            TableFrame {
                row_keys: vec![1, 2, 3],
                columns: vec![ColId(0)],
                data: vec![vec![1.0, 2.0, 3.0]],
            },
        )]
        .into_iter()
        .collect();

        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();
        assert_eq!(t.data[1], vec![0.0, 1.0, 3.0]);
        assert_eq!(t.data[2], vec![1.0, 3.0, 6.0]);
    }

    #[test]
    fn stack_propagates_nan_for_non_finite_values() {
        let mut p = Program::new();
        p.push(Transform::Stack {
            input: TableId(1),
            output: TableId(2),
            group_by: vec![ColId(0)],
            offset: StackOffset::Zero,
            sort_by: None,
            sort_order: SortOrder::Asc,
            field: ColId(1),
            output_start: ColId(2),
            output_end: ColId(3),
            columns: vec![ColId(0), ColId(1)],
        });

        let inputs: HashMap<_, _> = [(
            TableId(1),
            TableFrame {
                row_keys: vec![1, 2, 3],
                columns: vec![ColId(0), ColId(1)],
                data: vec![vec![0.0, 0.0, 0.0], vec![1.0, f64::NAN, 2.0]],
            },
        )]
        .into_iter()
        .collect();

        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();
        assert_eq!(t.data[2][0], 0.0);
        assert_eq!(t.data[3][0], 1.0);
        assert!(t.data[2][1].is_nan());
        assert!(t.data[3][1].is_nan());
        // NaNs should not advance the running offset.
        assert_eq!(t.data[2][2], 1.0);
        assert_eq!(t.data[3][2], 3.0);
    }

    #[test]
    fn stack_respects_upstream_sort_order_within_groups() {
        // This is the intended v0 workflow for Vega's `sort` parameter: sort upstream, then stack.
        let mut p = Program::new();
        p.push(Transform::Sort {
            input: TableId(1),
            output: TableId(2),
            by: ColId(1),
            order: SortOrder::Asc,
            columns: vec![ColId(0), ColId(1), ColId(2)],
        });
        p.push(Transform::Stack {
            input: TableId(2),
            output: TableId(3),
            group_by: vec![ColId(0)],
            offset: StackOffset::Zero,
            sort_by: None,
            sort_order: SortOrder::Asc,
            field: ColId(2),
            output_start: ColId(3),
            output_end: ColId(4),
            columns: vec![ColId(0), ColId(1), ColId(2)],
        });

        let inputs: HashMap<_, _> = [(
            TableId(1),
            TableFrame {
                row_keys: vec![10, 11, 12],
                columns: vec![ColId(0), ColId(1), ColId(2)],
                data: vec![
                    vec![0.0, 0.0, 0.0],    // group key
                    vec![2.0, 1.0, 3.0],    // sort key
                    vec![1.0, 10.0, 100.0], // value
                ],
            },
        )]
        .into_iter()
        .collect();

        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(3)).unwrap();

        // After sort (by col1 asc), order is keys [11,10,12] with values [10,1,100].
        // Stack should then accumulate in that order.
        assert_eq!(t.row_keys, vec![11, 10, 12]);
        assert_eq!(t.data[3], vec![0.0, 10.0, 11.0]);
        assert_eq!(t.data[4], vec![10.0, 11.0, 111.0]);
    }

    #[test]
    fn stack_sort_by_affects_layout_but_not_row_order() {
        let mut p = Program::new();
        p.push(Transform::Stack {
            input: TableId(1),
            output: TableId(2),
            group_by: vec![ColId(0)],
            offset: StackOffset::Zero,
            sort_by: Some(ColId(1)),
            sort_order: SortOrder::Asc,
            field: ColId(2),
            output_start: ColId(3),
            output_end: ColId(4),
            columns: vec![ColId(0), ColId(1), ColId(2)],
        });

        let inputs: HashMap<_, _> = [(
            TableId(1),
            TableFrame {
                row_keys: vec![10, 11, 12],
                columns: vec![ColId(0), ColId(1), ColId(2)],
                data: vec![
                    vec![0.0, 0.0, 0.0],    // group key
                    vec![2.0, 1.0, 3.0],    // sort key
                    vec![1.0, 10.0, 100.0], // value
                ],
            },
        )]
        .into_iter()
        .collect();

        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();

        // Output row order preserved.
        assert_eq!(t.row_keys, vec![10, 11, 12]);

        // But y0/y1 reflect stacking in sort-key order: rows [11,10,12] => values [10,1,100].
        assert_eq!(t.data[3], vec![10.0, 0.0, 11.0]);
        assert_eq!(t.data[4], vec![11.0, 10.0, 111.0]);
    }

    #[test]
    fn stack_normalize_produces_unit_range() {
        let mut p = Program::new();
        p.push(Transform::Stack {
            input: TableId(1),
            output: TableId(2),
            group_by: vec![ColId(0)],
            offset: StackOffset::Normalize,
            sort_by: Some(ColId(1)),
            sort_order: SortOrder::Asc,
            field: ColId(2),
            output_start: ColId(3),
            output_end: ColId(4),
            columns: vec![ColId(0), ColId(1), ColId(2)],
        });

        let inputs: HashMap<_, _> = [(
            TableId(1),
            TableFrame {
                row_keys: vec![10, 11],
                columns: vec![ColId(0), ColId(1), ColId(2)],
                data: vec![
                    vec![0.0, 0.0], // group key
                    vec![0.0, 1.0], // sort key
                    vec![1.0, 3.0], // values
                ],
            },
        )]
        .into_iter()
        .collect();

        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();
        assert_eq!(t.data[3], vec![0.0, 0.25]);
        assert_eq!(t.data[4], vec![0.25, 1.0]);
    }

    #[test]
    fn stack_center_uses_global_max_sum() {
        let mut p = Program::new();
        p.push(Transform::Stack {
            input: TableId(1),
            output: TableId(2),
            group_by: vec![ColId(0)],
            offset: StackOffset::Center,
            sort_by: Some(ColId(1)),
            sort_order: SortOrder::Asc,
            field: ColId(2),
            output_start: ColId(3),
            output_end: ColId(4),
            columns: vec![ColId(0), ColId(1), ColId(2)],
        });

        // Two groups: sum(abs)=4 and sum(abs)=2. Global max=4.
        let inputs: HashMap<_, _> = [(
            TableId(1),
            TableFrame {
                row_keys: vec![10, 11, 12, 13],
                columns: vec![ColId(0), ColId(1), ColId(2)],
                data: vec![
                    vec![0.0, 0.0, 1.0, 1.0], // group key
                    vec![0.0, 1.0, 0.0, 1.0], // sort key
                    vec![1.0, 3.0, 1.0, 1.0], // values
                ],
            },
        )]
        .into_iter()
        .collect();

        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();

        // Group 0: max=4,sum=4 => baseline 0
        assert_eq!(t.data[3][0], 0.0);
        assert_eq!(t.data[4][0], 1.0);
        assert_eq!(t.data[3][1], 1.0);
        assert_eq!(t.data[4][1], 4.0);

        // Group 1: max=4,sum=2 => baseline (4-2)/2 = 1
        assert_eq!(t.data[3][2], 1.0);
        assert_eq!(t.data[4][2], 2.0);
        assert_eq!(t.data[3][3], 2.0);
        assert_eq!(t.data[4][3], 3.0);
    }

    #[test]
    fn stack_wiggle_matches_d3_baseline() {
        let mut p = Program::new();
        p.push(Transform::Stack {
            input: TableId(1),
            output: TableId(2),
            group_by: vec![ColId(0)], // x
            offset: StackOffset::Wiggle,
            sort_by: Some(ColId(1)), // series
            sort_order: SortOrder::Asc,
            field: ColId(2), // y
            output_start: ColId(3),
            output_end: ColId(4),
            columns: vec![ColId(0), ColId(1), ColId(2)],
        });

        // 2 series across 3 x slices.
        // s0: [1, 1, 1]
        // s1: [1, 2, 3]
        //
        // D3/Vega wiggle baseline (y0 per slice): [0, -1/3, -17/24].
        let inputs: HashMap<_, _> = [(
            TableId(1),
            TableFrame {
                row_keys: vec![10, 11, 12, 13, 14, 15],
                columns: vec![ColId(0), ColId(1), ColId(2)],
                data: vec![
                    vec![0.0, 0.0, 1.0, 1.0, 2.0, 2.0], // x
                    vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0], // series
                    vec![1.0, 1.0, 1.0, 2.0, 1.0, 3.0], // y
                ],
            },
        )]
        .into_iter()
        .collect();

        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();

        fn assert_close(a: f64, b: f64) {
            let d = (a - b).abs();
            assert!(d < 1.0e-9, "expected {b}, got {a} (|Δ|={d})");
        }

        // x=0
        assert_close(t.data[3][0], 0.0);
        assert_close(t.data[4][0], 1.0);
        assert_close(t.data[3][1], 1.0);
        assert_close(t.data[4][1], 2.0);

        // x=1 baseline -1/3
        assert_close(t.data[3][2], -1.0 / 3.0);
        assert_close(t.data[4][2], 2.0 / 3.0);
        assert_close(t.data[3][3], 2.0 / 3.0);
        assert_close(t.data[4][3], 8.0 / 3.0);

        // x=2 baseline -17/24
        assert_close(t.data[3][4], -17.0 / 24.0);
        assert_close(t.data[4][4], 7.0 / 24.0);
        assert_close(t.data[3][5], 7.0 / 24.0);
        assert_close(t.data[4][5], 79.0 / 24.0);
    }

    #[test]
    fn sort_places_nans_last_and_is_deterministic() {
        let mut p = Program::new();
        p.push(Transform::Sort {
            input: TableId(1),
            output: TableId(2),
            by: ColId(1),
            order: SortOrder::Asc,
            columns: vec![ColId(0), ColId(1)],
        });

        // row_keys: [10,11,12,13]
        // sort key col1: [NaN, 1.0, NaN, 0.0]
        let inputs: HashMap<_, _> = [(
            TableId(1),
            TableFrame {
                row_keys: vec![10, 11, 12, 13],
                columns: vec![ColId(0), ColId(1)],
                data: vec![
                    vec![1.0, 2.0, 3.0, 4.0],           // col0 payload
                    vec![f64::NAN, 1.0, f64::NAN, 0.0], // col1 sort key
                ],
            },
        )]
        .into_iter()
        .collect();

        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();

        // Ascending: 0.0, 1.0, NaN, NaN. NaNs last, tie-broken by row_key (10 then 12).
        assert_eq!(t.row_keys, vec![13, 11, 10, 12]);

        assert_eq!(t.data[1][0], 0.0);
        assert_eq!(t.data[1][1], 1.0);
        assert!(t.data[1][2].is_nan());
        assert!(t.data[1][3].is_nan());
        assert_eq!(t.data[0], vec![4.0, 2.0, 1.0, 3.0]);
    }

    #[test]
    fn stack_sort_by_places_nans_last_deterministically() {
        let mut p = Program::new();
        p.push(Transform::Stack {
            input: TableId(1),
            output: TableId(2),
            group_by: vec![ColId(0)], // single group
            offset: StackOffset::Zero,
            sort_by: Some(ColId(1)), // sort key (contains NaNs)
            sort_order: SortOrder::Asc,
            field: ColId(2), // values to stack
            output_start: ColId(3),
            output_end: ColId(4),
            columns: vec![ColId(0), ColId(1), ColId(2)],
        });

        // One group (col0 = 0.0 for all rows).
        // sort_by col1: [NaN, 1.0, NaN, 0.0]
        // field col2:   [1,   10,  100, 1000]
        //
        // Stack order within group (Asc, NaNs last, tie by row_key):
        // row_key 13 (0.0) -> row_key 11 (1.0) -> row_key 10 (NaN) -> row_key 12 (NaN)
        // Values:            1000              10                1                100
        //
        // Cumulative y0/y1:
        // rk13: [0,1000]
        // rk11: [1000,1010]
        // rk10: [1010,1011]
        // rk12: [1011,1111]
        //
        // Output row order is preserved (10,11,12,13), but y0/y1 stored at those rows.
        let inputs: HashMap<_, _> = [(
            TableId(1),
            TableFrame {
                row_keys: vec![10, 11, 12, 13],
                columns: vec![ColId(0), ColId(1), ColId(2)],
                data: vec![
                    vec![0.0, 0.0, 0.0, 0.0],           // group key
                    vec![f64::NAN, 1.0, f64::NAN, 0.0], // sort key
                    vec![1.0, 10.0, 100.0, 1000.0],     // field
                ],
            },
        )]
        .into_iter()
        .collect();

        let out = p.execute(&inputs).unwrap();
        let t = out.tables.get(&TableId(2)).unwrap();

        // Row order preserved:
        assert_eq!(t.row_keys, vec![10, 11, 12, 13]);

        // y0 (output_start col3) in original row order:
        assert_eq!(t.data[3], vec![1010.0, 1000.0, 1011.0, 0.0]);

        // y1 (output_end col4) in original row order:
        assert_eq!(t.data[4], vec![1011.0, 1010.0, 1111.0, 1000.0]);
    }
}
