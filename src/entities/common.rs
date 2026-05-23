use std::cell::Cell;

use glam::Vec3;

use crate::scene::object::{GripDef, GripShape, PropValue, Property};

/// Linear / angular unit format pulled from the document header so the
/// per-thread properties pipeline can format values consistently without
/// passing the document through every callsite.
#[derive(Clone, Copy, Default)]
pub struct UnitContext {
    /// LUNITS — 1=Sci, 2=Decimal, 3=Engineering, 4=Architectural, 5=Fractional
    pub lunits: i16,
    /// LUPREC — decimal places (linear)
    pub luprec: i16,
    /// AUNITS — 0=Decimal degrees, 1=DMS, 2=Grad, 3=Rad. Surfaced via
    /// `format_angle`, which is read on demand by code that already
    /// formats angular values via radians.
    #[allow(dead_code)]
    pub aunits: i16,
    /// AUPREC — decimal places (angular)
    #[allow(dead_code)]
    pub auprec: i16,
}

thread_local! {
    static UNIT_CTX: Cell<UnitContext> = const { Cell::new(UnitContext {
        lunits: 2,
        luprec: 4,
        aunits: 0,
        auprec: 0,
    }) };
}

/// Set the per-thread unit context. Properties helpers consult it when
/// they format f64 values into display strings.
pub fn set_unit_context(ctx: UnitContext) {
    UNIT_CTX.with(|c| c.set(ctx));
}

pub fn unit_context() -> UnitContext {
    UNIT_CTX.with(|c| c.get())
}

/// Format a linear length using LUNITS / LUPREC. Architectural / fractional
/// produce "n'-d/D"" style strings (1 unit = 1 inch); decimal / scientific /
/// engineering / Windows-desktop fall back to plain decimal at LUPREC places.
pub fn format_length(value: f64) -> String {
    let ctx = unit_context();
    let prec = ctx.luprec.max(0) as usize;
    match ctx.lunits {
        1 => format!("{:.*e}", prec, value),
        3 => {
            // Engineering: ft-inches, decimal inches.
            let sign = if value < 0.0 { "-" } else { "" };
            let abs = value.abs();
            let feet = (abs / 12.0).trunc();
            let rem = abs - feet * 12.0;
            format!("{}{:.0}'-{:.*}\"", sign, feet, prec, rem)
        }
        4 | 5 => {
            // Architectural / Fractional — n + fraction with 1/2^p denom (1
            // unit = 1 inch). Use 6 as a moderate denominator power so the
            // result reads like 1/64".
            let sign = if value < 0.0 { "-" } else { "" };
            let abs = value.abs();
            let (feet, in_rem) = if ctx.lunits == 4 {
                let f = (abs / 12.0).trunc();
                (Some(f as i64), abs - f * 12.0)
            } else {
                (None, abs)
            };
            let whole = in_rem.trunc();
            let frac = in_rem - whole;
            let denom = 64u64;
            let numer = (frac * denom as f64).round() as i64;
            let mut n = numer as u64;
            let mut d = denom;
            while d > 1 && n % 2 == 0 && d % 2 == 0 {
                n /= 2;
                d /= 2;
            }
            let frac_str = if n == 0 || d == 1 {
                String::new()
            } else {
                format!(" {}/{}", n, d)
            };
            let unit_suffix = if ctx.lunits == 4 { "\"" } else { "" };
            match feet {
                Some(f) => format!("{}{}'-{:.0}{}{}", sign, f, whole, frac_str, unit_suffix),
                None => format!("{}{:.0}{}", sign, whole, frac_str),
            }
        }
        _ => format!("{:.*}", prec, value),
    }
}

/// Format an angle (input in radians) using AUNITS / AUPREC.
#[allow(dead_code)]
pub fn format_angle(value_rad: f64) -> String {
    let ctx = unit_context();
    let prec = ctx.auprec.max(0) as usize;
    match ctx.aunits {
        1 => {
            // DMS — degrees / minutes / seconds.
            let deg = value_rad.to_degrees();
            let sign = if deg < 0.0 { "-" } else { "" };
            let a = deg.abs();
            let d = a.floor();
            let m_full = (a - d) * 60.0;
            let m = m_full.floor();
            let s = (m_full - m) * 60.0;
            format!("{}{:.0}°{:.0}'{:.*}\"", sign, d, m, prec, s)
        }
        2 => {
            let g = value_rad.to_degrees() / 0.9;
            format!("{:.*}g", prec, g)
        }
        3 => format!("{:.*}r", prec, value_rad),
        _ => format!("{:.*}°", prec, value_rad.to_degrees()),
    }
}

