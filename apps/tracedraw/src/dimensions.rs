//! Dimension lines. A dimension is a group of its lines, its arrowheads
//! and its label, with what it measures and its style in the group's
//! object data (`dimension`), so it can be drawn again: when its style
//! changes, and when it is moved, scaled, turned or skewed, which bakes
//! the transform into the measured points so the value follows the new
//! size (dynamic dimensioning) and the label stays upright text.
//!
//! Linear dimensions (Parallel, Horizontal or Vertical, Segment) measure
//! the distance between two points along their line; angular ones the
//! angle at a centre between two points. The label is the value in the
//! style's units through the drawing scale (degrees for angles), with the
//! chosen decimals, the units' sign when shown, a prefix and a suffix; it
//! sits above the dimension line, in a gap in it, or below; a linear
//! one's runs along its line or stays horizontal.
//!
//! The label can be edited in place like any text (#644): typed text that
//! still holds the measured value keeps it live, what is around it
//! becoming the prefix and suffix; text without it stays as typed. Its
//! font, size and colour become the style's. Break Apart frees the lines,
//! the arrowheads and the label as objects of their own.

use crate::app::{App, Units};
use crate::i18n::tr;
use serde::{Deserialize, Serialize};
use tracedraw_core::{
    document::{Shape, ShapeKind, TextAlign, TextSpan},
    geometry::{Affine, BezPath, Point, Rect, Shape as _, Vec2},
    Color, Command, Fill, ShapeId, Stroke,
};

/// The object data key holding a dimension.
pub const DATA_KEY: &str = "dimension";

/// Where the label sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextPosition {
    #[default]
    Above,
    Centered,
    Below,
}

impl TextPosition {
    pub const ALL: [TextPosition; 3] = [
        TextPosition::Above,
        TextPosition::Centered,
        TextPosition::Below,
    ];

    pub fn key(self) -> &'static str {
        match self {
            TextPosition::Above => "dimension.text_above",
            TextPosition::Centered => "dimension.text_centered",
            TextPosition::Below => "dimension.text_below",
        }
    }
}

/// The marks at the ends of the dimension line (#475).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArrowStyle {
    /// A filled triangle.
    #[default]
    Filled,
    /// Two strokes, open.
    Open,
    /// A filled dot.
    Dot,
    /// An architect's tick across the line.
    Tick,
    /// Nothing.
    None,
}

impl ArrowStyle {
    pub const ALL: [ArrowStyle; 5] = [
        ArrowStyle::Filled,
        ArrowStyle::Open,
        ArrowStyle::Dot,
        ArrowStyle::Tick,
        ArrowStyle::None,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ArrowStyle::Filled => "dimension.arrow_filled",
            ArrowStyle::Open => "dimension.arrow_open",
            ArrowStyle::Dot => "dimension.arrow_dot",
            ArrowStyle::Tick => "dimension.arrow_tick",
            ArrowStyle::None => "dimension.arrow_none",
        }
    }
}

/// How a dimension's label reads and sits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DimensionStyle {
    /// Decimals, 0 to 6.
    pub precision: u8,
    /// A unit id (`Units::id`); empty for the drawing's units.
    pub units: String,
    pub show_units: bool,
    pub prefix: String,
    pub suffix: String,
    pub text_position: TextPosition,
    /// Label size, points.
    pub text_size: f64,
    /// The label's font; empty for the font of new text (as before the
    /// style had one).
    pub font: String,
    /// The colour of the lines and arrowheads, and of the label (#475).
    pub line_color: Color,
    pub text_color: Color,
    /// The lines' width, mm; 0 for a hairline.
    pub line_width: f64,
    pub arrows: ArrowStyle,
    /// The lines' ends and corners rounded.
    pub round_ends: bool,
    /// How far the extension lines stay from the measured points, and
    /// how far they run past the dimension line, mm (#644).
    pub extension_gap: f64,
    pub extension_overshoot: f64,
    /// The arrowheads' length, mm.
    pub arrow_size: f64,
    /// A linear dimension's label stays horizontal instead of running
    /// along its line.
    pub horizontal_text: bool,
    /// The extension lines meet the dimension line (or arc) in a corner
    /// instead of running past it, rounded by the corner radius, mm
    /// (#641).
    pub joined: bool,
    pub corner_radius: f64,
}

impl Default for DimensionStyle {
    fn default() -> Self {
        DimensionStyle {
            precision: 2,
            units: String::new(),
            show_units: true,
            prefix: String::new(),
            suffix: String::new(),
            text_position: TextPosition::Above,
            text_size: 10.0,
            font: String::new(),
            line_color: Color::BLACK,
            text_color: Color::BLACK,
            line_width: 0.0,
            arrows: ArrowStyle::Filled,
            round_ends: false,
            extension_gap: 0.0,
            extension_overshoot: OVERSHOOT,
            arrow_size: ARROW_LEN,
            horizontal_text: false,
            joined: false,
            corner_radius: 0.0,
        }
    }
}

impl DimensionStyle {
    /// The corner radius of joined lines, kept finite and in range.
    fn radius(&self) -> f64 {
        if self.corner_radius.is_finite() {
            self.corner_radius.clamp(0.0, 100.0)
        } else {
            0.0
        }
    }

    /// The extension gap and overshoot and the arrowheads' length, kept
    /// finite and in range.
    fn lengths(&self) -> (f64, f64, f64) {
        let ok = |v: f64, d: f64, max: f64| if v.is_finite() { v.clamp(0.0, max) } else { d };
        (
            ok(self.extension_gap, 0.0, 100.0),
            ok(self.extension_overshoot, OVERSHOOT, 100.0),
            ok(self.arrow_size, ARROW_LEN, 50.0),
        )
    }
}

/// What a dimension measures, in its group's space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Measure {
    /// From `a` to `b`, the dimension line through `at`.
    Linear { a: Point, b: Point, at: Point },
    /// The angle at `center` from `a` to `b` (counterclockwise), the arc
    /// through `at`.
    Angular {
        center: Point,
        a: Point,
        b: Point,
        at: Point,
    },
}

/// A dimension: what it measures and how it reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dimension {
    pub measure: Measure,
    #[serde(default)]
    pub style: DimensionStyle,
    /// A label typed in place that no longer holds the measured value: it
    /// shows as typed (#644).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// How the label was last drawn, to tell an edit made to it in place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawn: Option<DrawnLabel>,
}

/// A dimension's label as it was drawn: its text, the measured value as it
/// read in it, and its font.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawnLabel {
    pub text: String,
    pub value: String,
    pub font: String,
}

/// Where `value` stands in `text` as a whole number (no digit or decimal
/// sign right before it, no digit right after): its byte offset.
fn find_value(text: &str, value: &str) -> Option<usize> {
    if value.is_empty() {
        return None;
    }
    text.match_indices(value).map(|(i, _)| i).find(|&i| {
        let before = text[..i].chars().next_back();
        let after = text[i + value.len()..].chars().next();
        !before.is_some_and(|c| c.is_ascii_digit() || c == '.' || c == ',')
            && !after.is_some_and(|c| c.is_ascii_digit())
    })
}

/// Arrowheads: length and half width, mm.
const ARROW_LEN: f64 = 3.0;
const ARROW_HALF: f64 = 1.0;
/// Gap between the label and its line, and how far extension lines run
/// past the dimension line, mm.
const GAP: f64 = 1.0;
const OVERSHOOT: f64 = 2.0;

/// The magic number that makes a cubic a quarter circle.
const KAPPA: f64 = 0.552_284_749_8;

/// A corner from `p_in` through `corner` to `p_out`, rounded: a cubic
/// that leaves and meets the legs along them.
fn round_corner(path: &mut BezPath, p_in: Point, corner: Point, p_out: Point) {
    path.line_to(p_in);
    path.curve_to(
        p_in + (corner - p_in) * KAPPA,
        p_out + (corner - p_out) * KAPPA,
        p_out,
    );
}

