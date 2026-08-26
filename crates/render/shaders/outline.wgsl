struct Outline {
    vp: mat4x4<f32>,
    offset: vec4<f32>,
}

@group(0) @binding(0) var<uniform> o: Outline;

@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
    let scaled = pos * 1.004 - vec3<f32>(0.002);
    return o.vp * vec4<f32>(scaled + o.offset.xyz, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.05, 0.05, 0.05, 1.0);
}
