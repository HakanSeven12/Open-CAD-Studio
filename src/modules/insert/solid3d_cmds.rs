// 3D solid primitive commands — BOX, SPHERE, CYLINDER
// and 2D→3D extrusion commands — EXTRUDE, REVOLVE.
//
// All create a minimal Solid3D entity (empty ACIS, just as a document
// placeholder to hold the handle) and a MeshModel built with truck.
// The mesh is manually inserted into scene.meshes so it renders immediately.
//
// Round-trip limitation: saving and reopening the file will not restore
// the mesh because the ACIS data is empty.  Full ACIS generation requires
// a separate step (out of scope here).
//
// Coordinate convention in H7CAD: the viewport is Y-up (OpenGL style),
// so screen X→DXF X, screen Z→DXF Y, screen Y→DXF Z (height).
// truck works in standard math coordinates; we map accordingly.

use acadrust::{entities::Solid3D, EntityType};
use glam::Vec3;
use truck_modeling::builder;
use truck_modeling::{Point3, Rad, Vector3 as TruckVec3};

use crate::command::{CadCommand, CmdResult};
use crate::scene::mesh_model::MeshModel;
use crate::scene::truck_tess;

// ── Tessellation helper ────────────────────────────────────────────────────

fn solid_to_mesh(solid: &truck_modeling::Solid, color: [f32; 4], name: &str) -> Option<MeshModel> {
    match truck_tess::tessellate_solid(solid, [0.0; 3]) {
        truck_tess::TruckTessResult::Mesh { verts, normals, indices } => Some(MeshModel {
            name: name.to_string(),
            verts, normals, indices,
            color,
            selected: false,
        }),
        _ => None,
    }
}

fn shell_to_mesh(shell: &truck_modeling::Shell, color: [f32; 4], name: &str) -> Option<MeshModel> {
    match truck_tess::tessellate_shell(shell, [0.0; 3]) {
        truck_tess::TruckTessResult::Mesh { verts, normals, indices } => Some(MeshModel {
            name: name.to_string(),
            verts, normals, indices,
            color,
            selected: false,
        }),
        _ => None,
    }
}

// ── BOX command ────────────────────────────────────────────────────────────

pub struct BoxCommand {
    step: BoxStep,
    p1: Vec3,
    p2: Vec3,
    color: [f32; 4],
}

#[derive(PartialEq)]
enum BoxStep { Corner1, Corner2, Height }

impl BoxCommand {
    pub fn new(color: [f32; 4]) -> Self {
        Self { step: BoxStep::Corner1, p1: Vec3::ZERO, p2: Vec3::ZERO, color }
    }
}

impl CadCommand for BoxCommand {
    fn name(&self) -> &'static str { "BOX" }
    fn prompt(&self) -> String {
        match self.step {
            BoxStep::Corner1 => "BOX  First corner:".into(),
            BoxStep::Corner2 => "BOX  Opposite corner (XY):".into(),
            BoxStep::Height  => "BOX  Height:".into(),
        }
    }
    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        match self.step {
            BoxStep::Corner1 => { self.p1 = pt; self.step = BoxStep::Corner2; CmdResult::NeedPoint }
            BoxStep::Corner2 => { self.p2 = pt; self.step = BoxStep::Height;  CmdResult::NeedPoint }
            BoxStep::Height  => commit_box(self.p1, self.p2, (pt.y - self.p1.y).abs().max(1e-4), self.color),
        }
    }
    fn wants_text_input(&self) -> bool { self.step == BoxStep::Height }
    fn on_text_input(&mut self, text: &str) -> Option<CmdResult> {
        text.trim().parse::<f32>().ok().filter(|&h| h.abs() > 1e-6)
            .map(|h| commit_box(self.p1, self.p2, h.abs(), self.color))
    }
    fn on_enter(&mut self) -> CmdResult { CmdResult::Cancel }
}