/// The straight polyline `pts` with its inner corners rounded by
/// `radius` (each cut back at most half of an inner leg, the whole of an
/// end leg).
fn rounded_polyline(path: &mut BezPath, pts: &[Point], radius: f64) {
    let Some((&first, rest)) = pts.split_first() else {
        return;
    };
    path.move_to(first);
    let n = pts.len();
    for i in 1..n.saturating_sub(1) {
        let (prev, corner, next) = (pts[i - 1], pts[i], pts[i + 1]);
        let (lin, lout) = ((corner - prev).hypot(), (next - corner).hypot());
        let share = |end: bool| if end { 1.0 } else { 0.5 };
        let cut = radius
            .min(lin * share(i == 1))
            .min(lout * share(i + 2 == n));
        if cut <= 1e-9 || lin <= 1e-9 || lout <= 1e-9 {
            path.line_to(corner);
            continue;
        }
        let p_in = corner - (corner - prev) * (cut / lin);
        let p_out = corner + (next - corner) * (cut / lout);
        round_corner(path, p_in, corner, p_out);
    }
    if let Some(&last) = rest.last() {
        path.line_to(last);
    }
}

fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

impl Dimension {
    /// A dimension of `measure` in `style`, its label the measured one.
    pub fn new(measure: Measure, style: DimensionStyle) -> Dimension {
        Dimension {
            measure,
            style,
            text: None,
            drawn: None,
        }
    }

    /// The dimension a shape holds (a group made by the dimension tools).
    pub fn of(shape: &Shape) -> Option<Dimension> {
        if !matches!(shape.kind, ShapeKind::Group { .. }) {
            return None;
        }
        let (_, v) = shape.data.iter().find(|(k, _)| k == DATA_KEY)?;
        let d: Dimension = serde_json::from_str(v).ok()?;
        let ok = match d.measure {
            Measure::Linear { a, b, at } => finite(a) && finite(b) && finite(at),
            Measure::Angular { center, a, b, at } => {
                finite(center) && finite(a) && finite(b) && finite(at)
            }
        };
        ok.then_some(d)
    }

    /// The same dimension with its points moved through `t`.
    pub fn transformed(&self, t: Affine) -> Dimension {
        let measure = match self.measure {
            Measure::Linear { a, b, at } => Measure::Linear {
                a: t * a,
                b: t * b,
                at: t * at,
            },
            Measure::Angular { center, a, b, at } => Measure::Angular {
                center: t * center,
                a: t * a,
                b: t * b,
                at: t * at,
            },
        };
        Dimension {
            measure,
            ..self.clone()
        }
    }

    /// The measured value: page millimetres for a length, degrees for an
    /// angle.
    pub fn value(&self) -> f64 {
        match self.measure {
            Measure::Linear { a, b, .. } => (b - a).hypot(),
            Measure::Angular { center, a, b, .. } => sweep(center, a, b).to_degrees(),
        }
    }

    /// The measured value as the label shows it: the number with the
    /// style's decimals and the units' sign when shown.
    pub fn value_text(&self, drawing_units: Units) -> String {
        let s = &self.style;
        let decimals = s.precision.min(6) as usize;
        match self.measure {
            Measure::Linear { .. } => {
                let units = if s.units.is_empty() {
                    drawing_units
                } else {
                    Units::from_id(&s.units)
                };
                let v = units.world_from_mm(self.value());
                let unit = if s.show_units {
                    format!(" {}", units.short())
                } else {
                    String::new()
                };
                format!("{v:.decimals$}{unit}")
            }
            Measure::Angular { .. } => {
                let unit = if s.show_units { "\u{b0}" } else { "" };
                format!("{:.decimals$}{unit}", self.value())
            }
        }
    }

    /// The label: the text typed for it, else prefix, value, units,
    /// suffix.
    pub fn label(&self, drawing_units: Units) -> String {
        match &self.text {
            Some(t) => t.clone(),
            None => format!(
                "{}{}{}",
                self.style.prefix,
                self.value_text(drawing_units),
                self.style.suffix
            ),
        }
    }

    /// The label's font: the style's, else `font` (that of new text).
    fn font<'a>(&'a self, font: &'a str) -> &'a str {
        if self.style.font.trim().is_empty() {
            font
        } else {
            self.style.font.as_str()
        }
    }

    /// The same dimension, noting how its label is drawn now.
    pub fn noting_drawn(mut self, font: &str, drawing_units: Units) -> Dimension {
        self.drawn = Some(self.drawn_label(font, drawing_units));
        self
    }

    fn drawn_label(&self, font: &str, drawing_units: Units) -> DrawnLabel {
        DrawnLabel {
            text: self.label(drawing_units),
            value: self.value_text(drawing_units),
            font: self.font(font).to_string(),
        }
    }

    /// The dimension with what was done to the children of its group `g`
    /// read back (`None` when nothing was): a label edited in place, its
    /// font, size and colour; a label deleted brings the measured one
    /// back, as does a part missing. Compared with how the label was last
    /// drawn, so new units or a new default font leave it as it is.
    pub fn read_back(&self, g: &Shape) -> Option<Dimension> {
        let ShapeKind::Group { children } = &g.kind else {
            return None;
        };
        // A dimension saved before labels were noted has nothing to compare
        // with: its label reads as drawn (new units or a new default font
        // since would look like an edit).
        let Some(drawn) = self.drawn.clone() else {
            return (children.len() != 3).then(|| self.clone());
        };
        let mut next = self.clone();
        let label = children
            .iter()
            .find(|c| matches!(c.kind, ShapeKind::Text { .. }));
        match label.map(|l| (l, &l.kind)) {
            Some((l, ShapeKind::Text { spans, .. })) => {
                let text: String = spans.iter().map(|s| s.text.as_str()).collect();
                if text != drawn.text {
                    if text.trim().is_empty() {
                        next.text = None;
                    } else if let Some(i) = find_value(&text, &drawn.value) {
                        next.text = None;
                        next.style.prefix = text[..i].to_string();
                        next.style.suffix = text[i + drawn.value.len()..].to_string();
                    } else {
                        next.text = Some(text);
                    }
                }
                if let Some(sp) = spans.first() {
                    if sp.font_family != drawn.font && !sp.font_family.trim().is_empty() {
                        next.style.font = sp.font_family.clone();
                    }
                    let size = self.style.text_size.clamp(1.0, 500.0);
                    if sp.size_pt.is_finite() && (sp.size_pt - size).abs() > 1e-9 {
                        next.style.text_size = sp.size_pt.clamp(1.0, 500.0);
                    }
                }
                if let Fill::Solid(c) = l.fill {
                    next.style.text_color = c;
                }
            }
            _ => next.text = None,
        }
        (next != *self || children.len() != 3).then_some(next)
    }

