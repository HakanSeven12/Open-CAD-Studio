// Tessellation — convert acadrust EntityType to GPU-ready WireModel or MeshModel.
//
// Flow:
//   EntityType
//     ↓  acad_to_truck::convert()
//   TruckEntity  { object: TruckObject, snap_pts, tangent_geoms, key_vertices }
//     ↓  truck_tess::tessellate_*()
//   TruckTessResult::Lines → WireModel
//   TruckTessResult::Point → WireModel (small cross)
//   TruckTessResult::Mesh  → MeshModel
//   TruckObject::Text      → one WireModel per glyph stroke (elevation from entity Z)
//
// Entities not handled by acad_to_truck (Viewport, Hatch, …) are tessellated
// by the legacy geometry() path so nothing regresses.

use crate::entities::multileader::{catmull_rom_pts, v_offset_for_attachment};
use acadrust::entities::{
    Dimension, Leader, LeaderContentType, MultiLeader, MultiLeaderPathType, Text,
    TextAttachmentPointType,
};
use acadrust::tables::DimStyle;
use acadrust::types::{Color as AcadColor, Vector3};
use acadrust::{CadDocument, EntityType, Handle};
use glam::Vec3;

use crate::scene::acad_to_truck::{convert, TruckObject};
use crate::scene::truck_tess::{
    tessellate_edge, tessellate_vertex, tessellate_wire, TruckTessResult,
};
use crate::scene::wire_model::{SnapHint, TangentGeom, WireModel};

// ── Arc tessellation helpers ─────────────────────────────────────────────

/// Convert hatch-boundary arc `(start, end, ccw)` into a
/// `(start, signed_span)` ready for the sampling loop. Matches the
/// legacy `(TAU - sa, TAU - ea)` flip used here for years — direction
/// semantics are preserved on real files. (Wrap-through-2π is a known
/// edge case in that convention; do not "fix" it without a wider audit
/// of how upstream writers emit CW boundary arcs.)
pub(super) fn arc_signed_span(start: f64, end: f64, ccw: bool) -> (f64, f64) {
    const TAU: f64 = std::f64::consts::TAU;
    let (sa, ea) = if ccw { (start, end) } else { (TAU - start, TAU - end) };
    (sa, ea - sa)
}

/// Segment count for an arc, targeting `chord_tol_world` chord-height
/// error in world units. Floor 8, cap 512.
///
/// Two production callers:
/// - hatch fill polygon (built at load / on edit) passes a radius-
///   relative tol via [`fill_chord_tol`] — ~0.1% radius, zoom-free so
///   the polygon stays sharp at extreme zoom-in without re-tessellation.
/// - hatch wire outline (re-tessellated every frame inside the render
///   scope) passes a zoom-adaptive tol via [`wire_chord_tol`] — pulls
///   from the per-frame override set by `Scene::wires_for_block` so
///   far-out arcs collapse to a handful of segments.
pub(super) fn arc_segments(radius: f64, span_abs: f64, chord_tol_world: f64) -> u32 {
    if span_abs < 1e-9 || radius < 1e-9 {
        return 1;
    }
    let tol = chord_tol_world.max(1e-9).min(radius * 0.99);
    // θ where r * (1 - cos(θ/2)) = tol  →  θ = 2 * acos(1 - tol/r).
    let max_step = (2.0 * (1.0 - tol / radius).acos()).max(1e-6);
    ((span_abs / max_step).ceil() as u32).clamp(8, 512)
}

/// Chord tolerance for the load-time fill polygon: 0.1% of radius,
/// floor 1 µm so degenerate radii still produce a workable count.
pub(super) fn fill_chord_tol(radius: f64) -> f64 {
    (radius * 0.001).max(1e-6)
}

/// Chord tolerance for the per-frame wire outline: pulls the active
/// `truck_tess::set_curve_tol_override` value (Scene sets it to
/// `world_per_pixel × 0.5` so curves stay at ~half-pixel chord error at
/// the current zoom). When no override is active (snap / hit-test
/// passes, load-time builds), falls back to [`fill_chord_tol`] so we
/// never under-sample.
pub(super) fn wire_chord_tol(radius: f64) -> f64 {
    match crate::scene::truck_tess::active_curve_tol() {
        Some(t) => t.min(fill_chord_tol(radius)).max(1e-9),
        None => fill_chord_tol(radius),
    }
}

// ── Colour helper ──────────────────────────────────────────────────────────

