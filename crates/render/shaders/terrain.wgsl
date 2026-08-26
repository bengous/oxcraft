@group(0) @binding(0) var<uniform> u: Scene;
@group(0) @binding(1) var s: sampler;
@group(0) @binding(2) var t: texture_2d<f32>;

struct Scene {
    view_proj: mat4x4<f32>,
    sun_color: vec4<f32>,
    fog_color: vec4<f32>,
    fog_range: vec4<f32>,
    cam_pos: vec4<f32>,
}

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) light: f32,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) light: f32,
    @location(2) fog: f32,
}

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = u.view_proj * vec4<f32>(in.pos, 1.0);
    out.uv = in.uv;
    out.light = in.light;
    let dist = distance(in.pos, u.cam_pos.xyz);
    out.fog = clamp((dist - u.fog_range.x) / (u.fog_range.y - u.fog_range.x), 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(t, s, in.uv);
    if (tex.a < 0.5) {
        discard;
    }
    var col = tex.rgb * in.light * u.sun_color.rgb;
    col = mix(col, u.fog_color.rgb, in.fog);
    return vec4<f32>(col, 1.0);
}

@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(t, s, in.uv);
    var col = tex.rgb * in.light * u.sun_color.rgb;
    col = mix(col, u.fog_color.rgb, in.fog);
    return vec4<f32>(col, tex.a);
}
