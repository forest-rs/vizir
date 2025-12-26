// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

extern crate std;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use kurbo::Rect;
use peniko::color::palette::css;
use vizir_core::{ColId, MarkDiff, Scene, Table, TableData, TableId};
use vizir_transforms::{SortOrder, Transform};

use crate::{
    ScaleBand, ScaleContinuous, ScaleLinear, StackedAreaChartSpec, StackedBarChartSpec,
    StackedBarMarkSpec,
};

#[derive(Debug)]
struct StackedValues {
    cat: Vec<f64>,
    y0: Vec<f64>,
    y1: Vec<f64>,
}

impl TableData for StackedValues {
    fn row_count(&self) -> usize {
        self.cat.len().min(self.y0.len()).min(self.y1.len())
    }

    fn f64(&self, row: usize, col: ColId) -> Option<f64> {
        match col {
            ColId(0) => self.cat.get(row).copied(),
            ColId(1) => self.y0.get(row).copied(),
            ColId(2) => self.y1.get(row).copied(),
            _ => None,
        }
    }
}

fn find_enter_bounds(diffs: &[MarkDiff], id: vizir_core::MarkId) -> Rect {
    for d in diffs {
        if let MarkDiff::Enter {
            id: got, bounds, ..
        } = d
            && *got == id
        {
            return bounds.expect("rect marks should have bounds");
        }
    }
    panic!("missing Enter diff for {id:?}");
}

fn assert_rect_close(a: Rect, b: Rect) {
    let eps = 1e-9;
    assert!((a.x0 - b.x0).abs() <= eps, "x0 {a:?} != {b:?}");
    assert!((a.y0 - b.y0).abs() <= eps, "y0 {a:?} != {b:?}");
    assert!((a.x1 - b.x1).abs() <= eps, "x1 {a:?} != {b:?}");
    assert!((a.y1 - b.y1).abs() <= eps, "y1 {a:?} != {b:?}");
}

#[test]
fn stacked_bar_uses_category_for_x_and_y0_y1_for_vertical_span() {
    let table_id = TableId(1);
    let cat_col = ColId(0);
    let y0_col = ColId(1);
    let y1_col = ColId(2);

    let mut scene = Scene::new();
    let mut t = Table::new(table_id);
    t.row_keys = vec![10, 11];
    t.data = Some(Box::new(StackedValues {
        cat: vec![0.0, 1.0],
        y0: vec![0.0, 2.0],
        y1: vec![3.0, 5.0],
    }));
    scene.insert_table(t);

    let band = ScaleBand::new((10.0, 30.0), 2).with_padding(0.0, 0.0);
    let y_scale = ScaleLinear::new((0.0, 5.0), (100.0, 0.0));

    let marks = StackedBarMarkSpec::new(
        table_id,
        cat_col,
        y0_col,
        y1_col,
        band,
        ScaleContinuous::Linear(y_scale),
    )
    .with_fill(css::CORNFLOWER_BLUE)
    .marks(&scene.tables[&table_id].row_keys);

    let diffs = scene.tick(marks);

    // Row 0: cat 0 => x=[10,20], y0=0,y1=3 => y=[0..3] mapped to [100..40] => rect y0=40,y1=100
    let id0 = vizir_core::MarkId::for_row(table_id, 10);
    let b0 = find_enter_bounds(&diffs, id0);
    assert_rect_close(b0, Rect::new(10.0, 40.0, 20.0, 100.0));

    // Row 1: cat 1 => x=[20,30], y0=2,y1=5 => mapped [60..0] => rect y0=0,y1=60
    let id1 = vizir_core::MarkId::for_row(table_id, 11);
    let b1 = find_enter_bounds(&diffs, id1);
    assert_rect_close(b1, Rect::new(20.0, 0.0, 30.0, 60.0));
}

#[test]
fn stacked_area_chart_builds_stack_and_series_programs() {
    let spec = StackedAreaChartSpec::new(
        TableId(1),
        TableId(2),
        ColId(0),
        ColId(1),
        ColId(2),
        ColId(3),
        ColId(4),
    );

    let p = spec.program();
    assert!(
        matches!(p.transforms()[0], Transform::Stack { .. }),
        "expected first transform to be Stack"
    );

    let p = spec.series_program(TableId(10), 2.0);
    assert_eq!(p.transforms().len(), 2);
    assert!(
        matches!(p.transforms()[0], Transform::Filter { .. }),
        "expected Filter then Sort"
    );
    assert!(
        matches!(p.transforms()[1], Transform::Sort { .. }),
        "expected Filter then Sort"
    );
}

#[test]
fn stacked_bar_chart_defaults_sort_within_stack_by_series() {
    let spec = StackedBarChartSpec::new(
        TableId(1),
        TableId(2),
        ColId(0),
        ColId(1),
        ColId(2),
        ColId(3),
        ColId(4),
    );
    let p = spec.program();
    match &p.transforms()[0] {
        Transform::Stack {
            sort_by,
            sort_order,
            ..
        } => {
            assert_eq!(*sort_by, Some(ColId(1)));
            assert_eq!(*sort_order, SortOrder::Asc);
        }
        _ => panic!("expected Stack"),
    }
}