/// Convert an acadrust Color (ACI index or true-color) to a GPU RGBA value.
pub fn aci_to_rgba(color: &AcadColor) -> [f32; 4] {
    if let Some((r, g, b)) = color.rgb() {
        [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
    } else {
        WireModel::WHITE
    }
}

// ── Public entry points ────────────────────────────────────────────────────

/// Tessellate one entity into a WireModel.
/// For Text/MText entities this produces one WireModel with all glyph strokes
/// encoded as NaN-separated segments (wire_gpu skips NaN pairs).
/// For Solid3D entities this returns an empty wire; mesh tessellation lives
/// in `solid3d_tess` and is uploaded via the mesh pipeline instead.
pub fn tessellate(
    document: &CadDocument,
    handle: Handle,
    entity: &EntityType,
    selected: bool,
    entity_color: [f32; 4],
    pattern_length: f32,
    pattern: [f32; 8],
    line_weight_px: f32,
    world_offset: [f64; 3],
    anno_scale: f32,
) -> WireModel {
    let color = if selected {
        WireModel::SELECTED
    } else {
        entity_color
    };
    let name = handle.value().to_string();

    // Determine effective annotation scale for this entity.
    //
    // AutoCAD's R2007+ "annotative" system marks objects via extension-
    // dictionary records or "AcAnnoPO" / "AcAnnotativeData" xdata. Only
    // entities so marked should be auto-scaled by the viewport's
    // paper-scale; everything else is treated as manually pre-scaled
    // (old DXF/DWG convention with $DIMSCALE and oversized text).
    //
    // Default: NOT annotative (anno_scale = 1.0). Opt-in via explicit
    // xdata marker. Files that mark every entity annotative are rare; the
    // pre-R2007 manual-scale convention is far more common in field data.
    let anno_scale = {
        let xdata = &entity.common().extended_data;
        let is_annotative = xdata.get_record("AcAnnoPO").is_some()
            || xdata.get_record("AcAnnotativeData").is_some();
        if is_annotative {
            anno_scale
        } else {
            1.0
        }
    };

    // MultiLeader is handled by scene/mod.rs since it emits multiple WireModels
    // (leader, text, frame, fill) with distinct colors.
    if let EntityType::Leader(leader) = entity {
        return tessellate_leader_single(
            document,
            handle,
            leader,
            selected,
            entity_color,
            line_weight_px,
            world_offset,
            anno_scale,
        );
    }

    // ── Try the truck path first ───────────────────────────────────────────
    if let Some(te) = convert(entity, document) {
        match te.object {
            // ── Text / MText: pre-tessellated glyph strokes ───────────────
            TruckObject::Text(stroke_groups) => {
                // Each TextStroke keeps its strokes in glyph-local space and
                // its world origin as f64.  Subtract world_offset in f64 before
                // casting to f32 so large UTM coordinates don't crush precision.
                let [ox, oy, oz] = world_offset;
                let elev = entity_z(entity) - oz as f32;

                let mut points: Vec<[f32; 3]> = Vec::new();
                let mut first = true;
                // Annotation scale: scale glyph strokes relative to the text
                // insertion point (first group's origin) so multi-line MText
                // lines spread apart correctly as well as growing in size.
                let ref_origin = stroke_groups
                    .first()
                    .map(|g| g.origin)
                    .unwrap_or([0.0, 0.0]);
                let ref_lx = (ref_origin[0] - ox) as f32;
                let ref_ly = (ref_origin[1] - oy) as f32;
                for group in &stroke_groups {
                    let lx = (group.origin[0] - ox) as f32;
                    let ly = (group.origin[1] - oy) as f32;
                    let slx = (lx - ref_lx) * anno_scale + ref_lx;
                    let sly = (ly - ref_ly) * anno_scale + ref_ly;
                    for stroke in &group.strokes {
                        if stroke.len() < 2 {
                            continue;
                        }
                        if !first && !points.is_empty() {
                            points.push([f32::NAN, f32::NAN, f32::NAN]);
                        }
                        first = false;
                        for &[x, y] in stroke {
                            points.push([x * anno_scale + slx, y * anno_scale + sly, elev]);
                        }
                    }
                }

                let snap_pts = offset_snap_pts(te.snap_pts, world_offset);
                let [ox, oy, oz] = world_offset;
                let key_vertices: Vec<[f32; 3]> = te
                    .key_vertices
                    .into_iter()
                    .map(|[x, y, z]| [(x - ox) as f32, (y - oy) as f32, (z - oz) as f32])
                    .collect();
                return WireModel {
                    name,
                    points,
                    color,
                    selected,
                    pattern_length: 0.0,
                    pattern: [0.0; 8],
                    line_weight_px,
                    snap_pts,
                    tangent_geoms: te.tangent_geoms,
                    aci: 0,
                    key_vertices,
                    aabb: WireModel::UNBOUNDED_AABB,
                    plinegen: true,
                    vp_scissor: None,
                    fill_tris: vec![],
                };
            }

            // ── Standard topology objects ─────────────────────────────────
            TruckObject::Point(v) => {
                let result = tessellate_vertex(&v, world_offset);
                match result {
                    TruckTessResult::Point([x, y, z]) => {
                        let s = 0.1_f32;
                        let snap_pts = offset_snap_pts(te.snap_pts, world_offset);
                        let [ox, oy, oz] = world_offset;
                        let key_vertices: Vec<[f32; 3]> = te
                            .key_vertices
                            .into_iter()
                            .map(|[kx, ky, kz]| {
                                [(kx - ox) as f32, (ky - oy) as f32, (kz - oz) as f32]
                            })
                            .collect();
                        return WireModel {
                            name,
                            points: vec![
                                [x - s, y, z],
                                [x + s, y, z],
                                [x, y - s, z],
                                [x, y + s, z],
                            ],
                            color,
                            selected,
                            pattern_length: 0.0,
                            pattern: [0.0; 8],
                            line_weight_px: 1.0,
                            snap_pts,
                            tangent_geoms: te.tangent_geoms,
                            aci: 0,
                            key_vertices,
                            aabb: WireModel::UNBOUNDED_AABB,
                            plinegen: true,
                            vp_scissor: None,
                            fill_tris: vec![],
                        };
                    }
                    _ => {}
                }
            }

            TruckObject::Curve(e) => {
                if let TruckTessResult::Lines(points) = tessellate_edge(&e, world_offset) {
                    let [ox, oy, oz] = world_offset;
                    let snap_pts = offset_snap_pts(te.snap_pts, world_offset);
                    let key_vertices: Vec<[f32; 3]> = te
                        .key_vertices
                        .into_iter()
                        .map(|[x, y, z]| [(x - ox) as f32, (y - oy) as f32, (z - oz) as f32])
                        .collect();
                    return WireModel {
                        name,
                        points,
                        color,
                        selected,
                        pattern_length,
                        pattern,
                        line_weight_px,
                        snap_pts,
                        tangent_geoms: te.tangent_geoms,
                        aci: 0,
                        key_vertices,
                        aabb: WireModel::UNBOUNDED_AABB,
                        plinegen: true,
                        vp_scissor: None,
                        fill_tris: vec![],
                    };
                }
            }

            TruckObject::Contour(w) => {
                if let TruckTessResult::Lines(points) = tessellate_wire(&w, world_offset) {
                    let [ox, oy, oz] = world_offset;
                    let snap_pts = offset_snap_pts(te.snap_pts, world_offset);
                    let key_vertices: Vec<[f32; 3]> = te
                        .key_vertices
                        .into_iter()
                        .map(|[x, y, z]| [(x - ox) as f32, (y - oy) as f32, (z - oz) as f32])
                        .collect();
                    return WireModel {
                        name,
                        points,
                        color,
                        selected,
                        pattern_length,
                        pattern,
                        line_weight_px,
                        snap_pts,
                        tangent_geoms: te.tangent_geoms,
                        aci: 0,
                        key_vertices,
                        aabb: WireModel::UNBOUNDED_AABB,
                        plinegen: true,
                        vp_scissor: None,
                        fill_tris: vec![],
                    };
                }
            }

            TruckObject::Lines(points) => {
                // Points are world-space f64 from entity converters (polyline,
                // leader, mesh, solid2d, etc.). Subtract world_offset in f64
                // before casting to f32 so drawings at large UTM-style
                // coordinates keep sub-unit precision in the wire model.
                let [ox, oy, oz] = world_offset;
                let local_pts: Vec<[f32; 3]> = points
                    .into_iter()
                    .map(|[x, y, z]| {
                        if x.is_nan() {
                            [f32::NAN, f32::NAN, f32::NAN]
                        } else {
                            [(x - ox) as f32, (y - oy) as f32, (z - oz) as f32]
                        }
                    })
                    .collect();
                let snap_pts = offset_snap_pts(te.snap_pts, world_offset);
                let key_vertices: Vec<[f32; 3]> = te
                    .key_vertices
                    .into_iter()
                    .map(|[x, y, z]| [(x - ox) as f32, (y - oy) as f32, (z - oz) as f32])
                    .collect();
                let fill_tris: Vec<[f32; 3]> = te
                    .fill_tris
                    .into_iter()
                    .map(|[x, y, z]| [(x - ox) as f32, (y - oy) as f32, (z - oz) as f32])
                    .collect();
                return WireModel {
                    name,
                    points: local_pts,
                    color,
                    selected,
                    pattern_length: 0.0,
                    pattern: [0.0; 8],
                    line_weight_px,
                    snap_pts,
                    tangent_geoms: te.tangent_geoms,
                    aci: 0,
                    key_vertices,
                    aabb: WireModel::UNBOUNDED_AABB,
                    plinegen: true,
                    vp_scissor: None,
                    fill_tris,
                };
            }

            TruckObject::SegmentedLines(points) => {
                let [ox, oy, oz] = world_offset;
                let local_pts: Vec<[f32; 3]> = points
                    .into_iter()
                    .map(|[x, y, z]| {
                        if x.is_nan() {
                            [f32::NAN, f32::NAN, f32::NAN]
                        } else {
                            [(x - ox) as f32, (y - oy) as f32, (z - oz) as f32]
                        }
                    })
                    .collect();
                let snap_pts = offset_snap_pts(te.snap_pts, world_offset);
                let key_vertices: Vec<[f32; 3]> = te
                    .key_vertices
                    .into_iter()
                    .map(|[x, y, z]| [(x - ox) as f32, (y - oy) as f32, (z - oz) as f32])
                    .collect();
                return WireModel {
                    name,
                    points: local_pts,
                    color,
                    selected,
                    pattern_length,
                    pattern,
                    line_weight_px,
                    snap_pts,
                    tangent_geoms: te.tangent_geoms,
                    aci: 0,
                    key_vertices,
                    plinegen: false,
                    vp_scissor: None,
                    aabb: WireModel::UNBOUNDED_AABB,
                    fill_tris: vec![],
                };
            }

            TruckObject::Volume(_) => {
                // Solid3D / Region / Body → mesh tessellation lives in
                // `solid3d_tess`. As a wire fallback, render the pre-computed
                // edge wires stored in the entity when present (e.g. from
                // SOLVIEW output or when the SAT kernel cannot parse the
                // ACIS data).
                let wire_pts = solid_wire_fallback(entity, world_offset);
                let mut wm = WireModel::solid(name, wire_pts, color, selected);
                // Add insertion snap at point_of_reference.
                let [ox, oy, oz] = world_offset;
                let por = match entity {
                    EntityType::Solid3D(s) => Some(&s.point_of_reference),
                    EntityType::Region(r) => Some(&r.point_of_reference),
                    EntityType::Body(b) => Some(&b.point_of_reference),
                    _ => None,
                };
                if let Some(p) = por {
                    let sp = Vec3::new((p.x - ox) as f32, (p.y - oy) as f32, (p.z - oz) as f32);
                    wm.snap_pts.push((sp, SnapHint::Insertion));
                }
                return wm;
            }
        }
    }

    // ── Legacy fallback for Viewport and other unhandled types ────────────
    let (points, snap_pts, tangent_geoms, key_vertices) = legacy_geometry(entity, world_offset);
    WireModel {
        name,
        points,
        color,
        selected,
        aci: 0,
        pattern_length,
        pattern,
        line_weight_px,
        snap_pts,
        tangent_geoms,
        key_vertices,
        aabb: WireModel::UNBOUNDED_AABB,
        plinegen: true,
        vp_scissor: None,
        fill_tris: vec![],
    }
}

pub fn tessellate_dimension(
    document: &CadDocument,
    handle: Handle,
    dim: &Dimension,
    selected: bool,
    entity_color: [f32; 4],
    line_weight_px: f32,
    world_offset: [f64; 3],
    anno_scale: f32,
    // LOD hints — when present, synthesised dim text routes through the
    // top-level LOD ladder (baseline / greek / full) instead of the truck
    // path so far-out drawings collapse to a colored rect or baseline.
    selected_set: &std::collections::HashSet<acadrust::Handle>,
    active_viewport: Option<acadrust::Handle>,
    bg_color: [f32; 4],
    view_aabb: Option<[f32; 4]>,
    world_per_pixel: Option<f32>,
) -> Vec<WireModel> {
    let name = handle.value().to_string();
    // (Baked-block fast path moved up into scene::tessellate_entity so the
    // recursive call goes through the LOD ladder, not the truck path.)

    let style_name = &dim.base().style_name;
    let style = document.dim_styles.iter().find(|s| {
        s.name.eq_ignore_ascii_case(style_name)
            || (style_name.trim().is_empty() && s.name.eq_ignore_ascii_case("Standard"))
    });

    // DIMSCALE rule:
    //   dimstyle.dimscale > 0  →  final multiplier; ignore anno_scale.
    //   dimstyle.dimscale == 0 →  annotative: use anno_scale (= 1/vp_scale).
    let dim_scale = style
        .map(|s| {
            if s.dimscale > 1e-6 {
                s.dimscale
            } else {
                anno_scale as f64
            }
        })
        .unwrap_or(1.0);

    let (
        dimasz_raw,
        dimexo,
        dimexe,
        dim_txt,
        dimtsz_raw,
        dimsah,
        dimse1,
        dimse2,
        dimsd1,
        dimsd2,
        dimdle,
        dimfxl,
        dimfxlon,
        dimsoxd,
        dimcen,
    ) = style
        .map(|s| {
            (
                s.dimasz * dim_scale,
                (s.dimexo * dim_scale) as f32,
                (s.dimexe * dim_scale) as f32,
                s.dimtxt * dim_scale,
                s.dimtsz * dim_scale,
                s.dimsah,
                s.dimse1,
                s.dimse2,
                s.dimsd1,
                s.dimsd2,
                (s.dimdle * dim_scale) as f32,
                (s.dimfxl * dim_scale) as f32,
                s.dimfxlon,
                s.dimsoxd,
                (s.dimcen * dim_scale) as f32,
            )
        })
        .unwrap_or((
            0.18, 0.0, 0.0, 2.5, 0.0, false, false, false, false, false, 0.0, 1.0, false, false,
            0.09,
        ));

    // Arrow selection precedence:
    //   1. DIMTSZ>0 → oblique tick (overrides DIMBLK*).
    //   2. DIMSAH false → DIMBLK on both ends.
    //   3. DIMSAH true  → DIMBLK1 (first end), DIMBLK2 (second end).
    // Unknown / NULL block handles fall back to ClosedFilled.
    let dimasz = (dimasz_raw as f32).max(0.001);
    let (arrow1, arrow2) = if dimtsz_raw > 1e-9 {
        let t = ArrowKind::Tick {
            size: (dimtsz_raw as f32).max(0.001),
        };
        (t, t)
    } else if let Some(s) = style {
        if dimsah {
            (
                arrow_from_block(document, s.dimblk1, dimasz),
                arrow_from_block(document, s.dimblk2, dimasz),
            )
        } else {
            let a = arrow_from_block(document, s.dimblk, dimasz);
            (a, a)
        }
    } else {
        let a = ArrowKind::Triangle {
            size: dimasz,
            filled: true,
            size_mul: 1.0,
        };
        (a, a)
    };

    let mut geom = dimension_geometry(
        dim,
        &arrow1,
        &arrow2,
        DimLineParams {
            dimexo,
            dimexe,
            dimdle,
            dimfxl,
            dimfxlon,
            dimsoxd,
            dimcen,
            ticks: dimtsz_raw > 1e-9,
        },
        SuppressFlags {
            ext1: dimse1,
            ext2: dimse2,
            dim1: dimsd1,
            dim2: dimsd2,
        },
        world_offset,
    );

    // DIMTMOVE = 1: when the saved text_middle_point sits far from the
    // dim-line anchor, draw a short leader connecting them. (=0 anchors text
    // to the dim line — no leader; =2 frees text without a leader.)
    if let Some(s) = style {
        if s.dimtmove == 1 {
            if let Some((anchor, txt)) = dimtmove_leader_endpoints(dim, world_offset) {
                let gap = dim_txt as f32 * 0.5;
                if (txt - anchor).length() > gap * 2.0 {
                    add_segment(&mut geom.dim_lines, anchor, txt);
                }
            }
        }
        // DIMTOFL / DIMTIX / DIMATFIT / DIMUPT control autofit behaviour at
        // dim *creation*. At render time we honour the saved text and arrow
        // positions, so reading them here is a no-op — they shape geometry
        // upstream rather than here.
        let _ = (s.dimtofl, s.dimtix, s.dimatfit, s.dimupt);
        // DIMTXTDIRECTION (RTL) needs per-instance text mirroring on the Text
        // entity, which the current text struct can't carry. Tracked: read
        // and ignore so the file round-trips on save.
        let _ = s.dimtxtdirection;
        // DIMARCSYM only applies to arc-length dims; DIMJOGANG only to
        // jogged-radius dims. We don't ship those Dimension variants yet,
        // so the values are read for round-trip but not drawn.
        let _ = (s.dimarcsym, s.dimjogang);
        // DIMUNIT is the obsolete pre-R2000 linear unit format; DIMLUNIT
        // supersedes it. Read but not honoured.
        let _ = s.dimunit;
    }
    // Dimension entity fields that the render path doesn't yet use but are
    // preserved on save:
    //   - base.insertion_point: legacy anchor reference; render uses
    //     text_middle_point + dim-line geometry instead.
    //   - base.block_name: AutoCAD-style "*D..." anonymous block name for
    //     the dim graphics — we re-tessellate so don't need it.
    //   - base.version: DXF format marker (metadata only).
    let _ = (
        dim.base().insertion_point,
        &dim.base().block_name,
        dim.base().version,
    );

    // Per-spec colours: DIMCLRD (dim/arrows), DIMCLRE (ext), DIMCLRT (text).
    // 0=ByBlock and 256=ByLayer fall through to entity_color.
    let dim_color = if selected {
        WireModel::SELECTED
    } else {
        resolve_dim_color(style.map(|s| s.dimclrd).unwrap_or(0), entity_color)
    };
    let ext_color = if selected {
        WireModel::SELECTED
    } else {
        resolve_dim_color(style.map(|s| s.dimclre).unwrap_or(0), entity_color)
    };
    let text_color = if selected {
        entity_color // text wire color set by inner tessellate; keep entity tint
    } else {
        resolve_dim_color(style.map(|s| s.dimclrt).unwrap_or(0), entity_color)
    };

    let snap_pts = dimension_snap_pts(dim, world_offset);
    let key_vertices: Vec<[f32; 3]> = geom
        .dim_lines
        .iter()
        .chain(geom.ext_lines.iter())
        .copied()
        .filter(|p| !(p[0].is_nan() || p[1].is_nan() || p[2].is_nan()))
        .collect();

    // DIMLWD (dim line + arrows) and DIMLWE (extension lines). Negative
    // codes fall through to the entity's own resolved weight.
    let lw_dim = resolve_dim_lineweight_px(
        style.map(|s| s.dimlwd).unwrap_or(-2),
        line_weight_px,
    );
    let lw_ext = resolve_dim_lineweight_px(
        style.map(|s| s.dimlwe).unwrap_or(-2),
        line_weight_px,
    );

    // DIMLTEX (dim line) / DIMLTEX1 (ext1) / DIMLTEX2 (ext2) — linetype
    // handles → pattern. Looked up in document.line_types by handle.
    let lt_scale = document.header.linetype_scale as f32 * dim.base().common.linetype_scale as f32;
    let (dim_pat_len, dim_pat) = style
        .map(|s| resolve_pattern_by_handle(document, s.dimltex_handle, lt_scale))
        .unwrap_or((0.0, [0.0; 8]));
    let (ext1_pat_len, ext1_pat) = style
        .map(|s| resolve_pattern_by_handle(document, s.dimltex1_handle, lt_scale))
        .unwrap_or((0.0, [0.0; 8]));
    let (ext2_pat_len, ext2_pat) = style
        .map(|s| resolve_pattern_by_handle(document, s.dimltex2_handle, lt_scale))
        .unwrap_or((0.0, [0.0; 8]));

    let mut wires = Vec::new();

    if !geom.ext_lines.is_empty() {
        // If ext1 and ext2 have different linetypes, split into two wires so
        // each can carry its own pattern. Otherwise emit as a single wire.
        let split = ext1_pat_len != ext2_pat_len || ext1_pat != ext2_pat;
        if split {
            let (ext1, ext2) = split_ext_lines(&geom.ext_lines);
            if !ext1.is_empty() {
                wires.push(WireModel {
                    name: name.clone(),
                    points: ext1,
                    color: ext_color,
                    selected,
                    aci: 0,
                    pattern_length: ext1_pat_len,
                    pattern: ext1_pat,
                    line_weight_px: lw_ext,
                    snap_pts: vec![],
                    tangent_geoms: vec![],
                    key_vertices: vec![],
                    aabb: WireModel::UNBOUNDED_AABB,
                    plinegen: true,
                    vp_scissor: None,
                    fill_tris: vec![],
                });
            }
            if !ext2.is_empty() {
                wires.push(WireModel {
                    name: name.clone(),
                    points: ext2,
                    color: ext_color,
                    selected,
                    aci: 0,
                    pattern_length: ext2_pat_len,
                    pattern: ext2_pat,
                    line_weight_px: lw_ext,
                    snap_pts: vec![],
                    tangent_geoms: vec![],
                    key_vertices: vec![],
                    aabb: WireModel::UNBOUNDED_AABB,
                    plinegen: true,
                    vp_scissor: None,
                    fill_tris: vec![],
                });
            }
        } else {
            wires.push(WireModel {
                name: name.clone(),
                points: geom.ext_lines,
                color: ext_color,
                selected,
                aci: 0,
                pattern_length: ext1_pat_len,
                pattern: ext1_pat,
                line_weight_px: lw_ext,
                snap_pts: vec![],
                tangent_geoms: vec![],
                key_vertices: vec![],
                aabb: WireModel::UNBOUNDED_AABB,
                plinegen: true,
                vp_scissor: None,
                fill_tris: vec![],
            });
        }
    }

    wires.push(WireModel {
        name: name.clone(),
        points: geom.dim_lines,
        color: dim_color,
        selected,
        aci: 0,
        pattern_length: dim_pat_len,
        pattern: dim_pat,
        line_weight_px: lw_dim,
        snap_pts,
        tangent_geoms: vec![],
        key_vertices,
        aabb: WireModel::UNBOUNDED_AABB,
        plinegen: true,
        vp_scissor: None,
        fill_tris: geom.arrow_fill,
    });

    // DIMTFILL: 0=none, 1=drawing background (transparent → skip), 2=DIMTFILLCLR.
    if let Some(s) = style {
        if s.dimtfill == 2 {
            if let Some(rect) = text_fill_rect(dim, style, dim_txt, world_offset) {
                let fill_color = if selected {
                    WireModel::SELECTED
                } else {
                    let c = AcadColor::from_index(s.dimtfillclr);
                    aci_to_rgba(&c)
                };
                wires.push(WireModel {
                    name: name.clone(),
                    points: vec![],
                    color: fill_color,
                    selected,
                    aci: 0,
                    pattern_length: 0.0,
                    pattern: [0.0; 8],
                    line_weight_px: 1.0,
                    snap_pts: vec![],
                    tangent_geoms: vec![],
                    key_vertices: vec![],
                    aabb: WireModel::UNBOUNDED_AABB,
                    plinegen: true,
                    vp_scissor: None,
                    fill_tris: rect,
                });
            }
        }
    }

    if let Some(text) = dimension_text_entity(dim, dim_txt, style, document) {
        // Tolerance Text rendered separately so DIMTFAC scales its height
        // and DIMTOLJ aligns it vertically against the primary text.
        let tol_entity = dimension_tolerance_entity(dim, style, &text, dim_txt);
        // Route synthesised dim text through tessellate_entity so the
        // baseline/greek/full LOD ladder applies (zoom-out behaviour
        // matches top-level Text). The text already has dim_scale baked
        // into its height, so anno_scale stays 1.0.
        let synth_text_entity = EntityType::Text(text);
        let text_wires = crate::scene::tessellate_entity_dim_text(
            document,
            selected_set,
            active_viewport,
            world_offset,
            bg_color,
            1.0,
            &synth_text_entity,
            view_aabb,
            world_per_pixel,
            text_color,
        );
        for mut w in text_wires {
            w.name = name.clone();
            wires.push(w);
        }

        if let Some(tol_text) = tol_entity {
            let tol_entity_e = EntityType::Text(tol_text);
            let tol_wires = crate::scene::tessellate_entity_dim_text(
                document,
                selected_set,
                active_viewport,
                world_offset,
                bg_color,
                1.0,
                &tol_entity_e,
                view_aabb,
                world_per_pixel,
                text_color,
            );
            for mut w in tol_wires {
                w.name = name.clone();
                wires.push(w);
            }
        }
    }

    wires
}

fn resolve_dim_color(idx: i16, fallback: [f32; 4]) -> [f32; 4] {
    // DIMCLR* convention: 0 = BYBLOCK, 256 = BYLAYER → entity colour wins.
    if idx == 0 || idx == 256 {
        return fallback;
    }
    aci_to_rgba(&AcadColor::from_index(idx))
}

/// Resolve a DIMLWD / DIMLWE table value (the i16 lineweight code) into a
/// pixel width. -1 (ByLayer) / -2 (ByBlock) / -3 (Default) fall through to
/// the entity's already-resolved width.
fn resolve_dim_lineweight_px(code: i16, fallback_px: f32) -> f32 {
    const MM_TO_PX: f32 = 96.0 / 25.4;
    if code < 0 {
        return fallback_px;
    }
    // i16 value 0..=211 represents 1/100 mm.
    let mm = code as f32 / 100.0;
    (mm * MM_TO_PX).max(1.0)
}

/// Look up a linetype in the document's line_types table by handle and
/// resolve it to a (pattern_length, pattern) pair compatible with WireModel.
fn resolve_pattern_by_handle(
    doc: &CadDocument,
    handle: acadrust::types::Handle,
    scale: f32,
) -> (f32, [f32; 8]) {
    if handle.is_null() {
        return (0.0, [0.0; 8]);
    }
    let name = doc
        .line_types
        .iter()
        .find(|lt| lt.handle == handle)
        .map(|lt| lt.name.clone());
    match name {
        Some(n) => crate::scene::render::resolve_pattern(&doc.line_types, &n, scale),
        None => (0.0, [0.0; 8]),
    }
}

/// Split the combined ext-lines point list (NaN-separated segment pairs)
/// into "first" / "second" halves. `append_linear_dimension` writes ext1
/// before ext2, so the first segment is ext1 and the second is ext2.
fn split_ext_lines(points: &[[f32; 3]]) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    let mut groups: Vec<Vec<[f32; 3]>> = Vec::new();
    let mut current: Vec<[f32; 3]> = Vec::new();
    for &p in points {
        if p[0].is_nan() {
            if !current.is_empty() {
                groups.push(std::mem::take(&mut current));
            }
        } else {
            current.push(p);
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    let mut iter = groups.into_iter();
    let first = iter.next().unwrap_or_default();
    let rest: Vec<[f32; 3]> = iter.flatten().collect();
    (first, rest)
}

/// Endpoints for the DIMTMOVE=1 leader: (anchor on the dim line, saved
/// text_middle_point). Returns None when the dim has no saved text position
/// or has no well-defined dim-line midpoint (radius/diameter handled by
/// their own leg).
fn dimtmove_leader_endpoints(
    dim: &Dimension,
    world_offset: [f64; 3],
) -> Option<(Vec3, Vec3)> {
    let base = dim.base();
    let txt = base.text_middle_point;
    if txt.x * txt.x + txt.y * txt.y + txt.z * txt.z <= 1e-16 {
        return None;
    }
    let lv = |v| vec3_local(v, world_offset);
    let anchor = match dim {
        Dimension::Linear(d) => {
            let perp = Vec3::new(-(d.rotation.sin() as f32), d.rotation.cos() as f32, 0.0);
            let first = lv(d.first_point);
            let second = lv(d.second_point);
            let def = lv(d.definition_point);
            let off1 = def.dot(perp) - first.dot(perp);
            let off2 = def.dot(perp) - second.dot(perp);
            (first + perp * off1 + second + perp * off2) * 0.5
        }
        Dimension::Aligned(d) => {
            let first = lv(d.first_point);
            let second = lv(d.second_point);
            let axis = normalized_or(second - first, Vec3::X);
            let perp = Vec3::new(-axis.y, axis.x, 0.0);
            let def = lv(d.definition_point);
            let off1 = def.dot(perp) - first.dot(perp);
            let off2 = def.dot(perp) - second.dot(perp);
            (first + perp * off1 + second + perp * off2) * 0.5
        }
        Dimension::Radius(d) => lv(d.definition_point),
        Dimension::Diameter(d) => (lv(d.angle_vertex) + lv(d.definition_point)) * 0.5,
        _ => return None,
    };
    Some((anchor, lv(txt)))
}

/// Build a rectangle of filled triangles sitting under the dim text, used
/// when DIMTFILL = 2 (explicit fill colour). The rect width is estimated
/// from the formatted text length × character-cell width; an absolutely
/// correct box would need full text metrics from the font cache.
fn text_fill_rect(
    dim: &Dimension,
    style: Option<&DimStyle>,
    text_height: f64,
    world_offset: [f64; 3],
) -> Option<Vec<[f32; 3]>> {
    let value = dimension_text_value(dim, style)?;
    if value.is_empty() {
        return None;
    }
    let pos = dimension_text_pos_f64(dim, style, text_height);
    let dimgap = style.map(|s| s.dimgap).unwrap_or(0.0).max(0.0);
    // ~0.6 × text_height per character; matches average glyph aspect for
    // the bundled stick fonts. Inflate by 1 DIMGAP on each side.
    let approx_w =
        value.chars().count() as f64 * text_height * 0.6 + dimgap * 2.0;
    let approx_h = text_height + dimgap * 2.0;
    let rot = if dim.base().text_rotation.abs() > 1e-9 {
        dim.base().text_rotation
    } else {
        dimension_text_natural_rotation(dim)
    };
    let (sr, cr) = rot.sin_cos();
    let hx = approx_w * 0.5;
    let hy = approx_h * 0.5;
    let [ox, oy, oz] = world_offset;
    let cx = (pos.x - ox) as f32;
    let cy = (pos.y - oy) as f32;
    let cz = (pos.z - oz) as f32;
    let corner = |dx: f64, dy: f64| -> [f32; 3] {
        let lx = dx * cr - dy * sr;
        let ly = dx * sr + dy * cr;
        [cx + lx as f32, cy + ly as f32, cz]
    };
    let p1 = corner(-hx, -hy);
    let p2 = corner(hx, -hy);
    let p3 = corner(hx, hy);
    let p4 = corner(-hx, hy);
    Some(vec![p1, p2, p3, p1, p3, p4])
}

#[derive(Clone, Copy)]
enum ArrowKind {
    None,
    Triangle { size: f32, filled: bool, size_mul: f32 },
    Tick { size: f32 },
    Open { size: f32, half_angle: f32 },
    Dot { size: f32, filled: bool },
    Origin { size: f32 },
    Box_ { size: f32, filled: bool },
    Datum { size: f32, filled: bool },
}

fn arrow_from_block(
    doc: &CadDocument,
    handle: acadrust::types::Handle,
    dimasz: f32,
) -> ArrowKind {
    let name = if handle.is_null() {
        None
    } else {
        doc.block_records
            .iter()
            .find(|b| b.handle == handle)
            .map(|b| b.name.as_str())
    };
    arrow_from_block_name(name, dimasz)
}

fn arrow_from_block_name(name: Option<&str>, dimasz: f32) -> ArrowKind {
    // AutoCAD's standard arrow blocks are prefixed with "_" (e.g. "_OPEN").
    // Strip the prefix, upper-case, and switch on canonical names. Unknown
    // / missing names default to ClosedFilled.
    let n = name
        .map(|s| s.trim().trim_start_matches('_').to_ascii_uppercase())
        .unwrap_or_default();
    match n.as_str() {
        "" | "CLOSEDFILLED" => ArrowKind::Triangle {
            size: dimasz,
            filled: true,
            size_mul: 1.0,
        },
        "CLOSED" | "CLOSEDBLANK" => ArrowKind::Triangle {
            size: dimasz,
            filled: false,
            size_mul: 1.0,
        },
        "SMALL" => ArrowKind::Triangle {
            size: dimasz,
            filled: true,
            size_mul: 0.5,
        },
        "OPEN" => ArrowKind::Open {
            size: dimasz,
            half_angle: 9.5_f32.to_radians(),
        },
        "OPEN30" => ArrowKind::Open {
            size: dimasz,
            half_angle: 15.0_f32.to_radians(),
        },
        "OPEN90" => ArrowKind::Open {
            size: dimasz,
            half_angle: 45.0_f32.to_radians(),
        },
        "DOT" => ArrowKind::Dot {
            size: dimasz,
            filled: true,
        },
        "DOTSMALL" => ArrowKind::Dot {
            size: dimasz * 0.5,
            filled: true,
        },
        "DOTBLANK" => ArrowKind::Dot {
            size: dimasz,
            filled: false,
        },
        "DOTSMALLBLANK" => ArrowKind::Dot {
            size: dimasz * 0.5,
            filled: false,
        },
        "ORIGIN" | "ORIGIN2" | "ORIGININDICATOR" | "ORIGININDICATOR2" => {
            ArrowKind::Origin { size: dimasz }
        }
        "OBLIQUE" | "ARCHTICK" => ArrowKind::Tick { size: dimasz },
        "BOXFILLED" => ArrowKind::Box_ {
            size: dimasz,
            filled: true,
        },
        "BOXBLANK" | "BOX" => ArrowKind::Box_ {
            size: dimasz,
            filled: false,
        },
        "DATUMFILLED" | "DATUMTRIANGLEFILLED" => ArrowKind::Datum {
            size: dimasz,
            filled: true,
        },
        "DATUMBLANK" | "DATUMTRIANGLE" => ArrowKind::Datum {
            size: dimasz,
            filled: false,
        },
        "NONE" => ArrowKind::None,
        // INTEGRAL and other complex glyphs aren't reproduced here; fall through.
        _ => ArrowKind::Triangle {
            size: dimasz,
            filled: true,
            size_mul: 1.0,
        },
    }
}

#[derive(Clone, Copy, Default)]
struct SuppressFlags {
    ext1: bool,
    ext2: bool,
    dim1: bool,
    dim2: bool,
}

#[derive(Clone, Copy)]
struct DimLineParams {
    dimexo: f32,
    dimexe: f32,
    dimdle: f32,
    dimfxl: f32,
    dimfxlon: bool,
    dimsoxd: bool,
    dimcen: f32,
    ticks: bool,
}

struct DimGeom {
    ext_lines: Vec<[f32; 3]>,
    dim_lines: Vec<[f32; 3]>,
    arrow_fill: Vec<[f32; 3]>,
}

impl DimGeom {
    fn new() -> Self {
        Self {
            ext_lines: Vec::new(),
            dim_lines: Vec::new(),
            arrow_fill: Vec::new(),
        }
    }
}

fn tessellate_leader_single(
    document: &CadDocument,
    handle: Handle,
    leader: &Leader,
    selected: bool,
    entity_color: [f32; 4],
    line_weight_px: f32,
    world_offset: [f64; 3],
    anno_scale: f32,
) -> WireModel {
    let color = if selected {
        WireModel::SELECTED
    } else {
        entity_color
    };
    let name = handle.value().to_string();
    let [ox, oy, oz] = world_offset;
    let p3 =
        |v: &Vector3| -> [f32; 3] { [(v.x - ox) as f32, (v.y - oy) as f32, (v.z - oz) as f32] };
    let nan = [f32::NAN; 3];

    let verts = &leader.vertices;

    if verts.len() < 2 {
        return WireModel {
            name,
            points: vec![],
            color,
            selected,
            aci: 0,
            pattern_length: 0.0,
            pattern: [0.0; 8],
            line_weight_px,
            snap_pts: vec![],
            tangent_geoms: vec![],
            key_vertices: vec![],
            aabb: WireModel::UNBOUNDED_AABB,
            plinegen: true,
            vp_scissor: None,
            fill_tris: vec![],
        };
    }

    let mut points: Vec<[f32; 3]> = verts.iter().map(|v| p3(v)).collect();
    let mut tangents: Vec<TangentGeom> = Vec::new();
    let key_vertices: Vec<[f32; 3]> = verts.iter().map(|v| p3(v)).collect();
    let mut fill_tris: Vec<[f32; 3]> = Vec::new();

    for i in 0..verts.len().saturating_sub(1) {
        tangents.push(TangentGeom::Line {
            p1: p3(&verts[i]),
            p2: p3(&verts[i + 1]),
        });
    }

    if leader.arrow_enabled {
        // Resolve the active dim style → DIMLDRBLK to pick the arrow shape.
        // DIMASZ × DIMSCALE drives the size when available; otherwise fall
        // back to the legacy text-height heuristic.
        let style = document.dim_styles.iter().find(|s| {
            s.name.eq_ignore_ascii_case(&leader.dimension_style)
                || (leader.dimension_style.trim().is_empty()
                    && s.name.eq_ignore_ascii_case("Standard"))
        });
        let dim_scale = style
            .map(|s| {
                if s.dimscale > 1e-6 {
                    s.dimscale
                } else {
                    anno_scale as f64
                }
            })
            .unwrap_or(anno_scale as f64);
        let arrow_size = match style {
            Some(s) => (s.dimasz * dim_scale) as f32,
            None => (leader.text_height as f32).max(1.0) * 0.8 * anno_scale,
        };
        let arrow = match style {
            Some(s) => arrow_from_block(document, s.dimldrblk, arrow_size.max(0.001)),
            None => ArrowKind::Triangle {
                size: arrow_size.max(0.001),
                filled: true,
                size_mul: 1.0,
            },
        };

        let tip = &verts[0];
        let next = &verts[1];
        let dx = (next.x - tip.x) as f32;
        let dy = (next.y - tip.y) as f32;
        let len = (dx * dx + dy * dy).sqrt().max(1e-9);
        let dir = Vec3::new(dx / len, dy / len, 0.0);
        let tip_f = p3(tip);
        let tip_v = Vec3::new(tip_f[0], tip_f[1], tip_f[2]);
        // Reuse the dim arrow emitter so the leader shape matches the
        // DIMSTYLE in use (Closed Filled by default, Dot, Tick, …).
        let mut arrow_pts: Vec<[f32; 3]> = Vec::new();
        let mut arrow_geom = DimGeom::new();
        append_arrow(&mut arrow_geom, tip_v, dir, &arrow);
        if !arrow_geom.dim_lines.is_empty() {
            arrow_pts.push(nan);
            arrow_pts.extend(arrow_geom.dim_lines);
        }
        points.extend(arrow_pts);
        fill_tris.extend(arrow_geom.arrow_fill);
    }

    if leader.hookline_enabled {
        let last = verts.last().unwrap();
        let prev = &verts[verts.len() - 2];
        let sign = if (last.x - prev.x) >= 0.0 {
            1.0_f32
        } else {
            -1.0_f32
        };
        let land_len = leader.text_height as f32 * 1.5 * anno_scale;
        let last_f = p3(last);
        points.push(nan);
        points.push(last_f);
        points.push([last_f[0] + sign * land_len, last_f[1], last_f[2]]);
    }

    WireModel {
        name,
        points,
        color,
        selected,
        aci: 0,
        pattern_length: 0.0,
        pattern: [0.0; 8],
        line_weight_px,
        snap_pts: vec![],
        tangent_geoms: tangents,
        key_vertices,
        aabb: WireModel::UNBOUNDED_AABB,
        plinegen: true,
        vp_scissor: None,
        fill_tris,
    }
}

/// Convert an acadrust `Color` to RGBA, falling back to `inherited` for
/// `ByLayer` / `ByBlock` (assumes those are already resolved upstream).
fn color_or_inherit(c: &AcadColor, inherited: [f32; 4]) -> [f32; 4] {
    match c.rgb() {
        Some((r, g, b)) => [
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            inherited[3],
        ],
        None => inherited,
    }
}

pub fn tessellate_multileader(
    document: &CadDocument,
    handle: Handle,
    ml: &MultiLeader,
    selected: bool,
    entity_color: [f32; 4],
    line_weight_px: f32,
    world_offset: [f64; 3],
    anno_scale: f32,
    world_per_pixel: Option<f32>,
) -> Vec<WireModel> {
    // line_color falls back to the entity colour when the MultiLeader's own
    // colour is ByBlock/ByLayer; otherwise the leader uses its stored hue.
    let line_color = if selected {
        WireModel::SELECTED
    } else {
        color_or_inherit(&ml.line_color, entity_color)
    };
    // ml.line_weight is the leader-line weight override. Negative codes
    // (ByLayer/ByBlock/Default) fall through to the entity's already-resolved
    // pixel width.
    let leader_lw_px = match ml.line_weight {
        acadrust::types::LineWeight::Value(v) if v >= 0 => {
            (v as f32 / 100.0) * (96.0 / 25.4)
        }
        _ => line_weight_px,
    };
    // ml.line_type_handle — resolve via line_types table by handle and apply
    // the resulting dash pattern to the leader wire.
    let lt_scale = document.header.linetype_scale as f32 * ml.common.linetype_scale as f32;
    let (leader_pat_len, leader_pat) = match ml.line_type_handle {
        Some(h) if !h.is_null() => {
            let name = document
                .line_types
                .iter()
                .find(|lt| lt.handle == h)
                .map(|lt| lt.name.clone());
            match name {
                Some(n) => crate::scene::render::resolve_pattern(&document.line_types, &n, lt_scale),
                None => (0.0, [0.0; 8]),
            }
        }
        _ => (0.0, [0.0; 8]),
    };

    let name = handle.value().to_string();
    let nan = [f32::NAN; 3];
    let [ox, oy, oz] = world_offset;
    let p3 = |v: &acadrust::types::Vector3| -> [f32; 3] {
        [(v.x - ox) as f32, (v.y - oy) as f32, (v.z - oz) as f32]
    };

    // ── Scaling ──────────────────────────────────────────────────────────────
    // ml.scale_factor is always applied; anno_scale is only applied when the
    // multileader is marked annotative.
    let effective_scale = (ml.scale_factor as f32)
        * if ml.enable_annotation_scale {
            anno_scale
        } else {
            1.0
        };

    let arrow_size = ml.arrowhead_size as f32 * effective_scale;
    let draw_arrow = arrow_size > 0.0;
    let invisible = ml.path_type == MultiLeaderPathType::Invisible;
    // arrowhead_handle resolves through the block records to a named arrow
    // block (matches DIMLDRBLK on Dimension). Null handle / unknown name →
    // ClosedFilled triangle.
    let arrow_kind = match ml.arrowhead_handle {
        Some(h) if !h.is_null() => arrow_from_block(document, h, arrow_size.max(0.001)),
        _ => ArrowKind::Triangle {
            size: arrow_size.max(0.001),
            filled: true,
            size_mul: 1.0,
        },
    };

    // ── Leader / arrow / dogleg points ───────────────────────────────────────
    let mut points: Vec<[f32; 3]> = Vec::new();
    let mut key_verts: Vec<[f32; 3]> = Vec::new();
    let mut snap_pts: Vec<(Vec3, SnapHint)> = Vec::new();
    let mut tangents: Vec<TangentGeom> = Vec::new();
    let mut arrow_fill: Vec<[f32; 3]> = Vec::new();
    let mut first = true;

    for root in &ml.context.leader_roots {
        let cp = &root.connection_point;
        let cp_f = p3(cp);
        snap_pts.push((Vec3::from(cp_f), SnapHint::Node));

        for line in &root.lines {
            if line.points.is_empty() {
                continue;
            }

            if !invisible {
                if !first {
                    points.push(nan);
                }
                first = false;

                let mut ctrl: Vec<[f32; 3]> = line.points.iter().map(|p| p3(p)).collect();
                let last_f = *ctrl.last().unwrap_or(&cp_f);
                let dist = ((last_f[0] - cp_f[0]).powi(2) + (last_f[1] - cp_f[1]).powi(2)).sqrt();
                if dist > 1e-9 {
                    ctrl.push(cp_f);
                }
                for &c in &ctrl {
                    key_verts.push(c);
                    snap_pts.push((Vec3::from(c), SnapHint::Node));
                }

                if ml.path_type == MultiLeaderPathType::Spline && ctrl.len() >= 2 {
                    let ctrl_f64: Vec<[f64; 3]> = ctrl
                        .iter()
                        .map(|c| [c[0] as f64, c[1] as f64, c[2] as f64])
                        .collect();
                    for pt in catmull_rom_pts(&ctrl_f64, 8) {
                        points.push([pt[0] as f32, pt[1] as f32, pt[2] as f32]);
                    }
                } else {
                    for &c in &ctrl {
                        points.push(c);
                    }
                }
                for i in 0..ctrl.len().saturating_sub(1) {
                    tangents.push(TangentGeom::Line {
                        p1: ctrl[i],
                        p2: ctrl[i + 1],
                    });
                }
            }

            if draw_arrow {
                let tip = &line.points[0];
                let tip_f = p3(tip);
                let next = if line.points.len() >= 2 {
                    line.points[1]
                } else {
                    *cp
                };
                let dx = (next.x - tip.x) as f32;
                let dy = (next.y - tip.y) as f32;
                let dl = (dx * dx + dy * dy).sqrt().max(1e-9);
                let dir = Vec3::new(dx / dl, dy / dl, 0.0);
                let tip_v = Vec3::new(tip_f[0], tip_f[1], tip_f[2]);
                // Reuse the dim/leader arrow emitter so MultiLeader's arrow
                // matches the block referenced by arrowhead_handle.
                let mut arrow_geom = DimGeom::new();
                append_arrow(&mut arrow_geom, tip_v, dir, &arrow_kind);
                if !arrow_geom.dim_lines.is_empty() {
                    points.push(nan);
                    points.extend(arrow_geom.dim_lines);
                }
                arrow_fill.extend(arrow_geom.arrow_fill);
            }
        }

        if ml.enable_landing && ml.enable_dogleg && ml.dogleg_length > 0.0 {
            let dir = &root.direction;
            let dl = (dir.x * dir.x + dir.y * dir.y).sqrt().max(1e-9);
            let d = ml.dogleg_length * effective_scale as f64;
            let landing_end = [
                (cp.x + dir.x / dl * d - ox) as f32,
                (cp.y + dir.y / dl * d - oy) as f32,
                cp_f[2],
            ];
            points.push(nan);
            points.push(cp_f);
            points.push(landing_end);
            // extend_leader_to_text: continue past the landing to the text
            // insertion point along the same direction.
            if ml.extend_leader_to_text
                && ml.content_type == LeaderContentType::MText
                && !ml.context.text_string.is_empty()
            {
                let tx = (ml.context.text_location.x - ox) as f32;
                let ty = (ml.context.text_location.y - oy) as f32;
                points.push(landing_end);
                points.push([tx, ty, cp_f[2]]);
            }
        }
    }

    // The leader/arrow/dogleg wire goes out as a single WireModel. Text, frame,
    // and background fill (each with their own color) are appended as separate
    // WireModels so the renderer respects per-piece coloring.
    let mut wires: Vec<WireModel> = Vec::new();
    wires.push(WireModel {
        name: name.clone(),
        points,
        color: line_color,
        selected,
        aci: 0,
        pattern_length: leader_pat_len,
        pattern: leader_pat,
        line_weight_px: leader_lw_px,
        snap_pts,
        tangent_geoms: tangents,
        key_vertices: key_verts,
        aabb: WireModel::UNBOUNDED_AABB,
        plinegen: true,
        vp_scissor: None,
        fill_tris: arrow_fill,
    });

    // ── Block content ───────────────────────────────────────────────────────
    // When content_type == Block, the MultiLeader displays a block reference
    // at block_content_location with the recorded rotation/scale. Synthesize
    // an Insert and explode it through the standard tessellator. The block
    // resolves via block_content_handle (handle → block_record name).
    if ml.content_type == LeaderContentType::Block && ml.context.has_block_contents {
        let block_name = match ml.block_content_handle {
            Some(h) if !h.is_null() => document
                .block_records
                .iter()
                .find(|br| br.handle == h)
                .map(|br| br.name.clone()),
            _ => None,
        };
        if let Some(block_name) = block_name {
            let block_color = if selected {
                line_color
            } else {
                color_or_inherit(&ml.block_content_color, entity_color)
            };
            let mut synth_ins =
                acadrust::entities::Insert::new(block_name, ml.context.block_content_location);
            synth_ins.set_x_scale(ml.block_scale.x);
            synth_ins.set_y_scale(ml.block_scale.y);
            synth_ins.set_z_scale(ml.block_scale.z);
            synth_ins.rotation = ml.block_rotation;
            synth_ins.common.layer = ml.common.layer.clone();
            // block_connection_type (BlockExtents vs BasePoint) chooses the
            // anchor when *creating* the multileader; at render time the
            // file's stored leader endpoints already encode that choice.
            let _ = ml.block_connection_type;
            for sub in synth_ins.explode_from_document(document) {
                let normalized =
                    crate::modules::home::modify::explode::normalize_insert_entity(sub);
                let mut wire = tessellate(
                    document,
                    handle,
                    &normalized,
                    selected,
                    block_color,
                    leader_pat_len,
                    leader_pat,
                    leader_lw_px,
                    world_offset,
                    1.0,
                );
                wire.name = name.clone();
                wires.push(wire);
            }
            // Block attributes attached to the multileader — render each as
            // its own attribute entity at WCS location like INSERT does.
            for ba in &ml.block_attributes {
                let _ = ba; // BlockAttribute carries only the value override
                            // string; we'd need the AttributeDefinition handle
                            // to materialise it as ATTRIB geometry. Skipped
                            // until that wiring exists.
            }
        }
    }

    // ── Text strokes / frame / background fill ──────────────────────────────
    // Strip inline format codes, split / word-wrap into lines, then place each
    // line according to text_attachment_point (horizontal) and
    // text_left_attachment (vertical), with text_rotation/text_direction applied.
    if ml.content_type == LeaderContentType::MText && !ml.context.text_string.is_empty() {
        let ctx = &ml.context;
        // `ctx.text_height` (when > 0) is the already-resolved WCS text height
        // stored in the per-instance context — style × scale_factor × anno
        // scale are all already baked in. Multiplying by `effective_scale`
        // again would double-scale (e.g., a context height of 100 in a file
        // with scale_factor=20 was rendering at 2000 units — 20× too big).
        // Only the fallback path (file omits the context value) needs
        // scale_factor + annotation scale applied.
        let height = if ctx.text_height > 0.0 {
            ctx.text_height as f32
        } else {
            ml.text_height as f32 * effective_scale
        };

        let ins = &ctx.text_location;
        // Subtract world_offset in f64 before casting to f32: drawings often
        // sit at large absolute coordinates and casting first then subtracting
        // throws away the precision needed for the rotated sub-glyph offsets.
        let local_ins_x = (ins.x - ox) as f32;
        let local_ins_y = (ins.y - oy) as f32;
        let z = (ins.z - oz) as f32;

        // Rotation: prefer text_direction (transforms survive rotations /
        // mirrors when acadrust updates it) and fall back to text_rotation.
        // ml.text_angle_type then constrains the final angle:
        //   - Horizontal:        always 0
        //   - ParallelToLastLeaderLine: keep stored direction
        //   - Optimized:        clamp to (-π/2, π/2] (flip upside-down)
        // text_direction_negative finally adds π so reading goes the other way.
        let td = ctx.text_direction;
        let mut rot = if td.x.abs() > 1e-9 || td.y.abs() > 1e-9 {
            (td.y as f32).atan2(td.x as f32)
        } else {
            ctx.text_rotation as f32
        };
        match ml.text_angle_type {
            acadrust::entities::multileader::TextAngleType::Horizontal => rot = 0.0,
            acadrust::entities::multileader::TextAngleType::Optimized => {
                let pi = std::f32::consts::PI;
                if rot > pi / 2.0 {
                    rot -= pi;
                } else if rot <= -pi / 2.0 {
                    rot += pi;
                }
            }
            acadrust::entities::multileader::TextAngleType::ParallelToLastLeaderLine => {}
        }
        if ml.text_direction_negative {
            rot += std::f32::consts::PI;
        }

        // Resolve text style via handle when available, falling back to STANDARD.
        let style_name = ctx
            .text_style_handle
            .as_ref()
            .and_then(|h| {
                document
                    .text_styles
                    .iter()
                    .find(|s| s.handle == *h)
                    .map(|s| s.name.clone())
            })
            .unwrap_or_else(|| "STANDARD".to_string());
        let style = crate::entities::text_support::resolve_text_style(&style_name, document);
        let font_name = style.font_name;
        let font = crate::scene::cxf::get_font(&font_name);
        // MultiLeader text takes its width factor / oblique from the style
        // (no entity-level override exists for MText-content multileaders).
        // is_backward flips the sign on width_factor; is_upside_down adds π
        // to the text rotation — same convention as Text / MText.
        let base_wf = style.width_factor.max(0.01);
        let width_factor = if style.is_backward { -base_wf } else { base_wf };
        let oblique = style.oblique_angle;
        if style.is_upside_down {
            rot += std::f32::consts::PI;
        }
        let (cos_r, sin_r) = (rot.cos(), rot.sin());

        // Strip MText format codes (e.g. `{\fArial Black|b0|i0|c162|p34;...}`),
        // then split on \P / \n / \N and optionally word-wrap to text_width.
        let plain = crate::entities::text_support::strip_mtext_codes(&ctx.text_string);
        let explicit_lines = crate::entities::text_support::split_mtext_lines(&plain);
        let lines: Vec<String> = if ctx.text_width > 0.0 {
            // Measurement uses positive width magnitude; the sign on
            // width_factor is a render-time mirror flag.
            let scale = height / 9.0 * width_factor.abs();
            // ctx.text_width lives in the same WCS frame as ctx.text_height;
            // it does not need a second multiplication by effective_scale.
            let max_w = ctx.text_width as f32;
            explicit_lines
                .iter()
                .flat_map(|line| {
                    crate::entities::text_support::word_wrap(line, max_w, scale, font)
                })
                .collect()
        } else {
            explicit_lines
        };

        let ls_factor = if ctx.line_spacing_factor > 0.0 {
            ctx.line_spacing_factor as f32
        } else {
            1.0
        };
        let line_h = height * ls_factor * (5.0 / 3.0) * font.line_spacing;
        let n_lines = lines.len().max(1) as f32;

        // ml.text_alignment overrides the per-context attachment_point when
        // not Left (which is the default zero). Otherwise we honour the
        // context's stored attachment_point (Left/Center/Right).
        use acadrust::entities::multileader::{TextAlignmentType, TextAttachmentDirectionType};
        let h_anchor = match (ml.text_alignment, ctx.text_attachment_point) {
            (TextAlignmentType::Center, _) => 0.5_f32,
            (TextAlignmentType::Right, _) => 1.0,
            (TextAlignmentType::Left, TextAttachmentPointType::Center) => 0.5,
            (TextAlignmentType::Left, TextAttachmentPointType::Right) => 1.0,
            _ => 0.0,
        };
        // Pick the vertical-anchor attachment based on text_attachment_direction:
        //   Horizontal — leader attaches left/right; use ml.text_left_attachment
        //                (matches the file's stored ctx.text_left_attachment).
        //   Vertical   — leader attaches top/bottom; use ml.text_top_attachment
        //                or ml.text_bottom_attachment depending on which side
        //                the leader is coming from (chosen via the first
        //                root.direction.y sign).
        let leader_from_top = ml
            .context
            .leader_roots
            .first()
            .map(|r| r.direction.y < 0.0)
            .unwrap_or(true);
        let vertical_attach = match ml.text_attachment_direction {
            TextAttachmentDirectionType::Vertical => {
                if leader_from_top {
                    ml.text_top_attachment
                } else {
                    ml.text_bottom_attachment
                }
            }
            TextAttachmentDirectionType::Horizontal => ctx.text_left_attachment,
        };
        let v_offset = v_offset_for_attachment(vertical_attach, n_lines, height, line_h);

        // line_widths use positive magnitude for measurement; tessellate_text_ex
        // gets the signed width_factor below to honour the backward flag.
        let scale = height / 9.0 * width_factor.abs();
        let line_widths: Vec<f32> = lines
            .iter()
            .map(|line| crate::entities::text_support::measure_mtext_chars(line, scale, font))
            .collect();
        let max_line_w = line_widths.iter().cloned().fold(0.0_f32, f32::max);

        // Resolve text color (falls back to entity color for ByLayer / ByBlock).
        let text_color = if selected {
            line_color
        } else {
            color_or_inherit(&ctx.text_color, entity_color)
        };

        // Same LOD ladder used for top-level Text / MText (see scene/mod.rs):
        //   h_px < 1   → baseline line (skip glyphs)
        //   1 ≤ h < 5  → greeked rect in text color (skip glyphs)
        //   h_px ≥ 5   → full per-glyph stroke tessellation
        let lod_h_px = world_per_pixel.map(|wpp| height / wpp);
        let lod_mode = match lod_h_px {
            Some(h) if h < 1.0 => 0,
            Some(h) if h < 5.0 => 1,
            _ => 2,
        };

        // Helper: map a (local_x, local_y) in the text's pre-rotation frame
        // (origin at the insertion point) into WCS render space.
        let to_wcs = |lx: f32, ly: f32| -> [f32; 3] {
            [
                local_ins_x + lx * cos_r - ly * sin_r,
                local_ins_y + lx * sin_r + ly * cos_r,
                z,
            ]
        };

        if lod_mode == 0 {
            // Baseline of the top line only.
            let line_w = line_widths.first().copied().unwrap_or(0.0);
            let len_px = world_per_pixel
                .map(|wpp| line_w / wpp)
                .unwrap_or(f32::INFINITY);
            if len_px >= 2.0 {
                let line_y_local = v_offset;
                let p0 = to_wcs(-line_w * h_anchor, line_y_local);
                let p1 = to_wcs(line_w * (1.0 - h_anchor), line_y_local);
                wires.push(WireModel {
                    name: name.clone(),
                    points: vec![p0, p1],
                    color: text_color,
                    selected,
                    aci: 0,
                    pattern_length: 0.0,
                    pattern: [0.0; 8],
                    line_weight_px,
                    snap_pts: vec![(
                        Vec3::new(local_ins_x, local_ins_y, z),
                        SnapHint::Node,
                    )],
                    tangent_geoms: vec![],
                    key_vertices: vec![],
                    aabb: WireModel::UNBOUNDED_AABB,
                    plinegen: true,
                    vp_scissor: None,
                    fill_tris: vec![],
                });
            }
        } else if lod_mode == 1 {
            // One filled rect per line — keeps the visual "text lives here
            // per row" hint that multi-line MText carries, in the text's
            // own color. Empty `points` opts out of the face3d 0.45 dim so
            // the fill renders at full intensity.
            let mut greek_tris: Vec<[f32; 3]> = Vec::with_capacity(lines.len() * 6);
            for (i, _) in lines.iter().enumerate() {
                let li = i as f32;
                let line_y_bottom = -li * line_h + v_offset;
                let line_y_top = line_y_bottom + height;
                let line_w = line_widths[i];
                if line_w <= 0.0 {
                    continue;
                }
                let left = -line_w * h_anchor;
                let right = line_w * (1.0 - h_anchor);
                let bl = to_wcs(left, line_y_bottom);
                let br = to_wcs(right, line_y_bottom);
                let tr = to_wcs(right, line_y_top);
                let tl = to_wcs(left, line_y_top);
                greek_tris.extend_from_slice(&[bl, br, tr, bl, tr, tl]);
            }
            if !greek_tris.is_empty() {
                wires.push(WireModel {
                    name: name.clone(),
                    points: vec![],
                    color: text_color,
                    selected,
                    aci: 0,
                    pattern_length: 0.0,
                    pattern: [0.0; 8],
                    line_weight_px: 1.0,
                    snap_pts: vec![(
                        Vec3::new(local_ins_x, local_ins_y, z),
                        SnapHint::Node,
                    )],
                    tangent_geoms: vec![],
                    key_vertices: vec![],
                    aabb: WireModel::UNBOUNDED_AABB,
                    plinegen: true,
                    vp_scissor: None,
                    fill_tris: greek_tris,
                });
            }
        } else {
            // Build per-line stroke points in WCS.
            let mut text_points: Vec<[f32; 3]> = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                let li = i as f32;
                let line_y_local = -li * line_h + v_offset;
                let line_w = line_widths[i];
                let h_shift_local = -line_w * h_anchor;
                let wcs_dx = h_shift_local * cos_r - line_y_local * sin_r;
                let wcs_dy = h_shift_local * sin_r + line_y_local * cos_r;
                // Origin already in offset-relative space — tessellator will rotate
                // glyph offsets around it and produce points directly in render space.
                let origin = [local_ins_x + wcs_dx, local_ins_y + wcs_dy];
                let strokes = crate::scene::cxf::tessellate_text_ex(
                    origin,
                    height,
                    rot,
                    width_factor,
                    oblique,
                    &font_name,
                    line,
                );
                for stroke in &strokes {
                    if stroke.len() < 2 {
                        continue;
                    }
                    text_points.push(nan);
                    for &[x, y] in stroke {
                        text_points.push([x, y, z]);
                    }
                }
            }

            if !text_points.is_empty() {
                wires.push(WireModel {
                    name: name.clone(),
                    points: text_points,
                    color: text_color,
                    selected,
                    aci: 0,
                    pattern_length: 0.0,
                    pattern: [0.0; 8],
                    line_weight_px,
                    snap_pts: vec![(
                        Vec3::new(local_ins_x, local_ins_y, z),
                        SnapHint::Node,
                    )],
                    tangent_geoms: vec![],
                    key_vertices: vec![],
                    aabb: WireModel::UNBOUNDED_AABB,
                    plinegen: true,
                    vp_scissor: None,
                    fill_tris: vec![],
                });
            }
        }

        // Text frame / background-fill rectangle in local frame, then rotated to WCS.
        if ml.text_frame || ctx.background_fill_enabled {
            // Visual gap so the frame/fill doesn't touch glyph caps.
            let pad = height * 0.25;
            let block_top = v_offset + height + pad;
            let block_bottom = v_offset - (n_lines - 1.0) * line_h - pad;
            let block_left = -max_line_w * h_anchor - pad;
            let block_right = max_line_w * (1.0 - h_anchor) + pad;
            let local_corners: [[f32; 2]; 4] = [
                [block_left, block_bottom],
                [block_right, block_bottom],
                [block_right, block_top],
                [block_left, block_top],
            ];
            let wcs_corners: [[f32; 3]; 4] = std::array::from_fn(|i| {
                let lx = local_corners[i][0];
                let ly = local_corners[i][1];
                let wx = local_ins_x + lx * cos_r - ly * sin_r;
                let wy = local_ins_y + lx * sin_r + ly * cos_r;
                [wx, wy, z]
            });

            // Background fill — emit two triangles; renders under the text strokes.
            if ctx.background_fill_enabled {
                let fill_color = if selected {
                    line_color
                } else {
                    color_or_inherit(&ctx.background_fill_color, entity_color)
                };
                let fill_tris: Vec<[f32; 3]> = vec![
                    wcs_corners[0],
                    wcs_corners[1],
                    wcs_corners[2],
                    wcs_corners[0],
                    wcs_corners[2],
                    wcs_corners[3],
                ];
                wires.push(WireModel {
                    name: name.clone(),
                    points: vec![],
                    color: fill_color,
                    selected,
                    aci: 0,
                    pattern_length: 0.0,
                    pattern: [0.0; 8],
                    line_weight_px: 1.0,
                    snap_pts: vec![],
                    tangent_geoms: vec![],
                    key_vertices: vec![],
                    aabb: WireModel::UNBOUNDED_AABB,
                    plinegen: true,
                    vp_scissor: None,
                    fill_tris,
                });
            }

            // Text frame — closed rectangle, matches text color.
            if ml.text_frame {
                let frame_points: Vec<[f32; 3]> = vec![
                    wcs_corners[0],
                    wcs_corners[1],
                    wcs_corners[2],
                    wcs_corners[3],
                    wcs_corners[0],
                ];
                wires.push(WireModel {
                    name,
                    points: frame_points,
                    color: text_color,
                    selected,
                    aci: 0,
                    pattern_length: 0.0,
                    pattern: [0.0; 8],
                    line_weight_px,
                    snap_pts: vec![],
                    tangent_geoms: vec![],
                    key_vertices: vec![],
                    aabb: WireModel::UNBOUNDED_AABB,
                    plinegen: true,
                    vp_scissor: None,
                    fill_tris: vec![],
                });
            }
        }
    }

    wires
}

// ── Entity Z helper ───────────────────────────────────────────────────────

/// Extract the Z elevation from a text/mtext entity.
fn entity_z(entity: &EntityType) -> f32 {
    match entity {
        EntityType::Text(t) => t.insertion_point.z as f32,
        EntityType::MText(t) => t.insertion_point.z as f32,
        _ => 0.0,
    }
}

// ── Legacy geometry (Viewport, Hatch outline, unrecognised) ───────────────

type Geometry = (
    Vec<[f32; 3]>,
    Vec<(Vec3, SnapHint)>,
    Vec<TangentGeom>,
    Vec<[f32; 3]>,
);

fn legacy_geometry(entity: &EntityType, world_offset: [f64; 3]) -> Geometry {
    let [ox, oy, oz] = world_offset;
    match entity {
        EntityType::Viewport(vp) => {
            let cx = (vp.center.x - ox) as f32;
            let cy = (vp.center.y - oy) as f32;
            let cz = (vp.center.z - oz) as f32;
            let hw = (vp.width / 2.0) as f32;
            let hh = (vp.height / 2.0) as f32;
            let pts = vec![
                [cx - hw, cy - hh, cz],
                [cx + hw, cy - hh, cz],
                [cx + hw, cy + hh, cz],
                [cx - hw, cy + hh, cz],
                [cx - hw, cy - hh, cz],
            ];
            (pts, vec![], vec![], vec![])
        }
        EntityType::Insert(ins) => {
            let ip = Vec3::new(
                (ins.insert_point.x - ox) as f32,
                (ins.insert_point.y - oy) as f32,
                (ins.insert_point.z - oz) as f32,
            );
            let s = 0.1_f32;
            let pts = vec![
                [ip.x - s, ip.y, ip.z],
                [ip.x + s, ip.y, ip.z],
                [ip.x, ip.y - s, ip.z],
                [ip.x, ip.y + s, ip.z],
            ];
            (pts, vec![(ip, SnapHint::Insertion)], vec![], vec![])
        }
        EntityType::Hatch(h) => {
            let normal = (h.normal.x, h.normal.y, h.normal.z);
            // Convert a 2D OCS hatch boundary point to WCS, then subtract world_offset.
            let to_wcs = |x: f64, y: f64| -> [f32; 3] {
                let (wx, wy, wz) =
                    crate::scene::transform::ocs_point_to_wcs((x, y, h.elevation), normal);
                [(wx - ox) as f32, (wy - oy) as f32, (wz - oz) as f32]
            };
            let mut pts: Vec<[f32; 3]> = Vec::new();
            let mut key_verts: Vec<[f32; 3]> = Vec::new();
            let mut snap_pts: Vec<(Vec3, SnapHint)> = Vec::new();
            for path in &h.paths {
                for edge in &path.edges {
                    match edge {
                        acadrust::entities::BoundaryEdge::Polyline(poly) => {
                            // Hatch-boundary polyline vertices encode bulge in
                            // `Vector3.z`; straight segments emit just the
                            // start vertex, bulged segments tessellate the arc
                            // between v0 → v1.
                            let start_idx = pts.len();
                            let verts = &poly.vertices;
                            let count = verts.len();
                            let seg_count = if poly.is_closed {
                                count
                            } else {
                                count.saturating_sub(1)
                            };
                            if count > 0 && pts.len() != start_idx {
                                pts.push([f32::NAN; 3]);
                            }
                            for i in 0..seg_count {
                                let v0 = &verts[i];
                                let v1 = &verts[(i + 1) % count];
                                let bulge = v0.z;
                                if bulge.abs() < 1e-9 {
                                    let p = to_wcs(v0.x, v0.y);
                                    pts.push(p);
                                    key_verts.push(p);
                                    continue;
                                }
                                let theta = 4.0 * bulge.atan();
                                let dx = v1.x - v0.x;
                                let dy = v1.y - v0.y;
                                let d = (dx * dx + dy * dy).sqrt();
                                if d < 1e-12 {
                                    let p = to_wcs(v0.x, v0.y);
                                    pts.push(p);
                                    key_verts.push(p);
                                    continue;
                                }
                                let r = (d * 0.5) / (theta * 0.5).sin().abs();
                                let mx = (v0.x + v1.x) * 0.5;
                                let my = (v0.y + v1.y) * 0.5;
                                let px = -dy / d;
                                let py = dx / d;
                                let sign = if bulge > 0.0 { 1.0_f64 } else { -1.0_f64 };
                                let center_offset = r * (theta * 0.5).cos();
                                let cx = mx + sign * px * center_offset;
                                let cy = my + sign * py * center_offset;
                                let a0 = (v0.y - cy).atan2(v0.x - cx);
                                let a1 = (v1.y - cy).atan2(v1.x - cx);
                                let mut sweep = a1 - a0;
                                const TAU: f64 = std::f64::consts::TAU;
                                if bulge > 0.0 {
                                    if sweep <= 0.0 { sweep += TAU; }
                                } else if sweep >= 0.0 { sweep -= TAU; }
                                if sweep.abs() < 1e-9 {
                                    sweep = if bulge > 0.0 { TAU } else { -TAU };
                                }
                                let segs = arc_segments(r, sweep.abs(), wire_chord_tol(r));
                                for j in 0..segs {
                                    let a = a0 + sweep * (j as f64 / segs as f64);
                                    let p = to_wcs(cx + r * a.cos(), cy + r * a.sin());
                                    pts.push(p);
                                    if j == 0 {
                                        key_verts.push(p);
                                    }
                                }
                            }
                            // Close the loop visually for closed polylines by
                            // returning to the first emitted point.
                            if poly.is_closed {
                                if let Some(first) = pts.get(start_idx).cloned() {
                                    if first[0].is_finite() {
                                        pts.push(first);
                                    }
                                }
                            } else if let Some(last) = verts.last() {
                                let p = to_wcs(last.x, last.y);
                                pts.push(p);
                                key_verts.push(p);
                            }
                        }
                        acadrust::entities::BoundaryEdge::Line(ln) => {
                            let p0 = to_wcs(ln.start.x, ln.start.y);
                            let p1 = to_wcs(ln.end.x, ln.end.y);
                            if !pts.is_empty() {
                                pts.push([f32::NAN; 3]);
                            }
                            pts.push(p0);
                            pts.push(p1);
                            key_verts.push(p0);
                            key_verts.push(p1);
                        }
                        acadrust::entities::BoundaryEdge::CircularArc(arc) => {
                            let (sa, span) =
                                arc_signed_span(arc.start_angle, arc.end_angle, arc.counter_clockwise);
                            let segs = arc_segments(arc.radius, span.abs(), wire_chord_tol(arc.radius));
                            if !pts.is_empty() {
                                pts.push([f32::NAN; 3]);
                            }
                            for i in 0..=segs {
                                let t = sa + span * (i as f64 / segs as f64);
                                let p = to_wcs(
                                    arc.center.x + arc.radius * t.cos(),
                                    arc.center.y + arc.radius * t.sin(),
                                );
                                pts.push(p);
                                if i == 0 || i == segs {
                                    key_verts.push(p);
                                }
                            }
                            snap_pts.push((
                                Vec3::from(to_wcs(arc.center.x, arc.center.y)),
                                SnapHint::Center,
                            ));
                        }
                        acadrust::entities::BoundaryEdge::EllipticArc(ell) => {
                            let r_maj = (ell.major_axis_endpoint.x * ell.major_axis_endpoint.x
                                + ell.major_axis_endpoint.y * ell.major_axis_endpoint.y)
                                .sqrt();
                            let r_min = r_maj * ell.minor_axis_ratio;
                            let rot = ell
                                .major_axis_endpoint
                                .y
                                .atan2(ell.major_axis_endpoint.x);
                            let (sa, span) =
                                arc_signed_span(ell.start_angle, ell.end_angle, ell.counter_clockwise);
                            let segs = arc_segments(r_maj, span.abs(), wire_chord_tol(r_maj));
                            if !pts.is_empty() {
                                pts.push([f32::NAN; 3]);
                            }
                            let (cr, sr) = (rot.cos(), rot.sin());
                            for i in 0..=segs {
                                let t = sa + span * (i as f64 / segs as f64);
                                let lx = r_maj * t.cos();
                                let ly = r_min * t.sin();
                                let p = to_wcs(
                                    ell.center.x + lx * cr - ly * sr,
                                    ell.center.y + lx * sr + ly * cr,
                                );
                                pts.push(p);
                                if i == 0 || i == segs {
                                    key_verts.push(p);
                                }
                            }
                            snap_pts.push((
                                Vec3::from(to_wcs(ell.center.x, ell.center.y)),
                                SnapHint::Center,
                            ));
                        }
                        _ => {}
                    }
                }
            }
            if pts.is_empty() {
                pts = vec![[0.0, 0.0, 0.0], [0.0, 0.0, 0.0]];
            }
            (pts, snap_pts, vec![], key_verts)
        }
        EntityType::Ole2Frame(ole) => {
            // OLE objects carry a bounding rectangle in model space.
            // Render a simple X-through-rectangle placeholder.
            let x0 = (ole.upper_left_corner.x - ox) as f32;
            let y0 = (ole.lower_right_corner.y - oy) as f32;
            let x1 = (ole.lower_right_corner.x - ox) as f32;
            let y1 = (ole.upper_left_corner.y - oy) as f32;
            let z = (ole.upper_left_corner.z - oz) as f32;
            if (x1 - x0).abs() < 1e-6 && (y1 - y0).abs() < 1e-6 {
                // Degenerate / unknown size — show a small cross.
                let s = 0.5_f32;
                return (vec![[-s, 0.0, 0.0], [s, 0.0, 0.0]], vec![], vec![], vec![]);
            }
            let pts = vec![
                // Outer rectangle
                [x0, y0, z],
                [x1, y0, z],
                [x1, y0, z],
                [x1, y1, z],
                [x1, y1, z],
                [x0, y1, z],
                [x0, y1, z],
                [x0, y0, z],
                // Diagonal X
                [x0, y0, z],
                [x1, y1, z],
                [f32::NAN, f32::NAN, f32::NAN],
                [x1, y0, z],
                [x0, y1, z],
            ];
            (pts, vec![], vec![], vec![[x0, y0, z], [x1, y1, z]])
        }
        _ => {
            let s = 0.5_f32;
            (vec![[-s, 0.0, 0.0], [s, 0.0, 0.0]], vec![], vec![], vec![])
        }
    }
}

/// Extract pre-computed edge-wire points from Solid3D / Region / Body entities.
///
/// AutoCAD stores explicit wire geometry (from SOLVIEW / 3DPLOT) alongside the
/// ACIS data.  We use this as a visible fallback when the SAT tessellator
/// produces no mesh (e.g. binary SAB data or unsupported geometry).
fn solid_wire_fallback(entity: &EntityType, world_offset: [f64; 3]) -> Vec<[f32; 3]> {
    let [ox, oy, oz] = world_offset;
    let wires: &[acadrust::entities::Wire] = match entity {
        EntityType::Solid3D(s) => &s.wires,
        EntityType::Region(r) => &r.wires,
        EntityType::Body(b) => &b.wires,
        _ => return vec![],
    };

    if wires.is_empty() {
        return vec![];
    }

    let mut pts: Vec<[f32; 3]> = Vec::new();
    for wire in wires {
        if wire.points.len() < 2 {
            continue;
        }
        for v in &wire.points {
            pts.push([(v.x - ox) as f32, (v.y - oy) as f32, (v.z - oz) as f32]);
        }
        // NaN sentinel separates distinct wire segments.
        pts.push([f32::NAN, f32::NAN, f32::NAN]);
    }
    pts
}

fn dimension_geometry(
    dim: &Dimension,
    arrow1: &ArrowKind,
    arrow2: &ArrowKind,
    params: DimLineParams,
    suppress: SuppressFlags,
    world_offset: [f64; 3],
) -> DimGeom {
    let lv = |v| vec3_local(v, world_offset);
    let mut g = DimGeom::new();
    match dim {
        Dimension::Aligned(d) => {
            let first = lv(d.first_point);
            let second = lv(d.second_point);
            let def = lv(d.definition_point);
            let axis = normalized_or(second - first, Vec3::X);
            append_linear_dimension(
                &mut g,
                first,
                second,
                def,
                axis,
                arrow1,
                arrow2,
                params,
                suppress,
                d.ext_line_rotation as f32,
            );
        }
        Dimension::Linear(d) => {
            let first = lv(d.first_point);
            let second = lv(d.second_point);
            let def = lv(d.definition_point);
            let axis = Vec3::new(d.rotation.cos() as f32, d.rotation.sin() as f32, 0.0);
            append_linear_dimension(
                &mut g,
                first,
                second,
                def,
                normalized_or(axis, Vec3::X),
                arrow1,
                arrow2,
                params,
                suppress,
                d.ext_line_rotation as f32,
            );
        }
        Dimension::Radius(d) => {
            let center = lv(d.angle_vertex);
            let point = lv(d.definition_point);
            let text = dimension_text_position(dim, world_offset);
            add_segment(&mut g.dim_lines, center, point);
            // Honour leader_length: extend from the arrow tip past it
            // toward the text by that distance along (text - point).
            let leader_dir = normalized_or(text - point, Vec3::X);
            let leader = if d.leader_length.abs() > 1e-9 {
                point + leader_dir * (d.leader_length as f32)
            } else {
                text
            };
            add_segment(&mut g.dim_lines, point, leader);
            append_arrow(&mut g, point, normalized_or(center - point, Vec3::X), arrow1);
            let radius = (point - center).length();
            append_center_mark(&mut g, center, params.dimcen, radius);
        }
        Dimension::Diameter(d) => {
            let p1 = lv(d.angle_vertex);
            let p2 = lv(d.definition_point);
            add_segment(&mut g.dim_lines, p1, p2);
            append_arrow(&mut g, p1, normalized_or(p2 - p1, Vec3::X), arrow1);
            append_arrow(&mut g, p2, normalized_or(p1 - p2, Vec3::X), arrow2);
            // DIMETER leader: continue past p2 toward the text.
            if d.leader_length.abs() > 1e-9 {
                let text = dimension_text_position(dim, world_offset);
                let leader_dir = normalized_or(text - p2, p2 - p1);
                add_segment(
                    &mut g.dim_lines,
                    p2,
                    p2 + leader_dir * (d.leader_length as f32),
                );
            }
            let radius = (p2 - p1).length() * 0.5;
            append_center_mark(&mut g, (p1 + p2) * 0.5, params.dimcen, radius);
        }
        Dimension::Angular2Ln(d) => {
            append_angular_dimension(
                &mut g,
                lv(d.angle_vertex),
                lv(d.first_point),
                lv(d.second_point),
                lv(d.dimension_arc),
                arrow1,
                arrow2,
            );
        }
        Dimension::Angular3Pt(d) => {
            append_angular_dimension(
                &mut g,
                lv(d.angle_vertex),
                lv(d.first_point),
                lv(d.second_point),
                lv(d.definition_point),
                arrow1,
                arrow2,
            );
        }
        Dimension::Ordinate(d) => {
            add_segment(
                &mut g.dim_lines,
                lv(d.feature_location),
                lv(d.definition_point),
            );
            add_segment(
                &mut g.dim_lines,
                lv(d.definition_point),
                lv(d.leader_endpoint),
            );
        }
    }
    g
}

fn append_linear_dimension(
    g: &mut DimGeom,
    first: Vec3,
    second: Vec3,
    def: Vec3,
    axis: Vec3,
    arrow1: &ArrowKind,
    arrow2: &ArrowKind,
    params: DimLineParams,
    suppress: SuppressFlags,
    ext_line_rotation: f32,
) {
    let perp = Vec3::new(-axis.y, axis.x, 0.0);
    let dim_line_pos = def.dot(perp);
    let offset1 = dim_line_pos - first.dot(perp);
    let offset2 = dim_line_pos - second.dot(perp);
    let d1 = first + perp * offset1;
    let d2 = second + perp * offset2;
    let sign1 = if offset1 >= 0.0 { 1.0_f32 } else { -1.0 };
    let sign2 = if offset2 >= 0.0 { 1.0_f32 } else { -1.0 };

    // ext_line_rotation (DIMEDIT "Oblique"): rotate the extension lines by
    // this angle relative to perpendicular. The ext line still starts at
    // the def point; only the direction differs.
    let ext_dir = if ext_line_rotation.abs() > 1e-6 {
        let c = ext_line_rotation.cos();
        let s = ext_line_rotation.sin();
        // Rotate `perp` by ext_line_rotation around Z.
        Vec3::new(perp.x * c - perp.y * s, perp.x * s + perp.y * c, 0.0)
    } else {
        perp
    };

    // DIMFXLON / DIMFXL: fixed extension-line length from the dim line back
    // toward (but not past) the definition point. Otherwise grow from the
    // def point with DIMEXO gap, extending DIMEXE past the dim line.
    // When oblique, lengths are measured along ext_dir instead of perp.
    let (ext1_start, ext1_end, ext2_start, ext2_end) = if params.dimfxlon {
        let fxl = params.dimfxl.max(0.0);
        let s1 = d1 - ext_dir * (sign1 * fxl);
        let e1 = d1 + ext_dir * (sign1 * params.dimexe);
        let s2 = d2 - ext_dir * (sign2 * fxl);
        let e2 = d2 + ext_dir * (sign2 * params.dimexe);
        (s1, e1, s2, e2)
    } else {
        (
            first + ext_dir * (sign1 * params.dimexo),
            d1 + ext_dir * (sign1 * params.dimexe),
            second + ext_dir * (sign2 * params.dimexo),
            d2 + ext_dir * (sign2 * params.dimexe),
        )
    };
    if !suppress.ext1 {
        add_segment(&mut g.ext_lines, ext1_start, ext1_end);
    }
    if !suppress.ext2 {
        add_segment(&mut g.ext_lines, ext2_start, ext2_end);
    }

    // DIMDLE: dim line overshoots the ext line by `dimdle` at each end,
    // but only when ticks are in use (DIMTSZ > 0). With arrowheads this
    // is ignored, matching AutoCAD.
    let dle = if params.ticks { params.dimdle } else { 0.0 };
    let dir_d1_to_d2 = normalized_or(d2 - d1, axis);
    let d1_out = d1 - dir_d1_to_d2 * dle;
    let d2_out = d2 + dir_d1_to_d2 * dle;
    // DIMSD1/DIMSD2: when *both* set, omit the dim line entirely. AutoCAD
    // splits at text otherwise — without that pivot info, leave as-is.
    let _ = params.dimsoxd; // DIMSOXD: only meaningful when text is auto-placed
                            // outside the ext lines; we honour the saved
                            // text_middle_point so this is a no-op for files.
    if !(suppress.dim1 && suppress.dim2) {
        add_segment(&mut g.dim_lines, d1_out, d2_out);
    }
    append_arrow(g, d1, normalized_or(d2 - d1, axis), arrow1);
    append_arrow(g, d2, normalized_or(d1 - d2, -axis), arrow2);
}

/// Draw a center mark for radius/diameter dimensions.
///   DIMCEN > 0 → small "+" of half-length |DIMCEN| at the centre.
///   DIMCEN < 0 → small "+" *plus* four line segments extending from the
///                circle (radius - |DIMCEN|) outward to (radius + |DIMCEN|).
///   DIMCEN = 0 → no mark.
fn append_center_mark(g: &mut DimGeom, center: Vec3, dimcen: f32, radius: f32) {
    let mag = dimcen.abs();
    if mag <= 1e-6 {
        return;
    }
    // Small "+" at the centre.
    let h = mag;
    add_segment(
        &mut g.dim_lines,
        Vec3::new(center.x - h, center.y, center.z),
        Vec3::new(center.x + h, center.y, center.z),
    );
    add_segment(
        &mut g.dim_lines,
        Vec3::new(center.x, center.y - h, center.z),
        Vec3::new(center.x, center.y + h, center.z),
    );
    if dimcen < 0.0 && radius > mag + 1e-6 {
        let inner = (radius - mag).max(0.0);
        let outer = radius + mag;
        // Four short radial strokes spanning the circle edge.
        add_segment(
            &mut g.dim_lines,
            Vec3::new(center.x + inner, center.y, center.z),
            Vec3::new(center.x + outer, center.y, center.z),
        );
        add_segment(
            &mut g.dim_lines,
            Vec3::new(center.x - inner, center.y, center.z),
            Vec3::new(center.x - outer, center.y, center.z),
        );
        add_segment(
            &mut g.dim_lines,
            Vec3::new(center.x, center.y + inner, center.z),
            Vec3::new(center.x, center.y + outer, center.z),
        );
        add_segment(
            &mut g.dim_lines,
            Vec3::new(center.x, center.y - inner, center.z),
            Vec3::new(center.x, center.y - outer, center.z),
        );
    }
}

fn append_angular_dimension(
    g: &mut DimGeom,
    vertex: Vec3,
    first: Vec3,
    second: Vec3,
    arc_point: Vec3,
    arrow1: &ArrowKind,
    arrow2: &ArrowKind,
) {
    add_segment(&mut g.ext_lines, vertex, first);
    add_segment(&mut g.ext_lines, vertex, second);

    let radius = vertex.distance(arc_point);
    if radius <= 1e-6 {
        return;
    }

    let start = (first.y - vertex.y).atan2(first.x - vertex.x);
    let mut end = (second.y - vertex.y).atan2(second.x - vertex.x);
    let mut delta = end - start;
    while delta <= 0.0 {
        delta += std::f32::consts::TAU;
    }
    if delta > std::f32::consts::PI {
        end -= std::f32::consts::TAU;
        delta = end - start;
    }

    let steps = 32;
    let mut arc_pts = Vec::with_capacity((steps + 1) as usize);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let a = start + delta * t;
        arc_pts.push(vertex + Vec3::new(a.cos() * radius, a.sin() * radius, 0.0));
    }
    add_polyline(&mut g.dim_lines, &arc_pts);

    if arc_pts.len() >= 2 {
        append_arrow(
            g,
            arc_pts[0],
            normalized_or(arc_pts[1] - arc_pts[0], Vec3::X),
            arrow1,
        );
        let n = arc_pts.len();
        append_arrow(
            g,
            arc_pts[n - 1],
            normalized_or(arc_pts[n - 2] - arc_pts[n - 1], Vec3::X),
            arrow2,
        );
    }
}

