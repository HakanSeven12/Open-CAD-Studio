# Text-bearing Entity Format-Code Integration

Audit of which entities carry MText-style inline format codes (`\H`, `\W`,
`\Q`, `\T`, `\A`, `\C`, `\c`, `\f`, `\F`, `\p…q[lcrjd]…;`, `\L\l`, `\O\o`,
`\K\k`, `\S…;`, `\U+XXXX`, `\M`, `\X`, `{ }`) and whether their renderer
currently honours them.

The MTEXT entity itself was upgraded to a run-aware parser + renderer in
commit `f12e864`; this file tracks the follow-up work for the other
text-bearing entities.

## Status matrix

| Entity                              | Content format         | Current renderer                                | Needs integration |
| ----------------------------------- | ---------------------- | ----------------------------------------------- | ----------------- |
| **MTEXT**                           | MText codes            | Run-aware parser + run renderer                 | Done              |
| **TABLE** cell                      | MText codes (`\f` `\C` `\H` standard in real DXF) | Raw string → `cxf::tessellate_text_ex`; codes are rendered as literal glyphs | **Yes — high priority** |
| **MLEADER** (`LeaderContentType::MText`) | MText codes        | `strip_mtext_codes` + `split_mtext_lines` (legacy strip path); codes are dropped silently | **Yes — high priority** |
| **ATTRIB / ATTDEF** with `mtext_flag = MultiLine / ConstantMultiLine` | MText codes | Text-style render (single-line layout) | **Yes — conditional on flag** |
| **DIMENSION** `text_override`       | MText codes (rare)     | Usually rendered via the baked `block_name` sub-entity path (which goes through MTEXT) | Optional — only matters for unbaked dim text |
| **TEXT**                            | `%%x` specials only    | `cxf::tessellate_text_ex` handles specials      | No                |
| **TOLERANCE**                       | Feature Control Frame (structured)            | Dedicated FCF pipeline                          | No                |
| **LEADER** (legacy single-leader)   | Plain text             | Dedicated pipeline                              | No                |

## What each entity loses today

### TABLE cell

Real-world AutoCAD tables emit every cell as an MText-formatted string —
font changes, per-column colour, height overrides, paragraph alignment. The
current call site at [`src/entities/table.rs:245-253`](src/entities/table.rs#L245-L253)
passes the raw string to `cxf::tessellate_text_ex`, which only recognises
the `%%x` specials and the `\L\l \O\o \K\k` decoration markers. Every other
`\<letter>…;` block falls through and gets drawn as a literal backslash +
letter sequence, so a cell like `\C7;Red` shows up as the visible text
`\C7;Red` on screen.

This is the most visible regression to fix.

### MLEADER (MText content)

Two parallel call sites strip codes via `strip_mtext_codes` +
`split_mtext_lines`:

- [`src/entities/multileader.rs:241-313`](src/entities/multileader.rs#L241-L313)
- [`src/scene/tessellate.rs:1720-1920`](src/scene/tessellate.rs#L1720-L1920)

The codes are silently dropped rather than rendered as glyph noise, so the
text looks plausible — but every inline override (font / colour / height /
paragraph align) is invisible. The two copies should be folded into the
shared run renderer at the same time as the integration.

### ATTRIB / ATTDEF (when `mtext_flag` is set)

`AttributeDefinition::mtext_flag` and `AttributeEntity::mtext_flag` carry
`MTextFlag::MultiLine` / `ConstantMultiLine` when the attribute value is
authored as MText. Current attribute rendering treats the value as a single
Text line regardless of the flag. When the flag is set, the value should
flow through the MText pipeline.

### DIMENSION `text_override`

Modern AutoCAD typically bakes dimensions into a `*D###` block whose
sub-entities include an MTEXT carrying any inline formatting. That path
already goes through the run renderer via the block-defn cache. The
`text_override` field on the unbaked Dimension *can* contain MText codes,
but in practice the codes appear in the baked block. Lowest priority.

## Refactor strategy

Extract a shared helper:

```rust
fn render_mtext_string(
    value: &str,
    entity_height: f32,
    base_wf: f32,
    base_oblique: f32,
    base_font: &str,
    rect_w: f32,
    attach_h_anchor: f32,
    attach_v_anchor: f32,
    rotation: f32,
    insertion: Vec3,
    line_spacing_factor: f32,
    is_upside_down: bool,
    is_vertical: bool,
) -> Vec<TextStroke>;
```

Then have MTEXT, TABLE cell, MLEADER (and MText-flag ATTRIB) all call it.
This collapses the four nearly-identical layout loops into one and makes
the inline-code coverage uniform across entities.

## Suggested order

1. **TABLE** — restores visibly broken rendering in any DXF/DWG that uses
   inline cell formatting (which is most of them).
2. **MLEADER** — restores the silent-fail formatting. Touches two call
   sites; fold them into the shared helper at the same time.
3. **ATTRIB / ATTDEF** — gated on `mtext_flag`. Touches one call site.
4. **DIMENSION `text_override`** — optional; only matters for unbaked dims.

## Out of scope

- **TEXT** entity does not accept MText codes (DXF spec), only `%%x`
  specials, which `cxf::tessellate_text_ex` already handles.
- **TOLERANCE** uses a structured FCF representation; no MText codes apply.
- Legacy **LEADER** (single-leader, pre-MLEADER) carries plain text only.
