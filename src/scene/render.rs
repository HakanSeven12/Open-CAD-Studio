// GPU rendering primitives, shader::Program / shader::Primitive impls,
// and entity render-style helpers for the Scene.

use acadrust::tables::LineType;
use acadrust::types::{Color as AcadColor, LineWeight};
use acadrust::{EntityType, Handle};
use glam::Mat4;
use iced::mouse;
use iced::widget::shader::{self, Viewport};
use iced::{Rectangle, Size};

use super::pipeline::viewcube::{hover_id, VIEWCUBE_PX};
use super::pipeline::Pipeline;
use super::tessellate;
use super::{HatchModel, ImageModel, MeshModel, Scene, Uniforms, WireModel};

// ── PaperViewportPipeline / PaperViewportPrimitive ────────────────────────
//
// Newtype wrappers around Pipeline / Primitive so that the active-MSPACE
// viewport widget gets its own Iced storage entry (keyed by TypeId of the
// Pipeline type).  This prevents the shared-pipeline prepare() overwrite
// that occurs when PaperSheet and the viewport widget both use `Pipeline`.

/// Dedicated pipeline for the MSPACE active-viewport shader widget.
pub struct PaperViewportPipeline(pub(super) Pipeline);

impl iced::widget::shader::Pipeline for PaperViewportPipeline {
    fn new(
        device: &iced::wgpu::Device,
        queue: &iced::wgpu::Queue,
        format: iced::wgpu::TextureFormat,
    ) -> Self {
        Self(Pipeline::new(device, queue, format))
    }
}

/// Primitive returned by `PaperViewportPane`; delegates everything to the
/// inner `Primitive` via the dedicated `PaperViewportPipeline`.
#[derive(Debug)]
pub struct PaperViewportPrimitive(pub(super) Primitive);

impl shader::Primitive for PaperViewportPrimitive {
    type Pipeline = PaperViewportPipeline;

    fn prepare(
        &self,
        pipeline: &mut PaperViewportPipeline,
        device: &iced::wgpu::Device,
        queue: &iced::wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        self.0.prepare(&mut pipeline.0, device, queue, bounds, viewport);
    }

    fn render(
        &self,
        pipeline: &PaperViewportPipeline,
        encoder: &mut iced::wgpu::CommandEncoder,
        target: &iced::wgpu::TextureView,
        clip: &Rectangle<u32>,
    ) {
        self.0.render(&pipeline.0, encoder, target, clip);
    }
}

// ── Camera hover state (shader::Program::State) ───────────────────────────

#[derive(Clone, Default)]
pub struct CameraState {
    pub hover_region: Option<usize>,
}

// ── GPU primitive ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Primitive {
    pub(super) wires: Vec<WireModel>,
    pub(super) hatches: Vec<HatchModel>,
    /// Wipeout fills — rendered in a separate pass AFTER wires.
    pub(super) wipeout_hatches: Vec<HatchModel>,
    pub(super) images: Vec<ImageModel>,
    pub(super) meshes: Vec<MeshModel>,
    pub(super) uniforms: Uniforms,
    /// Camera rotation matrix derived from the quaternion.
    /// Used by the ViewCube pipeline — no gimbal lock.
    pub(super) cam_rotation: Mat4,
    pub(super) hover_region: Option<usize>,
    /// Background color used to clear the MSAA buffer at the start of each frame.
    pub(super) bg_color: [f32; 4],
    pub(super) show_viewcube: bool,
}

// ── shader::Primitive impl ────────────────────────────────────────────────

impl shader::Primitive for Primitive {
    type Pipeline = Pipeline;

