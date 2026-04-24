//! 2-D Iced canvas widget for the paper-space view.
//!
//! Replaces the 3-D shader widget used for the paper sheet so that paper-space
//! entities (title blocks, annotations, viewport borders) can be interacted
//! with directly — no need to enter MSPACE just to click on them.
//!
//! When MSPACE is active, a separate `PaperViewportPane` shader widget is
//! stacked on top of this canvas for the active viewport's 3-D content
//! (see `view.rs::paper_canvas_view()`).

use iced::widget::canvas;
use iced::{mouse, Color, Point, Rectangle};

use super::hatch_model::{HatchModel, HatchPattern};
use super::Scene;
use crate::app::Message;

// ── PaperCanvas ───────────────────────────────────────────────────────────────

pub struct PaperCanvas<'a> {
    pub scene: &'a Scene,
}

impl<'a> PaperCanvas<'a> {
    pub fn new(scene: &'a Scene) -> Self {
        Self { scene }
    }
}

// ── canvas::Program impl ──────────────────────────────────────────────────────

impl<'a> canvas::Program<Message> for PaperCanvas<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        // Update vp_size so viewport_screen_rect() knows the canvas dimensions
        // and can position the MSPACE shader overlay correctly.
        self.scene.selection.borrow_mut().vp_size = (bounds.width, bounds.height);

        let cam = self.scene.camera.borrow();
        let aspect = if bounds.height > 0.0 {
            bounds.width / bounds.height
        } else {
            1.0
        };
        let half_h = cam.ortho_size();
        let half_w = half_h * aspect;
        let tx = cam.target.x;
        let ty = cam.target.y;
        drop(cam);

        // Closure: paper-space world coords → canvas pixel coords.
        let to_px = move |wx: f32, wy: f32| Point {
            x: (wx - tx + half_w) / (2.0 * half_w) * bounds.width,
            y: (ty + half_h - wy) / (2.0 * half_h) * bounds.height,
        };

        let mut frame = canvas::Frame::new(renderer, bounds.size());

        // ── Desk background (area outside the paper sheet) ────────────────────
        const DESK: Color = Color { r: 0.22, g: 0.24, b: 0.28, a: 1.0 };
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), DESK);

        // ── White paper area — use layout paper limits (actual paper size).
        if let Some(((px0, py0), (px1, py1))) = self.scene.paper_limits().map(
            |((x0, y0), (x1, y1))| ((x0 as f32, y0 as f32), (x1 as f32, y1 as f32)),
        ) {
            let tl = to_px(px0, py1);
            let br = to_px(px1, py0);
            let pw = br.x - tl.x;
            let ph = br.y - tl.y;
            if pw > 0.0 && ph > 0.0 {
                let [r, g, b, a] = self.scene.paper_bg_color;
                frame.fill_rectangle(tl, iced::Size::new(pw, ph), Color { r, g, b, a });
            }
        }

        // ── Wipeout fills (rendered before wires, cover background) ──────────
        for hatch in &self.scene.paper_canvas_wipeouts() {
            draw_hatch(&mut frame, hatch, &to_px);
        }

        // ── Hatch fills ───────────────────────────────────────────────────────
        for hatch in &self.scene.paper_canvas_hatches() {
            draw_hatch(&mut frame, hatch, &to_px);
        }

        // px-per-world-unit scale for linetype dash lengths.
        let world_to_px_scale = if half_w > 0.0 {
            bounds.width / (2.0 * half_w)
        } else {
            1.0
        };

        // ── Wires (entity lines + inactive viewport projections) ──────────────
        for wire in &self.scene.paper_canvas_wires() {
            let [r, g, b, a] = wire.color;
            let color = Color { r, g, b, a };

            let path = canvas::Path::new(|b| {
                let mut started = false;
                for &[wx, wy, _] in &wire.points {
                    if wx.is_nan() || wy.is_nan() {
                        started = false;
                        continue;
                    }
                    let p = to_px(wx, wy);
                    if started {
                        b.line_to(p);
                    } else {
                        b.move_to(p);
                        started = true;
                    }
                }
            });

            // Convert WireModel linetype pattern (world units) to pixel lengths.
            // Keep the Vec alive for the duration of frame.stroke().
            let dash_segments: Vec<f32> = if wire.pattern_length > 0.0 {
                wire.pattern
                    .iter()
                    .take_while(|&&v| v != 0.0)
                    .map(|&v| v.abs() * world_to_px_scale)
                    .collect()
            } else {
                vec![]
            };

            frame.stroke(
                &path,
                canvas::Stroke {
                    style: canvas::Style::Solid(color),
                    width: wire.line_weight_px.max(1.0),
                    line_cap: canvas::LineCap::Square,
                    line_join: canvas::LineJoin::Miter,
                    line_dash: canvas::LineDash {
                        segments: &dash_segments,
                        offset: 0,
                    },
                },
            );
        }

        vec![frame.into_geometry()]
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Draw one HatchModel (solid, gradient, or pattern) onto `frame`.
/// Patterns and gradients are approximated as solid fills in 2-D canvas mode.
fn draw_hatch(
    frame: &mut canvas::Frame,
    hatch: &HatchModel,
    to_px: &impl Fn(f32, f32) -> Point,
) {
    if hatch.boundary.is_empty() {
        return;
    }

    let [r, g, b, a] = hatch.color;
    let color = Color { r, g, b, a };

    let path = canvas::Path::new(|builder| {
        let first = to_px(hatch.boundary[0][0], hatch.boundary[0][1]);
        builder.move_to(first);
        for &[x, y] in &hatch.boundary[1..] {
            builder.line_to(to_px(x, y));
        }
        builder.close();
    });

    match &hatch.pattern {
        HatchPattern::Solid => {
            frame.fill(&path, color);
        }
        HatchPattern::Pattern(_) => {
            // Pattern hatches: draw just the boundary outline for now.
            frame.stroke(
                &path,
                canvas::Stroke {
                    style: canvas::Style::Solid(color),
                    width: 1.0,
                    ..Default::default()
                },
            );
        }
        HatchPattern::Gradient { color2, .. } => {
            // Gradient: average the two colours as a solid fill.
            let avg = Color {
                r: (color.r + color2[0]) * 0.5,
                g: (color.g + color2[1]) * 0.5,
                b: (color.b + color2[2]) * 0.5,
                a: (color.a + color2[3]) * 0.5,
            };
            frame.fill(&path, avg);
        }
    }
}

