// Wire shader — renders 1-D CAD entities as screen-aligned quads.
// Topology: TriangleList, 6 vertices drawn per INSTANCE.
//
// One instance = one segment. The six vertex IDs map to the corners of a
// two-triangle quad; the vertex shader derives `which_end` (0=A end, 1=B end)
// and `side` (±1 perpendicular) from `@builtin(vertex_index)` and expands the
// quad by `half_width` pixels perpendicular to the segment direction in
// screen space.
//
// Linetype is applied entirely on the GPU:
//   • distance = cumulative arc-length, linearly interpolated from
//     (distance_a, distance_b) by `which_end`.
//   • pattern_length > 0 enables the dash test; 0 = solid (no discard).
//   • pat0/pat1 encode up to 8 elements: positive=dash, negative=gap.

struct Uniforms {
    view_proj:        mat4x4<f32>,
    camera_pos:       vec4<f32>,
    viewport_size:    vec2<f32>,
    world_per_pixel:  f32,
    // LWDISPLAY toggle: 0.0 = force 1 px (half_width 0.5), 1.0 = use the
    // per-instance baked half_width. Lets the LWT button switch without
    // retessellating.
    lwdisplay_enable: f32,
}
@group(0) @binding(0) var<uniform> u: Uniforms;

struct InstanceIn {
    @location(0) pos_a:          vec3<f32>,
    @location(1) pos_b:          vec3<f32>,
    @location(2) color:          vec4<f32>,
    @location(3) distance_a:     f32,
    @location(4) distance_b:     f32,
    @location(5) half_width:     f32,
    @location(6) pattern_length: f32,
    @location(7) pat0:           vec4<f32>,
    @location(8) pat1:           vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip_pos:       vec4<f32>,
    @location(0)       color:          vec4<f32>,
    @location(1)       distance:       f32,
    @location(2)       pattern_length: f32,
    @location(3)       pat0:           vec4<f32>,
    @location(4)       pat1:           vec4<f32>,
}

@vertex fn vs_main(@builtin(vertex_index) vid: u32, in: InstanceIn) -> VertexOut {
    // Two-triangle quad corner table:
    //   vid 0,1,2 = (A,-1) (B,-1) (B,+1)
    //   vid 3,4,5 = (A,-1) (B,+1) (A,+1)
    let which_end_arr = array<f32, 6>(0.0, 1.0, 1.0, 0.0, 1.0, 0.0);
    let side_arr      = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    let which_end = which_end_arr[vid];
    let side      = side_arr[vid];

    let clip_a = u.view_proj * vec4<f32>(in.pos_a, 1.0);
    let clip_b = u.view_proj * vec4<f32>(in.pos_b, 1.0);

    // NDC of both endpoints.
    let ndc_a = clip_a.xy / clip_a.w;
    let ndc_b = clip_b.xy / clip_b.w;

    // Screen-space pixel positions.
    let screen_a = ndc_a * u.viewport_size * 0.5;
    let screen_b = ndc_b * u.viewport_size * 0.5;

    // Screen-space perpendicular to segment direction.
    let seg = screen_b - screen_a;
    let seg_len = length(seg);
    var perp: vec2<f32>;
    if seg_len > 1e-4 {
        let dir = seg / seg_len;
        perp = vec2<f32>(-dir.y, dir.x);
    } else {
        perp = vec2<f32>(0.0, 1.0);
    }

    // Convert perpendicular from screen pixels to NDC offset.
    let perp_ndc = perp / (u.viewport_size * 0.5);

    // Select the clip-space position for this vertex's endpoint.
    let clip_pos = mix(clip_a, clip_b, which_end);

    // LWDISPLAY off → collapse to a 1-pixel-wide line (half_width = 0.5).
    let hw = select(0.5, in.half_width, u.lwdisplay_enable > 0.5);

    // Offset in clip space (multiply by w to un-apply perspective division).
    let ndc_offset = perp_ndc * hw * side;
    let final_clip = clip_pos + vec4<f32>(ndc_offset * clip_pos.w, 0.0, 0.0);

    var out: VertexOut;
    out.clip_pos       = final_clip;
    out.color          = in.color;
    out.distance       = mix(in.distance_a, in.distance_b, which_end);
    out.pattern_length = in.pattern_length;
    out.pat0           = in.pat0;
    out.pat1           = in.pat1;
    return out;
}

// Returns true if arc-length `dist` falls inside a dash segment.
fn in_dash(dist: f32, pat_len: f32, p0: vec4<f32>, p1: vec4<f32>) -> bool {
    let d   = ((dist % pat_len) + pat_len) % pat_len;
    var pos = 0.0f;
    let elems = array<f32, 8>(p0.x, p0.y, p0.z, p0.w, p1.x, p1.y, p1.z, p1.w);
    for (var i = 0u; i < 8u; i++) {
        let elem = elems[i];
        if elem == 0.0 { break; }
        let len = abs(elem);
        if d < pos + len { return elem > 0.0; }
        pos += len;
    }
    return false;
}

@fragment fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    if in.pattern_length > 0.0 {
        if !in_dash(in.distance, in.pattern_length, in.pat0, in.pat1) {
            discard;
        }
    }
    return in.color;
}