    /// The group's children: lines, arrowheads, label (ids from `ids`).
    pub fn shapes(&self, ids: [ShapeId; 3], font: &str, drawing_units: Units) -> [Shape; 3] {
        let text = self.label(drawing_units);
        let font = self.font(font);
        let (ext_gap, overshoot, arrow_len) = self.style.lengths();
        let arrow_half = arrow_len * ARROW_HALF / ARROW_LEN;
        let mut label = Shape::new(
            ids[2],
            ShapeKind::Text {
                spans: vec![TextSpan::new(
                    text.clone(),
                    font.to_string(),
                    self.style.text_size.clamp(1.0, 500.0),
                )],
                origin: Point::ZERO,
                frame: None,
                align: TextAlign::Left,
                para: Default::default(),
                on_path: None,
            },
        );
        label.fill = Fill::Solid(self.style.text_color);
        label.stroke = None;
        // The label's own box: its width and how far it reaches below and
        // above its baseline.
        let tb = label.local_path().bounding_box();
        let tb = if tb.width().is_finite() && tb.height().is_finite() && tb.width() > 0.0 {
            tb
        } else {
            // No outlines (no font): a box from the size alone.
            let h = self.style.text_size * 25.4 / 72.0;
            Rect::new(0.0, 0.0, h * 0.55 * text.chars().count() as f64, h * 0.7)
        };
        let mut lines = BezPath::new();
        let mut arrows = BezPath::new();
        // The open and tick marks are strokes, drawn with the lines.
        let mut marks = BezPath::new();
        let style = self.style.arrows;
        let mut arrow = |tip: Point, back_dir: Vec2| {
            let n = Vec2::new(-back_dir.y, back_dir.x);
            let back = tip + back_dir * arrow_len;
            match style {
                ArrowStyle::Filled => {
                    arrows.move_to(tip);
                    arrows.line_to(back + n * arrow_half);
                    arrows.line_to(back - n * arrow_half);
                    arrows.close_path();
                }
                ArrowStyle::Open => {
                    marks.move_to(back + n * arrow_half);
                    marks.line_to(tip);
                    marks.line_to(back - n * arrow_half);
                }
                ArrowStyle::Dot => {
                    if arrow_half > 1e-9 {
                        arrows.extend(kurbo::Circle::new(tip, arrow_half).path_elements(0.01));
                    }
                }
                ArrowStyle::Tick => {
                    // At 45 degrees across the line.
                    let d = (n + back_dir) * (arrow_len * 0.5 / 2f64.sqrt());
                    marks.move_to(tip - d);
                    marks.line_to(tip + d);
                }
                ArrowStyle::None => {}
            }
        };
        let pos = self.style.text_position;
        match self.measure {
            Measure::Linear { a, b, at } => {
                let len = (b - a).hypot();
                let dir = if len > 1e-9 {
                    (b - a) / len
                } else {
                    Vec2::new(1.0, 0.0)
                };
                let n = Vec2::new(-dir.y, dir.x);
                let off = (at - a).dot(n);
                let side = if off < 0.0 { -n } else { n };
                let (a2, b2) = (a + n * off, b + n * off);
                // Extension lines, from a gap off the measured points to
                // past the dimension line.
                let gap = ext_gap.min(off.abs());
                let joined = self.style.joined;
                if !joined {
                    for (p, q) in [(a, a2), (b, b2)] {
                        lines.move_to(p + side * gap);
                        lines.line_to(q + side * overshoot);
                    }
                }
                let (a1, b1) = (a + side * gap, b + side * gap);
                // The label reads left to right, upright.
                let read = if dir.x < -1e-9 || (dir.x.abs() <= 1e-9 && dir.y < 0.0) {
                    -dir
                } else {
                    dir
                };
                let up = Vec2::new(-read.y, read.x);
                let mid = a2.midpoint(b2);
                let horizontal = self.style.horizontal_text;
                // How far the label reaches along the line from its middle.
                let half = if horizontal {
                    (tb.width() * dir.x.abs() + tb.height() * dir.y.abs()) / 2.0
                } else {
                    tb.width() / 2.0
                };
                // The dimension line, with a gap for a centred label.
                // Joined, the extension lines run into it in a corner.
                let radius = self.style.radius();
                if pos == TextPosition::Centered && len > 2.0 * (half + GAP) {
                    let (g1, g2) = (mid - dir * (half + GAP), mid + dir * (half + GAP));
                    if joined {
                        rounded_polyline(&mut lines, &[a1, a2, g1], radius);
                        rounded_polyline(&mut lines, &[g2, b2, b1], radius);
                    } else {
                        lines.move_to(a2);
                        lines.line_to(g1);
                        lines.move_to(g2);
                        lines.line_to(b2);
                    }
                } else if joined {
                    rounded_polyline(&mut lines, &[a1, a2, b2, b1], radius);
                } else {
                    lines.move_to(a2);
                    lines.line_to(b2);
                }
                arrow(a2, dir);
                arrow(b2, -dir);
                if horizontal {
                    // Upright, its middle off the line's middle by its
                    // reach across the line.
                    let reach = (tb.width() * up.x.abs() + tb.height() * up.y.abs()) / 2.0;
                    let c = match pos {
                        TextPosition::Above => mid + up * (GAP + reach),
                        TextPosition::Below => mid - up * (GAP + reach),
                        TextPosition::Centered => mid,
                    };
                    let origin = c - Vec2::new(tb.x0 + tb.width() / 2.0, (tb.y0 + tb.y1) / 2.0);
                    label.transform = Affine::translate(origin.to_vec2());
                } else {
                    let baseline = match pos {
                        TextPosition::Above => mid + up * (GAP - tb.y0),
                        TextPosition::Below => mid - up * (GAP + tb.y1),
                        TextPosition::Centered => mid - up * ((tb.y0 + tb.y1) / 2.0),
                    };
                    let origin = baseline - read * (tb.x0 + half);
                    label.transform =
                        Affine::translate(origin.to_vec2()) * Affine::rotate(read.atan2());
                }
            }
            Measure::Angular { center, a, b, at } => {
                let a0 = (a - center).atan2();
                let sw = sweep(center, a, b);
                let r = (at - center).hypot().max(1.0);
                let ray = |t: f64| Vec2::new(t.cos(), t.sin());
                let joined = self.style.joined;
                if !joined {
                    for t in [a0, a0 + sw] {
                        lines.move_to(center + ray(t) * ext_gap.min(r));
                        lines.line_to(center + ray(t) * (r + overshoot));
                    }
                }
                let mid = a0 + sw / 2.0;
                let (w, h) = (tb.width(), tb.height());
                // A centred label leaves a gap in the arc.
                let gap = if pos == TextPosition::Centered {
                    ((w / 2.0 + GAP) / r).min(sw / 2.0)
                } else {
                    0.0
                };
                if joined && sw > 1e-6 {
                    // Each side runs into the arc in a corner, rounded:
                    // the cut along the side, the same length along the
                    // arc.
                    let g = ext_gap.min(r);
                    let piece = if gap > 0.0 { sw / 2.0 - gap } else { sw / 2.0 };
                    let cut = self
                        .style
                        .radius()
                        .min(r - g)
                        .min(r * piece.max(0.0))
                        .max(0.0);
                    let delta = cut / r;
                    let a1 = a0 + sw;
                    let arc = |from: f64, to: f64| kurbo::Arc {
                        center,
                        radii: Vec2::new(r, r),
                        start_angle: from,
                        sweep_angle: to - from,
                        x_rotation: 0.0,
                    };
                    let corner_in = |path: &mut BezPath, t: f64, toward: f64| {
                        round_corner(
                            path,
                            center + ray(t) * (r - cut),
                            center + ray(t) * r,
                            center + ray(toward) * r,
                        );
                    };
                    let (end1, start2) = if gap > 0.0 {
                        (mid - gap, mid + gap)
                    } else {
                        (a1 - delta, a1 - delta)
                    };
                    lines.move_to(center + ray(a0) * g);
                    corner_in(&mut lines, a0, a0 + delta);
                    lines.extend(arc(a0 + delta, end1).append_iter(0.01));
                    if gap > 0.0 {
                        lines.move_to(center + ray(start2) * r);
                        lines.extend(arc(start2, a1 - delta).append_iter(0.01));
                    }
                    // Out of the arc into the second side.
                    let (from, corner, to) = (
                        center + ray(a1 - delta) * r,
                        center + ray(a1) * r,
                        center + ray(a1) * (r - cut),
                    );
                    lines.curve_to(
                        from + (corner - from) * KAPPA,
                        to + (corner - to) * KAPPA,
                        to,
                    );
                    lines.line_to(center + ray(a1) * g);
                }
                if !joined || sw <= 1e-6 {
                    let steps = 48;
                    let mut drawing = false;
                    for i in 0..=steps {
                        let t = a0 + sw * i as f64 / steps as f64;
                        let q = center + ray(t) * r;
                        if gap > 0.0 && (t - mid).abs() < gap {
                            drawing = false;
                            continue;
                        }
                        if drawing {
                            lines.line_to(q);
                        } else {
                            lines.move_to(q);
                            drawing = true;
                        }
                    }
                }
                // Arrowheads along the arc at both ends.
                if sw > 1e-6 {
                    arrow(center + ray(a0) * r, Vec2::new(-a0.sin(), a0.cos()));
                    let a1 = a0 + sw;
                    arrow(center + ray(a1) * r, Vec2::new(a1.sin(), -a1.cos()));
                }
                // The label upright, its middle out from, on or inside the
                // arc.
                let reach = (w * mid.cos().abs() + h * mid.sin().abs()) / 2.0;
                let p = match pos {
                    TextPosition::Above => center + ray(mid) * (r + GAP + reach),
                    TextPosition::Centered => center + ray(mid) * r,
                    TextPosition::Below => center + ray(mid) * (r - GAP - reach).max(0.0),
                };
                let origin = p - Vec2::new(tb.x0 + w / 2.0, (tb.y0 + tb.y1) / 2.0);
                label.transform = Affine::translate(origin.to_vec2());
            }
        }
        lines.extend(marks);
        let mut line = Shape::new(
            ids[0],
            ShapeKind::Path {
                path: lines,
                closed: false,
            },
        );
        line.fill = Fill::None;
        let mut stroke = if self.style.line_width > 1e-9 {
            Stroke::new(self.style.line_color, self.style.line_width.min(50.0))
        } else {
            Stroke::hairline(self.style.line_color)
        };
        if self.style.round_ends {
            stroke.cap = tracedraw_core::LineCap::Round;
            stroke.join = tracedraw_core::LineJoin::Round;
        }
        line.stroke = Some(stroke);
        let mut heads = Shape::new(
            ids[1],
            ShapeKind::Path {
                path: arrows,
                closed: true,
            },
        );
        heads.fill = Fill::Solid(self.style.line_color);
        heads.stroke = None;
        [line, heads, label]
    }
}

