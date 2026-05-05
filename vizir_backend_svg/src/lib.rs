// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! SVG rendering adapter for evaluated `vizir_core` marks.
//!
//! `vizir_backend_svg` owns conversion from [`vizir_core::MarkDiff`] streams into an SVG
//! document. It does not own chart construction, mark evaluation, or browser integration.

#![no_std]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use hashbrown::HashMap;
use kurbo::{Cap, Join, PathEl, Rect};
use peniko::Brush;
use vizir_core::{MarkDiff, MarkId, MarkPayload, PathStroke, TextAnchor, TextBaseline};

/// Retained SVG-facing scene built from evaluated mark diffs.
#[derive(Debug, Default)]
pub struct SvgScene {
    marks: HashMap<MarkId, MarkSnapshot>,
    view_box: Option<Rect>,
}

#[derive(Clone, Debug, PartialEq)]
struct MarkSnapshot {
    z_index: i32,
    payload: MarkPayload,
}

impl SvgScene {
    /// Create an empty SVG scene.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Remove all retained marks and any explicit view box.
    pub fn clear(&mut self) {
        self.marks.clear();
        self.view_box = None;
    }

    /// Return the number of retained marks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.marks.len()
    }

    /// Return `true` when there are no retained marks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }

    /// Set an explicit view box that is unioned with computed mark bounds during serialization.
    pub fn set_view_box(&mut self, view_box: Rect) {
        self.view_box = Some(view_box);
    }

    /// Apply mark diffs to the retained scene.
    pub fn apply_diffs(&mut self, diffs: &[MarkDiff]) {
        for diff in diffs {
            match diff {
                MarkDiff::Enter {
                    id, z_index, new, ..
                } => {
                    self.insert(*id, *z_index, (**new).clone());
                }
                MarkDiff::Update {
                    id,
                    new_z_index,
                    new,
                    ..
                } => {
                    self.insert(*id, *new_z_index, (**new).clone());
                }
                MarkDiff::Exit { id, .. } => {
                    self.marks.remove(id);
                }
            }
        }
    }

    /// Insert or replace one evaluated mark payload.
    pub fn insert(&mut self, id: MarkId, z_index: i32, payload: MarkPayload) {
        self.marks.insert(id, MarkSnapshot { z_index, payload });
    }

    /// Serialize the scene as a complete SVG document string.
    #[must_use]
    pub fn to_svg_string(&self) -> String {
        let view_box = self.resolved_view_box();
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

        for id in self.sorted_mark_ids() {
            let snapshot = self.marks.get(&id).expect("id from keys");
            write_payload(&mut out, &snapshot.payload);
        }

        out.push_str("</svg>\n");
        out
    }

    /// Return the explicit view box unioned with computed mark bounds.
    #[must_use]
    pub fn resolved_view_box(&self) -> Rect {
        let computed = self.computed_view_box();
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
        view_box.unwrap_or_else(|| Rect::new(0.0, 0.0, 100.0, 100.0))
    }

    fn sorted_mark_ids(&self) -> Vec<MarkId> {
        let mut ids: Vec<_> = self.marks.keys().copied().collect();
        ids.sort_by_key(|id| {
            let snapshot = self.marks.get(id).expect("id from keys");
            (snapshot.z_index, id.0)
        });
        ids
    }

    fn computed_view_box(&self) -> Option<Rect> {
        let mut rect: Option<Rect> = None;
        for snapshot in self.marks.values() {
            let b = match &snapshot.payload {
                MarkPayload::Text(t) => Some(estimate_text_bounds_anchored(
                    t.pos.x,
                    t.pos.y,
                    t.font_size,
                    t.anchor,
                    t.baseline,
                    &t.text,
                )),
                _ => snapshot.payload.bounds(),
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
            let pad = 10.0;
            Rect::new(r.x0 - pad, r.y0 - pad, r.x1 + pad, r.y1 + pad)
        })
    }
}