fn commit_box(p1: Vec3, p2: Vec3, height: f32, color: [f32; 4]) -> CmdResult {
    // Map H7CAD coords to truck: x→x, z→y, y→z
    let x0 = p1.x.min(p2.x) as f64;
    let y0 = p1.z.min(p2.z) as f64;
    let x1 = p1.x.max(p2.x) as f64;
    let y1 = p1.z.max(p2.z) as f64;
    let z0 = p1.y as f64;
    let h  = height as f64;

    // Build face at z=z0, then sweep to z0+h.
    let v00 = builder::vertex(Point3::new(x0, y0, z0));
    let v10 = builder::vertex(Point3::new(x1, y0, z0));
    let v11 = builder::vertex(Point3::new(x1, y1, z0));
    let v01 = builder::vertex(Point3::new(x0, y1, z0));
    let e0 = builder::line(&v00, &v10);
    let e1 = builder::line(&v10, &v11);
    let e2 = builder::line(&v11, &v01);
    let e3 = builder::line(&v01, &v00);
    let wire: truck_modeling::Wire = [e0, e1, e2, e3].into_iter().collect();
    let face = match builder::try_attach_plane(&[wire]) {
        Ok(f) => f,
        Err(_) => return CmdResult::Cancel,
    };
    // tsweep on Face → Solid
    let solid = builder::tsweep(&face, TruckVec3::new(0.0, 0.0, h));
    CmdResult::CommitSolid3D { mesh_fn: Box::new(move |name| solid_to_mesh(&solid, color, &name)) }
}

// ── SPHERE command ─────────────────────────────────────────────────────────

pub struct SphereCommand {
    step: SphereStep,
    center: Vec3,
    color: [f32; 4],
}

#[derive(PartialEq)]
enum SphereStep { Center, Radius }

impl SphereCommand {
    pub fn new(color: [f32; 4]) -> Self {
        Self { step: SphereStep::Center, center: Vec3::ZERO, color }
    }
}

impl CadCommand for SphereCommand {
    fn name(&self) -> &'static str { "SPHERE" }
    fn prompt(&self) -> String {
        match self.step {
            SphereStep::Center => "SPHERE  Center:".into(),
            SphereStep::Radius => "SPHERE  Radius:".into(),
        }
    }
    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        match self.step {
            SphereStep::Center => { self.center = pt; self.step = SphereStep::Radius; CmdResult::NeedPoint }
            SphereStep::Radius => commit_sphere(self.center, (pt - self.center).length(), self.color),
        }
    }
    fn wants_text_input(&self) -> bool { self.step == SphereStep::Radius }
    fn on_text_input(&mut self, text: &str) -> Option<CmdResult> {
        text.trim().parse::<f32>().ok().filter(|&r| r > 1e-6)
            .map(|r| commit_sphere(self.center, r, self.color))
    }
    fn on_enter(&mut self) -> CmdResult { CmdResult::Cancel }
}

fn commit_sphere(center: Vec3, radius: f32, color: [f32; 4]) -> CmdResult {
    let cx = center.x as f64;
    let cy = center.z as f64;
    let cz = center.y as f64;
    let r  = radius as f64;

    // Build a half-circle arc wire from north pole to south pole (XZ plane).
    let north = builder::vertex(Point3::new(cx, cy, cz + r));
    let south = builder::vertex(Point3::new(cx, cy, cz - r));
    let east  = Point3::new(cx + r, cy, cz);
    let arc   = builder::circle_arc(&north, &south, east);
    // rsweep the wire around the Z axis for a full revolution → Shell.
    let wire: truck_modeling::Wire = std::iter::once(arc).collect();
    let axis_pt = Point3::new(cx, cy, cz);
    let axis    = TruckVec3::new(0.0, 0.0, 1.0);
    let shell   = builder::rsweep(&wire, axis_pt, axis, Rad(std::f64::consts::TAU));
    CmdResult::CommitSolid3D { mesh_fn: Box::new(move |name| shell_to_mesh(&shell, color, &name)) }
}