fn push_tri(out: &mut Vec<[f32; 3]>, a: Vec3, b: Vec3, c: Vec3) {
    out.push([a.x, a.y, a.z]);
    out.push([b.x, b.y, b.z]);
    out.push([c.x, c.y, c.z]);
}

fn append_arrow(g: &mut DimGeom, tip: Vec3, dir: Vec3, arrow: &ArrowKind) {
    let dir = normalized_or(dir, Vec3::X);
    let perp = Vec3::new(-dir.y, dir.x, 0.0);
    match *arrow {
        ArrowKind::None => {}
        ArrowKind::Triangle {
            size,
            filled,
            size_mul,
        } => {
            let size = size * size_mul;
            let base = tip + dir * size;
            // ~1:6 length:half-width ratio (≈9.5° half-angle) matches
            // AutoCAD's standard ClosedFilled block.
            let half_w = size / 6.0;
            let left = base + perp * half_w;
            let right = base - perp * half_w;
            add_segment(&mut g.dim_lines, tip, left);
            add_segment(&mut g.dim_lines, left, right);
            add_segment(&mut g.dim_lines, right, tip);
            if filled {
                push_tri(&mut g.arrow_fill, tip, left, right);
            }
        }
        ArrowKind::Tick { size } => {
            // 45° oblique tick crossing the dim line at the tip; `size` is
            // the half-length (matches AutoCAD's DIMTSZ semantics).
            let off = (dir + perp).normalize_or_zero() * size;
            add_segment(&mut g.dim_lines, tip - off, tip + off);
        }
        ArrowKind::Open { size, half_angle } => {
            let base = tip + dir * size;
            let half_w = size * half_angle.tan();
            let left = base + perp * half_w;
            let right = base - perp * half_w;
            add_segment(&mut g.dim_lines, tip, left);
            add_segment(&mut g.dim_lines, tip, right);
        }
        ArrowKind::Dot { size, filled } => {
            let r = size * 0.5;
            const N: usize = 16;
            let mut ring: Vec<Vec3> = Vec::with_capacity(N + 1);
            for i in 0..=N {
                let a = i as f32 * std::f32::consts::TAU / N as f32;
                ring.push(tip + Vec3::new(a.cos() * r, a.sin() * r, 0.0));
            }
            add_polyline(&mut g.dim_lines, &ring);
            if filled {
                for i in 0..N {
                    push_tri(&mut g.arrow_fill, tip, ring[i], ring[i + 1]);
                }
            }
        }
        ArrowKind::Origin { size } => {
            // Small filled dot at the tip with a perpendicular tick crossing
            // the dim line — matches "_ORIGIN" / "_ORIGIN2" blocks.
            let r = size * 0.25;
            const N: usize = 12;
            let mut ring: Vec<Vec3> = Vec::with_capacity(N + 1);
            for i in 0..=N {
                let a = i as f32 * std::f32::consts::TAU / N as f32;
                ring.push(tip + Vec3::new(a.cos() * r, a.sin() * r, 0.0));
            }
            add_polyline(&mut g.dim_lines, &ring);
            for i in 0..N {
                push_tri(&mut g.arrow_fill, tip, ring[i], ring[i + 1]);
            }
            let half = size * 0.5;
            add_segment(&mut g.dim_lines, tip - perp * half, tip + perp * half);
        }
        ArrowKind::Box_ { size, filled } => {
            let half = size * 0.5;
            let p1 = tip - dir * half - perp * half;
            let p2 = tip + dir * half - perp * half;
            let p3 = tip + dir * half + perp * half;
            let p4 = tip - dir * half + perp * half;
            add_segment(&mut g.dim_lines, p1, p2);
            add_segment(&mut g.dim_lines, p2, p3);
            add_segment(&mut g.dim_lines, p3, p4);
            add_segment(&mut g.dim_lines, p4, p1);
            if filled {
                push_tri(&mut g.arrow_fill, p1, p2, p3);
                push_tri(&mut g.arrow_fill, p1, p3, p4);
            }
        }
        ArrowKind::Datum { size, filled } => {
            // Right-pointing triangle with the base perpendicular to the dim
            // line at the tip and the apex along +dir.
            let half = size * 0.5;
            let base_a = tip + perp * half;
            let base_b = tip - perp * half;
            let apex = tip + dir * size;
            add_segment(&mut g.dim_lines, base_a, apex);
            add_segment(&mut g.dim_lines, apex, base_b);
            add_segment(&mut g.dim_lines, base_b, base_a);
            if filled {
                push_tri(&mut g.arrow_fill, base_a, apex, base_b);
            }
        }
    }
}

