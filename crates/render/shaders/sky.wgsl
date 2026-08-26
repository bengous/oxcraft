struct Sky {
    inv_vp: mat4x4<f32>,
    cam_pos: vec4<f32>,
    zenith: vec4<f32>,
    horizon: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    viewport: vec4<f32>,
}

@group(0) @binding(0) var<uniform> sky: Sky;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(pos[vi], 0.99999, 1.0);
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let ndc = vec2<f32>(
        frag.x / sky.viewport.x * 2.0 - 1.0,
        1.0 - frag.y / sky.viewport.y * 2.0,
    );
    let world = sky.inv_vp * vec4<f32>(ndc, 1.0, 1.0);
    let dir = normalize(world.xyz / world.w - sky.cam_pos.xyz);
    let t = pow(clamp(dir.y * 1.6 + 0.25, 0.0, 1.0), 0.8);
    var col = mix(sky.horizon.rgb, sky.zenith.rgb, t);
    col = mix(col, sky.horizon.rgb * 0.55, clamp(-dir.y * 3.0, 0.0, 1.0));
    let sd = max(dot(dir, normalize(sky.sun_dir.xyz)), 0.0);
    col += sky.sun_color.rgb * pow(sd, 600.0) * 3.0;
    col += sky.sun_color.rgb * pow(sd, 6.0) * 0.12;
    return vec4<f32>(col, 1.0);
}