// ── CYLINDER command ───────────────────────────────────────────────────────

pub struct CylinderCommand {
    step: CylStep,
    center: Vec3,
    radius: f32,
    color: [f32; 4],
}

#[derive(PartialEq)]
enum CylStep { Center, Radius, Height }

impl CylinderCommand {
    pub fn new(color: [f32; 4]) -> Self {
        Self { step: CylStep::Center, center: Vec3::ZERO, radius: 1.0, color }
    }
}

impl CadCommand for CylinderCommand {
    fn name(&self) -> &'static str { "CYLINDER" }
    fn prompt(&self) -> String {
        match self.step {
            CylStep::Center => "CYLINDER  Center of base:".into(),
            CylStep::Radius => "CYLINDER  Radius:".into(),
            CylStep::Height => "CYLINDER  Height:".into(),
        }
    }
    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        match self.step {
            CylStep::Center => { self.center = pt; self.step = CylStep::Radius; CmdResult::NeedPoint }
            CylStep::Radius => {
                self.radius = (pt - self.center).length().max(1e-4);
                self.step = CylStep::Height;
                CmdResult::NeedPoint
            }
            CylStep::Height => commit_cylinder(self.center, self.radius, (pt.y - self.center.y).abs().max(1e-4), self.color),
        }
    }
    fn wants_text_input(&self) -> bool { matches!(self.step, CylStep::Radius | CylStep::Height) }
    fn on_text_input(&mut self, text: &str) -> Option<CmdResult> {
        let v = text.trim().parse::<f32>().ok().filter(|&v| v > 1e-6)?;
        match self.step {
            CylStep::Radius => { self.radius = v; self.step = CylStep::Height; Some(CmdResult::NeedPoint) }
            CylStep::Height => Some(commit_cylinder(self.center, self.radius, v, self.color)),
            _ => None,
        }
    }
    fn on_enter(&mut self) -> CmdResult { CmdResult::Cancel }
}

fn commit_cylinder(center: Vec3, radius: f32, height: f32, color: [f32; 4]) -> CmdResult {
    let cx = center.x as f64;
    let cy = center.z as f64;
    let cz = center.y as f64;
    let r  = radius as f64;
    let h  = height as f64;

    // Build circle face at z=cz, sweep upward.
    let right = builder::vertex(Point3::new(cx + r, cy, cz));
    let left  = builder::vertex(Point3::new(cx - r, cy, cz));
    let top_t = Point3::new(cx, cy + r, cz);
    let bot_t = Point3::new(cx, cy - r, cz);
    let upper = builder::circle_arc(&right, &left, top_t);
    let lower = builder::circle_arc(&left, &right, bot_t);
    let wire: truck_modeling::Wire = [upper, lower].into_iter().collect();
    let face = match builder::try_attach_plane(&[wire]) {
        Ok(f) => f,
        Err(_) => return CmdResult::Cancel,
    };
    let solid = builder::tsweep(&face, TruckVec3::new(0.0, 0.0, h));
    CmdResult::CommitSolid3D { mesh_fn: Box::new(move |name| solid_to_mesh(&solid, color, &name)) }
}

// ── EXTRUDE command ────────────────────────────────────────────────────────

pub struct ExtrudeCommand {
    step: ExtrudeStep,
    pub target_handle: acadrust::Handle,
    color: [f32; 4],
}

#[derive(PartialEq)]
enum ExtrudeStep { Pick, Height }

impl ExtrudeCommand {
    pub fn new(color: [f32; 4]) -> Self {
        Self { step: ExtrudeStep::Pick, target_handle: acadrust::Handle::NULL, color }
    }
}