fn add_segment(points: &mut Vec<[f32; 3]>, a: Vec3, b: Vec3) {
    if !points.is_empty() {
        points.push([f32::NAN, f32::NAN, f32::NAN]);
    }
    points.push([a.x, a.y, a.z]);
    points.push([b.x, b.y, b.z]);
}

fn add_polyline(points: &mut Vec<[f32; 3]>, polyline: &[Vec3]) {
    if polyline.len() < 2 {
        return;
    }
    if !points.is_empty() {
        points.push([f32::NAN, f32::NAN, f32::NAN]);
    }
    points.extend(polyline.iter().map(|p| [p.x, p.y, p.z]));
}

fn dimension_snap_pts(dim: &Dimension, world_offset: [f64; 3]) -> Vec<(Vec3, SnapHint)> {
    let lv = |v: acadrust::types::Vector3| {
        Vec3::new(
            (v.x - world_offset[0]) as f32,
            (v.y - world_offset[1]) as f32,
            (v.z - world_offset[2]) as f32,
        )
    };
    let node = |v: acadrust::types::Vector3| (lv(v), SnapHint::Node);
    match dim {
        Dimension::Linear(d) => vec![
            node(d.first_point),
            node(d.second_point),
            node(d.definition_point),
        ],
        Dimension::Aligned(d) => vec![
            node(d.first_point),
            node(d.second_point),
            node(d.definition_point),
        ],
        Dimension::Radius(d) => vec![node(d.angle_vertex), node(d.definition_point)],
        Dimension::Diameter(d) => vec![node(d.angle_vertex), node(d.definition_point)],
        Dimension::Angular2Ln(d) => vec![
            node(d.angle_vertex),
            node(d.first_point),
            node(d.second_point),
            node(d.definition_point),
        ],
        Dimension::Angular3Pt(d) => vec![
            node(d.angle_vertex),
            node(d.first_point),
            node(d.second_point),
            node(d.definition_point),
        ],
        Dimension::Ordinate(d) => vec![
            node(d.definition_point),
            node(d.feature_location),
            node(d.leader_endpoint),
        ],
    }
}

