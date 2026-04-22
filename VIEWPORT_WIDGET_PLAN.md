<!--
CONTEXT PROMPT FOR AI ASSISTANT
=================================
You are working on H7CAD, a CAD application written in Rust using the Iced GUI framework and wgpu for GPU rendering. The project is at /home/hakanseven/Kodlama/H7CAD.

This document describes a planned refactoring. Steps 1–4 are DONE and committed. Steps 5–6 remain.

Key files to read before continuing:
- src/scene/viewport_pane.rs     — NEW: ViewportPane widget (Model / PaperSheet / Paper modes)
- src/scene/render.rs            — Scene render helpers (build_primitive, build_paper_sheet_primitive, build_viewport_primitive)
- src/scene/mod.rs               — Scene struct; viewport_screen_rect(), camera_for_viewport(), model_wires_for_viewport(), paper_sheet_wires()
- src/app/view.rs                — paper_canvas_view() function; view() uses ViewportPane::model + paper_canvas_view
- src/scene/camera.rs            — Camera struct (arcball, orthographic/perspective)
- src/entities/viewport.rs       — Viewport DXF entity: grips, properties, transform impls
- src/ui/statusbar.rs            — layout tab bar (space_tab function, LayoutSwitch message)

Architecture summary (after Steps 1–4):
- Model tab: shader(ViewportPane::model(&tab.scene)).width(Fill).height(Fill)
- Paper tab: paper_canvas_view(tab) which builds a stack![] of:
    layer 1: shader(ViewportPane::paper_sheet(scene))  — paper-space entities, paper camera
    layer 2+: shader(ViewportPane::paper(scene, handle)) per Viewport entity,
              positioned with Space offsets computed from viewport_screen_rect()
- Mouse events in paper space still route through the same viewport_mouse mouse_area overlay.
- ViewCube is shown in Model and PaperSheet modes; hidden in Paper mode.

What still needs to be done (Steps 5–6):
5. Mouse routing per-viewport in paper space (MSPACE double-click to enter, Escape to exit).
   Currently the viewport_mouse mouse_area covers the full canvas; MSPACE detection already
   works via Scene::active_viewport + viewport_at_paper_point(). Needs wiring to route
   pan/zoom/select to the correct viewport widget.
6. Per-viewport layer freeze already works in build_viewport_primitive via model_wires_for_viewport().
   Verify that the existing frozen_layers list on each Viewport entity is respected correctly.
-->

# Unified ViewportPane Widget Plan

## Current State (as of implementation start)

- `src/scene/render.rs`: `Scene` implements `shader::Program<Msg>` → single full-screen GPU widget
- `src/app/view.rs:106`: `shader(&tab.scene).width(Fill).height(Fill)` — same widget for both model and paper space, only render content differs
- `src/entities/viewport.rs`: `Viewport` entity is a pure DXF data struct with no shader or widget of its own

## Target Architecture

```
Model tab  →  ViewportPane (mode: Model)    — Fill × Fill
Paper tab  →  paper_canvas_view()
               ├── ViewportPane (mode: PaperSheet)    — Fill × Fill (paper entities + camera)
               ├── ViewportPane (mode: Paper, VP1)    — vp.width × vp.height px
               └── ViewportPane (mode: Paper, VP2)    — vp.width × vp.height px
```

---

## ✅ Step 1 — `ViewportPane` Struct

**File:** `src/scene/viewport_pane.rs`

```rust
pub enum ViewportPaneMode {
    Model,
    PaperSheet,              // paper-space entities, paper camera, no viewport projection
    Paper { handle: Handle }, // model content through a specific viewport's camera
}
pub struct ViewportPane<'a> { pub scene: &'a Scene, pub mode: ViewportPaneMode }
```

Implements `shader::Program<Msg>`. `Scene`'s own `shader::Program` impl has been **removed** — all rendering now goes through `ViewportPane`.

Scene helper methods added to `render.rs`:
- `build_primitive()` — model/full paper space render (was `draw()`)
- `build_paper_sheet_primitive()` — paper entities only, no viewport projection
- `build_viewport_primitive(vp_handle)` — model content through viewport camera
- `update_viewcube_state()` / `viewcube_mouse_interaction()`