    fn prepare(
        &self,
        pipeline: &mut Pipeline,
        device: &iced::wgpu::Device,
        queue: &iced::wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let phys = viewport.physical_size();
        let full_size = Size::new(phys.width, phys.height);
        // MSAA and depth textures are sized to the shader widget's clip bounds,
        // not the full surface — so the MSAA resolve can't overwrite other widgets.
        let scale = viewport.scale_factor() as f32;
        let clip_size = Size::new(
            (bounds.width * scale).ceil() as u32,
            (bounds.height * scale).ceil() as u32,
        );
        pipeline.ensure_depth_texture(device, clip_size);
        pipeline.viewcube.ensure_depth_texture(device, full_size);
        pipeline.upload_uniforms(queue, &self.uniforms);
        pipeline.upload_hatches(device, &self.hatches);
        pipeline.upload_wipeouts(device, &self.wipeout_hatches);
        pipeline.upload_images(device, queue, &self.images);
        pipeline.upload_meshes(device, &self.meshes);
        pipeline.upload_wires(device, &self.wires);
        if self.show_viewcube {
            pipeline.viewcube.upload(
                queue,
                self.cam_rotation,
                bounds.width as u32,
                bounds.height as u32,
                self.hover_region,
            );
        }
    }

    fn render(
        &self,
        pipeline: &Pipeline,
        encoder: &mut iced::wgpu::CommandEncoder,
        target: &iced::wgpu::TextureView,
        clip: &Rectangle<u32>,
    ) {
        pipeline.render(encoder, target, *clip, self.bg_color);
        if self.show_viewcube {
            pipeline.viewcube.render(encoder, target, *clip);
        }
    }
}

// ── Render-style helpers (impl Scene) ────────────────────────────────────

impl Scene {
    /// Returns (entity_color, pattern_length, pattern, line_weight_px, aci).
    pub(super) fn render_style(&self, e: &EntityType) -> ([f32; 4], f32, [f32; 8], f32, u8) {
        let layer_name = &e.common().layer;
        let (entity_color, aci) = {
            let ec = &e.common().color;
            let resolved = if *ec == AcadColor::ByLayer {
                self.document
                    .layers
                    .get(layer_name)
                    .map(|l| &l.color)
                    .unwrap_or(&AcadColor::WHITE)
            } else {
                ec
            };
            let aci = match resolved {
                AcadColor::Index(i) => *i,
                _ => 0,
            };
            let [r, g, b, _] = tessellate::aci_to_rgba(resolved);
            let alpha = 1.0 - e.common().transparency.as_percent() as f32;
            ([r, g, b, alpha], aci)
        };

        let lt_name = self.resolved_linetype_name(e);
        let lt_scale = e.common().linetype_scale as f32;
        let (pattern_length, pattern) =
            resolve_pattern(&self.document.line_types, lt_name, lt_scale);

        let line_weight_px = {
            let ew = &e.common().line_weight;
            let resolved = match ew {
                LineWeight::ByLayer | LineWeight::ByBlock | LineWeight::Default => self
                    .document
                    .layers
                    .get(layer_name)
                    .map(|l| &l.line_weight)
                    .unwrap_or(&LineWeight::Default),
                _ => ew,
            };
            const MM_TO_PX: f32 = 96.0 / 25.4;
            resolved
                .millimeters()
                .map(|mm| (mm as f32 * MM_TO_PX).max(1.0))
                .unwrap_or(1.0)
        };

        (entity_color, pattern_length, pattern, line_weight_px, aci)
    }

    pub(super) fn resolved_linetype_name<'a>(&'a self, e: &'a EntityType) -> &'a str {
        let elt = &e.common().linetype;
        if elt.is_empty() || elt.eq_ignore_ascii_case("bylayer") {
            self.document
                .layers
                .get(&e.common().layer)
                .map(|l| l.line_type.as_str())
                .unwrap_or("Continuous")
        } else {
            elt.as_str()
        }
    }
}

// ── Primitive builder helpers (called by ViewportPane's shader::Program impl) ──

