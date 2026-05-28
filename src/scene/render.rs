// GPU rendering primitives, shader::Program / shader::Primitive impls,
// and entity render-style helpers for the Scene.

use acadrust::tables::LineType;
use acadrust::types::{Color as AcadColor, LineWeight};
use acadrust::{CadDocument, EntityType, Handle};
use glam::Mat4;
use iced::mouse;
use iced::widget::shader::{self, Viewport};
use iced::{Rectangle, Size};

use std::sync::Arc;

use super::pipeline::viewcube::{hover_id, VIEWCUBE_PX};
use super::pipeline::MultiPipeline;
use super::tess_util;
use super::{HatchModel, ImageModel, MeshLodSet, Scene, Uniforms, ViewportInstance, WireModel};

// ── Camera hover state (shader::Program::State) ───────────────────────────

#[derive(Clone, Default)]
pub struct CameraState {
    pub hover_region: Option<usize>,
}

// ── GPU primitive ─────────────────────────────────────────────────────────

/// Everything needed to render one viewport: its geometry, camera, render
/// mode, and the screen rectangle it occupies. The unified renderer carries
/// a `Vec<ViewportData>` (one per tiled / floating viewport); each gets its
/// own inner `Pipeline` instance drawn into its own rectangle.
#[derive(Debug)]
pub struct ViewportData {
    pub(super) wires: Arc<Vec<WireModel>>,
    /// 3DFACE entity wires — separated so they are uploaded to the dedicated
    /// face3d pipeline (fill + batched edges) instead of N individual WireGpu.
    pub(super) face3d_wires: Arc<Vec<WireModel>>,
    pub(super) hatches: Arc<Vec<HatchModel>>,
    /// Wipeout fills — rendered in a separate pass AFTER wires.
    pub(super) wipeout_hatches: Arc<Vec<HatchModel>>,
    pub(super) images: Arc<Vec<ImageModel>>,
    pub(super) meshes: Arc<Vec<MeshLodSet>>,
    pub(super) uniforms: Uniforms,
    /// Camera rotation matrix derived from the quaternion.
    /// Used by the ViewCube pipeline — no gimbal lock.
    pub(super) cam_rotation: Mat4,
    pub(super) hover_region: Option<usize>,
    pub(super) show_viewcube: bool,
    /// Header.fill_mode (FILLMODE): when false, hatch / wipeout / face3d-fill
    /// uploads short-circuit so the renderer draws only wireframe.
    pub(super) fill_mode: bool,
    /// Per-view "Wireframe vs Solid" toggle. When `true`, 3D face fills
    /// are dropped on the upload path so 3D faces draw as edges only.
    /// Hatch / wipeout uploads are deliberately *not* gated by this flag —
    /// the user toggle should only affect 3D solids, not 2D fills.
    pub(super) view_wireframe: bool,
    /// Whether the active render mode wants 3D mesh fills uploaded. Off
    /// in `Wireframe2D` / `Wireframe3D`; on for every shaded variant. Set
    /// at the same point `view_wireframe` is computed so the two stay in
    /// lock-step for the gating logic in `prepare()`.
    pub(super) mesh_fill: bool,
    /// Whether the active render mode wants 3D mesh / face edges
    /// rendered on top of fills. Most shaded modes turn this off; the
    /// `*WithEdges` variants and the pure wireframes leave it on.
    pub(super) show_3d_edges: bool,
    /// HiddenLine routes 3D fills through a depth-only prepass so edges
    /// occluded by closer geometry are culled by the LessEqual depth
    /// test on the wire passes that follow.
    pub(super) hidden_line: bool,
    pub(super) geometry_epoch: u64,
    /// Camera generation captured when this Primitive was assembled. Paired
    /// with `geometry_epoch` so the wire buffers re-upload when the view
    /// changes (frustum culling produces a different wire list).
    pub(super) camera_generation: u64,
    /// Screen rectangle this viewport fills, **normalized** to the widget
    /// bounds (each component in 0..1). A single full-widget view is
    /// `(0, 0, 1, 1)`; tiled / floating viewports are sub-rectangles.
    /// Normalized form lets `render()` derive the physical sub-clip from
    /// the surface clip without needing the scale factor.
    pub(super) screen_rect: Rectangle,
}

