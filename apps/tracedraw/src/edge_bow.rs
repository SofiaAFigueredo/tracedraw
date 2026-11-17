//! The Shape tool's edge bow handles (#2978): a small arc-shaped handle
//! just outside the middle of each edge of a selected rectangle, found
//! more easily than the bare edge itself. Dragging one bows that edge
//! outward, the same way dragging the edge does (#2188); every edge
//! bows by the same amount, the one dragged alone with Ctrl. The handle
//! stays where it is drawn, offset from the edge by a fixed number of
//! screen pixels, while the drag itself follows the pointer's move from
//! where it began, so picking the handle up never jumps the edge to
//! match that offset.

use crate::app::{App, Drag};
use crate::tools::Tool;
use tracedraw_core::document::Corners;
use tracedraw_core::geometry::{Point, Vec2};
use tracedraw_core::{Command, Shape, ShapeId, ShapeKind};

/// How far a handle sits from the edge it bows, screen pixels: just
/// outside the shape, so it does not read as the edge itself.
pub const OFFSET_PX: f32 = 10.0;
/// The handle's size, screen pixels.
pub const RADIUS_PX: f32 = 5.0;
const PICK_PX: f32 = 8.0;

/// One edge bow handle on the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Handle {
    pub shape: ShapeId,
    pub edge: usize,
    pub at: egui::Pos2,
    /// The outward direction the handle's arc bulges in, screen space.
    pub out: egui::Vec2,
}

/// Dragging an edge bow handle: the rectangle as it was when the drag
/// began, the edge grabbed, the page point the drag began at, and
/// whether the first step has been recorded yet.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeBowDrag {
    pub shape: ShapeId,
    pub start: Box<Shape>,
    pub edge: usize,
    pub from: Point,
    pub begun: bool,
}

impl App {
    /// The edge bow handles to show now, in screen space: one just
    /// outside the middle of each edge of the rectangles the Shape tool
    /// edits, none while one is dragged.
    pub fn edge_bow_handles(&self) -> Vec<Handle> {
        if self.tool != Tool::Shape || matches!(self.drag, Drag::EdgeBow(_)) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for s in self.shape_tool_shapes() {
            if s.spline.is_some() || !matches!(s.kind, ShapeKind::Rect { .. }) {
                continue;
            }
            let Some((rect, c)) = s.kind.rect_corners() else {
                continue;
            };
            let rect = rect.abs();
            for edge in 0..4 {
                let (mid, out_dir) = c.edge_middle(edge, rect);
                let node = self.view.to_screen(s.transform * mid);
                let toward = self.view.to_screen(s.transform * (mid + out_dir));
                let d = toward - node;
                if d.length() < 1e-6 || !d.x.is_finite() || !d.y.is_finite() {
                    continue;
                }
                let d = d.normalized();
                out.push(Handle {
                    shape: s.id,
                    edge,
                    at: node + d * OFFSET_PX,
                    out: d,
                });
            }
        }
        out
    }

    /// The edge bow handle under the page point `p`.
    pub fn edge_bow_handle_at(&self, p: Point) -> Option<Handle> {
        let s = self.view.to_screen(p);
        self.edge_bow_handles()
            .into_iter()
            .map(|w| (w, (w.at - s).length()))
            .filter(|(_, d)| *d <= PICK_PX)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(w, _)| w)
    }

    /// Begin dragging the edge bow handle `w` from the page point `p`.
    /// `false` when the shape it names is gone.
    pub fn start_edge_bow_drag(&mut self, w: Handle, p: Point) -> bool {
        let Some(s) = self.doc().find_shape(w.shape) else {
            return false;
        };
        if !matches!(s.kind, ShapeKind::Rect { .. }) {
            return false;
        }
        self.drag = Drag::EdgeBow(Box::new(EdgeBowDrag {
            shape: w.shape,
            start: Box::new(s.clone()),
            edge: w.edge,
            from: p,
            begun: false,
        }));
        true
    }

    /// The edge bow handle dragged to the page point `p`: the edge
    /// grabbed bows to follow the pointer's move since the drag began
    /// (local space), every edge together unless `single` (Ctrl) keeps
    /// it alone. One undo step for the whole drag.
    pub fn edge_bow_drag_to(&mut self, mut d: EdgeBowDrag, p: Point, single: bool) {
        let det = d.start.transform.determinant();
        if !det.is_finite() || det.abs() <= 1e-12 {
            return;
        }
        let inv = d.start.transform.inverse();
        let delta = (inv * p) - (inv * d.from);
        let Some(kind) = bowed_kind(&d.start.kind, d.edge, delta, single) else {
            return;
        };
        let cmd = Command::SetShapeKind {
            shape: d.shape,
            kind,
        };
        let _ = if d.begun {
            self.engine.amend(&cmd)
        } else {
            self.engine.run_with_label(&cmd, "Edge Bulge")
        };
        d.begun = true;
        self.drag = Drag::EdgeBow(Box::new(d));
    }
}