fn write_payload(out: &mut String, payload: &MarkPayload) {
    match payload {
        MarkPayload::Rect(r) => {
            out.push_str(&format!(
                r#"<rect x="{}" y="{}" width="{}" height="{}""#,
                r.rect.x0,
                r.rect.y0,
                r.rect.width(),
                r.rect.height(),
            ));
            write_paint_attr(out, "fill", &r.fill);
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
            write_paint_attr(out, "fill", &t.fill);
            out.push('>');
            out.push_str(&escape_xml(&t.text));
            out.push_str("</text>\n");
        }
        MarkPayload::Path(p) => {
            let d = path_to_svg(&p.path);
            out.push_str(&format!(r#"<path d="{d}""#));
            write_optional_paint_attr(out, "fill", p.fill.as_ref());
            if let Some(stroke) = &p.stroke {
                write_stroke_attrs(out, stroke);
            }
            out.push_str("/>\n");
        }
    }
}

fn path_to_svg(path: &kurbo::BezPath) -> String {
    let mut out = String::new();
    for (index, element) in path.elements().iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        match element {
            PathEl::MoveTo(p) => out.push_str(&format!("M{},{}", p.x, p.y)),
            PathEl::LineTo(p) => out.push_str(&format!("L{},{}", p.x, p.y)),
            PathEl::QuadTo(p1, p2) => {
                out.push_str(&format!("Q{},{} {},{}", p1.x, p1.y, p2.x, p2.y));
            }
            PathEl::CurveTo(p1, p2, p3) => {
                out.push_str(&format!(
                    "C{},{} {},{} {},{}",
                    p1.x, p1.y, p2.x, p2.y, p3.x, p3.y
                ));
            }
            PathEl::ClosePath => out.push('Z'),
        }
    }
    out
}

fn estimate_text_bounds_anchored(
    x: f64,
    y: f64,
    font_size: f64,
    anchor: TextAnchor,
    baseline: TextBaseline,
    text: &str,
) -> Rect {
    let glyph_w = 0.6 * font_size;
    let width = glyph_w * text.chars().count() as f64;
    let half_height = 0.5 * font_size;
    let y_midline = match baseline {
        TextBaseline::Middle => y,
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

fn write_optional_paint_attr(out: &mut String, name: &str, brush: Option<&Brush>) {
    match brush {
        Some(brush) => write_paint_attr(out, name, brush),
        None => out.push_str(&format!(r#" {name}="none""#)),
    }
}

fn write_stroke_attrs(out: &mut String, stroke: &PathStroke) {
    write_paint_attr(out, "stroke", &stroke.brush);
    out.push_str(&format!(r#" stroke-width="{}""#, stroke.style.width));
    out.push_str(match (stroke.style.start_cap, stroke.style.end_cap) {
        (Cap::Butt, Cap::Butt) => r#" stroke-linecap="butt""#,
        (Cap::Square, Cap::Square) => r#" stroke-linecap="square""#,
        (Cap::Round, Cap::Round) => r#" stroke-linecap="round""#,
        _ => "",
    });
    out.push_str(match stroke.style.join {
        Join::Bevel => r#" stroke-linejoin="bevel""#,
        Join::Miter => r#" stroke-linejoin="miter""#,
        Join::Round => r#" stroke-linejoin="round""#,
    });
    out.push_str(&format!(
        r#" stroke-miterlimit="{}""#,
        stroke.style.miter_limit
    ));
    if !stroke.style.dash_pattern.is_empty() {
        out.push_str(r#" stroke-dasharray=""#);
        for (index, dash) in stroke.style.dash_pattern.iter().enumerate() {
            if index > 0 {
                out.push(' ');
            }
            out.push_str(&dash.to_string());
        }
        out.push('"');
    }
    if stroke.style.dash_offset != 0.0 {
        out.push_str(&format!(
            r#" stroke-dashoffset="{}""#,
            stroke.style.dash_offset
        ));
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

#[cfg(test)]
mod tests {
    extern crate std;

    use kurbo::{BezPath, Point, Rect, Stroke};
    use peniko::Color;
    use vizir_core::{PathChannels, PathStroke, RectChannels, TextChannels};

    use super::*;

    #[test]
    fn serializes_in_stable_z_order() {
        let mut scene = SvgScene::new();
        scene.insert(
            MarkId(2),
            1,
            MarkPayload::Rect(RectChannels {
                rect: Rect::new(20.0, 0.0, 30.0, 10.0),
                fill: Color::from_rgb8(0, 0, 2).into(),
            }),
        );
        scene.insert(
            MarkId(1),
            1,
            MarkPayload::Rect(RectChannels {
                rect: Rect::new(10.0, 0.0, 20.0, 10.0),
                fill: Color::from_rgb8(0, 0, 1).into(),
            }),
        );

        let svg = scene.to_svg_string();
        let first = svg.find(r##"fill="#000001""##).expect("first mark");
        let second = svg.find(r##"fill="#000002""##).expect("second mark");
        assert!(first < second, "marks should sort by z-index then MarkId");
    }

    #[test]
    fn escapes_text_content() {
        let mut scene = SvgScene::new();
        scene.insert(
            MarkId(1),
            0,
            MarkPayload::Text(TextChannels {
                pos: Point::new(0.0, 0.0),
                text: "<A&B>".into(),
                fill: Color::BLACK.into(),
                ..TextChannels::default()
            }),
        );

        assert!(scene.to_svg_string().contains("&lt;A&amp;B&gt;"));
    }

    #[test]
    fn serializes_path_stroke_when_present() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((1.0, 1.0));

        let mut scene = SvgScene::new();
        scene.insert(
            MarkId(1),
            0,
            MarkPayload::Path(PathChannels {
                path,
                fill: None,
                stroke: Some(PathStroke {
                    brush: Color::BLACK.into(),
                    style: Stroke::new(2.0),
                }),
            }),
        );

        let svg = scene.to_svg_string();
        assert!(svg.contains(r#"fill="none""#));
        assert!(svg.contains(r##"stroke="#000000""##));
        assert!(svg.contains(r#"stroke-width="2""#));
    }
}
