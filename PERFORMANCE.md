# Performance Optimization Plan

## Problem

The application lags severely when large files are open — even simple mouse movement causes noticeable frame drops.

### Root Cause

Every frame (triggered by any mouse event), Iced calls `shader::Program::draw()` → `build_primitive()`.
This function:

1. **Re-tessellates every entity from scratch** — `wires_for_block()` iterates all document entities and calls `tessellate_one()` for each, every single frame.
2. **Recreates all GPU buffers** — `prepare()` calls `upload_wires/hatches/images/meshes()`, each of which allocates new `wgpu::Buffer` objects for every entity, every frame.
3. **Linear-scans all objects** to find `SortEntitiesTable` on every frame.

During pure mouse navigation (pan/zoom), nothing in the geometry data changes — only the camera matrix (uniforms) changes. Despite this, all work above is repeated for every mouse-move event.

---

## Option A — Wire Tessellation Cache ✅ Implemented

**Status:** Done

Added `wire_cache: RefCell<Option<(u64, Arc<Vec<WireModel>>)>>` to `Scene`.
`entity_wires_arc()` checks the cache first; if `geometry_epoch` matches it returns the cached
`Arc` (O(1) refcount bump). On a cache miss it tessellates, wraps the result in `Arc`, stores it,
and returns a clone of the `Arc`.

`build_primitive()` uses `entity_wires_arc()` directly:
- **No preview wires (navigation):** stores the Arc as-is — zero Vec clones, zero tessellation.
- **Command active (preview wires present):** clones the cached entity Vec once, appends
  preview/interim wires, wraps in a new Arc — only one allocation instead of full tessellation.

`Primitive::wires` changed from `Vec<WireModel>` to `Arc<Vec<WireModel>>`.

**Impact:** During navigation in large files the per-frame cost of the wire step drops from
O(entities × tessellation) to O(1). Combined with Option B the GPU upload is also skipped,
making navigation frames nearly free regardless of file size.

---

## Option B — GPU Buffer Cache ✅ Implemented

**Status:** Done

Add a `geometry_epoch: u64` counter to `Scene`. Bump it whenever geometry-affecting state changes.
Carry the epoch through `Primitive` to `Pipeline`, which stores `cached_epoch: u64`.

In `prepare()`:
- **Always** upload uniforms (camera changes every frame).
- **Skip** geometry buffer uploads (`upload_wires/hatches/images/meshes`) when `geometry_epoch == cached_epoch`.
- After uploading, set `pipeline.cached_epoch = geometry_epoch`.

**Impact:** During navigation, only a single 192-byte uniform write reaches the GPU instead of
re-creating thousands of buffers. Fixes mouse-movement lag in large files.

**Difficulty:** Medium. Requires bumping the epoch in all geometry-mutation paths.

### Epoch bump locations

| Method | Location |
|--------|----------|
| `add_entity()` | `scene/mod.rs` |
| `erase_entities()` | `scene/mod.rs` |
| `populate_hatches_from_document()` | `scene/mod.rs` |
| `populate_images_from_document()` | `scene/mod.rs` |
| `populate_meshes_from_document()` | `scene/mod.rs` |
| `set_preview_wires()` | `scene/mod.rs` |
| `clear_preview_wire()` | `scene/mod.rs` |
| `set_interim_wire()` | `scene/mod.rs` |
| `select_entity()` | `scene/mod.rs` |
| `deselect_all()` | `scene/mod.rs` |
| `expand_selection_for_groups()` | `scene/mod.rs` |
| `toggle_layer_visibility()` | `scene/mod.rs` |
| `transform_entities()` | `scene/mod.rs` |
| `copy_entities()` | `scene/mod.rs` |
| `apply_grip()` | `scene/mod.rs` |
| `clear()` | `scene/mod.rs` |

---

## Option C — Parallel Tessellation (Rayon) ✅ Implemented

**Status:** Done

Extracted the body of `tessellate_one` into a free function `tessellate_entity(document, selected,
active_viewport, e)` that takes only `Send`-safe arguments (no `&self`, no `Rc`). Added `rayon = "1"`
to `Cargo.toml`.

`wires_for_block()` now:
1. Collects the filtered entity list into a `Vec<&EntityType>` sequentially (fast, no tessellation).
2. Calls `rayon::into_par_iter()` on that Vec, invoking `tessellate_entity` in parallel.
3. Flattens the results and applies the sort order via the O(1) cache from Option E.

`tessellate_one` is reduced to a thin shim that delegates to `tessellate_entity` — kept for any
remaining callers.

**Impact:** Spreads tessellation across all CPU cores. Improves file-open time and post-edit
refresh. Does **not** fix per-frame navigation lag (Options A/B address that).

**Difficulty:** Easy. Extracting the free function was the only meaningful change.

---

## Option D — Frustum / Viewport Culling

**Status:** Planned

Build a spatial index (R-tree or uniform grid) over entity bounding boxes.
During `wires_for_block()`, skip entities whose bounding box lies entirely outside the camera frustum.

**Impact:** Dramatic speedup for large drawings when zoomed in — only visible entities are
tessellated and uploaded. No benefit when the full drawing is visible.

**Difficulty:** Hard. Requires maintaining an up-to-date spatial index and computing per-entity
bounding boxes for all entity types.

---

## Option E — SortEntitiesTable Scan Cache ✅ Implemented

**Status:** Done

Added `sort_cache: RefCell<Option<(u64, HashMap<Handle, HashMap<u64, u64>>)>>` to `Scene`.

On the first call to `wires_for_block()` after a geometry change, the cache scans all
`document.objects` once and builds an index: `block_handle → (entity_handle.value() →
sort_handle.value())`. Subsequent calls within the same epoch do an O(1) HashMap lookup
instead of the previous O(objects) `find_map` scan.

The cache is keyed by `geometry_epoch` and invalidated automatically alongside the wire
and GPU caches whenever geometry changes.