pub fn square_grip(id: usize, world: Vec3) -> GripDef {
    GripDef {
        id,
        world,
        is_midpoint: false,
        shape: GripShape::Square,
    }
}

pub fn diamond_grip(id: usize, world: Vec3) -> GripDef {
    GripDef {
        id,
        world,
        is_midpoint: true,
        shape: GripShape::Diamond,
    }
}

pub fn triangle_grip(id: usize, world: Vec3) -> GripDef {
    GripDef {
        id,
        world,
        is_midpoint: false,
        shape: GripShape::Triangle,
    }
}

pub fn edit_prop(label: &'static str, field: &'static str, value: f64) -> Property {
    Property {
        label: label.into(),
        field,
        value: PropValue::EditText(format_length(value)),
    }
}

pub fn ro_prop(label: &'static str, field: &'static str, value: impl Into<String>) -> Property {
    Property {
        label: label.into(),
        field,
        value: PropValue::ReadOnly(value.into()),
    }
}

pub fn parse_f64(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok()
}
/// Compute the filled boundary polygon for one polyline segment.
/// For straight segments: a rectangle/trapezoid.
/// For arc segments: an arc band (outer arc + reversed inner arc).
/// Returns `None` if the segment is degenerate.
pub(crate) fn polyline_segment_fill(
    p0: [f32; 2],
    p1: [f32; 2],
    hw0: f32,
    hw1: f32,
    bulge: f32,
) -> Option<Vec<[f32; 2]>> {
    if bulge.abs() < 1e-9 {
        // Straight segment — rectangle or trapezoid
        let dx = p1[0] - p0[0];
        let dy = p1[1] - p0[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            return None;
        }
        let nx = -dy / len;
        let ny = dx / len;
        Some(vec![
            [p0[0] + hw0 * nx, p0[1] + hw0 * ny],
            [p1[0] + hw1 * nx, p1[1] + hw1 * ny],
            [p1[0] - hw1 * nx, p1[1] - hw1 * ny],
            [p0[0] - hw0 * nx, p0[1] - hw0 * ny],
        ])
    } else {
        // Arc segment — arc band polygon.
        // Center math matches `bulge_to_arc` in modules/home/modify/explode.rs:
        //   r = chord * (1 + b²) / (4·|b|)
        //   d = r * (1 - b²) / (1 + b²)   (signed: negative ⇒ major arc, center
        //                                  flips to the opposite side of chord)
        //   center = midpoint + sign(b) · d · left_perp(chord)
        let b = bulge as f64;
        let b2 = b * b;
        let dx = (p1[0] - p0[0]) as f64;
        let dy = (p1[1] - p0[1]) as f64;
        let chord_len = (dx * dx + dy * dy).sqrt();
        if chord_len < 1e-9 || b.abs() < 1e-12 {
            return None;
        }
        let r = chord_len * (1.0 + b2) / (4.0 * b.abs());
        let d_perp = r * (1.0 - b2) / (1.0 + b2);
        let mx = ((p0[0] + p1[0]) * 0.5) as f64;
        let my = ((p0[1] + p1[1]) * 0.5) as f64;
        let perp_x = -dy / chord_len;
        let perp_y = dx / chord_len;
        let sign = b.signum();
        let cx = (mx + sign * d_perp * perp_x) as f32;
        let cy = (my + sign * d_perp * perp_y) as f32;
        let a0 = ((p0[1] - cy) as f32).atan2((p0[0] - cx) as f32);
        let a1 = ((p1[1] - cy) as f32).atan2((p1[0] - cx) as f32);
        let (sa, mut ea) = if bulge > 0.0 { (a0, a1) } else { (a1, a0) };
        if ea < sa {
            ea += std::f32::consts::TAU;
        }
        let span = ea - sa;
        let segs = ((span.abs() / std::f32::consts::TAU) * 24.0)
            .ceil()
            .max(4.0) as u32;
        let r = r as f32;
        let r_outer = |t: f32| r + (hw0 + (hw1 - hw0) * t);
        let r_inner = |t: f32| (r - (hw0 + (hw1 - hw0) * t)).max(0.0);
        let mut boundary = Vec::with_capacity((segs as usize + 1) * 2);
        let inv = 1.0 / segs as f32;
        for j in 0..=segs {
            let t = j as f32 * inv;
            let ang = sa + span * t;
            let ro = r_outer(t);
            boundary.push([cx + ro * ang.cos(), cy + ro * ang.sin()]);
        }
        for j in (0..=segs).rev() {
            let t = j as f32 * inv;
            let ang = sa + span * t;
            let ri = r_inner(t);
            boundary.push([cx + ri * ang.cos(), cy + ri * ang.sin()]);
        }
        Some(boundary)
    }
}
