// Stratum terminal shader — renders character grid cells.
//
// Each cell is a quad (2 triangles) with:
// - Position (screen-space coordinates)
// - UV coordinates into the glyph atlas
// - Foreground color
// - Background color

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) fg_color: vec4<f32>,
    @location(3) bg_color: vec4<f32>,
    @location(4) is_glyph: f32, // 1.0 = glyph quad, 0.0 = background quad
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) fg_color: vec4<f32>,
    @location(2) bg_color: vec4<f32>,
    @location(3) is_glyph: f32,
}

@group(0) @binding(0)
var glyph_texture: texture_2d<f32>;
@group(0) @binding(1)
var glyph_sampler: sampler;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.fg_color = in.fg_color;
    out.bg_color = in.bg_color;
    out.is_glyph = in.is_glyph;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.is_glyph > 0.5) {
        // Glyph rendering — sample alpha from atlas, use fg color
        let alpha = textureSample(glyph_texture, glyph_sampler, in.uv).r;
        if (alpha < 0.01) {
            discard;
        }
        return vec4<f32>(in.fg_color.rgb, in.fg_color.a * alpha);
    } else {
        // Background quad — solid color
        return in.bg_color;
    }
}
