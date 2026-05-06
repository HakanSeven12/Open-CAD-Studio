// Ellipse tool — ribbon dropdown + all H7CAD ellipse creation methods.
//
// Commands:
//   ELLIPSE      — Center, Axes  (center → major endpoint → minor distance)
//   ELLIPSE_AXIS — Axis, End     (axis endpoint 1 → endpoint 2 → minor distance)
//   ELLIPSE_ARC  — Ellipse Arc   (shape as above, then start/end parametric angles)

use acadrust::types::Vector3;
use acadrust::{Ellipse, EntityType};

use crate::command::{CadCommand, CmdResult};
use crate::modules::IconKind;
use crate::scene::wire_model::WireModel;
use glam::Vec3;

fn parse_f32(text: &str) -> Option<f32> {
    text.trim().replace(',', ".").parse().ok()
}

const TAU: f32 = std::f32::consts::TAU;

// ── Icons ─────────────────────────────────────────────────────────────────

const ICON_CTR: IconKind = IconKind::Svg(include_bytes!(
    "../../../../assets/icons/ellipse/ellipse_ctr.svg"
));
const ICON_AXIS: IconKind = IconKind::Svg(include_bytes!(
    "../../../../assets/icons/ellipse/ellipse_axis.svg"
));
const ICON_ARC: IconKind = IconKind::Svg(include_bytes!(
    "../../../../assets/icons/ellipse/ellipse_arc.svg"
));

// ── Dropdown metadata ─────────────────────────────────────────────────────

pub const DROPDOWN_ID: &str = "ELLIPSE";

pub const DROPDOWN_ITEMS: &[(&str, &str, IconKind)] = &[
    ("ELLIPSE", "Center, Axes", ICON_CTR),
    ("ELLIPSE_AXIS", "Axis, End", ICON_AXIS),
    ("ELLIPSE_ARC", "Ellipse Arc", ICON_ARC),
];

pub const ICON: IconKind = ICON_CTR;

// ── Shared helpers ────────────────────────────────────────────────────────

/// Preview wire for a full or partial ellipse.
fn ellipse_wire(
    center: Vec3,
    major: Vec3, // vector from center to major-axis endpoint
    ratio: f32,  // minor/major
    t_start: f32,
    t_end: f32,
) -> WireModel {
    let r_major = major.length();
    if r_major < 1e-9 {
        return WireModel::solid("rubber_band".into(), vec![], WireModel::CYAN, false);
    }
    let major_dir = major / r_major;
    let v = major_dir.cross(Vec3::Z).normalize();
    let segs = 64u32;
    // Unwrap t_end so the arc goes counter-clockwise.
    let t_e = if t_end <= t_start { t_end + TAU } else { t_end };
    let pts: Vec<[f32; 3]> = (0..=segs)
        .map(|i| {
            let t = t_start + (t_e - t_start) * (i as f32 / segs as f32);
            let p = center + t.cos() * r_major * major_dir + t.sin() * r_major * ratio * v;
            [p.x, p.y, p.z]
        })
        .collect();
    WireModel::solid("rubber_band".into(), pts, WireModel::CYAN, false)
}

/// Convert a world point to the parametric angle on the ellipse.
fn param_angle(center: Vec3, major_dir: Vec3, v: Vec3, pt: Vec3, ratio: f32) -> f32 {
    let d = pt - center;
    let u_proj = d.dot(major_dir);
    let v_proj = d.dot(v);
    // Inverse-map: on ellipse x=cos(t)*r_major, y=sin(t)*r_major*ratio
    // → t = atan2(v_proj / (r_major*ratio), u_proj / r_major) but we normalise
    v_proj.atan2(u_proj * ratio).rem_euclid(TAU)
}

/// Build the final Ellipse entity.
fn make_ellipse(center: Vec3, major: Vec3, ratio: f32, t_start: f32, t_end: f32) -> Ellipse {
    Ellipse {
        center: Vector3::new(center.x as f64, center.y as f64, center.z as f64),
        major_axis: Vector3::new(major.x as f64, major.y as f64, major.z as f64),
        minor_axis_ratio: ratio as f64,
        start_parameter: t_start as f64,
        end_parameter: t_end as f64,
        ..Default::default()
    }
}