Scene helper methods added to `mod.rs`:
- `paper_sheet_wires()` — paper-space entity wires without viewport content
- `camera_for_viewport(handle)` — build Camera from Viewport entity data
- `model_wires_for_viewport(handle)` — model wires filtered by viewport layer freeze

---

## ✅ Step 2 — Per-Viewport Camera

Implemented via `camera_for_viewport()` in `src/scene/mod.rs`. Derives a `Camera` from the Viewport entity's `view_direction`, `view_target`, and `view_height` each frame — no separate HashMap needed since `pan_active_viewport()` / `zoom_active_viewport()` already write back to the entity.

---

## ✅ Step 3 — Paper-Space Coordinates → Pixel Conversion

```rust
pub fn viewport_screen_rect(&self, vp_handle: Handle, canvas_px: (f32, f32)) -> Option<iced::Rectangle>
```

Added to `src/scene/mod.rs`. Uses `paper_limits()` as the paper extent and `scene.selection.borrow().vp_size` as the live canvas size.

---

## ✅ Step 4 — `paper_canvas_view` Function

Added to `src/app/view.rs`. Builds a `stack![]` with:
1. `shader(ViewportPane::paper_sheet(scene))` — full-size paper background
2. One `shader(ViewportPane::paper(scene, handle))` per viewport, positioned via Space offsets
3. `viewport_3d` in `view()` is now `paper_canvas_view(tab)` when `is_paper`

---

## ✅ Step 5 — Mouse Routing per Viewport

The full MSPACE routing chain is already wired and verified:

- **Double-click** inside a paper viewport → `Message::EnterViewport(handle)` → `scene.active_viewport = Some(handle)` (`update.rs:1704`, `2540`)
- **Escape** → `Message::ExitViewport` → `scene.active_viewport = None` (`update.rs:658`)
- **Middle-drag** in MSPACE → `pan_active_viewport()` via `update.rs:1079–1080`
- **Scroll** in MSPACE → `zoom_active_viewport()` via `update.rs:1784–1792`

`camera_for_viewport()` reads `view_target` / `view_height` fresh from the entity every frame, so the `ViewportPane::Paper` widget automatically reflects pan/zoom changes without any extra wiring.

---

## ✅ Step 6 — Per-Viewport Layer Freeze Verification

`model_wires_for_viewport()` (`mod.rs:2208`) uses the same handle-based filtering as the older `entity_content_wires()` (`mod.rs:697`):

1. Reads `vp.frozen_layers: Vec<Handle>` from the viewport entity
2. Looks up each entity's layer by name → gets its `Handle`
3. Skips entities whose layer handle is in the frozen set

Both sets of handles come from the same `CadDocument`, so they match correctly. Global layer visibility (`l.flags.off || l.flags.frozen`) is also applied before the per-viewport check.

---

## Implementation Order

1. ✅ `ViewportPane` + render helpers — model tab behavior unchanged
2. ✅ Per-viewport camera — derived from entity each frame
3. ✅ `viewport_screen_rect()` — paper → pixel coordinate mapping
4. ✅ `paper_canvas_view()` — stack of PaperSheet + Paper widgets
5. ✅ Mouse routing verification for MSPACE in paper space
6. ✅ Per-viewport layer freeze verification

---

## Notes

- `CameraState` (viewcube hover) is per `ViewportPane` instance — Iced manages this automatically via `shader::Program::State`.
- Paper-space entities (title blocks, frames) are rendered by `ViewportPane::PaperSheet` using the same paper-space camera as before.
- The ViewCube is hidden in `ViewportPane::Paper` mode (only shown in Model and PaperSheet).
- `selection_overlay` and grip markers use paper-space screen coordinates — this is correct for PSPACE. For MSPACE, grips of model entities are projected through the paper camera, which may need adjustment in a future step.
- Known limitation: `viewport_screen_rect()` uses the last-rendered canvas size (`scene.selection.borrow().vp_size`), so viewport positions are correct from the second frame onward.