impl CadCommand for ExtrudeCommand {
    fn name(&self) -> &'static str { "EXTRUDE" }
    fn prompt(&self) -> String {
        match self.step {
            ExtrudeStep::Pick   => "EXTRUDE  Select closed profile (Circle, LwPolyline…):".into(),
            ExtrudeStep::Height => "EXTRUDE  Height:".into(),
        }
    }
    fn needs_entity_pick(&self) -> bool { self.step == ExtrudeStep::Pick }
    fn on_entity_pick(&mut self, handle: acadrust::Handle, _pt: Vec3) -> CmdResult {
        if handle.is_null() { return CmdResult::NeedPoint; }
        self.target_handle = handle;
        self.step = ExtrudeStep::Height;
        CmdResult::NeedPoint
    }
    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if self.step == ExtrudeStep::Height {
            return CmdResult::ExtrudeEntity { handle: self.target_handle, height: pt.y.abs().max(1e-4), color: self.color };
        }
        CmdResult::NeedPoint
    }
    fn wants_text_input(&self) -> bool { self.step == ExtrudeStep::Height }
    fn on_text_input(&mut self, text: &str) -> Option<CmdResult> {
        text.trim().parse::<f32>().ok().filter(|&h| h.abs() > 1e-6)
            .map(|h| CmdResult::ExtrudeEntity { handle: self.target_handle, height: h.abs(), color: self.color })
    }
    fn on_enter(&mut self) -> CmdResult { CmdResult::Cancel }
}

// ── REVOLVE command ────────────────────────────────────────────────────────

pub struct RevolveCommand {
    step: RevolveStep,
    target_handle: acadrust::Handle,
    axis_start: Vec3,
    axis_end: Vec3,
    color: [f32; 4],
}

#[derive(PartialEq)]
enum RevolveStep { Pick, AxisStart, AxisEnd, Angle }

impl RevolveCommand {
    pub fn new(color: [f32; 4]) -> Self {
        Self {
            step: RevolveStep::Pick,
            target_handle: acadrust::Handle::NULL,
            axis_start: Vec3::ZERO,
            axis_end: Vec3::new(0.0, 0.0, 1.0),
            color,
        }
    }
}

impl CadCommand for RevolveCommand {
    fn name(&self) -> &'static str { "REVOLVE" }
    fn prompt(&self) -> String {
        match self.step {
            RevolveStep::Pick      => "REVOLVE  Select profile:".into(),
            RevolveStep::AxisStart => "REVOLVE  Axis start point:".into(),
            RevolveStep::AxisEnd   => "REVOLVE  Axis end point:".into(),
            RevolveStep::Angle     => "REVOLVE  Angle of revolution <360>:".into(),
        }
    }
    fn needs_entity_pick(&self) -> bool { self.step == RevolveStep::Pick }
    fn on_entity_pick(&mut self, handle: acadrust::Handle, _pt: Vec3) -> CmdResult {
        if handle.is_null() { return CmdResult::NeedPoint; }
        self.target_handle = handle;
        self.step = RevolveStep::AxisStart;
        CmdResult::NeedPoint
    }
    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        match self.step {
            RevolveStep::AxisStart => { self.axis_start = pt; self.step = RevolveStep::AxisEnd; CmdResult::NeedPoint }
            RevolveStep::AxisEnd   => { self.axis_end   = pt; self.step = RevolveStep::Angle;   CmdResult::NeedPoint }
            RevolveStep::Angle     => self.make_revolve(360.0),
            _ => CmdResult::NeedPoint,
        }
    }
    fn wants_text_input(&self) -> bool { self.step == RevolveStep::Angle }
    fn on_text_input(&mut self, text: &str) -> Option<CmdResult> {
        let angle = if text.trim().is_empty() { 360.0f32 }
            else { text.trim().parse::<f32>().ok().filter(|&a| a.abs() > 1e-3)? };
        Some(self.make_revolve(angle.abs()))
    }
    fn on_enter(&mut self) -> CmdResult {
        if self.step == RevolveStep::Angle { self.make_revolve(360.0) } else { CmdResult::Cancel }
    }
}

