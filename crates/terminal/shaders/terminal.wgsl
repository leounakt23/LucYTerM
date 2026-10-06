// Terminal cell shader (Prompt 2.4): instanced quads over a glyph atlas.
//
// One draw call renders every visible cell:
// - vertex buffer: fullscreen quad corners (-0.5..0.5, uv 0..1)
// - instance buffer: one `CellInstance` per cell (grid position/size,
//   atlas UV rect, fg/bg RGBA)
// - uniforms: screen size (px), cell size (px), DPI scale
//
// The fragment shader samples the atlas (white glyph on transparent) and
// composites fg text over the bg fill. Reverse-video / selection inversion
// is resolved on the CPU when instances are built, so the shader stays
// branchless and 60fps-friendly.

struct Uniforms {
    screen_size: vec2<f32>,
    cell_size: vec2<f32>,
    scale_factor: f32,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var atlas_sampler: sampler;
@group(0) @binding(2) var atlas_texture: texture_2d<f32>;

struct VertexIn {
    @location(0) quad_pos: vec2<f32>,
    @location(1) quad_uv: vec2<f32>,
};

struct InstanceIn {
    // Cell rect in pixels: (x, y, w, h).
    @location(2) rect: vec4<f32>,
    // Atlas UV rect: (u0, v0, u1, v1).
    @location(3) uv_rect: vec4<f32>,
    // Linear-space colors.
    @location(4) fg: vec4<f32>,
    @location(5) bg: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) fg: vec4<f32>,
    @location(2) bg: vec4<f32>,
};

@vertex
fn vs_main(vertex: VertexIn, instance: InstanceIn) -> VertexOut {
    var out: VertexOut;
    // rect.xy is the top-left corner in pixels; y grows downwards.
    let px = instance.rect.xy + vertex.quad_pos * instance.rect.zw;
    // NDC: x in [-1, 1], y in [-1, 1] with y flipped.
    let ndc = vec2<f32>(
        (px.x / uniforms.screen_size.x) * 2.0 - 1.0,
        1.0 - (px.y / uniforms.screen_size.y) * 2.0,
    );
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = mix(instance.uv_rect.xy, instance.uv_rect.zw, vertex.quad_uv);
    out.fg = instance.fg;
    out.bg = instance.bg;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let glyph = textureSample(atlas_texture, atlas_sampler, in.uv);
    // Atlas stores white glyph coverage in rgb + alpha; use alpha as mask.
    let mask = glyph.a;
    let rgb = mix(in.bg.rgb, in.fg.rgb, mask);
    let alpha = max(in.bg.a, mask * in.fg.a);
    return vec4<f32>(rgb, alpha);
}