/// The counterclockwise angle at `center` from `a` to `b`, 0 to a turn.
fn sweep(center: Point, a: Point, b: Point) -> f64 {
    let s = (b - center).atan2() - (a - center).atan2();
    if s < 0.0 {
        s + std::f64::consts::TAU
    } else {
        s
    }
}

fn with_data(mut data: Vec<(String, String)>, d: &Dimension) -> Vec<(String, String)> {
    data.retain(|(k, _)| k != DATA_KEY);
    if let Ok(v) = serde_json::to_string(d) {
        data.push((DATA_KEY.into(), v));
    }
    data
}

impl App {
    /// A new dimension on the active layer, in the bar's style, one undo
    /// step.
    pub fn add_dimension(&mut self, measure: Measure, label: &'static str) {
        let Some(layer) = self.active_layer() else {
            return;
        };
        let d = Dimension::new(measure, self.dimension_style.clone())
            .noting_drawn(&self.text_font, self.units);
        let ids = [
            self.engine.new_shape_id(),
            self.engine.new_shape_id(),
            self.engine.new_shape_id(),
        ];
        let children = d.shapes(ids, &self.text_font, self.units);
        let group = self.engine.new_shape_id();
        let mut g = Shape::new(
            group,
            ShapeKind::Group {
                children: children.to_vec(),
            },
        );
        g.data = with_data(Vec::new(), &d);
        if let Err(e) = self
            .engine
            .run_batch(label, &[Command::AddShape { layer, shape: g }])
        {
            self.status = e.to_string();
            return;
        }
        self.select(vec![group]);
    }

    /// Parallel, Horizontal or Vertical and Segment dimensions: from `a` to
    /// `b`, the line through `offset_at`.
    pub fn create_dimension(&mut self, a: Point, b: Point, offset_at: Point) {
        self.add_dimension(
            Measure::Linear {
                a,
                b,
                at: offset_at,
            },
            "Dimension",
        );
    }

    /// An angular dimension: at `center` from `a` to `b`, the arc through
    /// `label_at`.
    pub fn create_angular_dimension(&mut self, center: Point, a: Point, b: Point, label_at: Point) {
        self.add_dimension(
            Measure::Angular {
                center,
                a,
                b,
                at: label_at,
            },
            "Angular Dimension",
        );
    }

    /// The commands that draw dimension group `g` (as it is in its
    /// parent) again from `d`, its own transform baked in.
    fn redraw_commands(&mut self, g: &Shape, d: &Dimension) -> Vec<Command> {
        let ids = match &g.kind {
            ShapeKind::Group { children } if children.len() == 3 => {
                [children[0].id, children[1].id, children[2].id]
            }
            _ => [
                self.engine.new_shape_id(),
                self.engine.new_shape_id(),
                self.engine.new_shape_id(),
            ],
        };
        let mut cmds = Vec::new();
        let t = g.transform;
        let d = if t != Affine::IDENTITY && t.determinant().abs() > 1e-12 {
            // The group's transform goes into the points, the group back to
            // none (in page space: about its parent).
            let parent = self
                .doc()
                .parent_transform(g.id)
                .unwrap_or(Affine::IDENTITY);
            cmds.push(Command::TransformShapes {
                shapes: vec![g.id],
                transform: parent * t.inverse() * parent.inverse(),
            });
            d.transformed(t)
        } else {
            d.clone()
        };
        let d = d.noting_drawn(&self.text_font, self.units);
        let children = d.shapes(ids, &self.text_font, self.units);
        cmds.push(Command::SetShapeKind {
            shape: g.id,
            kind: ShapeKind::Group {
                children: children.to_vec(),
            },
        });
        cmds.push(Command::SetObjectData {
            shape: g.id,
            data: with_data(g.data.clone(), &d),
        });
        cmds
    }

    /// The selected dimensions.
    pub fn selected_dimensions(&self) -> Vec<(ShapeId, Dimension)> {
        self.selection
            .iter()
            .filter_map(|id| {
                let s = self.doc().find_shape(*id)?;
                Dimension::of(s).map(|d| (*id, d))
            })
            .collect()
    }

    /// Change the style new dimensions take and the selected ones have,
    /// one undo step for the selected ones.
    pub fn set_dimension_style(&mut self, f: impl Fn(&mut DimensionStyle)) {
        f(&mut self.dimension_style);
        let mut cmds = Vec::new();
        for (id, mut d) in self.selected_dimensions() {
            let before = d.style.clone();
            f(&mut d.style);
            if d.style == before {
                continue;
            }
            let Some(g) = self.doc().find_shape(id).cloned() else {
                continue;
            };
            cmds.extend(self.redraw_commands(&g, &d));
        }
        if cmds.is_empty() {
            return;
        }
        if let Err(e) = self.engine.run_batch("Dimension Style", &cmds) {
            self.status = e.to_string();
        }
    }

    /// The selected dimensions show their measured value again instead of
    /// a text typed for their label, one undo step.
    pub fn measured_dimension_labels(&mut self) {
        let mut cmds = Vec::new();
        for (id, mut d) in self.selected_dimensions() {
            if d.text.take().is_none() {
                continue;
            }
            let Some(g) = self.doc().find_shape(id).cloned() else {
                continue;
            };
            cmds.extend(self.redraw_commands(&g, &d));
        }
        if cmds.is_empty() {
            return;
        }
        if let Err(e) = self.engine.run_batch("Dimension Label", &cmds) {
            self.status = e.to_string();
        }
    }

    /// The first selected dimension shows a text typed for its label.
    pub fn dimension_label_typed(&self) -> bool {
        self.selected_dimensions()
            .first()
            .is_some_and(|(_, d)| d.text.is_some())
    }

