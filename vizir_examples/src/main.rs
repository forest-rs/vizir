// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Example binary for `vizir_core`.

use kurbo::BezPath;
use peniko::Color;
use vizir_core::{InputRef, Mark, MarkDiff, Scene, SignalId, TableId};

fn main() {
    let mut scene = Scene::new();

    let zoom = SignalId(1);
    scene.insert_signal(zoom, 1.0_f32);

    let table = TableId(1);
    scene.set_table_row_keys(table, vec![10, 11, 12]);

    let diffs = scene.tick_table_rows(table, |id, row_key, _row| {
        Mark::builder(id)
            .x_compute([InputRef::Signal { signal: zoom }], move |ctx, _| {
                f64::from(ctx.signal::<f32>(zoom).unwrap_or(1.0)) * row_key as f64
            })
            .y_const(0.0)
            .w_const(5.0)
            .h_const(5.0)
            .fill_const(Color::from_rgba8(0, 0, 255, 255))
            .build()
    });

    println!("tick#1: {} diffs", diffs.len());
    print_diffs(&diffs);

    scene.set_signal(zoom, 2.0_f32).unwrap();
    let diffs = scene.update();
    println!("tick#2 (zoom): {} diffs", diffs.len());
    print_diffs(&diffs);

    // Add one `Text` mark and one `Path` mark to demonstrate other kinds.
    let text_id = vizir_core::MarkId::from_raw(0x1);
    let text = Mark::builder(text_id)
        .text()
        .x_const(10.0)
        .y_const(20.0)
        .text_compute([InputRef::Signal { signal: zoom }], move |ctx, _| {
            let z = ctx.signal::<f32>(zoom).unwrap_or(0.0);
            format!("zoom={z}")
        })
        .font_size_const(14.0)
        .fill_const(Color::from_rgba8(0, 0, 0, 255))
        .build();

    let path_id = vizir_core::MarkId::from_raw(0x2);
    let mut triangle = BezPath::new();
    triangle.move_to((0.0, 0.0));
    triangle.line_to((10.0, 0.0));
    triangle.line_to((5.0, 10.0));
    triangle.close_path();
    let path = Mark::builder(path_id)
        .path()
        .path_const(triangle)
        .fill_const(Color::from_rgba8(255, 0, 0, 255))
        .build();

    let diffs = scene.tick([text, path]);
    println!("tick#2b (text+path): {} diffs", diffs.len());
    print_diffs(&diffs);

    scene.set_table_row_keys(table, vec![11, 12, 13]);
    let diffs = scene.tick_table_rows(table, |id, row_key, _row| {
        Mark::builder(id).x_const(row_key as f64).build()
    });
    println!("tick#3 (rows): {} diffs", diffs.len());
    print_diffs(&diffs);
}

fn print_diffs(diffs: &[MarkDiff]) {
    for d in diffs {
        match d {
            MarkDiff::Enter {
                id, kind, bounds, ..
            } => println!("  Enter {id:?} kind={kind:?} bounds={bounds:?}"),
            MarkDiff::Update {
                id,
                kind,
                old_bounds,
                new_bounds,
                ..
            } => println!("  Update {id:?} kind={kind:?} old={old_bounds:?} new={new_bounds:?}"),
            MarkDiff::Exit {
                id, kind, bounds, ..
            } => println!("  Exit  {id:?} kind={kind:?} bounds={bounds:?}"),
        }
    }
}