#[derive(Debug)]
pub struct Primitive {
    /// One entry per viewport drawn this frame (≥1).
    pub(super) viewports: Vec<ViewportData>,
    /// Background color used to clear each viewport's MSAA buffer.
    pub(super) bg_color: [f32; 4],
}

/// Flags the render pipeline consumes, derived from
/// [`acadrust::entities::ViewportRenderMode`]. Each shaded variant fills
/// 3D faces and meshes; the pure wireframes drop the fill and keep only
/// edges. `*WithEdges` variants render both. HiddenLine uses a depth
/// prepass: face/mesh fills are uploaded but routed through depth-only
/// pipelines so hidden edges drop out. `FlatShaded` vs `GouraudShaded`
/// differ in shader uniform only and produce identical fill flags here.
#[derive(Clone, Copy, Debug)]
pub struct RenderModeFlags {
    pub face3d_fill: bool,
    pub mesh_fill: bool,
    pub show_3d_edges: bool,
    pub hidden_line: bool,
    /// `true` for FlatShaded / FlatShadedWithEdges. The mesh shader
    /// reads `Uniforms.flat_shade` and replaces the smooth per-vertex
    /// normal with a per-triangle face normal so each triangle reads
    /// as a single tone.
    pub flat_shade: bool,
}

pub fn render_mode_flags(
    mode: acadrust::entities::ViewportRenderMode,
) -> RenderModeFlags {
    use acadrust::entities::ViewportRenderMode as M;
    match mode {
        M::Wireframe2D | M::Wireframe3D => RenderModeFlags {
            face3d_fill: false,
            mesh_fill: false,
            show_3d_edges: true,
            hidden_line: false,
            flat_shade: false,
        },
        M::HiddenLine => RenderModeFlags {
            face3d_fill: true,
            mesh_fill: true,
            show_3d_edges: true,
            hidden_line: true,
            flat_shade: false,
        },
        M::FlatShaded => RenderModeFlags {
            face3d_fill: true,
            mesh_fill: true,
            show_3d_edges: false,
            hidden_line: false,
            flat_shade: true,
        },
        M::GouraudShaded => RenderModeFlags {
            face3d_fill: true,
            mesh_fill: true,
            show_3d_edges: false,
            hidden_line: false,
            flat_shade: false,
        },
        M::FlatShadedWithEdges => RenderModeFlags {
            face3d_fill: true,
            mesh_fill: true,
            show_3d_edges: true,
            hidden_line: false,
            flat_shade: true,
        },
        M::GouraudShadedWithEdges => RenderModeFlags {
            face3d_fill: true,
            mesh_fill: true,
            show_3d_edges: true,
            hidden_line: false,
            flat_shade: false,
        },
    }
}

// ── shader::Primitive impl ────────────────────────────────────────────────

impl shader::Primitive for Primitive {
    type Pipeline = MultiPipeline;