    /// The label of the dimension at `p` (page space), when the pointer is
    /// on it: the text to edit in place (#644).
    pub fn dimension_label_at(&self, p: Point) -> Option<ShapeId> {
        let id = self.hit_test(p)?;
        let doc = self.doc();
        // In focus mode on a dimension the label itself is hit.
        fn holding(shapes: &[Shape], id: ShapeId) -> Option<&Shape> {
            shapes.iter().find_map(|s| match &s.kind {
                ShapeKind::Group { children } => {
                    if children.iter().any(|c| c.id == id) {
                        Some(s)
                    } else {
                        holding(children, id)
                    }
                }
                _ => None,
            })
        }
        let g = match doc.find_shape(id) {
            Some(s) if matches!(s.kind, ShapeKind::Text { .. }) => {
                doc.all_layers().find_map(|l| holding(&l.shapes, id))?
            }
            s => s?,
        };
        Dimension::of(g)?;
        let ShapeKind::Group { children } = &g.kind else {
            return None;
        };
        let label = children
            .iter()
            .find(|c| matches!(c.kind, ShapeKind::Text { .. }))?;
        let to_page = doc.parent_transform(label.id).unwrap_or(Affine::IDENTITY) * label.transform;
        let tol = 3.0 / self.view.zoom as f64;
        let b = (to_page * label.local_path()).bounding_box();
        (b.is_finite() && b.inflate(tol, tol).contains(p)).then_some(label.id)
    }

    /// Edit the label of the dimension at `p` in place with the Text tool,
    /// the caret where the pointer is. False when no label is there.
    pub fn edit_dimension_label_at(&mut self, p: Point) -> bool {
        let Some(label) = self.dimension_label_at(p) else {
            return false;
        };
        if self.tool != crate::tools::Tool::Text {
            self.set_tool(crate::tools::Tool::Text);
        }
        self.begin_text_edit(label);
        if let Some(idx) = self.text_hit_char(p) {
            self.text_set_caret_at(idx, false);
        }
        true
    }

    /// The commands that break the dimension `id` apart (`None` when it
    /// is not one on a layer of its own): its lines, arrowheads and label
    /// become objects of their own, arrowheads drawn with the lines
    /// leaving nothing behind (#644).
    pub(crate) fn dimension_break_commands(&self, id: ShapeId) -> Option<Vec<Command>> {
        let doc = self.doc();
        let g = doc.find_shape(id)?;
        Dimension::of(g)?;
        doc.locate(id).ok()?;
        let ShapeKind::Group { children } = &g.kind else {
            return None;
        };
        let empty: Vec<ShapeId> = children
            .iter()
            .filter(|c| match &c.kind {
                ShapeKind::Path { path, .. } => path.elements().is_empty(),
                _ => false,
            })
            .map(|c| c.id)
            .collect();
        let mut cmds = vec![Command::Ungroup { group: id }];
        if !empty.is_empty() {
            cmds.push(Command::DeleteShapes { shapes: empty });
        }
        Some(cmds)
    }

    /// The style the bar shows: the first selected dimension's, else the
    /// one new dimensions take.
    pub fn dimension_style_shown(&self) -> DimensionStyle {
        self.selected_dimensions()
            .into_iter()
            .next()
            .map(|(_, d)| d.style)
            .unwrap_or_else(|| self.dimension_style.clone())
    }

    /// Dynamic dimensioning: dimensions moved, scaled, turned or skewed
    /// since the last look are drawn again with the transform in their
    /// points, and those whose label was edited in place with the edit
    /// read back (see [`Dimension::read_back`]), as part of the step that
    /// changed them. Runs once per change of the drawing.
    pub fn bake_dimensions(&mut self) {
        let revision = self.engine.revision();
        if revision == self.dimensions_checked {
            return;
        }
        fn walk(s: &Shape, out: &mut Vec<(Shape, Dimension)>) {
            if let Some(d) = Dimension::of(s) {
                match d.read_back(s) {
                    Some(next) => out.push((s.clone(), next)),
                    None if s.transform != Affine::IDENTITY => out.push((s.clone(), d)),
                    None => {}
                }
                return;
            }
            match &s.kind {
                ShapeKind::Group { children } => children.iter().for_each(|c| walk(c, out)),
                ShapeKind::ClipFrame { frame, contents } => {
                    walk(frame, out);
                    contents.iter().for_each(|c| walk(c, out));
                }
                _ => {}
            }
        }
        let mut changed = Vec::new();
        for layer in self.doc().all_layers() {
            for s in &layer.shapes {
                walk(s, &mut changed);
            }
        }
        for (g, d) in changed {
            for cmd in self.redraw_commands(&g, &d) {
                if let Err(e) = self.engine.amend(&cmd) {
                    self.status = e.to_string();
                    break;
                }
            }
        }
        self.dimensions_checked = self.engine.revision();
    }
}

impl App {
    /// The nearest segment of an object on the page within 6 pixels of
    /// `p`: its two ends (page space).
    pub fn segment_ends_at(&self, p: Point) -> Option<(Point, Point)> {
        use tracedraw_core::geometry::{ParamCurve, ParamCurveNearest};
        let tol = 6.0 / self.view.zoom as f64;
        let mut best: Option<(Point, Point, f64)> = None;
        for layer in self.page_layers() {
            if !layer.visible || layer.locked {
                continue;
            }
            for s in &layer.shapes {
                if !s.visible || !self.visual_bounds(s).inflate(tol, tol).contains(p) {
                    continue;
                }
                for seg in s.page_path().segments() {
                    let d = seg.nearest(p, 1e-6).distance_sq.sqrt();
                    if d <= tol && best.is_none_or(|b| d < b.2) {
                        best = Some((seg.start(), seg.end(), d));
                    }
                }
            }
        }
        best.map(|(a, b, _)| (a, b))
    }

    /// The two points a linear dimension tool measures between, from the
    /// points clicked: Horizontal or Vertical keeps the larger of the two
    /// spans.
    fn measured_pair(&self, a: Point, b: Point) -> (Point, Point) {
        if self.tool == crate::tools::Tool::HorizontalVerticalDimension {
            if (b.x - a.x).abs() >= (b.y - a.y).abs() {
                (a, Point::new(b.x, a.y))
            } else {
                (a, Point::new(a.x, b.y))
            }
        } else {
            (a, b)
        }
    }

    /// The point the dimension tools take for `p`: with the constrain key
    /// (`ctrl`) held while the second point of a linear dimension, or a
    /// side of an angular one, is given, the leg from the first point (the
    /// vertex) keeps to steps of the constrain angle, as the curve tools
    /// do.
    pub fn dimension_point(&self, p: Point, ctrl: bool) -> Point {
        if !ctrl {
            return p;
        }
        let angular = self.tool == crate::tools::Tool::AngularDimension;
        let from = match (angular, &self.dimension_points[..]) {
            (false, [a]) => *a,
            (true, [center]) | (true, [center, _]) => *center,
            _ => return p,
        };
        crate::interaction::constrain_from(from, p, self.settings.constrain_angle)
    }

    /// The dimension the tool would make with the pointer at `at`, while
    /// its points are being given (for the preview).
    pub fn dimension_preview(&self, at: Point) -> Option<Dimension> {
        let pts = &self.dimension_points;
        let measure = match self.tool {
            crate::tools::Tool::AngularDimension => match pts[..] {
                [center, a, b] => Measure::Angular { center, a, b, at },
                _ => return None,
            },
            crate::tools::Tool::ParallelDimension
            | crate::tools::Tool::HorizontalVerticalDimension
            | crate::tools::Tool::SegmentDimension => match pts[..] {
                [a, b] => {
                    let (a, b) = self.measured_pair(a, b);
                    Measure::Linear { a, b, at }
                }
                _ => return None,
            },
            _ => return None,
        };
        Some(Dimension::new(measure, self.dimension_style.clone()))
    }

