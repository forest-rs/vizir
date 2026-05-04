// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Chart demos for `vizir_core`.

mod html;

use kurbo::Rect;
use vizir_backend_svg::SvgScene;

fn main() {
    let sections = shared_scenario_sections();
    let html = html::render_report("VizIR charts demo", &sections);
    std::fs::write("vizir_charts_demo.html", html).expect("write vizir_charts_demo.html");
    println!("wrote vizir_charts_demo.html");
}

fn shared_scenario_sections() -> Vec<html::HtmlSection> {
    vizir_demo_scenarios::static_scenarios()
        .iter()
        .map(|scenario| {
            let frame = scenario.build();
            html::HtmlSection {
                title: scenario.title,
                description: scenario.description,
                svg: render_diffs_to_svg(frame.view, &frame.diffs),
            }
        })
        .collect()
}

fn render_diffs_to_svg(view: Rect, diffs: &[vizir_core::MarkDiff]) -> String {
    let mut svg_scene = SvgScene::new();
    svg_scene.set_view_box(view);
    svg_scene.apply_diffs(diffs);
    svg_scene.to_svg_string()
}