impl Scene {
    /// Build a full-scene Primitive for the model or paper view (the camera
    /// stored in `self.camera` is used as-is).
    pub(super) fn build_primitive(
        &self,
        hover_region: Option<usize>,
        bounds: Rectangle,
        show_viewcube: bool,
    ) -> Primitive {
        let cam = self.camera.borrow();
        self.selection.borrow_mut().vp_size = (bounds.width, bounds.height);

        let mut all_wires = self.entity_wires();
        if let Some(iw) = &self.interim_wire {
            all_wires.push(iw.clone());
        }
        all_wires.extend(self.preview_wires.iter().cloned());

        let bg_color = if self.current_layout == "Model" {
            self.bg_color
        } else {
            self.paper_bg_color
        };

        Primitive {
            wires: all_wires,
            hatches: self.synced_hatch_models(),
            wipeout_hatches: self.wipeout_models(),
            images: self.images.values().cloned().collect(),
            meshes: self.meshes.values().cloned().collect(),
            uniforms: Uniforms::new(&cam, bounds),
            cam_rotation: cam.view_rotation_mat(),
            hover_region,
            bg_color,
            show_viewcube,
        }
    }


    /// Build a Primitive that renders model-space content through a specific
    /// paper-space viewport's camera, applying its layer-freeze list.
    pub(super) fn build_viewport_primitive(
        &self,
        vp_handle: Handle,
        hover_region: Option<usize>,
        bounds: Rectangle,
    ) -> Primitive {
        let cam = match self.camera_for_viewport(vp_handle) {
            Some(c) => c,
            None => return self.build_primitive(hover_region, bounds, false),
        };

        let mut all_wires = self.model_wires_for_viewport(vp_handle);
        if let Some(iw) = &self.interim_wire {
            all_wires.push(iw.clone());
        }
        all_wires.extend(self.preview_wires.iter().cloned());

        Primitive {
            wires: all_wires,
            hatches: self.synced_hatch_models(),
            wipeout_hatches: self.wipeout_models(),
            images: self.images.values().cloned().collect(),
            meshes: self.meshes.values().cloned().collect(),
            uniforms: Uniforms::new(&cam, bounds),
            cam_rotation: cam.view_rotation_mat(),
            hover_region,
            bg_color: self.bg_color,
            show_viewcube: false,
        }
    }

    /// Wrap `build_viewport_primitive()` in `PaperViewportPrimitive` for use
    /// by `PaperViewportPane`, which needs its own dedicated pipeline type.
    pub(super) fn build_active_viewport_primitive(
        &self,
        vp_handle: Handle,
        hover_region: Option<usize>,
        bounds: Rectangle,
    ) -> PaperViewportPrimitive {
        PaperViewportPrimitive(self.build_viewport_primitive(vp_handle, hover_region, bounds))
    }

    /// Update viewcube hover state from cursor position within `bounds`.
    pub(super) fn update_viewcube_state(
        &self,
        state: &mut CameraState,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) {
        let pos = cursor.position_in(bounds);
        let cam_rotation = self.camera.borrow().view_rotation_mat();
        if let Some(p) = pos {
            state.hover_region = hover_id(
                p.x,
                p.y,
                bounds.width,
                bounds.height,
                cam_rotation,
                VIEWCUBE_PX,
            );
        } else {
            state.hover_region = None;
        }
    }

    pub(super) fn viewcube_mouse_interaction(
        &self,
        state: &CameraState,
    ) -> mouse::Interaction {
        if state.hover_region.is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

// ── Linetype pattern helper ───────────────────────────────────────────────

pub(super) fn resolve_pattern(
    table: &acadrust::tables::Table<LineType>,
    name: &str,
    scale: f32,
) -> (f32, [f32; 8]) {
    let solid = (0.0, [0.0f32; 8]);
    if name.eq_ignore_ascii_case("continuous")
        || name.eq_ignore_ascii_case("bylayer")
        || name.eq_ignore_ascii_case("byblock")
        || name.is_empty()
    {
        return solid;
    }
    let lt = match table.get(name) {
        Some(lt) => lt,
        None => return solid,
    };
    if lt.is_continuous() || lt.elements.is_empty() {
        return solid;
    }

    let mut pat = [0.0f32; 8];
    let mut pat_len = 0.0f32;
    for (i, el) in lt.elements.iter().take(8).enumerate() {
        let raw = el.length as f32 * scale;
        let encoded = if raw == 0.0 {
            0.01 * scale.max(0.01)
        } else {
            raw
        };
        pat[i] = encoded;
        pat_len += encoded.abs();
    }
    if pat_len < 1e-6 {
        return solid;
    }
    (pat_len, pat)
}