    /// The dimension tools' clicks and drags. Linear tools: a drag from
    /// one point to the other (or two clicks), then a click where the
    /// dimension line goes; the Segment tool's first click on a segment
    /// takes its two ends. Angular: the vertex, a point on each side, then
    /// where the arc goes.
    pub fn dimension_input(&mut self, response: &egui::Response, p: Point) {
        use crate::tools::Tool;
        use egui::PointerButton;
        let p = self.snap_point(p);
        let ctrl = response
            .ctx
            .input(|i| i.modifiers.command || i.modifiers.ctrl);
        let p = self.dimension_point(p, ctrl);
        let angular = self.tool == Tool::AngularDimension;
        if !angular {
            if response.drag_started_by(PointerButton::Primary) && self.dimension_points.is_empty()
            {
                // Where the button went down, not where the drag was seen.
                let origin = response
                    .ctx
                    .input(|i| i.pointer.press_origin())
                    .map(|o| self.snap_point(self.view.to_page(o)))
                    .unwrap_or(p);
                self.dimension_points.push(origin);
                self.dimension_dragging = true;
                return;
            }
            if response.drag_stopped() && self.dimension_dragging {
                self.dimension_dragging = false;
                let start = self.dimension_points.first().copied();
                if start.is_some_and(|s| (p - s).hypot() > 1e-6) {
                    self.dimension_points.push(p);
                } else {
                    self.dimension_points.clear();
                }
                return;
            }
        }
        if !response.clicked_by(PointerButton::Primary) {
            return;
        }
        if self.tool == Tool::SegmentDimension && self.dimension_points.is_empty() {
            if let Some((a, b)) = self.segment_ends_at(p) {
                self.dimension_points = vec![a, b];
                return;
            }
        }
        self.dimension_points.push(p);
        let pts = self.dimension_points.clone();
        match (angular, &pts[..]) {
            (true, [center, a, b, at]) => {
                self.dimension_points.clear();
                self.create_angular_dimension(*center, *a, *b, *at);
            }
            (false, [a, b, at]) => {
                self.dimension_points.clear();
                let (a, b) = self.measured_pair(*a, *b);
                self.create_dimension(a, b, *at);
            }
            _ => {}
        }
    }
}

