struct Crack {
    vp: mat4x4<f32>,
    offset: vec4<f32>,
    stage_uv: vec4<f32>,
}

@group(0) @binding(0) var<uniform> c: Crack;
@group(0) @binding(1) var s: sampler;
@group(0) @binding(2) var t: texture_2d<f32>;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let inflated = in.pos * 1.002 - vec3<f32>(0.001);
    out.clip = c.vp * vec4<f32>(inflated + c.offset.xyz, 1.0);
    out.uv = vec2<f32>(in.uv.x * c.stage_uv.y + c.stage_uv.x, in.uv.y);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let a = textureSample(t, s, in.uv).r;
    return vec4<f32>(0.0, 0.0, 0.0, a * 0.8);
}