// ── 1. Center mode ────────────────────────────────────────────────────────
//   Step 1: center   Step 2: major-axis endpoint   Step 3: minor-axis point

enum CtrStep {
    Center,
    MajorAxis { center: Vec3 },
    MinorRatio { center: Vec3, major: Vec3 },
}

pub struct EllipseCommand {
    step: CtrStep,
}

impl EllipseCommand {
    pub fn new() -> Self {
        Self {
            step: CtrStep::Center,
        }
    }
}

impl CadCommand for EllipseCommand {
    fn name(&self) -> &'static str {
        "ELLIPSE"
    }

    fn prompt(&self) -> String {
        match &self.step {
            CtrStep::Center => "ELLIPSE  Specify center:".into(),
            CtrStep::MajorAxis { .. } => "ELLIPSE  Specify major axis endpoint:".into(),
            CtrStep::MinorRatio { major, .. } => format!(
                "ELLIPSE  Specify minor axis point or type half-length  [major r={:.3}]:",
                major.length()
            ),
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        match &self.step {
            CtrStep::Center => {
                self.step = CtrStep::MajorAxis { center: pt };
                CmdResult::NeedPoint
            }
            CtrStep::MajorAxis { center } => {
                let center = *center;
                self.step = CtrStep::MinorRatio {
                    center,
                    major: pt - center,
                };
                CmdResult::NeedPoint
            }
            CtrStep::MinorRatio { center, major } => {
                let (center, major) = (*center, *major);
                let ratio = minor_ratio(center, major, pt);
                CmdResult::CommitAndExit(EntityType::Ellipse(make_ellipse(
                    center, major, ratio, 0.0, TAU,
                )))
            }
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    fn on_text_input(&mut self, text: &str) -> Option<CmdResult> {
        if let CtrStep::MinorRatio { center, major } = &self.step {
            let r_minor = parse_f32(text)?;
            if r_minor > 0.0 {
                let ratio = (r_minor / major.length()).clamp(1e-6, 1.0);
                return Some(CmdResult::CommitAndExit(EntityType::Ellipse(make_ellipse(
                    *center, *major, ratio, 0.0, TAU,
                ))));
            }
        }
        None
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        match &self.step {
            CtrStep::MajorAxis { center } => Some(line_wire(*center, pt)),
            CtrStep::MinorRatio { center, major } => {
                let ratio = minor_ratio(*center, *major, pt).max(0.001);
                Some(ellipse_wire(*center, *major, ratio, 0.0, TAU))
            }
            _ => None,
        }
    }
}

// ── 2. Axis, End mode ─────────────────────────────────────────────────────
//   Step 1: axis endpoint 1   Step 2: axis endpoint 2   Step 3: minor-axis point

enum AxisStep {
    Pt1,
    Pt2 { p1: Vec3 },
    MinorRatio { center: Vec3, major: Vec3 },
}

pub struct EllipseAxisCommand {
    step: AxisStep,
}

impl EllipseAxisCommand {
    pub fn new() -> Self {
        Self {
            step: AxisStep::Pt1,
        }
    }
}

impl CadCommand for EllipseAxisCommand {
    fn name(&self) -> &'static str {
        "ELLIPSE_AXIS"
    }

    fn prompt(&self) -> String {
        match &self.step {
            AxisStep::Pt1 => "ELLIPSE (Axis)  Specify first endpoint of major axis:".into(),
            AxisStep::Pt2 { .. } => "ELLIPSE (Axis)  Specify second endpoint of major axis:".into(),
            AxisStep::MinorRatio { major, .. } => format!(
                "ELLIPSE (Axis)  Specify minor axis point or type half-length  [major r={:.3}]:",
                major.length()
            ),
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        match &self.step {
            AxisStep::Pt1 => {
                self.step = AxisStep::Pt2 { p1: pt };
                CmdResult::NeedPoint
            }
            AxisStep::Pt2 { p1 } => {
                let center = (*p1 + pt) * 0.5;
                let major = pt - center; // half-vector
                self.step = AxisStep::MinorRatio { center, major };
                CmdResult::NeedPoint
            }
            AxisStep::MinorRatio { center, major } => {
                let (center, major) = (*center, *major);
                let ratio = minor_ratio(center, major, pt);
                CmdResult::CommitAndExit(EntityType::Ellipse(make_ellipse(
                    center, major, ratio, 0.0, TAU,
                )))
            }
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    fn on_text_input(&mut self, text: &str) -> Option<CmdResult> {
        if let AxisStep::MinorRatio { center, major } = &self.step {
            let r_minor = parse_f32(text)?;
            if r_minor > 0.0 {
                let ratio = (r_minor / major.length()).clamp(1e-6, 1.0);
                return Some(CmdResult::CommitAndExit(EntityType::Ellipse(make_ellipse(
                    *center, *major, ratio, 0.0, TAU,
                ))));
            }
        }
        None
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        match &self.step {
            AxisStep::Pt1 => None,
            AxisStep::Pt2 { p1 } => Some(line_wire(*p1, pt)),
            AxisStep::MinorRatio { center, major } => {
                let ratio = minor_ratio(*center, *major, pt).max(0.001);
                Some(ellipse_wire(*center, *major, ratio, 0.0, TAU))
            }
        }
    }
}

// ── 3. Ellipse Arc mode ───────────────────────────────────────────────────
//   Same shape steps as Center mode, then: start parameter, end parameter.

enum ArcStep {
    Center,
    MajorAxis {
        center: Vec3,
    },
    MinorRatio {
        center: Vec3,
        major: Vec3,
    },
    StartAngle {
        center: Vec3,
        major: Vec3,
        ratio: f32,
    },
    EndAngle {
        center: Vec3,
        major: Vec3,
        ratio: f32,
        t_start: f32,
    },
}

pub struct EllipseArcCommand {
    step: ArcStep,
    prev_pt: Option<Vec3>,
    cw: bool,
}

impl EllipseArcCommand {
    pub fn new() -> Self {
        Self {
            step: ArcStep::Center,
            prev_pt: None,
            cw: false,
        }
    }
}

impl CadCommand for EllipseArcCommand {
    fn name(&self) -> &'static str {
        "ELLIPSE_ARC"
    }

    fn prompt(&self) -> String {
        match &self.step {
            ArcStep::Center => "ELLIPSE ARC  Specify center:".into(),
            ArcStep::MajorAxis { .. } => "ELLIPSE ARC  Specify major axis endpoint:".into(),
            ArcStep::MinorRatio { major, .. } => format!(
                "ELLIPSE ARC  Specify minor axis point or type half-length  [major r={:.3}]:",
                major.length()
            ),
            ArcStep::StartAngle { .. } => {
                "ELLIPSE ARC  Specify start angle point or type degrees:".into()
            }
            ArcStep::EndAngle { t_start, .. } => format!(
                "ELLIPSE ARC  Specify end angle point or type degrees  [start={:.1}°]:",
                t_start.to_degrees()
            ),
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        match &self.step {
            ArcStep::Center => {
                self.step = ArcStep::MajorAxis { center: pt };
                CmdResult::NeedPoint
            }
            ArcStep::MajorAxis { center } => {
                let center = *center;
                self.step = ArcStep::MinorRatio {
                    center,
                    major: pt - center,
                };
                CmdResult::NeedPoint
            }
            ArcStep::MinorRatio { center, major } => {
                let (center, major) = (*center, *major);
                let ratio = minor_ratio(center, major, pt);
                self.step = ArcStep::StartAngle {
                    center,
                    major,
                    ratio,
                };
                CmdResult::NeedPoint
            }
            ArcStep::StartAngle {
                center,
                major,
                ratio,
            } => {
                let (center, major, ratio) = (*center, *major, *ratio);
                let t_start = angle_from_point(center, major, ratio, pt);
                self.prev_pt = None; // reset direction tracking for end-angle step
                self.step = ArcStep::EndAngle {
                    center,
                    major,
                    ratio,
                    t_start,
                };
                CmdResult::NeedPoint
            }
            ArcStep::EndAngle {
                center,
                major,
                ratio,
                t_start,
            } => {
                let (center, major, ratio, t_start) = (*center, *major, *ratio, *t_start);
                let t_end = angle_from_point(center, major, ratio, pt);
                let entity = if self.cw {
                    make_ellipse(center, major, ratio, t_end, t_start)
                } else {
                    make_ellipse(center, major, ratio, t_start, t_end)
                };
                CmdResult::CommitAndExit(EntityType::Ellipse(entity))
            }
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        CmdResult::Cancel
    }

    fn on_text_input(&mut self, text: &str) -> Option<CmdResult> {
        let val = parse_f32(text)?;
        match &self.step {
            ArcStep::MinorRatio { center, major } => {
                if val > 0.0 {
                    let ratio = (val / major.length()).clamp(1e-6, 1.0);
                    let (c, m) = (*center, *major);
                    self.step = ArcStep::StartAngle {
                        center: c,
                        major: m,
                        ratio,
                    };
                    return Some(CmdResult::NeedPoint);
                }
            }
            ArcStep::StartAngle {
                center,
                major,
                ratio,
            } => {
                let t_start = val.to_radians();
                let (c, m, r) = (*center, *major, *ratio);
                self.prev_pt = None;
                self.step = ArcStep::EndAngle {
                    center: c,
                    major: m,
                    ratio: r,
                    t_start,
                };
                return Some(CmdResult::NeedPoint);
            }
            ArcStep::EndAngle {
                center,
                major,
                ratio,
                t_start,
            } => {
                // Typed degrees: positive = CCW, negative = CW.
                let t_end = val.to_radians();
                return Some(CmdResult::CommitAndExit(EntityType::Ellipse(make_ellipse(
                    *center, *major, *ratio, *t_start, t_end,
                ))));
            }
            _ => {}
        }
        None
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        match &self.step {
            ArcStep::MajorAxis { center } => Some(line_wire(*center, pt)),
            ArcStep::MinorRatio { center, major } => {
                let ratio = minor_ratio(*center, *major, pt).max(0.001);
                Some(ellipse_wire(*center, *major, ratio, 0.0, TAU))
            }
            ArcStep::StartAngle {
                center,
                major,
                ratio,
            } => {
                let t = angle_from_point(*center, *major, *ratio, pt);
                Some(ellipse_wire(*center, *major, *ratio, 0.0, t))
            }
            ArcStep::EndAngle {
                center,
                major,
                ratio,
                t_start,
            } => {
                // Detect rotation direction via cross product.
                if let Some(prev) = self.prev_pt {
                    let p = prev - *center;
                    let c = pt - *center;
                    let sign = p.x * c.y - p.y * c.x;
                    if sign.abs() > 1e-6 {
                        self.cw = sign < 0.0;
                    }
                }
                self.prev_pt = Some(pt);
                let t_end = angle_from_point(*center, *major, *ratio, pt);
                Some(if self.cw {
                    ellipse_wire(*center, *major, *ratio, t_end, *t_start)
                } else {
                    ellipse_wire(*center, *major, *ratio, *t_start, t_end)
                })
            }
            _ => None,
        }
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────

fn minor_ratio(center: Vec3, major: Vec3, pt: Vec3) -> f32 {
    let r_major = major.length();
    if r_major < 1e-9 {
        return 0.5;
    }
    let major_dir = major / r_major;
    let to_pt = pt - center;
    let perp = to_pt - major_dir * to_pt.dot(major_dir);
    let r_minor = perp.length().max(1e-6);
    (r_minor / r_major).clamp(1e-6, 1.0)
}

fn angle_from_point(center: Vec3, major: Vec3, ratio: f32, pt: Vec3) -> f32 {
    let r_major = major.length();
    if r_major < 1e-9 {
        return 0.0;
    }
    let major_dir = major / r_major;
    let v = major_dir.cross(Vec3::Z).normalize();
    param_angle(center, major_dir, v, pt, ratio)
}

fn line_wire(from: Vec3, to: Vec3) -> WireModel {
    WireModel {
        name: "rubber_band".into(),
        points: vec![[from.x, from.y, from.z], [to.x, to.y, to.z]],
        color: WireModel::CYAN,
        selected: false,
        pattern_length: 0.0,
        pattern: [0.0; 8],
        line_weight_px: 1.0,
        snap_pts: vec![],
        tangent_geoms: vec![],
        aci: 0,
            key_vertices: vec![],
            aabb: WireModel::UNBOUNDED_AABB,
            plinegen: true,
            vp_scissor: None,
    }
}