/// The units list of the dimension bar: the drawing's units first.
pub fn units_choices() -> Vec<(String, String)> {
    let mut out = vec![(String::new(), tr("dimension.drawing_units"))];
    out.extend(Units::ALL.iter().map(|u| (u.id().to_string(), u.label())));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label_of(app: &App, id: ShapeId) -> String {
        let g = app.doc().find_shape(id).expect("group");
        let ShapeKind::Group { children } = &g.kind else {
            panic!("not a group");
        };
        match &children[2].kind {
            ShapeKind::Text { spans, .. } => spans.iter().map(|s| s.text.as_str()).collect(),
            other => panic!("{other:?}"),
        }
    }

    /// The preview follows the pointer; Horizontal or Vertical keeps the
    /// larger span; a segment's ends are found under the pointer.
    #[test]
    fn previews_and_segments() {
        let mut app = App::headless();
        app.view.zoom = 1.0;
        app.tool = crate::tools::Tool::HorizontalVerticalDimension;
        assert!(app.dimension_preview(Point::ZERO).is_none());
        app.dimension_points = vec![Point::new(0.0, 0.0), Point::new(30.0, 4.0)];
        let d = app
            .dimension_preview(Point::new(10.0, 20.0))
            .expect("preview");
        assert_eq!(
            d.measure,
            Measure::Linear {
                a: Point::new(0.0, 0.0),
                b: Point::new(30.0, 0.0),
                at: Point::new(10.0, 20.0)
            }
        );
        app.dimension_points.clear();
        app.new_shape(ShapeKind::Rect {
            rect: Rect::new(10.0, 10.0, 50.0, 30.0),
            radius: 0.0,
            corners: None,
        })
        .expect("shape");
        let (a, b) = app
            .segment_ends_at(Point::new(30.0, 10.5))
            .expect("segment");
        let (lo, hi) = if a.x < b.x { (a, b) } else { (b, a) };
        assert_eq!((lo, hi), (Point::new(10.0, 10.0), Point::new(50.0, 10.0)));
        assert!(app.segment_ends_at(Point::new(30.0, 20.0)).is_none());
    }

    #[test]
    fn labels_follow_the_style() {
        let d = Dimension {
            measure: Measure::Linear {
                a: Point::new(0.0, 0.0),
                b: Point::new(25.4, 0.0),
                at: Point::new(0.0, 10.0),
            },
            style: DimensionStyle::default(),
            text: None,
            drawn: None,
        };
        assert_eq!(d.label(Units::Millimeters), "25.40 mm");
        let inches = Dimension {
            style: DimensionStyle {
                units: "in".into(),
                precision: 3,
                prefix: "W ".into(),
                suffix: " max".into(),
                ..Default::default()
            },
            ..d.clone()
        };
        assert_eq!(inches.label(Units::Millimeters), "W 1.000 \" max");
        let bare = Dimension {
            style: DimensionStyle {
                show_units: false,
                precision: 0,
                ..Default::default()
            },
            ..d
        };
        assert_eq!(bare.label(Units::Millimeters), "25");
        let angle = Dimension {
            measure: Measure::Angular {
                center: Point::ZERO,
                a: Point::new(10.0, 0.0),
                b: Point::new(0.0, 10.0),
                at: Point::new(5.0, 5.0),
            },
            style: DimensionStyle {
                precision: 1,
                ..Default::default()
            },
            text: None,
            drawn: None,
        };
        assert_eq!(angle.label(Units::Millimeters), "90.0\u{b0}");
    }

    /// The style's font, colours, line width, arrowheads and rounded ends
    /// reach the dimension's lines, heads and label; a style saved before
    /// they existed reads as before (#475).
    #[test]
    fn the_style_sets_the_look_of_lines_heads_and_label() {
        use tracedraw_core::LineCap;
        let red = Color::rgb8(200, 0, 0);
        let blue = Color::rgb8(0, 0, 200);
        let d = Dimension {
            measure: Measure::Linear {
                a: Point::new(0.0, 0.0),
                b: Point::new(50.0, 0.0),
                at: Point::new(0.0, 10.0),
            },
            style: DimensionStyle {
                font: "Some Font".into(),
                line_color: red,
                text_color: blue,
                line_width: 0.5,
                arrows: ArrowStyle::Open,
                round_ends: true,
                ..Default::default()
            },
            text: None,
            drawn: None,
        };
        let ids = [ShapeId(1), ShapeId(2), ShapeId(3)];
        let [line, heads, label] = d.shapes(ids, "Arial", Units::Millimeters);
        let st = line.stroke.clone().expect("stroke");
        assert_eq!((st.color, st.width, st.cap), (red, 0.5, LineCap::Round));
        assert_eq!(label.fill, Fill::Solid(blue));
        match &label.kind {
            ShapeKind::Text { spans, .. } => assert_eq!(spans[0].font_family, "Some Font"),
            _ => panic!("label"),
        }
        // Open heads are strokes with the lines: no filled heads.
        let heads_path = |s: &Shape| match &s.kind {
            ShapeKind::Path { path, .. } => path.elements().len(),
            _ => 0,
        };
        assert_eq!(heads_path(&heads), 0);
        let plain = Dimension {
            style: DimensionStyle::default(),
            ..d.clone()
        };
        let [pl, ph, plabel] = plain.shapes(ids, "Arial", Units::Millimeters);
        assert!(heads_path(&ph) > 0, "filled heads at first");
        assert!(
            line_count(&line) > line_count(&pl),
            "the open heads' strokes"
        );
        match &plabel.kind {
            ShapeKind::Text { spans, .. } => assert_eq!(spans[0].font_family, "Arial"),
            _ => panic!("label"),
        }
        for a in [ArrowStyle::Dot, ArrowStyle::Tick, ArrowStyle::None] {
            let x = Dimension {
                style: DimensionStyle {
                    arrows: a,
                    ..Default::default()
                },
                ..d.clone()
            };
            let [_, h, _] = x.shapes(ids, "Arial", Units::Millimeters);
            assert_eq!(heads_path(&h) > 0, a == ArrowStyle::Dot, "{a:?}");
        }
        let old: DimensionStyle = serde_json::from_str(r#"{"precision":1}"#).expect("old");
        assert_eq!(old.line_color, Color::BLACK);
        assert_eq!(old.arrows, ArrowStyle::Filled);
        assert!(old.font.is_empty());
    }

    fn line_count(s: &Shape) -> usize {
        match &s.kind {
            ShapeKind::Path { path, .. } => path
                .elements()
                .iter()
                .filter(|e| matches!(e, tracedraw_core::geometry::PathEl::MoveTo(_)))
                .count(),
            _ => 0,
        }
    }

    /// A dimension is a group that knows what it measures; its style is
    /// changed from the bar; scaling it changes the value it shows and
    /// leaves the group without a transform; undo takes both back.
    #[test]
    fn dimensions_redraw_for_their_style_and_their_size() {
        let mut app = App::headless();
        app.create_dimension(
            Point::new(0.0, 0.0),
            Point::new(40.0, 0.0),
            Point::new(20.0, 10.0),
        );
        let id = app.selection[0];
        assert_eq!(label_of(&app, id), "40.00 mm");
        // The dimension line runs at y 10, the extension lines past it.
        let g = app.doc().find_shape(id).cloned().expect("group");
        assert!(g.bounds().y1 >= 12.0 - 1e-6, "{:?}", g.bounds());
        app.set_dimension_style(|s| {
            s.precision = 0;
            s.suffix = " long".into();
        });
        assert_eq!(label_of(&app, id), "40 mm long");
        assert_eq!(app.dimension_style.precision, 0);
        // Scale it by 2 about the origin: dynamic dimensioning.
        app.run(Command::TransformShapes {
            shapes: vec![id],
            transform: Affine::scale(2.0),
        });
        let depth = app.engine.history_labels().0.len();
        app.bake_dimensions();
        assert_eq!(app.engine.history_labels().0.len(), depth);
        assert_eq!(label_of(&app, id), "80 mm long");
        let g = app.doc().find_shape(id).cloned().expect("group");
        assert_eq!(g.transform, Affine::IDENTITY);
        match Dimension::of(&g).expect("dimension").measure {
            Measure::Linear { b, .. } => assert_eq!(b, Point::new(80.0, 0.0)),
            other => panic!("{other:?}"),
        }
        app.undo();
        app.bake_dimensions();
        assert_eq!(label_of(&app, id), "40 mm long");
        // An angular dimension and a centred label.
        app.dimension_style.text_position = TextPosition::Centered;
        app.create_angular_dimension(
            Point::new(100.0, 0.0),
            Point::new(110.0, 0.0),
            Point::new(100.0, 10.0),
            Point::new(108.0, 8.0),
        );
        let a = app.selection[0];
        assert_eq!(label_of(&app, a), "90\u{b0} long");
        // Something that is not a dimension is not one.
        assert!(Dimension::of(&Shape::new(
            ShapeId(1),
            ShapeKind::Group { children: vec![] }
        ))
        .is_none());
    }

    fn label_id(app: &App, id: ShapeId) -> ShapeId {
        match &app.doc().find_shape(id).expect("group").kind {
            ShapeKind::Group { children } => children[2].id,
            other => panic!("{other:?}"),
        }
    }

    /// Edit the label of dimension `id` as the Text tool would, then let
    /// the frame read it back.
    fn type_label(app: &mut App, id: ShapeId, text: &str) {
        let lid = label_id(app, id);
        let mut t = app.doc().find_shape(lid).cloned().expect("label");
        if let ShapeKind::Text { spans, .. } = &mut t.kind {
            spans.truncate(1);
            spans[0].text = text.into();
        }
        app.run(Command::SetShapeKind {
            shape: lid,
            kind: t.kind,
        });
        app.bake_dimensions();
    }

    fn dimension(app: &App, id: ShapeId) -> Dimension {
        Dimension::of(app.doc().find_shape(id).expect("group")).expect("dimension")
    }

    fn forty(app: &mut App) -> ShapeId {
        app.create_dimension(
            Point::new(0.0, 0.0),
            Point::new(40.0, 0.0),
            Point::new(20.0, 10.0),
        );
        app.selection[0]
    }

    /// Break Apart frees a dimension's lines, arrowheads and label as
    /// objects of their own, selected, in one undo step; arrowheads drawn
    /// with the lines leave no empty object (#644).
    #[test]
    fn break_apart_frees_the_lines_heads_and_label() {
        let mut app = App::headless();
        let id = forty(&mut app);
        let depth = app.engine.history_labels().0.len();
        app.break_apart();
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        assert!(app.doc().find_shape(id).is_none(), "no dimension left");
        let top: Vec<Shape> = app
            .doc()
            .all_layers()
            .flat_map(|l| l.shapes.clone())
            .collect();
        assert_eq!(top.len(), 3);
        assert_eq!(app.selection.len(), 3);
        assert!(top.iter().all(|s| Dimension::of(s).is_none()));
        assert!(top.iter().any(
            |s| matches!(&s.kind, ShapeKind::Text { spans, .. } if spans[0].text == "40.00 mm")
        ));
        app.undo();
        assert!(Dimension::of(app.doc().find_shape(id).expect("back")).is_some());
        // Open heads: the lines and the label.
        app.dimension_style.arrows = ArrowStyle::Open;
        let open = forty(&mut app);
        app.select(vec![open]);
        app.break_apart();
        assert_eq!(app.selection.len(), 2);
    }

    /// A label edited in place stays live while it holds the measured
    /// value (the text around it becomes the prefix and suffix, scaling
    /// changes the number); without it, it stays as typed until the
    /// measured value is asked back; a deleted label comes back measured;
    /// its font, size and colour become the style's (#644).
    #[test]
    fn a_label_edited_in_place_stays_linked() {
        let mut app = App::headless();
        let id = forty(&mut app);
        let depth = app.engine.history_labels().0.len();
        type_label(&mut app, id, "W 40.00 mm max");
        assert_eq!(app.engine.history_labels().0.len(), depth + 1);
        let d = dimension(&app, id);
        assert_eq!(
            (d.style.prefix.as_str(), d.style.suffix.as_str()),
            ("W ", " max")
        );
        assert_eq!(d.text, None);
        app.run(Command::TransformShapes {
            shapes: vec![id],
            transform: Affine::scale(2.0),
        });
        app.bake_dimensions();
        assert_eq!(label_of(&app, id), "W 80.00 mm max");
        // Without the number: as typed, whatever the size.
        type_label(&mut app, id, "About eighty");
        assert_eq!(dimension(&app, id).text.as_deref(), Some("About eighty"));
        app.run(Command::TransformShapes {
            shapes: vec![id],
            transform: Affine::scale(0.5),
        });
        app.bake_dimensions();
        assert_eq!(label_of(&app, id), "About eighty");
        assert!(app.dimension_label_typed());
        app.measured_dimension_labels();
        assert_eq!(label_of(&app, id), "W 40.00 mm max");
        assert!(!app.dimension_label_typed());
        // A longer number is not the value with a prefix.
        let plain = forty(&mut app);
        type_label(&mut app, plain, "140.00 mm");
        let d = dimension(&app, plain);
        assert_eq!(d.text.as_deref(), Some("140.00 mm"));
        assert!(d.style.prefix.is_empty());
        // Deleted, the measured label comes back.
        let lid = label_id(&app, plain);
        app.run(Command::DeleteShapes { shapes: vec![lid] });
        app.bake_dimensions();
        assert_eq!(label_of(&app, plain), "40.00 mm");
        // Font, size and colour of the label become the style's.
        let lid = label_id(&app, plain);
        let mut t = app.doc().find_shape(lid).cloned().expect("label");
        if let ShapeKind::Text { spans, .. } = &mut t.kind {
            spans[0].font_family = "Other Font".into();
            spans[0].size_pt = 14.0;
        }
        let red = Color::rgb8(200, 0, 0);
        app.engine
            .run_batch(
                "Edit",
                &[
                    Command::SetShapeKind {
                        shape: lid,
                        kind: t.kind,
                    },
                    Command::SetFill {
                        shapes: vec![lid],
                        fill: Fill::Solid(red),
                    },
                ],
            )
            .expect("edit");
        app.bake_dimensions();
        let st = dimension(&app, plain).style;
        assert_eq!(
            (st.font.as_str(), st.text_size, st.text_color),
            ("Other Font", 14.0, red)
        );
        // Undo takes the edit and what was read back together.
        app.undo();
        app.bake_dimensions();
        assert_eq!(dimension(&app, plain).style.text_size, 10.0);
    }

    /// New units or a new font for new text do not make a measured label
    /// a typed one; a dimension saved before labels were noted reads its
    /// label as measured.
    #[test]
    fn new_units_or_default_font_keep_labels_measured() {
        let mut app = App::headless();
        let id = forty(&mut app);
        app.units = Units::Inches;
        app.text_font = "Another Font".into();
        app.run(Command::SetObjectData {
            shape: id,
            data: app.doc().find_shape(id).expect("g").data.clone(),
        });
        app.bake_dimensions();
        let d = dimension(&app, id);
        assert_eq!(d.text, None);
        assert!(d.style.font.is_empty() && d.style.prefix.is_empty());
        let mut old = d.clone();
        old.drawn = None;
        let json = serde_json::to_string(&old).expect("json");
        assert!(!json.contains("drawn") && !json.contains("\"text\""));
        let g = app.doc().find_shape(id).cloned().expect("g");
        assert_eq!(old.read_back(&g), None);
    }

    /// The extension lines' gap and overshoot, the arrowheads' size and a
    /// horizontal label; a style saved before them reads as before.
    #[test]
    fn more_options_shape_the_dimension() {
        let measure = Measure::Linear {
            a: Point::new(0.0, 0.0),
            b: Point::new(30.0, 30.0),
            at: Point::new(0.0, 20.0),
        };
        let ids = [ShapeId(1), ShapeId(2), ShapeId(3)];
        let plain = Dimension::new(measure, DimensionStyle::default());
        let [pl, ph, plabel] = plain.shapes(ids, "Arial", Units::Millimeters);
        let styled = Dimension::new(
            measure,
            DimensionStyle {
                extension_gap: 2.0,
                extension_overshoot: 5.0,
                arrow_size: 6.0,
                horizontal_text: true,
                ..Default::default()
            },
        );
        let [l, h, label] = styled.shapes(ids, "Arial", Units::Millimeters);
        let first = |s: &Shape| match &s.kind {
            ShapeKind::Path { path, .. } => match (path.elements()[0], path.elements()[1]) {
                (
                    tracedraw_core::geometry::PathEl::MoveTo(a),
                    tracedraw_core::geometry::PathEl::LineTo(b),
                ) => (a, b),
                other => panic!("{other:?}"),
            },
            _ => panic!("path"),
        };
        let (a0, b0) = first(&pl);
        let (a1, b1) = first(&l);
        assert!(a0.distance(Point::ZERO) < 1e-9);
        assert!((a1.distance(Point::ZERO) - 2.0).abs() < 1e-9, "{a1:?}");
        assert!((b1.distance(a1) - (b0.distance(a0) + 3.0 - 2.0)).abs() < 1e-9);
        // The first arrowhead's side, from its tip.
        let side = |s: &Shape| {
            let (tip, back) = first(s);
            tip.distance(back)
        };
        assert!((side(&h) / side(&ph) - 2.0).abs() < 1e-9);
        // The slanted label turns with its line, the horizontal one not.
        assert!(plabel.transform.as_coeffs()[1].abs() > 0.1);
        assert_eq!(label.transform.as_coeffs()[1], 0.0);
        let old: DimensionStyle = serde_json::from_str(r#"{"precision":1}"#).expect("old");
        assert_eq!(
            (
                old.extension_gap,
                old.extension_overshoot,
                old.arrow_size,
                old.horizontal_text
            ),
            (0.0, OVERSHOOT, ARROW_LEN, false)
        );
        let odd = DimensionStyle {
            arrow_size: f64::NAN,
            extension_gap: f64::INFINITY,
            ..Default::default()
        };
        assert_eq!(odd.lengths(), (0.0, OVERSHOOT, ARROW_LEN));
    }

    /// Joined, the extension lines run into the dimension line (or arc)
    /// in one curve with corners, nothing past them, the corners rounded
    /// by the corner radius; a centred label breaks it in two (#641).
    #[test]
    fn joined_lines_meet_in_rounded_corners() {
        use tracedraw_core::geometry::PathEl;
        let ids = [ShapeId(1), ShapeId(2), ShapeId(3)];
        let lines = |measure: Measure, style: DimensionStyle| {
            let [l, _, _] = Dimension::new(measure, style).shapes(ids, "Arial", Units::Millimeters);
            match l.kind {
                ShapeKind::Path { path, .. } => path,
                other => panic!("{other:?}"),
            }
        };
        let count =
            |p: &BezPath, f: fn(&PathEl) -> bool| p.elements().iter().filter(|e| f(e)).count();
        let linear = Measure::Linear {
            a: Point::new(0.0, 0.0),
            b: Point::new(40.0, 0.0),
            at: Point::new(20.0, 10.0),
        };
        let sharp = lines(
            linear,
            DimensionStyle {
                joined: true,
                ..Default::default()
            },
        );
        assert_eq!(count(&sharp, |e| matches!(e, PathEl::MoveTo(_))), 1);
        assert_eq!(count(&sharp, |e| matches!(e, PathEl::CurveTo(..))), 0);
        assert!(sharp
            .elements()
            .contains(&PathEl::LineTo(Point::new(0.0, 10.0))));
        // Nothing past the dimension line.
        assert!((sharp.bounding_box().y1 - 10.0).abs() < 1e-9);
        let round = lines(
            linear,
            DimensionStyle {
                joined: true,
                corner_radius: 2.0,
                ..Default::default()
            },
        );
        assert_eq!(count(&round, |e| matches!(e, PathEl::MoveTo(_))), 1);
        assert_eq!(count(&round, |e| matches!(e, PathEl::CurveTo(..))), 2);
        let corner = Point::new(0.0, 10.0);
        assert!(!round
            .elements()
            .iter()
            .any(|e| matches!(e, PathEl::LineTo(p) | PathEl::CurveTo(_, _, p) if *p == corner)));
        assert!(round
            .elements()
            .contains(&PathEl::LineTo(Point::new(0.0, 8.0))));
        let centred = lines(
            linear,
            DimensionStyle {
                joined: true,
                corner_radius: 2.0,
                text_position: TextPosition::Centered,
                ..Default::default()
            },
        );
        assert_eq!(count(&centred, |e| matches!(e, PathEl::MoveTo(_))), 2);
        // An angle: one curve from side to side through the arc.
        let angle = Measure::Angular {
            center: Point::ZERO,
            a: Point::new(30.0, 0.0),
            b: Point::new(0.0, 30.0),
            at: Point::new(14.0, 14.0),
        };
        let arc = lines(
            angle,
            DimensionStyle {
                joined: true,
                corner_radius: 3.0,
                ..Default::default()
            },
        );
        assert_eq!(count(&arc, |e| matches!(e, PathEl::MoveTo(_))), 1);
        assert!(count(&arc, |e| matches!(e, PathEl::CurveTo(..))) >= 3);
        match arc.elements().last() {
            Some(PathEl::LineTo(p)) => assert!(p.distance(Point::ZERO) < 1e-9),
            other => panic!("{other:?}"),
        }
        let r = Point::new(14.0, 14.0).distance(Point::ZERO);
        assert!(arc.bounding_box().x1 <= r + 1e-6, "nothing past the arc");
        let old: DimensionStyle = serde_json::from_str(r#"{"precision":1}"#).expect("old");
        assert_eq!((old.joined, old.corner_radius), (false, 0.0));
        let odd = DimensionStyle {
            corner_radius: f64::NAN,
            ..Default::default()
        };
        assert_eq!(odd.radius(), 0.0);
    }
}
