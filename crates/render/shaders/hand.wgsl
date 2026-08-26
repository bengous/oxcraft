struct Hand {
    proj: mat4x4<f32>,
    sun_color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> h: Hand;
@group(0) @binding(1) var s: sampler;
@group(0) @binding(2) var t: texture_2d<f32>;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec3<f32>,
}

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
}

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = h.proj * vec4<f32>(in.pos, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(t, s, in.uv);
    if (tex.a < 0.5) {
        discard;
    }
    return vec4<f32>(tex.rgb * in.color * h.sun_color.rgb, 1.0);
}
