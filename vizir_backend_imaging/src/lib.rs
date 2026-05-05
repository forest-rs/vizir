// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `imaging` adapter for evaluated `vizir_core` marks.
//!
//! `vizir_backend_imaging` owns conversion from [`vizir_core::MarkDiff`] streams into
//! [`imaging`] drawing commands. It does not own chart construction, mark evaluation, text
//! shaping, or GPU/window lifecycle.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use hashbrown::HashMap;
use imaging::kurbo::Affine;
#[cfg(feature = "parley")]
use imaging::kurbo::Vec2;
use imaging::peniko::Fill;
#[cfg(feature = "parley")]
use imaging::peniko::{Brush, Style};
use imaging::{PaintSink, Painter, record};
#[cfg(feature = "parley")]
use parley::style::{FontFamily, FontStack, GenericFamily, StyleProperty};
#[cfg(feature = "parley")]
use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext};
use vizir_core::{MarkDiff, MarkId, MarkPayload, TextChannels};

/// Retained imaging-facing scene built from evaluated mark diffs.
#[derive(Debug, Default)]
pub struct ImagingScene {
    marks: HashMap<MarkId, MarkSnapshot>,
}

#[derive(Clone, Debug, PartialEq)]
struct MarkSnapshot {
    z_index: i32,
    payload: MarkPayload,
}

/// Text emission hook used by [`ImagingScene`] when painting text marks.
///
/// `vizir_core` text payloads are intentionally unshaped. Implement this trait in a downstream
/// layer that owns font selection and shaping, then emit shaped glyph runs into `sink`.
pub trait TextPainter {
    /// Paint one text payload into the supplied sink using the already-computed scene transform.
    fn paint_text(&mut self, sink: &mut dyn PaintSink, transform: Affine, text: &TextChannels);
}

/// Text painter that intentionally skips text output.
#[derive(Clone, Copy, Debug, Default)]
pub struct SkipText;

impl TextPainter for SkipText {
    fn paint_text(&mut self, _sink: &mut dyn PaintSink, _transform: Affine, _text: &TextChannels) {}
}

/// Text painter that shapes unstyled text with `parley` and emits imaging glyph runs.
///
/// This type is available with the `parley` feature, which enables system font discovery through
/// `parley` and keeps the default backend `no_std` dependency surface small.
#[cfg(feature = "parley")]
pub struct ParleyTextPainter {
    font_cx: FontContext,
    layout_cx: LayoutContext<()>,
}

#[cfg(feature = "parley")]
impl core::fmt::Debug for ParleyTextPainter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ParleyTextPainter").finish_non_exhaustive()
    }
}

#[cfg(feature = "parley")]
impl Default for ParleyTextPainter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "parley")]
impl ParleyTextPainter {
    /// Create a text painter with fresh `parley` font and layout contexts.
    #[must_use]
    pub fn new() -> Self {
        Self {
            font_cx: FontContext::new(),
            layout_cx: LayoutContext::new(),
        }
    }

    fn font_size_f32(font_size: f64) -> f32 {
        if !font_size.is_finite() {
            return 0.0;
        }
        let font_size = font_size.max(0.0);
        if font_size >= f64::from(f32::MAX) {
            f32::MAX
        } else {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "font size is clamped to f32::MAX before casting"
            )]
            {
                font_size as f32
            }
        }
    }
}

#[cfg(feature = "parley")]
impl TextPainter for ParleyTextPainter {
    fn paint_text(&mut self, sink: &mut dyn PaintSink, global: Affine, text: &TextChannels) {
        // Workaround for vello_hybrid 0.0.7: that renderer reserves alpha-zero solid paints for
        // clipping and asserts when a glyph draw uses one. Subsequent vello_hybrid versions do not
        // need this guard.
        if solid_brush_alpha_is_zero(&text.fill) {
            return;
        }

        if text.text.is_empty() {
            return;
        }

        let text_content = text.text.split('\n').next().unwrap_or("");
        if text_content.is_empty() {
            return;
        }

        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text_content, 1.0, true);
        builder.push_default(StyleProperty::FontSize(Self::font_size_f32(text.font_size)));
        builder.push_default(StyleProperty::FontStack(FontStack::from(
            FontFamily::Generic(GenericFamily::SansSerif),
        )));

        let mut layout: parley::Layout<()> = builder.build(text_content);
        layout.break_all_lines(None);
        layout.align(None, Alignment::Start, AlignmentOptions::default());

