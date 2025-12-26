// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal SVG dump utilities for `vizir_charts_demo`.

use std::collections::HashMap;

use kurbo::Rect;
use peniko::Brush;
use vizir_core::{MarkDiff, MarkId, MarkPayload, TextAnchor, TextBaseline};

#[derive(Debug, Default)]
pub(crate) struct SvgScene {
    marks: HashMap<MarkId, (i32, MarkPayload)>,
    view_box: Option<Rect>,
}

impl SvgScene {
    pub(crate) fn set_view_box(&mut self, view_box: Rect) {
        self.view_box = Some(view_box);
    }

    pub(crate) fn apply_diffs(&mut self, diffs: &[MarkDiff]) {
        for diff in diffs {
            match diff {
                MarkDiff::Enter {
                    id, z_index, new, ..
                } => {
                    self.marks.insert(*id, (*z_index, (**new).clone()));
                }
                MarkDiff::Update {
                    id,
                    new_z_index,
                    new,
                    ..
                } => {
                    self.marks.insert(*id, (*new_z_index, (**new).clone()));
                }
                MarkDiff::Exit { id, .. } => {
                    self.marks.remove(id);
                }
            }
        }
    }

    pub(crate) fn to_svg_string(&self) -> String {
        let computed = self.view_box();
        let view_box = match (self.view_box, computed) {
            (Some(a), Some(b)) => Some(Rect::new(
                a.x0.min(b.x0),
                a.y0.min(b.y0),
                a.x1.max(b.x1),
                a.y1.max(b.y1),
            )),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let view_box = view_box.unwrap_or_else(|| Rect::new(0.0, 0.0, 100.0, 100.0));
        let mut out = String::new();

        out.push_str(r#"<svg xmlns="http://www.w3.org/2000/svg" "#);
        out.push_str(&format!(
            r#"viewBox="{} {} {} {}" width="{}" height="{}" preserveAspectRatio="xMinYMin meet">"#,
            view_box.x0,
            view_box.y0,
            view_box.width(),
            view_box.height(),
            view_box.width(),
            view_box.height()
        ));
        out.push('\n');

        let mut ids: Vec<_> = self.marks.keys().copied().collect();
        ids.sort_by_key(|id| {
            let (z, _payload) = self.marks.get(id).expect("id from keys");
            (*z, id.0)
        });

        for id in ids {
            let (_z, payload) = self.marks.get(&id).expect("id from keys");
            match payload {
                MarkPayload::Rect(r) => {
                    out.push_str(&format!(
                        r#"<rect x="{}" y="{}" width="{}" height="{}""#,
                        r.rect.x0,
                        r.rect.y0,
                        r.rect.width(),
                        r.rect.height(),
                    ));
                    write_paint_attr(&mut out, "fill", &r.fill);
                    out.push_str("/>\n");
                }
                MarkPayload::Text(t) => {
                    let baseline = match t.baseline {
                        TextBaseline::Middle => "middle",
                        TextBaseline::Alphabetic => "alphabetic",
                        TextBaseline::Hanging => "hanging",
                        TextBaseline::Ideographic => "ideographic",
                    };
                    out.push_str(&format!(
                        r#"<text x="{}" y="{}" font-size="{}" dominant-baseline="{}""#,
                        t.pos.x, t.pos.y, t.font_size, baseline
                    ));
                    if t.angle != 0.0 {
                        out.push_str(&format!(
                            r#" transform="rotate({} {} {})""#,
                            t.angle, t.pos.x, t.pos.y
                        ));
                    }
                    out.push_str(match t.anchor {
                        TextAnchor::Start => r#" text-anchor="start""#,
                        TextAnchor::Middle => r#" text-anchor="middle""#,
                        TextAnchor::End => r#" text-anchor="end""#,
                    });
                    write_paint_attr(&mut out, "fill", &t.fill);
                    out.push('>');
                    out.push_str(&escape_xml(&t.text));
                    out.push_str("</text>\n");
                }
                MarkPayload::Path(p) => {
                    let d = p.path.to_svg();
                    out.push_str(&format!(r#"<path d="{d}""#));
                    write_paint_attr(&mut out, "fill", &p.fill);
                    if p.stroke_width > 0.0 {
                        write_paint_attr(&mut out, "stroke", &p.stroke);
                        out.push_str(&format!(r#" stroke-width="{}""#, p.stroke_width));
                    }
                    out.push_str("/>\n");
                }
            }
        }

        out.push_str("</svg>\n");
        out
    }

    fn view_box(&self) -> Option<Rect> {
        let mut rect: Option<Rect> = None;
        for (_z, payload) in self.marks.values() {
            let b = match payload {
                MarkPayload::Text(t) => Some(estimate_text_bounds_anchored(
                    t.pos.x,
                    t.pos.y,
                    t.font_size,
                    t.anchor,
                    t.baseline,
                    &t.text,
                )),
                _ => payload.bounds(),
            }?;
            rect = Some(match rect {
                None => b,
                Some(r) => Rect::new(
                    r.x0.min(b.x0),
                    r.y0.min(b.y0),
                    r.x1.max(b.x1),
                    r.y1.max(b.y1),
                ),
            });
        }

        rect.map(|r| {
            // Add a small padding margin.
            let pad = 10.0;
            Rect::new(r.x0 - pad, r.y0 - pad, r.x1 + pad, r.y1 + pad)
        })
    }
}

fn estimate_text_bounds_anchored(
    x: f64,
    y: f64,
    font_size: f64,
    anchor: TextAnchor,
    baseline: TextBaseline,
    text: &str,
) -> Rect {
    // Very rough heuristic: assume ~0.6em average glyph width.
    //
    // `y` is interpreted according to the given baseline; we approximate a midline from it.
    let glyph_w = 0.6 * font_size;
    let width = glyph_w * text.chars().count() as f64;
    let half_height = 0.5 * font_size;
    let y_midline = match baseline {
        TextBaseline::Middle => y,
        // Approximate ascent/descent splits; this is only for demo SVG viewBox computation.
        TextBaseline::Alphabetic => y - 0.3 * font_size,
        TextBaseline::Hanging => y + 0.3 * font_size,
        TextBaseline::Ideographic => y - 0.2 * font_size,
    };
    let (x0, x1) = match anchor {
        TextAnchor::Start => (x, x + width),
        TextAnchor::Middle => (x - width / 2.0, x + width / 2.0),
        TextAnchor::End => (x - width, x),
    };
    Rect::new(x0, y_midline - half_height, x1, y_midline + half_height)
}

fn svg_paint(brush: &Brush) -> (String, Option<f64>) {
    match brush {
        Brush::Solid(color) => {
            let rgba = color.to_rgba8();
            let fill = format!("#{:02x}{:02x}{:02x}", rgba.r, rgba.g, rgba.b);
            let fill_opacity = if rgba.a == 255 {
                None
            } else {
                Some(f64::from(rgba.a) / 255.0)
            };
            (fill, fill_opacity)
        }
        _ => ("none".to_string(), None),
    }
}

fn write_paint_attr(out: &mut String, name: &str, brush: &Brush) {
    let (value, opacity) = svg_paint(brush);
    out.push_str(&format!(r#" {name}="{value}""#));
    if let Some(o) = opacity {
        out.push_str(&format!(r#" {name}-opacity="{o}""#));
    }
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}