fn dimension_text_entity(
    dim: &Dimension,
    text_height: f64,
    style: Option<&DimStyle>,
    document: &CadDocument,
) -> Option<Text> {
    let value = dimension_text_value(dim, style)?;
    // Use f64 position directly to avoid f32 round-trip precision loss at large
    // coordinates (e.g. Turkish UTM ~4,000,000 m). tessellate() will apply
    // world_offset when rendering this synthetic Text entity.
    let pos_f64 = dimension_text_pos_f64(dim, style, text_height);
    let base = dim.base();

    // DIMTIH/DIMTOH: when set, text is forced horizontal (rotation = 0)
    // regardless of the dim line angle. Honour explicit base.text_rotation
    // first, then horizontal_direction override, then DIMTIH/DIMTOH.
    let dimtih = style.map(|s| s.dimtih).unwrap_or(false);
    let dimtoh = style.map(|s| s.dimtoh).unwrap_or(false);
    let rotation = if base.text_rotation.abs() > 1e-9 {
        base.text_rotation
    } else if base.horizontal_direction.abs() > 1e-9 {
        // horizontal_direction is the in-plane reading direction the writer
        // baked in (used for vertical / oblique dims).
        base.horizontal_direction
    } else if dimtih || dimtoh {
        0.0
    } else {
        dimension_text_natural_rotation(dim)
    };

    let mut text = Text::with_value(value, pos_f64)
        .with_height(text_height)
        .with_rotation(rotation);

    // Text style resolution priority:
    //   1. DIMTXSTY by handle (most reliable; survives rename)
    //   2. DIMTXSTY by name
    //   3. dim's own style_name (rare fallback)
    text.style = style
        .and_then(|s| {
            if !s.dimtxsty_handle.is_null() {
                document
                    .text_styles
                    .iter()
                    .find(|ts| ts.handle == s.dimtxsty_handle)
                    .map(|ts| ts.name.clone())
            } else {
                None
            }
        })
        .or_else(|| {
            style
                .map(|s| s.dimtxsty.clone())
                .filter(|n| !n.trim().is_empty())
        })
        .unwrap_or_else(|| base.style_name.clone());

    // Map AttachmentPointType (1..9 grid) to Text horizontal + vertical
    // alignments. 1=TopLeft … 9=BottomRight (column-major).
    let (ha, va) = attachment_to_text_align(base.attachment_point);
    text.horizontal_alignment = ha;
    text.vertical_alignment = va;
    // line_spacing_factor controls multi-line text spacing in MText. Our
    // synthetic Text is single-line so this is a no-op, but pass through
    // for completeness.
    let _ = base.line_spacing_factor;
    // normal would rotate the dim plane out of XY. The local 2D pipeline
    // assumes XY, so non-XY normals are read but not applied here.
    let _ = base.normal;

    text.common = base.common.clone();
    Some(text)
}