        let Some(line) = layout.lines().next() else {
            return;
        };

        let metrics = line.metrics();
        let width = f64::from(metrics.advance);
        let ascent = f64::from(metrics.ascent);
        let descent = f64::from(metrics.descent);
        let leading = f64::from(metrics.leading);
        let baseline_offset = f64::from(metrics.baseline);
        let height = ascent + descent + leading;

        let ref_x = match text.anchor {
            vizir_core::TextAnchor::Start => 0.0,
            vizir_core::TextAnchor::Middle => 0.5 * width,
            vizir_core::TextAnchor::End => width,
        };

        let top = baseline_offset - ascent;
        let ref_y = match text.baseline {
            vizir_core::TextBaseline::Alphabetic | vizir_core::TextBaseline::Ideographic => {
                baseline_offset
            }
            vizir_core::TextBaseline::Hanging => top,
            vizir_core::TextBaseline::Middle => top + 0.5 * height,
        };

        let transform = global
            * (Affine::translate(Vec2::new(text.pos.x, text.pos.y))
                * Affine::rotate(text.angle.to_radians())
                * Affine::translate(Vec2::new(-ref_x, -ref_y)));
        let fill_style = Style::Fill(Fill::NonZero);

        for item in line.items() {
            let parley::PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let font = run.run().font();
            let glyphs: Vec<_> = run
                .positioned_glyphs()
                .map(|glyph| record::Glyph {
                    id: glyph.id,
                    x: glyph.x,
                    y: glyph.y,
                })
                .collect();

            Painter::new(sink)
                .glyphs(font, &text.fill)
                .transform(transform)
                .font_size(run.run().font_size())
                .hint(true)
                .draw(&fill_style, &glyphs);
        }
    }
}

#[cfg(feature = "parley")]
fn solid_brush_alpha_is_zero(brush: &Brush) -> bool {
    matches!(brush, Brush::Solid(color) if color.components[3] == 0.0)
}