impl RevolveCommand {
    fn make_revolve(&self, angle_deg: f32) -> CmdResult {
        CmdResult::RevolveEntity {
            handle: self.target_handle,
            axis_start: self.axis_start,
            axis_end: self.axis_end,
            angle_deg,
            color: self.color,
        }
    }
}

// ── SWEEP command ──────────────────────────────────────────────────────────

pub struct SweepCommand {
    step: SweepStep,
    profile_handle: acadrust::Handle,
    color: [f32; 4],
}

#[derive(PartialEq)]
enum SweepStep { PickProfile, PickPath }

impl SweepCommand {
    pub fn new(color: [f32; 4]) -> Self {
        Self { step: SweepStep::PickProfile, profile_handle: acadrust::Handle::NULL, color }
    }
}

impl CadCommand for SweepCommand {
    fn name(&self) -> &'static str { "SWEEP" }
    fn prompt(&self) -> String {
        match self.step {
            SweepStep::PickProfile => "SWEEP  Select profile to sweep:".into(),
            SweepStep::PickPath    => "SWEEP  Select path (Line, Arc, LwPolyline):".into(),
        }
    }
    fn needs_entity_pick(&self) -> bool { true }
    fn on_entity_pick(&mut self, handle: acadrust::Handle, _pt: Vec3) -> CmdResult {
        if handle.is_null() { return CmdResult::NeedPoint; }
        match self.step {
            SweepStep::PickProfile => {
                self.profile_handle = handle;
                self.step = SweepStep::PickPath;
                CmdResult::NeedPoint
            }
            SweepStep::PickPath => {
                CmdResult::SweepEntity {
                    profile_handle: self.profile_handle,
                    path_handle: handle,
                    color: self.color,
                }
            }
        }
    }
    fn on_point(&mut self, _pt: Vec3) -> CmdResult { CmdResult::NeedPoint }
    fn on_enter(&mut self) -> CmdResult { CmdResult::Cancel }
}

// ── LOFT command ───────────────────────────────────────────────────────────

pub struct LoftCommand {
    profiles: Vec<acadrust::Handle>,
    color: [f32; 4],
}

impl LoftCommand {
    pub fn new(color: [f32; 4]) -> Self {
        Self { profiles: Vec::new(), color }
    }
}

impl CadCommand for LoftCommand {
    fn name(&self) -> &'static str { "LOFT" }
    fn prompt(&self) -> String {
        if self.profiles.is_empty() {
            "LOFT  Select first cross-section:".into()
        } else {
            format!("LOFT  Select next cross-section ({} selected, Enter to finish):", self.profiles.len())
        }
    }
    fn needs_entity_pick(&self) -> bool { true }
    fn on_entity_pick(&mut self, handle: acadrust::Handle, _pt: Vec3) -> CmdResult {
        if handle.is_null() { return CmdResult::NeedPoint; }
        // Avoid duplicate picks.
        if !self.profiles.contains(&handle) {
            self.profiles.push(handle);
        }
        CmdResult::NeedPoint
    }
    fn on_point(&mut self, _pt: Vec3) -> CmdResult { CmdResult::NeedPoint }
    fn wants_text_input(&self) -> bool { self.profiles.len() >= 2 }
    fn on_text_input(&mut self, _text: &str) -> Option<CmdResult> { None }
    fn on_enter(&mut self) -> CmdResult {
        if self.profiles.len() < 2 {
            CmdResult::Cancel
        } else {
            CmdResult::LoftEntities { handles: self.profiles.clone(), color: self.color }
        }
    }
}

// ── Placeholder Solid3D entity construction ────────────────────────────────

/// Create a minimal Solid3D entity with empty ACIS data (placeholder only).
pub fn empty_solid3d() -> EntityType {
    EntityType::Solid3D(Solid3D::new())
}