    fn prepare(
        &self,
        pipeline: &mut MultiPipeline,
        device: &iced::wgpu::Device,
        queue: &iced::wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let phys = viewport.physical_size();
        let full_size = Size::new(phys.width, phys.height);
        let scale = viewport.scale_factor() as f32;
        pipeline.ensure_len(device, queue, self.viewports.len());

        for (i, vp) in self.viewports.iter().enumerate() {
            let inner = &mut pipeline.inners[i];
            // The MSAA / depth / resolve textures are always sized to the
            // FULL viewport rectangle (not the on-canvas-visible portion)
            // so the camera matrices render at consistent aspect / scale.
            // The blit step picks the visible sub-rectangle out via the
            // shader's UV crop uniform, which lets partially off-canvas
            // viewports composite to their visible surface area without
            // drift.
            let clip_size = Size::new(
                (vp.screen_rect.width * bounds.width * scale).ceil().max(1.0) as u32,
                (vp.screen_rect.height * bounds.height * scale).ceil().max(1.0) as u32,
            );
            inner.ensure_depth_texture(device, clip_size);
            inner.viewcube.ensure_depth_texture(device, full_size);
            // Compute the UV crop for this viewport. `screen_rect` is in
            // normalized canvas units (0..1) but may extend negative or
            // beyond 1 when the viewport hangs off the canvas. The on-
            // canvas portion in viewport-local UV is straightforward to
            // derive from how much sticks out on each side.
            let sr = vp.screen_rect;
            let (uo_x, us_x) = uv_crop_axis(sr.x, sr.width);
            let (uo_y, us_y) = uv_crop_axis(sr.y, sr.height);
            inner.upload_blit_uv(queue, [uo_x, uo_y], [us_x, us_y]);
            inner.upload_uniforms(queue, &vp.uniforms);
            let cur_key = (vp.geometry_epoch, vp.camera_generation);
            let fill_mode = vp.fill_mode;
            // 3D face fill requires *both* the doc-level FILLMODE *and* the
            // per-view Solid toggle. Hatches / wipeouts deliberately ignore
            // the view toggle so 2D fills stay on even when the user picks
            // the Wireframe overlay style.
            let face3d_fill_active = fill_mode && !vp.view_wireframe;
            if cur_key != inner.cached_epoch {
                // Static buffers (hatches/images/meshes) only need refresh on
                // a real geometry change, not on every camera tick.
                if vp.geometry_epoch != inner.cached_epoch.0 {
                    if fill_mode {
                        inner.upload_hatches(device, &vp.hatches[..]);
                        inner.upload_wipeouts(device, &vp.wipeout_hatches[..]);
                    } else {
                        inner.upload_hatches(device, &[]);
                        inner.upload_wipeouts(device, &[]);
                    }
                    inner.upload_images(device, queue, &vp.images[..]);
                    inner.upload_meshes(device, &vp.meshes[..]);
                }
                // Wires re-upload on every camera change because the visible
                // subset shifts under frustum culling.
                inner.upload_wires(device, &vp.wires[..]);
                inner.upload_face3d(
                    device,
                    &vp.face3d_wires[..],
                    &vp.wires[..],
                    !face3d_fill_active,
                );
                inner.cached_epoch = cur_key;
            }
            let vproj = vp.uniforms.view_proj;
            inner.compute_wire_scissors(vproj, clip_size.width, clip_size.height);
            inner.compute_wipeout_scissors(vproj, clip_size.width, clip_size.height);
            inner.compute_image_scissors(vproj, clip_size.width, clip_size.height);
            inner.compute_hatch_lod(queue, vproj, clip_size.width, clip_size.height);
            inner.compute_wipeout_lod(vproj, clip_size.width, clip_size.height);
            inner.compute_mesh_lod(vproj, clip_size.width, clip_size.height);
            if vp.show_viewcube {
                inner.viewcube.upload(
                    queue,
                    vp.cam_rotation,
                    (vp.screen_rect.width * bounds.width) as u32,
                    (vp.screen_rect.height * bounds.height) as u32,
                    vp.hover_region,
                );
            }
        }
    }

    fn render(
        &self,
        pipeline: &MultiPipeline,
        encoder: &mut iced::wgpu::CommandEncoder,
        target: &iced::wgpu::TextureView,
        clip: &Rectangle<u32>,
    ) {
        let cw = clip.width as f32;
        let ch = clip.height as f32;
        let clip_right = clip.x + clip.width;
        let clip_bottom = clip.y + clip.height;
        for (i, vp) in self.viewports.iter().enumerate() {
            let Some(inner) = pipeline.inners.get(i) else {
                break;
            };
            // Where the viewport would land on the surface in absolute
            // pixels (i32 because either edge may stick off the canvas).
            let vp_full_x = clip.x as i32 + (vp.screen_rect.x * cw) as i32;
            let vp_full_y = clip.y as i32 + (vp.screen_rect.y * ch) as i32;
            let vp_full_w = (vp.screen_rect.width * cw).max(1.0) as i32;
            let vp_full_h = (vp.screen_rect.height * ch).max(1.0) as i32;
            // Intersect with the surface clip — that's the slice we blit.
            let dest_x = vp_full_x.max(clip.x as i32);
            let dest_y = vp_full_y.max(clip.y as i32);
            let dest_right = (vp_full_x + vp_full_w).min(clip_right as i32);
            let dest_bottom = (vp_full_y + vp_full_h).min(clip_bottom as i32);
            if dest_right <= dest_x || dest_bottom <= dest_y {
                continue;
            }
            let surface_dest = Rectangle {
                x: dest_x as u32,
                y: dest_y as u32,
                width: (dest_right - dest_x) as u32,
                height: (dest_bottom - dest_y) as u32,
            };
            let vp_size = Size::new(vp_full_w.max(1) as u32, vp_full_h.max(1) as u32);
            // `mesh_fill` is false for Wireframe 2D / Wireframe 3D — flip
            // the draw path so meshes use the wireframe pipeline + the
            // pre-built triangle-edge index buffer.
            let mesh_wireframe = !vp.mesh_fill;
            inner.render(
                encoder,
                target,
                vp_size,
                surface_dest,
                self.bg_color,
                mesh_wireframe,
                vp.hidden_line,
                vp.show_3d_edges,
            );
            // The ViewCube renders directly to the surface at the full
            // viewport rect. Skip it when the viewport's top-right corner
            // (where the cube sits) is off-canvas — wgpu's `set_viewport`
            // rejects negative origins, and a clamped cube would scale
            // distortedly. The active viewport is normally fully visible.
            if vp.show_viewcube
                && vp_full_x >= clip.x as i32
                && vp_full_y >= clip.y as i32
                && vp_full_x + vp_full_w <= clip_right as i32
                && vp_full_y + vp_full_h <= clip_bottom as i32
            {
                let vp_clip = Rectangle {
                    x: vp_full_x as u32,
                    y: vp_full_y as u32,
                    width: vp_full_w as u32,
                    height: vp_full_h as u32,
                };
                inner.viewcube.render(encoder, target, vp_clip);
            }
        }
    }
}