fn attachment_to_text_align(
    attach: acadrust::entities::dimension::AttachmentPointType,
) -> (
    acadrust::entities::text::TextHorizontalAlignment,
    acadrust::entities::text::TextVerticalAlignment,
) {
    use acadrust::entities::dimension::AttachmentPointType as A;
    use acadrust::entities::text::{TextHorizontalAlignment as H, TextVerticalAlignment as V};
    match attach {
        A::TopLeft => (H::Left, V::Top),
        A::TopCenter => (H::Center, V::Top),
        A::TopRight => (H::Right, V::Top),
        A::MiddleLeft => (H::Left, V::Middle),
        A::MiddleCenter => (H::Center, V::Middle),
        A::MiddleRight => (H::Right, V::Middle),
        A::BottomLeft => (H::Left, V::Bottom),
        A::BottomCenter => (H::Center, V::Bottom),
        A::BottomRight => (H::Right, V::Bottom),
    }
}

fn dimension_text_natural_rotation(dim: &Dimension) -> f64 {
    let angle = match dim {
        Dimension::Linear(d) => d.rotation,
        Dimension::Aligned(d) => {
            let dx = d.second_point.x - d.first_point.x;
            let dy = d.second_point.y - d.first_point.y;
            dy.atan2(dx)
        }
        _ => 0.0,
    };
    // Clamp to (-π/2, π/2] so text never appears upside-down.
    let pi = std::f64::consts::PI;
    if angle > pi / 2.0 {
        angle - pi
    } else if angle <= -pi / 2.0 {
        angle + pi
    } else {
        angle
    }
}