impl ImagingScene {
    /// Create an empty imaging scene.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Remove all retained marks.
    pub fn clear(&mut self) {
        self.marks.clear();
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

    /// Paint retained marks into an arbitrary imaging sink, skipping text marks.
    pub fn paint(&self, sink: &mut dyn PaintSink) {
        let mut text = SkipText;
        self.paint_transformed_with_text(sink, Affine::IDENTITY, &mut text);
    }

    /// Paint retained marks into an arbitrary imaging sink with a transform, skipping text marks.
    pub fn paint_transformed(&self, sink: &mut dyn PaintSink, transform: Affine) {
        let mut text = SkipText;
        self.paint_transformed_with_text(sink, transform, &mut text);
    }

    /// Paint retained marks into an arbitrary imaging sink using a caller-provided text painter.
    pub fn paint_with_text(&self, sink: &mut dyn PaintSink, text: &mut impl TextPainter) {
        self.paint_transformed_with_text(sink, Affine::IDENTITY, text);
    }

    /// Paint retained marks into an arbitrary imaging sink with a transform and text painter.
    pub fn paint_transformed_with_text(
        &self,
        sink: &mut dyn PaintSink,
        transform: Affine,
        text: &mut impl TextPainter,
    ) {
        for id in self.sorted_mark_ids() {
            let snapshot = self.marks.get(&id).expect("id from keys");
            paint_payload(sink, transform, text, &snapshot.payload);
        }
    }

    /// Record retained marks into an owned imaging scene, skipping text marks.
    #[must_use]
    pub fn to_recording(&self) -> record::Scene {
        self.to_recording_transformed(Affine::IDENTITY)
    }

    /// Record retained marks into an owned imaging scene with a transform, skipping text marks.
    #[must_use]
    pub fn to_recording_transformed(&self, transform: Affine) -> record::Scene {
        let mut scene = record::Scene::new();
        self.paint_transformed(&mut scene, transform);
        scene
    }

    /// Record retained marks into an owned imaging scene with a caller-provided text painter.
    #[must_use]
    pub fn to_recording_with_text(&self, text: &mut impl TextPainter) -> record::Scene {
        self.to_recording_transformed_with_text(Affine::IDENTITY, text)
    }

    /// Record retained marks into an owned imaging scene with a transform and text painter.
    #[must_use]
    pub fn to_recording_transformed_with_text(
        &self,
        transform: Affine,
        text: &mut impl TextPainter,
    ) -> record::Scene {
        let mut scene = record::Scene::new();
        self.paint_transformed_with_text(&mut scene, transform, text);
        scene
    }

    fn sorted_mark_ids(&self) -> Vec<MarkId> {
        let mut ids: Vec<_> = self.marks.keys().copied().collect();
        ids.sort_by_key(|id| {
            let snapshot = self.marks.get(id).expect("id from keys");
            (snapshot.z_index, id.0)
        });
        ids
    }
}

fn paint_payload(
    sink: &mut dyn PaintSink,
    transform: Affine,
    text: &mut impl TextPainter,
    payload: &MarkPayload,
) {
    match payload {
        MarkPayload::Rect(rect) => {
            Painter::new(sink)
                .fill(rect.rect, &rect.fill)
                .transform(transform)
                .draw();
        }
        MarkPayload::Path(path) => {
            if let Some(fill) = &path.fill {
                Painter::new(sink)
                    .fill(&path.path, fill)
                    .transform(transform)
                    .fill_rule(Fill::NonZero)
                    .draw();
            }
            if let Some(stroke) = &path.stroke {
                Painter::new(sink)
                    .stroke(&path.path, &stroke.style, &stroke.brush)
                    .transform(transform)
                    .draw();
            }
        }
        MarkPayload::Text(channels) => {
            text.paint_text(sink, transform, channels);
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use imaging::kurbo::{BezPath, Point, Rect, Stroke};
    use imaging::peniko::Color;
    use imaging::record::{Command, Draw, Geometry};
    use vizir_core::{PathChannels, PathStroke, RectChannels, TextChannels};

    use super::*;

    #[test]
    fn records_marks_in_stable_z_order() {
        let mut scene = ImagingScene::new();
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

        let recording = scene.to_recording();
        assert_eq!(recording.commands().len(), 2);
        let first = draw_for_command(&recording, 0);
        assert!(matches!(
            first,
            Draw::Fill {
                shape: Geometry::Rect(rect),
                ..
            } if *rect == Rect::new(10.0, 0.0, 20.0, 10.0)
        ));
    }

    #[test]
    fn records_path_fill_and_stroke() {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((1.0, 1.0));

        let mut scene = ImagingScene::new();
        scene.insert(
            MarkId(1),
            0,
            MarkPayload::Path(PathChannels {
                path,
                fill: Some(Color::from_rgb8(1, 2, 3).into()),
                stroke: Some(PathStroke {
                    brush: Color::BLACK.into(),
                    style: Stroke::new(2.0),
                }),
            }),
        );

        let recording = scene.to_recording();
        assert_eq!(recording.commands().len(), 2);
        assert!(matches!(draw_for_command(&recording, 0), Draw::Fill { .. }));
        assert!(matches!(
            draw_for_command(&recording, 1),
            Draw::Stroke { .. }
        ));
    }

    #[test]
    fn delegates_text_to_text_painter() {
        #[derive(Default)]
        struct CountingText {
            calls: usize,
        }

        impl TextPainter for CountingText {
            fn paint_text(
                &mut self,
                _sink: &mut dyn PaintSink,
                _transform: Affine,
                _text: &TextChannels,
            ) {
                self.calls += 1;
            }
        }

        let mut scene = ImagingScene::new();
        scene.insert(
            MarkId(1),
            0,
            MarkPayload::Text(TextChannels {
                pos: Point::new(0.0, 0.0),
                text: "hello".into(),
                fill: Color::BLACK.into(),
                ..TextChannels::default()
            }),
        );

        let mut recording = record::Scene::new();
        let mut text = CountingText::default();
        scene.paint_with_text(&mut recording, &mut text);
        assert_eq!(text.calls, 1);
    }

    #[cfg(feature = "parley")]
    #[test]
    fn parley_text_painter_skips_solid_zero_alpha_text() {
        let mut text_painter = ParleyTextPainter::new();
        let mut scene = record::Scene::new();
        let text = TextChannels {
            pos: Point::new(0.0, 0.0),
            text: "hidden text".into(),
            font_size: 16.0,
            fill: Color::TRANSPARENT.into(),
            ..TextChannels::default()
        };

        text_painter.paint_text(&mut scene, Affine::IDENTITY, &text);

        assert!(scene.commands().is_empty());
    }

    fn draw_for_command(scene: &record::Scene, command_index: usize) -> &Draw {
        let Command::Draw(id) = scene.commands()[command_index] else {
            panic!("expected draw command");
        };
        scene.draw_op(id)
    }
}