/// The rectangle's kind after edge `edge` bows to follow a move of `d`
/// (local space) from where it was when the drag began (`start`, the
/// rectangle's kind at that time): every edge bows by the same amount,
/// the one dragged alone with `single` (#2188, #2978).
fn bowed_kind(start: &ShapeKind, edge: usize, d: Vec2, single: bool) -> Option<ShapeKind> {
    let (rect, mut c) = start.rect_corners()?;
    let rect = rect.abs();
    let straight = Corners {
        bulge: [0.0; 4],
        ..c
    };
    let (base, out_dir) = straight.edge_middle(edge, rect);
    let (now, _) = c.edge_middle(edge, rect);
    let start_out = (now - base).dot(out_dir);
    let value = Corners::bulge_for(edge, rect, start_out + d.dot(out_dir));
    if single {
        c.bulge[edge] = value;
    } else {
        let delta = value - c.edge_bulge(edge);
        let was = c;
        for (i, b) in c.bulge.iter_mut().enumerate() {
            *b = (was.edge_bulge(i) + delta).clamp(0.0, 1.0);
        }
        c.bulge[edge] = value;
    }
    Some(ShapeKind::rect_with_corners(rect, c))
}

/// Draw the edge bow handles: a small arc, its convex side pointing
/// outward, where dragging would bow that edge.
pub fn draw(app: &App, painter: &egui::Painter, color: egui::Color32) {
    for w in app.edge_bow_handles() {
        let perp = egui::vec2(-w.out.y, w.out.x);
        let mut pts = Vec::with_capacity(9);
        for i in 0..=8 {
            let t = -1.0 + 2.0 * i as f32 / 8.0;
            let bulge = (1.0 - t * t).max(0.0).sqrt();
            pts.push(w.at + perp * (RADIUS_PX * t) + w.out * (RADIUS_PX * bulge * 0.9));
        }
        painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5, color)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracedraw_core::geometry::Rect;

    fn rect(corners: Corners) -> Shape {
        Shape::new(
            ShapeId(1),
            ShapeKind::rect_with_corners(Rect::new(0.0, 0.0, 40.0, 20.0), corners),
        )
    }

    /// Dragging the handle bows every edge by the same amount; with Ctrl
    /// only the edge dragged (#2188, #2978). The handle sits where the
    /// drag began, so the first step of a drag follows the pointer's
    /// move from there rather than jumping to an absolute position; a
    /// drag back inside straightens the edge again.
    #[test]
    fn dragging_the_handle_bows_the_edges() {
        let s = rect(Corners::default());
        // Top edge 40 long: a quarter circle lifts its middle by
        // (sqrt 2 - 1) * 20.
        let full = (std::f64::consts::SQRT_2 - 1.0) * 20.0;
        let d = Vec2::new(0.0, full / 2.0);
        let kind = bowed_kind(&s.kind, Corners::EDGE_TOP, d, false).unwrap();
        let (_, c) = kind.rect_corners().unwrap();
        assert!(c.bulge.iter().all(|b| (b - 0.5).abs() < 1e-9), "{c:?}");
        let kind = bowed_kind(&s.kind, Corners::EDGE_TOP, d, true).unwrap();
        let (_, c) = kind.rect_corners().unwrap();
        assert!((c.bulge[Corners::EDGE_TOP] - 0.5).abs() < 1e-9);
        assert_eq!(c.bulge[Corners::EDGE_BOTTOM], 0.0);
        let bowed = Shape::new(ShapeId(1), kind);
        // No further move: the edge stays exactly as it was (no jump).
        let kind = bowed_kind(&bowed.kind, Corners::EDGE_TOP, Vec2::new(0.0, 0.0), true).unwrap();
        let (_, c) = kind.rect_corners().unwrap();
        assert!((c.bulge[Corners::EDGE_TOP] - 0.5).abs() < 1e-9);
        // A move back past the straight edge straightens it.
        let kind = bowed_kind(&bowed.kind, Corners::EDGE_TOP, Vec2::new(0.0, -5.0), true).unwrap();
        let (_, c) = kind.rect_corners().unwrap();
        assert_eq!(c.bulge[Corners::EDGE_TOP], 0.0);
    }
}