fn dimension_text_value(dim: &Dimension, style: Option<&DimStyle>) -> Option<String> {
    let (main, tol) = dimension_text_parts(dim, style)?;
    // Tolerance is appended inline for callers (e.g. fill rect width) that
    // don't render a separate tolerance entity. The visual pipeline that
    // does emit a separate tolerance text re-derives the parts itself.
    match tol {
        Some(t) => Some(format!("{} {}", main, t)),
        None => Some(main),
    }
}

/// Returns (primary_text, tolerance_suffix). The tolerance is emitted as a
/// separate Text entity so DIMTFAC can scale its height and DIMTOLJ can
/// align it vertically against the primary value.
fn dimension_text_parts(
    dim: &Dimension,
    style: Option<&DimStyle>,
) -> Option<(String, Option<String>)> {
    let base = dim.base();
    let is_angular = matches!(dim, Dimension::Angular2Ln(_) | Dimension::Angular3Pt(_));

    // Auto-generated body (the value AutoCAD would emit if the user did not
    // override it). Built first so user_text "<>" substitution can re-use it.
    let primary_raw = if is_angular {
        format_angular_value(dim.measurement(), style)
    } else {
        let v = format_linear_value(dim.measurement(), style);
        match dim {
            Dimension::Radius(_) => format!("R{}", v),
            Dimension::Diameter(_) => format!("Ø{}", v),
            _ => v,
        }
    };

    // Build tolerance / limits suffix separately so the caller can render
    // it as its own Text entity at DIMTFAC × DIMTXT height.
    let tolerance_suffix = build_tolerance_suffix(dim.measurement(), style, is_angular);
    let primary = apply_dimpost(&primary_raw, style);

    // Alternate units appended in brackets when DIMALT is on (linear only).
    let primary = if !is_angular {
        match alternate_units_text(dim.measurement(), style) {
            Some(alt) => format!("{} [{}]", primary, alt),
            None => primary,
        }
    } else {
        primary
    };

    // Explicit user override (mtext-style "user_text") wins, but "<>" inside
    // it substitutes the measured value. " " (single space) suppresses text.
    if let Some(user_text) = &base.user_text {
        if user_text.is_empty() || user_text.trim().is_empty() {
            return None;
        }
        return Some((user_text.replace("<>", &primary), tolerance_suffix));
    }
    if !base.text.trim().is_empty() {
        return Some((base.text.replace("<>", &primary), tolerance_suffix));
    }
    Some((primary, tolerance_suffix))
}

fn build_tolerance_suffix(
    measurement: f64,
    style: Option<&DimStyle>,
    is_angular: bool,
) -> Option<String> {
    let s = style?;
    let dimtdec = s.dimtdec.max(0) as usize;
    let dimtzin = s.dimtzin;
    let fmt = |v: f64| -> String {
        let raw = format!("{:.*}", dimtdec, v);
        apply_linear_zero_suppression(&raw, dimtzin)
    };
    if s.dimlim {
        let high = measurement + s.dimtp;
        let low = measurement - s.dimtm;
        return Some(format!("{}/{}", fmt(high), fmt(low)));
    }
    if s.dimtol {
        let unit = if is_angular { "°" } else { "" };
        if (s.dimtp - s.dimtm).abs() < 1e-12 && s.dimtp.abs() > 1e-12 {
            return Some(format!("±{}{}", fmt(s.dimtp), unit));
        }
        if s.dimtp.abs() > 1e-12 || s.dimtm.abs() > 1e-12 {
            return Some(format!("+{}{} / -{}{}", fmt(s.dimtp), unit, fmt(s.dimtm), unit));
        }
    }
    None
}

/// Build the bracketed alternate-units suffix when DIMALT is enabled.
/// When DIMTOL is also on, the bracketed text includes the tolerance
/// component formatted with DIMALTTD / DIMALTTZ.
fn alternate_units_text(measurement: f64, style: Option<&DimStyle>) -> Option<String> {
    let s = style?;
    if !s.dimalt {
        return None;
    }
    let mut v = measurement * s.dimaltf;
    if s.dimaltrnd > 1e-12 {
        v = (v / s.dimaltrnd).round() * s.dimaltrnd;
    }
    let dec = s.dimaltd.max(0) as usize;
    let raw = format_with_unit(v, s.dimaltu, dec, s.dimfrac);
    let suppressed = apply_linear_zero_suppression(&raw, s.dimaltz);
    let sep_swapped = swap_decimal_sep(&suppressed, s.dimdsep);
    // Alt-unit tolerance suffix using DIMALTTD / DIMALTTZ.
    let alt_value = if s.dimtol {
        let alttdec = s.dimalttd.max(0) as usize;
        let alttzin = s.dimalttz;
        let fmt = |x: f64| -> String {
            let raw = format!("{:.*}", alttdec, x * s.dimaltf);
            swap_decimal_sep(&apply_linear_zero_suppression(&raw, alttzin), s.dimdsep)
        };
        if (s.dimtp - s.dimtm).abs() < 1e-12 && s.dimtp.abs() > 1e-12 {
            format!("{}±{}", sep_swapped, fmt(s.dimtp))
        } else if s.dimtp.abs() > 1e-12 || s.dimtm.abs() > 1e-12 {
            format!("{} +{} / -{}", sep_swapped, fmt(s.dimtp), fmt(s.dimtm))
        } else {
            sep_swapped
        }
    } else if s.dimlim {
        let alttdec = s.dimalttd.max(0) as usize;
        let alttzin = s.dimalttz;
        let fmt = |x: f64| -> String {
            let raw = format!("{:.*}", alttdec, x * s.dimaltf);
            swap_decimal_sep(&apply_linear_zero_suppression(&raw, alttzin), s.dimdsep)
        };
        format!("{}/{}", fmt(measurement + s.dimtp), fmt(measurement - s.dimtm))
    } else {
        sep_swapped
    };
    // DIMAPOST wraps the alt value (same "<>" convention as DIMPOST).
    let wrapped = if s.dimapost.is_empty() {
        alt_value
    } else if s.dimapost.contains("<>") {
        s.dimapost.replace("<>", &alt_value)
    } else {
        format!("{}{}", alt_value, s.dimapost)
    };
    Some(wrapped)
}

/// Build the secondary tolerance Text entity at `DIMTXT × DIMTFAC` height,
/// positioned to the right of the primary text and vertically aligned per
/// `DIMTOLJ` (0=bottom, 1=middle, 2=top). Returns None when DIMTOL/DIMLIM
/// produce no tolerance string (e.g. both DIMTP and DIMTM are zero).
fn dimension_tolerance_entity(
    dim: &Dimension,
    style: Option<&DimStyle>,
    primary: &Text,
    primary_height: f64,
) -> Option<Text> {
    let s = style?;
    let is_angular = matches!(dim, Dimension::Angular2Ln(_) | Dimension::Angular3Pt(_));
    let tol = build_tolerance_suffix(dim.measurement(), style, is_angular)?;
    let dimtfac = if s.dimtfac.abs() < 1e-12 {
        1.0
    } else {
        s.dimtfac
    };
    let tol_height = primary_height * dimtfac;

    // Approximate widths from glyph counts (~0.6 × cell size per char).
    let primary_w = primary.value.chars().count() as f64 * primary_height * 0.6;
    let tol_w = tol.chars().count() as f64 * tol_height * 0.6;
    let gap = primary_height * 0.2;
    let dx_local = primary_w * 0.5 + tol_w * 0.5 + gap;
    let dy_local = match s.dimtolj {
        0 => -primary_height * 0.5 + tol_height * 0.5, // bottom-aligned with primary baseline
        2 => primary_height * 0.5 - tol_height * 0.5,  // top-aligned with primary top
        _ => 0.0,                                       // centred (default for ±)
    };
    let rot = primary.rotation;
    let (sr, cr) = rot.sin_cos();
    let pos = Vector3::new(
        primary.insertion_point.x + dx_local * cr - dy_local * sr,
        primary.insertion_point.y + dx_local * sr + dy_local * cr,
        primary.insertion_point.z,
    );
    let mut t = Text::with_value(tol, pos)
        .with_height(tol_height)
        .with_rotation(rot);
    t.style = primary.style.clone();
    t.common = primary.common.clone();
    t.horizontal_alignment = acadrust::entities::text::TextHorizontalAlignment::Center;
    t.vertical_alignment = acadrust::entities::text::TextVerticalAlignment::Middle;
    Some(t)
}