/// On-canvas-visible UV crop on one axis. `pos` and `size` are in the
/// shader widget's normalized 0..1 coords. Returns `(uv_offset, uv_scale)`
/// applied as `actual_uv = quad_uv * uv_scale + uv_offset` in the blit
/// shader — identity `(0.0, 1.0)` for fully on-canvas viewports.
fn uv_crop_axis(pos: f32, size: f32) -> (f32, f32) {
    if size <= 0.0 {
        return (0.0, 1.0);
    }
    let left_off = (-pos).max(0.0);
    let right_off = (pos + size - 1.0).max(0.0);
    let visible = (size - left_off - right_off).max(0.0);
    (left_off / size, visible / size)
}

/// Apply a clip-space crop to `view_proj` so the sub-rect of the original
/// view defined by UV offset `(uo, vo)` + scale `(us, vs)` is remapped to
/// NDC `[-1, 1]^2`. Identity transform when the sub-rect is the whole
/// view (`uo=vo=0`, `us=vs=1`). Used by viewports that hang off the
/// canvas — the camera frustum stays at full-vp aspect, but only the
/// visible portion lands in the MSAA target.
fn crop_view_proj(view_proj: glam::Mat4, uo: f32, vo: f32, us: f32, vs: f32) -> glam::Mat4 {
    // Build the matrix that maps the visible clip-space sub-rect
    //   x ∈ [2uo - 1, 2(uo+us) - 1]
    //   y ∈ [1 - 2(vo+vs), 1 - 2vo]
    // back to NDC [-1, 1]^2. (Texture v is top-down → camera y flips.)
    let us = us.max(1e-6);
    let vs = vs.max(1e-6);
    let sx = 1.0 / us;
    let sy = 1.0 / vs;
    let tx = (1.0 - 2.0 * uo - us) / us;
    let ty = -(1.0 - 2.0 * vo - vs) / vs;
    let crop = glam::Mat4::from_cols_array(&[
        sx, 0.0, 0.0, 0.0, // col 0
        0.0, sy, 0.0, 0.0, // col 1
        0.0, 0.0, 1.0, 0.0, // col 2
        tx, ty, 0.0, 1.0, // col 3
    ]);
    crop * view_proj
}

// ── Render-style helpers (impl Scene) ────────────────────────────────────

impl Scene {
    /// Returns (entity_color, pattern_length, pattern, line_weight_px, aci).
    pub(super) fn render_style(&self, e: &EntityType) -> ([f32; 4], f32, [f32; 8], f32, u8) {
        let (color, pl, pat, lw, aci) = render_style_for(&self.document, e);
        let bg = if self.current_layout == "Model" {
            self.bg_color
        } else {
            self.paper_bg_color
        };
        (adapt_to_bg(color, bg), pl, pat, lw, aci)
    }
}

