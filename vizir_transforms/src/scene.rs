// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Convenience execution helpers for running transforms against a `vizir_core::Scene`.
//!
//! This is an ergonomics layer: chart/demo code shouldn't have to manually extract `TableFrame`s,
//! run `Program::execute`, and then re-insert output tables.

extern crate alloc;

use alloc::vec::Vec;

use hashbrown::hash_map::Entry;
use hashbrown::{HashMap, HashSet};
use vizir_core::{ColId, Scene, Table, TableId};

use crate::Program;
use crate::program::{ExecutionError, ProgramOutput};
use crate::table::{TableFrame, TableFrameError};
use crate::transform::Transform;

/// Errors returned when executing a [`Program`] against a [`Scene`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneExecutionError {
    /// The referenced input table does not exist in the scene.
    MissingInput(TableId),
    /// The referenced input table exists, but has no data accessor.
    MissingData(TableId),
    /// Failed to extract a numeric frame from an input table.
    FrameError {
        /// The table id that failed frame extraction.
        table: TableId,
        /// The underlying extraction error.
        err: TableFrameError,
    },
    /// Failed while executing the program.
    Execution(ExecutionError),
}

impl Program {
    /// Execute this program using tables from the given scene.
    ///
    /// This extracts the required numeric columns into `TableFrame`s and runs the program in
    /// full-recompute mode. The returned output contains owned tables (`TableFrame`) keyed by
    /// their output ids.
    pub fn execute_on_scene(&self, scene: &Scene) -> Result<ProgramOutput, SceneExecutionError> {
        let required = required_input_columns(self.transforms());
        let mut inputs: HashMap<TableId, TableFrame> = HashMap::new();

        for (table_id, cols) in required {
            let Some(table) = scene.tables.get(&table_id) else {
                return Err(SceneExecutionError::MissingInput(table_id));
            };
            if table.data.is_none() {
                return Err(SceneExecutionError::MissingData(table_id));
            }
            let mut columns: Vec<ColId> = cols.into_iter().collect();
            columns.sort_by_key(|c| c.0);
            let frame = TableFrame::from_table(table, columns).map_err(|err| {
                SceneExecutionError::FrameError {
                    table: table_id,
                    err,
                }
            })?;
            inputs.insert(table_id, frame);
        }

        self.execute(&inputs)
            .map_err(SceneExecutionError::Execution)
    }

    /// Execute this program against the scene, inserting/updating output tables.
    ///
    /// Output tables are inserted if missing. If a table exists, its `row_keys` and `data` are
    /// replaced and its version is bumped once.
    pub fn apply_to_scene(&self, scene: &mut Scene) -> Result<ProgramOutput, SceneExecutionError> {
        let out = self.execute_on_scene(scene)?;
        for (id, frame) in out.tables.iter() {
            upsert_frame_as_table(scene, *id, frame.clone());
        }
        Ok(out)
    }
}

fn required_input_columns(transforms: &[Transform]) -> HashMap<TableId, HashSet<ColId>> {
    let mut out: HashMap<TableId, HashSet<ColId>> = HashMap::new();
    let mut produced: HashSet<TableId> = HashSet::new();

    for t in transforms {
        match t {
            Transform::Filter {
                input,
                output,
                predicate,
                columns,
            } => {
                if !produced.contains(input) {
                    let set = out.entry(*input).or_default();
                    for &c in columns {
                        set.insert(c);
                    }
                    set.insert(predicate.col);
                }
                produced.insert(*output);
            }
            Transform::Project {
                input,
                output,
                columns,
            } => {
                if !produced.contains(input) {
                    let set = out.entry(*input).or_default();
                    for &c in columns {
                        set.insert(c);
                    }
                }
                produced.insert(*output);
            }
            Transform::Sort {
                input,
                output,
                by,
                columns,
                ..
            } => {
                if !produced.contains(input) {
                    let set = out.entry(*input).or_default();
                    for &c in columns {
                        set.insert(c);
                    }
                    set.insert(*by);
                }
                produced.insert(*output);
            }
            Transform::Aggregate {
                input,
                output,
                group_by,
                fields,
            } => {
                if !produced.contains(input) {
                    let set = out.entry(*input).or_default();
                    for &c in group_by {
                        set.insert(c);
                    }
                    for f in fields {
                        set.insert(f.input);
                    }
                }
                produced.insert(*output);
            }
            Transform::Bin {
                input,
                output,
                input_col,
                columns,
                ..
            } => {
                if !produced.contains(input) {
                    let set = out.entry(*input).or_default();
                    for &c in columns {
                        set.insert(c);
                    }
                    set.insert(*input_col);
                }
                produced.insert(*output);
            }
            Transform::Stack {
                input,
                output,
                group_by,
                offset: _,
                sort_by,
                field,
                columns,
                ..
            } => {
                if !produced.contains(input) {
                    let set = out.entry(*input).or_default();
                    for &c in columns {
                        set.insert(c);
                    }
                    for &c in group_by {
                        set.insert(c);
                    }
                    if let Some(sort_by) = sort_by {
                        set.insert(*sort_by);
                    }
                    set.insert(*field);
                }
                produced.insert(*output);
            }
        }
    }

    out
}

fn upsert_frame_as_table(scene: &mut Scene, id: TableId, frame: TableFrame) {
    match scene.tables.entry(id) {
        Entry::Occupied(mut e) => {
            let Table { data, row_keys, .. } = frame.into_table(id);
            let existing = e.get_mut();
            existing.row_keys = row_keys;
            existing.data = data;
            existing.bump();
        }
        Entry::Vacant(e) => {
            e.insert(frame.into_table(id));
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::boxed::Box;
    use alloc::vec;
    use alloc::vec::Vec;

    use vizir_core::{ColId, Scene, Table, TableData, TableId};

    use super::*;
    use crate::transform::Transform;

    #[derive(Debug)]
    struct TwoCols {
        a: Vec<f64>,
        b: Vec<f64>,
    }

    impl TableData for TwoCols {
        fn row_count(&self) -> usize {
            self.a.len().min(self.b.len())
        }

        fn f64(&self, row: usize, col: ColId) -> Option<f64> {
            match col {
                ColId(0) => self.a.get(row).copied(),
                ColId(1) => self.b.get(row).copied(),
                _ => None,
            }
        }
    }

    #[test]
    fn apply_to_scene_inserts_output_table_and_bumps_on_update() {
        let source_id = TableId(1);
        let out_id = TableId(2);

        let mut scene = Scene::new();
        let mut t = Table::new(source_id);
        t.row_keys = vec![10, 11, 12];
        t.data = Some(Box::new(TwoCols {
            a: vec![1.0, 2.0, 3.0],
            b: vec![3.0, 2.0, 1.0],
        }));
        scene.insert_table(t);

        let mut p = Program::new();
        p.push(Transform::Project {
            input: source_id,
            output: out_id,
            columns: vec![ColId(0)],
        });

        // First run: insert the output table.
        p.apply_to_scene(&mut scene).unwrap();
        let v1 = scene.tables.get(&out_id).unwrap().version;

        // Second run: updates existing table and bumps version once.
        p.apply_to_scene(&mut scene).unwrap();
        let v2 = scene.tables.get(&out_id).unwrap().version;

        assert_ne!(v1, v2);
    }
}