/// Wrap a measured value with the style's DIMPOST prefix/suffix template.
/// "<>" inside DIMPOST is replaced by the value; absent "<>" appends.
fn apply_dimpost(value: &str, style: Option<&DimStyle>) -> String {
    let post = style.map(|s| s.dimpost.as_str()).unwrap_or("");
    if post.is_empty() {
        return value.to_string();
    }
    if post.contains("<>") {
        post.replace("<>", value)
    } else {
        format!("{}{}", value, post)
    }
}

/// Format a linear measurement honouring DIMLFAC, DIMRND, DIMDEC, DIMZIN, DIMDSEP, DIMLUNIT.
fn format_linear_value(measurement: f64, style: Option<&DimStyle>) -> String {
    let (dec, zin, lfac, rnd, dsep, lunit, frac) = style
        .map(|s| {
            (
                s.dimdec, s.dimzin, s.dimlfac, s.dimrnd, s.dimdsep, s.dimlunit, s.dimfrac,
            )
        })
        .unwrap_or((4, 8, 1.0, 0.0, 46, 2, 0));

    let lfac = if lfac.abs() < 1e-12 { 1.0 } else { lfac };
    let mut v = measurement * lfac;
    if rnd > 1e-12 {
        v = (v / rnd).round() * rnd;
    }
    let dec = dec.max(0) as usize;
    let raw = format_with_unit(v, lunit, dec, frac);
    let suppressed = apply_linear_zero_suppression(&raw, zin);
    swap_decimal_sep(&suppressed, dsep)
}

/// Dispatch on DIMLUNIT / DIMALTU.
///   1 = Scientific
///   2 = Decimal (default)
///   3 = Engineering   (feet + decimal inches; 1 unit = 1 inch)
///   4 = Architectural (feet + fractional inches)
///   5 = Fractional    (integer + fractional inches)
///   6 = Windows desktop → falls back to Decimal
/// `dimfrac` controls denominator power for arch/fractional output (0/1/2);
/// rendered inline as "n/d" (stacked glyphs require MText support).
fn format_with_unit(value: f64, unit: i16, dec: usize, dimfrac: i16) -> String {
    match unit {
        1 => format!("{:.*e}", dec, value),
        3 => format_engineering(value, dec),
        4 => format_architectural(value, dimfrac),
        5 => format_fractional(value, dimfrac),
        _ => format!("{:.*}", dec, value),
    }
}

fn format_engineering(inches: f64, dec: usize) -> String {
    let sign = if inches < 0.0 { "-" } else { "" };
    let abs = inches.abs();
    let feet = (abs / 12.0).trunc();
    let rem_in = abs - feet * 12.0;
    format!("{}{:.0}'-{:.*}\"", sign, feet, dec, rem_in)
}

fn format_architectural(inches: f64, dimfrac: i16) -> String {
    let sign = if inches < 0.0 { "-" } else { "" };
    let abs = inches.abs();
    let feet = (abs / 12.0).trunc();
    let rem_in_total = abs - feet * 12.0;
    let whole = rem_in_total.trunc();
    let frac = rem_in_total - whole;
    let frac_str = fraction_string(frac, dimfrac);
    if frac_str.is_empty() {
        format!("{}{:.0}'-{:.0}\"", sign, feet, whole)
    } else {
        format!("{}{:.0}'-{:.0} {}\"", sign, feet, whole, frac_str)
    }
}

fn format_fractional(value: f64, dimfrac: i16) -> String {
    let sign = if value < 0.0 { "-" } else { "" };
    let abs = value.abs();
    let whole = abs.trunc();
    let frac = abs - whole;
    let frac_str = fraction_string(frac, dimfrac);
    if frac_str.is_empty() {
        format!("{}{:.0}", sign, whole)
    } else if whole == 0.0 {
        format!("{}{}", sign, frac_str)
    } else {
        format!("{}{:.0} {}", sign, whole, frac_str)
    }
}

fn fraction_string(frac: f64, dimfrac: i16) -> String {
    // DIMFRAC denominator: AutoCAD encodes this on DIMSTYLE via DIMLUNIT
    // pairing — the value we accept is the *power-of-2* exponent (1..=6 ish).
    // Pick a sensible cap so the printed fraction stays readable.
    let exp = (dimfrac.clamp(0, 8) as u32).max(2) + 2; // 2..=10 → 4..=1024
    let denom = 1u64 << exp;
    let numer = (frac * denom as f64).round() as i64;
    if numer <= 0 {
        return String::new();
    }
    let mut n = numer as u64;
    let mut d = denom;
    while n % 2 == 0 && d % 2 == 0 {
        n /= 2;
        d /= 2;
    }
    if n == 0 {
        String::new()
    } else if d == 1 {
        format!("{}", n) // whole-number overflow back to caller
    } else {
        format!("{}/{}", n, d)
    }
}

/// Format an angular measurement (input in degrees as Dimension::measurement
/// returns for angular variants) honouring DIMAUNIT, DIMADEC, DIMAZIN.
fn format_angular_value(measurement_deg: f64, style: Option<&DimStyle>) -> String {
    let (aunit, adec, azin) = style
        .map(|s| (s.dimaunit, s.dimadec, s.dimazin))
        .unwrap_or((0, 2, 0));
    let adec = adec.max(0) as usize;

    match aunit {
        // 1 = Degrees / Minutes / Seconds
        1 => format_dms(measurement_deg, adec, azin),
        // 2 = Gradians
        2 => {
            let g = measurement_deg / 0.9;
            let raw = format!("{:.*}", adec, g);
            format!("{}g", apply_angular_zero_suppression(&raw, azin))
        }
        // 3 = Radians
        3 => {
            let r = measurement_deg.to_radians();
            let raw = format!("{:.*}", adec, r);
            format!("{}r", apply_angular_zero_suppression(&raw, azin))
        }
        // 0 or unknown = Decimal Degrees
        _ => {
            let raw = format!("{:.*}", adec, measurement_deg);
            format!("{}°", apply_angular_zero_suppression(&raw, azin))
        }
    }
}

fn format_dms(deg: f64, sec_dec: usize, azin: i16) -> String {
    let sign = if deg < 0.0 { "-" } else { "" };
    let abs = deg.abs();
    let d = abs.floor();
    let m_full = (abs - d) * 60.0;
    let m = m_full.floor();
    let s = (m_full - m) * 60.0;
    let s_str = format!("{:.*}", sec_dec, s);
    let mut out = format!("{}{:.0}°{:.0}'{}\"", sign, d, m, s_str);
    if azin & 4 != 0 {
        // suppress 0° / 0' parts
        if d == 0.0 {
            out = out.trim_start_matches('0').to_string();
            out = out.replacen("°", "", 1);
        }
    }
    out
}

/// Apply DIMZIN bit flags to a formatted linear value.
///  bit 0 (1)  suppress 0' (imperial feet)        — not applicable for decimal
///  bit 1 (2)  suppress 0" (imperial inches)      — not applicable for decimal
///  bit 2 (4)  suppress leading zeros             (e.g. ".5" not "0.5")
///  bit 3 (8)  suppress trailing zeros            (e.g. "1.5" not "1.50")
/// Default = 8 (trailing-zero suppression on).
fn apply_linear_zero_suppression(s: &str, zin: i16) -> String {
    let mut out = s.to_string();
    if zin & 8 != 0 {
        out = strip_trailing_zeros(&out);
    }
    if zin & 4 != 0 {
        out = strip_leading_zero(&out);
    }
    out
}

fn apply_angular_zero_suppression(s: &str, azin: i16) -> String {
    // DIMAZIN: 0=neither, 1=leading, 2=trailing, 3=both.
    let mut out = s.to_string();
    if azin & 2 != 0 {
        out = strip_trailing_zeros(&out);
    }
    if azin & 1 != 0 {
        out = strip_leading_zero(&out);
    }
    out
}

fn strip_trailing_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.to_string();
    }
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn strip_leading_zero(s: &str) -> String {
    // "0.5" → ".5",  "-0.5" → "-.5",  "0" stays.
    if let Some(rest) = s.strip_prefix("-0.") {
        return format!("-.{rest}");
    }
    if let Some(rest) = s.strip_prefix("0.") {
        return format!(".{rest}");
    }
    s.to_string()
}

fn swap_decimal_sep(s: &str, dsep_code: i16) -> String {
    // DIMDSEP holds an ASCII code (0 means default '.'). 46='.', 44=',', etc.
    if dsep_code <= 0 || dsep_code == 46 {
        return s.to_string();
    }
    let ch = char::from_u32(dsep_code as u32).unwrap_or('.');
    s.replace('.', &ch.to_string())
}

fn dimension_text_position(dim: &Dimension, world_offset: [f64; 3]) -> Vec3 {
    let lv = |v| vec3_local(v, world_offset);
    let base = dim.base();
    let pos = lv(base.text_middle_point);
    if pos.length_squared() > 1e-8 {
        return pos;
    }
    match dim {
        Dimension::Aligned(d) => (lv(d.first_point) + lv(d.second_point)) * 0.5,
        Dimension::Linear(d) => (lv(d.first_point) + lv(d.second_point)) * 0.5,
        Dimension::Radius(d) => (lv(d.angle_vertex) + lv(d.definition_point)) * 0.5,
        Dimension::Diameter(d) => (lv(d.angle_vertex) + lv(d.definition_point)) * 0.5,
        Dimension::Angular2Ln(d) => lv(d.dimension_arc),
        Dimension::Angular3Pt(d) => lv(d.definition_point),
        Dimension::Ordinate(d) => lv(d.leader_endpoint),
    }
}

fn vec3_local(v: Vector3, off: [f64; 3]) -> Vec3 {
    Vec3::new(
        (v.x - off[0]) as f32,
        (v.y - off[1]) as f32,
        (v.z - off[2]) as f32,
    )
}

fn offset_snap_pts(pts: Vec<(Vec3, SnapHint)>, off: [f64; 3]) -> Vec<(Vec3, SnapHint)> {
    let [ox, oy, oz] = off;
    pts.into_iter()
        .map(|(p, h)| {
            (
                Vec3::new(p.x - ox as f32, p.y - oy as f32, p.z - oz as f32),
                h,
            )
        })
        .collect()
}

/// Returns the text position of a dimension in DXF world-space (f64, no offset applied).
/// Used when building a synthetic Text entity so tessellate() can apply world_offset itself.
/// When the saved `text_middle_point` is zero (i.e. AutoCAD never wrote one),
/// computes a fallback from the dim geometry and applies DIMTAD/DIMGAP.
fn dimension_text_pos_f64(
    dim: &Dimension,
    style: Option<&DimStyle>,
    text_height: f64,
) -> Vector3 {
    let base = dim.base();
    let p = base.text_middle_point;
    if p.x * p.x + p.y * p.y + p.z * p.z > 1e-16 {
        return p;
    }
    let mid = match dim {
        Dimension::Aligned(d) => Vector3::new(
            (d.first_point.x + d.second_point.x) * 0.5,
            (d.first_point.y + d.second_point.y) * 0.5,
            (d.first_point.z + d.second_point.z) * 0.5,
        ),
        Dimension::Linear(d) => Vector3::new(
            (d.first_point.x + d.second_point.x) * 0.5,
            (d.first_point.y + d.second_point.y) * 0.5,
            (d.first_point.z + d.second_point.z) * 0.5,
        ),
        Dimension::Radius(d) => Vector3::new(
            (d.angle_vertex.x + d.definition_point.x) * 0.5,
            (d.angle_vertex.y + d.definition_point.y) * 0.5,
            (d.angle_vertex.z + d.definition_point.z) * 0.5,
        ),
        Dimension::Diameter(d) => Vector3::new(
            (d.angle_vertex.x + d.definition_point.x) * 0.5,
            (d.angle_vertex.y + d.definition_point.y) * 0.5,
            (d.angle_vertex.z + d.definition_point.z) * 0.5,
        ),
        Dimension::Angular2Ln(d) => d.dimension_arc,
        Dimension::Angular3Pt(d) => d.definition_point,
        Dimension::Ordinate(d) => d.leader_endpoint,
    };

    // DIMTAD: 0=centred (on the line), 1=above (offset perpendicular), 2=outside,
    //         3=JIS. We honour 0 and 1; 2/3 fall back to "above".
    let dimtad = style.map(|s| s.dimtad).unwrap_or(1);
    let dimgap = style.map(|s| s.dimgap).unwrap_or(0.0);
    // DIMJUST horizontal placement on the dim line (only meaningful for
    // linear/aligned dims). 0=centred, 1=near first ext, 2=near second ext,
    // 3=above first ext (perpendicular text), 4=above second ext.
    let dimjust = style.map(|s| s.dimjust).unwrap_or(0);
    // DIMTVP vertical-position multiplier (units of dimtxt). Only honoured
    // when DIMTAD == 0; offsets text perpendicular to the dim line.
    let dimtvp = style.map(|s| s.dimtvp).unwrap_or(0.0);

    // Need axis + perp_sign (toward "above").
    let (axis_x, axis_y, perp_sign, p1, p2) = match dim {
        Dimension::Linear(d) => {
            let ax = d.rotation.cos();
            let ay = d.rotation.sin();
            let px = -ay;
            let py = ax;
            let off = (d.definition_point.x - d.first_point.x) * px
                + (d.definition_point.y - d.first_point.y) * py;
            (
                ax,
                ay,
                if off >= 0.0 { 1.0 } else { -1.0 },
                d.first_point,
                d.second_point,
            )
        }
        Dimension::Aligned(d) => {
            let dx = d.second_point.x - d.first_point.x;
            let dy = d.second_point.y - d.first_point.y;
            let len = (dx * dx + dy * dy).sqrt().max(1e-12);
            let ax = dx / len;
            let ay = dy / len;
            let px = -ay;
            let py = ax;
            let off = (d.definition_point.x - d.first_point.x) * px
                + (d.definition_point.y - d.first_point.y) * py;
            (
                ax,
                ay,
                if off >= 0.0 { 1.0 } else { -1.0 },
                d.first_point,
                d.second_point,
            )
        }
        _ => {
            // Non-linear: only DIMTAD offset applies; no horizontal shift along axis.
            let off_perp = if dimtad == 0 {
                dimtvp * text_height
            } else {
                text_height * 0.5 + dimgap
            };
            return Vector3::new(mid.x, mid.y + off_perp * perp_sign_default(), mid.z);
        }
    };

    // Horizontal slide along the dim axis to honour DIMJUST. Slide endpoints
    // are the dim-line endpoints (projection of p1/p2 onto the dim line),
    // approximated here as the def-points themselves (we don't have axis-
    // projected d1/d2 here without more plumbing).
    let along_offset = match dimjust {
        1 => (p1.x - mid.x) * axis_x + (p1.y - mid.y) * axis_y,
        2 => (p2.x - mid.x) * axis_x + (p2.y - mid.y) * axis_y,
        3 => (p1.x - mid.x) * axis_x + (p1.y - mid.y) * axis_y,
        4 => (p2.x - mid.x) * axis_x + (p2.y - mid.y) * axis_y,
        _ => 0.0,
    };

    // Perpendicular offset: DIMTAD 0 → DIMTVP * dimtxt, else above-line gap.
    let perp_offset = if dimtad == 0 {
        dimtvp * text_height * perp_sign
    } else {
        (text_height * 0.5 + dimgap) * perp_sign
    };

    Vector3::new(
        mid.x + axis_x * along_offset + (-axis_y) * perp_offset,
        mid.y + axis_y * along_offset + (axis_x) * perp_offset,
        mid.z,
    )
}

fn perp_sign_default() -> f64 {
    1.0
}

fn normalized_or(v: Vec3, fallback: Vec3) -> Vec3 {
    if v.length_squared() <= 1e-12 {
        fallback
    } else {
        v.normalize()
    }
}