// ── Document-only render-style helpers (no &self, safe to call from parallel contexts) ──

/// Resolves the effective linetype name for an entity, falling back to the
/// layer's linetype when the entity's own linetype is "ByLayer".
pub(super) fn linetype_name_for<'a>(document: &'a CadDocument, e: &'a EntityType) -> &'a str {
    let elt = &e.common().linetype;
    if elt.is_empty() || elt.eq_ignore_ascii_case("bylayer") {
        document
            .layers
            .get(&e.common().layer)
            .map(|l| l.line_type.as_str())
            .unwrap_or("Continuous")
    } else {
        elt.as_str()
    }
}

/// Returns `(entity_color, pattern_length, pattern, line_weight_px, aci)` for
/// an entity, resolving ByLayer color and linetype from the document.
pub(super) fn render_style_for(
    document: &CadDocument,
    e: &EntityType,
) -> ([f32; 4], f32, [f32; 8], f32, u8) {
    let layer_name = &e.common().layer;
    let (entity_color, aci) = {
        let ec = &e.common().color;
        let resolved = if *ec == AcadColor::ByLayer {
            document
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
        let [r, g, b, _] = tess_util::aci_to_rgba(resolved);
        let alpha = 1.0 - e.common().transparency.as_percent() as f32;
        ([r, g, b, alpha], aci)
    };

    let lt_name = linetype_name_for(document, e);
    // Effective scale = global LTSCALE × per-entity scale (both default to 1.0).
    let lt_scale = document.header.linetype_scale as f32 * e.common().linetype_scale as f32;
    let (pattern_length, pattern) = resolve_pattern(&document.line_types, lt_name, lt_scale);

    let line_weight_px = {
        // LWDISPLAY is no longer evaluated here — the toggle is now applied in
        // the wire shader via `Uniforms.lwdisplay_enable`, so we always bake the
        // entity's resolved (layer-inherited) weight. Toggling lineweight
        // visibility costs only a uniform write, not a retessellate.
        let ew = &e.common().line_weight;
        let resolved = match ew {
            LineWeight::ByLayer | LineWeight::ByBlock | LineWeight::Default => document
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

/// Like `render_style_for` but resolves ByBlock properties by inheriting from
/// the INSERT entity's already-resolved style. Call this for exploded block
/// sub-entities so that ByBlock color/linetype/lineweight propagate correctly.
pub(crate) fn render_style_for_block_sub(
    document: &CadDocument,
    e: &EntityType,
    insert_color: [f32; 4],
    insert_pat_len: f32,
    insert_pat: [f32; 8],
    insert_lw_px: f32,
) -> ([f32; 4], f32, [f32; 8], f32, u8) {
    let (color, pat_len, pat, lw_px, aci) = render_style_for(document, e);
    let common = e.common();

    let final_color = if common.color == AcadColor::ByBlock {
        insert_color
    } else {
        color
    };

    let (final_pat_len, final_pat) = if common.linetype.eq_ignore_ascii_case("byblock") {
        (insert_pat_len, insert_pat)
    } else {
        (pat_len, pat)
    };

    let final_lw = if matches!(common.line_weight, LineWeight::ByBlock) {
        insert_lw_px
    } else {
        lw_px
    };

    (final_color, final_pat_len, final_pat, final_lw, aci)
}

/// Adapt white→black or black→white based on background luminance.
/// White entities on light backgrounds become black, black entities on dark
/// backgrounds become white. All other colors pass through unchanged.
pub(crate) fn adapt_to_bg(color: [f32; 4], bg: [f32; 4]) -> [f32; 4] {
    let lum = 0.299 * bg[0] + 0.587 * bg[1] + 0.114 * bg[2];
    let is_white = color[0] > 0.95 && color[1] > 0.95 && color[2] > 0.95;
    let is_black = color[0] < 0.05 && color[1] < 0.05 && color[2] < 0.05;
    if is_white && lum > 0.5 {
        [0.0, 0.0, 0.0, color[3]]
    } else if is_black && lum <= 0.5 {
        [1.0, 1.0, 1.0, color[3]]
    } else {
        color
    }
}

// ── Primitive builder helpers (called by ViewportPane's shader::Program impl) ──

impl Scene {
    /// Build the unified multi-viewport `Primitive` for the current layout.
    /// Model layout → one full-window viewport (more once tiled); paper
    /// layout → one viewport per floating content viewport. Each entry is
    /// rendered into its own screen rectangle by its own inner pipeline.
    pub(super) fn build_viewports(
        &self,
        bounds: Rectangle,
        model_render_mode: acadrust::entities::ViewportRenderMode,
        _hover_region: Option<usize>,
    ) -> Primitive {
        // Hover comes from the scene cell driven by the app-level
        // `CursorMoved` handler — the cube overlay sits above the shader
        // and would otherwise mask the move event from `Program::update`.
        let hover_region = self.viewcube_hover.get();
        self.selection.borrow_mut().vp_size = (bounds.width, bounds.height);
        if bounds.height > 0.0 {
            self.set_render_aspect(bounds.width / bounds.height);
            self.set_render_pixel_scale(bounds.width, bounds.height);
        }
        let canvas = (bounds.width.max(1.0), bounds.height.max(1.0));
        let instances = self.active_viewports(canvas.0, canvas.1, model_render_mode);
        // Transparent clear — outside drawn geometry the resolve texture
        // stays at alpha=0, so the alpha-blended blit reveals whatever the
        // underlying widget painted (model layout: container bg; paper
        // layout: PaperCanvas sheet — including inside content viewport
        // rectangles).
        let bg_color = [0.0, 0.0, 0.0, 0.0];
        let viewports: Vec<ViewportData> = instances
            .iter()
            .filter_map(|inst| self.viewport_data_for(inst, canvas, hover_region))
            .collect();
        // Empty viewports → blit nothing. The widget container's
        // background (model bg or the PaperCanvas widget below) stays
        // visible. This happens in paper layouts without content viewports.
        Primitive { viewports, bg_color }
    }

    /// Build one `ViewportData` from a `ViewportInstance`: gathers the
    /// viewport's geometry (full model for the Model view / `Handle::NULL`,
    /// or the layer-frozen subset for a paper viewport), its camera
    /// uniforms, and the normalized screen rectangle.
    fn viewport_data_for(
        &self,
        inst: &ViewportInstance,
        canvas: (f32, f32),
        hover_region: Option<usize>,
    ) -> Option<ViewportData> {
        let flags = render_mode_flags(inst.render_mode);
        let view_wireframe = !flags.face3d_fill;

        // Clip the viewport rect to the canvas; size the per-viewport MSAA
        // / depth / resolve textures to that visible portion. Sizing them
        // to the full vp rect would blow past wgpu's per-dimension texture
        // limit (8192 on common GPUs) once paper-space zoom grows the rect
        // far enough off the canvas.
        let full = inst.screen_rect;
        if full.width <= 0.0 || full.height <= 0.0 {
            return None;
        }
        let visible_x = full.x.max(0.0);
        let visible_y = full.y.max(0.0);
        let visible_x_end = (full.x + full.width).min(canvas.0);
        let visible_y_end = (full.y + full.height).min(canvas.1);
        let visible_w = (visible_x_end - visible_x).max(0.0);
        let visible_h = (visible_y_end - visible_y).max(0.0);
        if visible_w < 1.0 || visible_h < 1.0 {
            return None;
        }
        let uo = ((visible_x - full.x) / full.width).clamp(0.0, 1.0);
        let vo = ((visible_y - full.y) / full.height).clamp(0.0, 1.0);
        let us = (visible_w / full.width).clamp(0.0, 1.0);
        let vs = (visible_h / full.height).clamp(0.0, 1.0);

        let base_arc = if inst.handle == acadrust::Handle::NULL {
            self.entity_wires_arc()
        } else {
            self.model_wires_for_viewport_arc(inst.handle)
        };
        let (face3d_wires, other_wires) = split_face3d_wires(&base_arc, &self.document);
        let all_wires = if self.interim_wire.is_none() && self.preview_wires.is_empty() {
            Arc::new(other_wires)
        } else {
            let mut v = other_wires;
            if let Some(iw) = &self.interim_wire {
                v.push(iw.clone());
            }
            v.extend(self.preview_wires.iter().cloned());
            Arc::new(v)
        };

        // Build the camera at the *full* viewport's aspect so the ortho
        // frustum matches what the viewport entity stores, then post-
        // multiply by a clip-space "zoom into the visible sub-rect" that
        // maps the visible portion to NDC [-1, 1]. Geometry passes
        // rasterize into a visible-sized MSAA, so `viewport_size` (used
        // by the wire shader to extrude line thickness in screen pixels)
        // must be the visible size — but `world_per_pixel` is invariant
        // under cropping (full_h cancels with vs) so the value computed
        // from the full bounds is the one we want.
        let full_bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: full.width.max(1.0),
            height: full.height.max(1.0),
        };
        let mut uniforms =
            Uniforms::new(&inst.camera, full_bounds, self.document.header.lineweight_display);
        uniforms.view_proj = crop_view_proj(uniforms.view_proj, uo, vo, us, vs);
        uniforms.viewport_size = [visible_w, visible_h];
        uniforms.flat_shade = if flags.flat_shade { 1.0 } else { 0.0 };

        // `screen_rect` carries the *visible* sub-rectangle in normalized
        // canvas coords — that's what `Pipeline::prepare` uses to size
        // the per-viewport textures and what `Primitive::render` uses to
        // pick the surface destination. The UV crop uniform reads as
        // identity here, since the texture already covers exactly the
        // visible portion.
        let screen_rect = Rectangle {
            x: visible_x / canvas.0,
            y: visible_y / canvas.1,
            width: visible_w / canvas.0,
            height: visible_h / canvas.1,
        };

        Some(ViewportData {
            wires: all_wires,
            face3d_wires: Arc::new(face3d_wires),
            hatches: self.hatch_models_arc(),
            wipeout_hatches: self.wipeout_models_arc(),
            images: self.images_arc(),
            meshes: self.meshes_arc(),
            uniforms,
            cam_rotation: inst.camera.view_rotation_mat(),
            // Only the active viewport gets the hovered-region highlight.
            hover_region: if inst.active { hover_region } else { None },
            show_viewcube: inst.active,
            fill_mode: self.document.header.fill_mode,
            view_wireframe,
            mesh_fill: flags.mesh_fill,
            show_3d_edges: flags.show_3d_edges,
            hidden_line: flags.hidden_line,
            geometry_epoch: self.geometry_epoch,
            camera_generation: self.camera_generation,
            screen_rect,
        })
    }

    /// Update viewcube hover state from cursor position within `bounds`.
    ///
    /// The cube draws in the top-right of the *active model tile* (which fills
    /// the canvas when there is a single tile), so the hover hit-test maps the
    /// cursor into that tile's local space and uses the tile's dimensions.
    pub(super) fn update_viewcube_state(
        &self,
        state: &mut CameraState,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) {
        let pos = cursor.position_in(bounds);
        let cam_rotation = self.camera.borrow().view_rotation_mat();
        if let Some(p) = pos {
            let tile = self.active_model_tile_bounds(bounds.width, bounds.height);
            state.hover_region = hover_id(
                p.x - tile.x,
                p.y - tile.y,
                tile.width,
                tile.height,
                cam_rotation,
                VIEWCUBE_PX,
            );
        } else {
            state.hover_region = None;
        }
    }

    pub(super) fn viewcube_mouse_interaction(&self, state: &CameraState) -> mouse::Interaction {
        if state.hover_region.is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

// ── Linetype pattern helper ───────────────────────────────────────────────

pub(crate) fn resolve_pattern(
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

/// Partition a wire list into (face3d_wires, other_wires).
///
/// Uses a document handle lookup so no changes to WireModel are needed.
/// O(N) per geometry epoch — acceptable since it runs once per epoch.
fn split_face3d_wires(
    wires: &[WireModel],
    document: &acadrust::CadDocument,
) -> (Vec<WireModel>, Vec<WireModel>) {
    let mut face3d = Vec::new();
    let mut others = Vec::new();
    for w in wires {
        let is_face3d = w
            .name
            .parse::<u64>()
            .ok()
            .and_then(|v| document.get_entity(Handle::new(v)))
            .map(|e| matches!(e, EntityType::Face3D(_)))
            .unwrap_or(false);
        if is_face3d {
            face3d.push(w.clone());
        } else {
            others.push(w.clone());
        }
    }
    (face3d, others)
}
