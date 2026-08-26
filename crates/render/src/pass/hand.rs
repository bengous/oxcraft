//! Hand pass: the first-person arm and the block it holds, built in view
//! space every frame and drawn over the finished world. It runs in the
//! overlay pass, whose depth buffer starts cleared, so the arm and the held
//! block sort against each other and never against world geometry.

use bytemuck::Zeroable;
use glam::{Mat4, Quat, Vec3};
use ox_core::atlas::{T_WHITE, tile_uv};
use ox_core::blocks::{BlockId, TileId, def};
use ox_core::mesher::{FACE_UV, FACES};

use crate::FrameParams;
use crate::gpu::Gpu;
use crate::pass::{depth_stencil, pad};

/// One hand vertex in view space: position, atlas UV and RGB tint.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct HandVertex {
    /// View-space position.
    pub(crate) pos: [f32; 3],
    /// Atlas texture coordinate.
    pub(crate) uv: [f32; 2],
    /// Multiplied into the sampled texel.
    pub(crate) color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct HandUniforms {
    proj: [[f32; 4]; 4],
    sun_color: [f32; 4],
}

/// What the hand shows this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandParams {
    /// Block held in the hand.
    pub block: BlockId,
    /// Swing phase: 0 at rest, rising to 1 at the end of a swing.
    pub swing: f32,
}

const FOV_DEG: f32 = 70.0;
const CUBOIDS: usize = 3;
/// Vertices in one [`build`] result.
pub(crate) const VERTEX_COUNT: usize = CUBOIDS * 24;
/// Indices in one [`build`] result.
pub(crate) const INDEX_COUNT: usize = CUBOIDS * 36;
const SKIN: [f32; 3] = [0.82, 0.62, 0.48];
const SLEEVE: [f32; 3] = [0.29, 0.62, 0.80];
const HAND_POS: Vec3 = Vec3::new(0.62, -0.5, -1.05);
const SHOULDER_POS: Vec3 = Vec3::new(1.15, -1.25, -0.6);
const HELD_HALF: f32 = 0.12;

/// Perspective used by the hand alone, so it never clips against the near
/// plane of the world camera.
pub(crate) fn projection(aspect: f32) -> [[f32; 4]; 4] {
    glam::camera::rh::proj::directx::perspective(FOV_DEG.to_radians(), aspect, 0.05, 10.0)
        .to_cols_array_2d()
}

fn swing_transform(p: f32) -> Mat4 {
    let a = (p.sqrt() * std::f32::consts::PI).sin();
    let b = (p.sqrt() * std::f32::consts::TAU).sin();
    let c = (p * p * std::f32::consts::PI).sin();
    let d = (p * std::f32::consts::PI).sin();
    Mat4::from_translation(Vec3::new(-0.45 * a, 0.18 * b, -0.25 * d))
        * Mat4::from_rotation_y(-20f32.to_radians() * c)
        * Mat4::from_rotation_z(-25f32.to_radians() * a)
        * Mat4::from_rotation_x(-75f32.to_radians() * a)
}

/// One built hand: fixed-size geometry, so a frame builds it without
/// touching the heap.
struct HandMesh {
    verts: [HandVertex; VERTEX_COUNT],
    idx: [u16; INDEX_COUNT],
    faces: usize,
}

impl HandMesh {
    fn new() -> Self {
        Self {
            verts: [HandVertex::zeroed(); VERTEX_COUNT],
            idx: [0; INDEX_COUNT],
            faces: 0,
        }
    }

    /// Appends the six faces of the box spanning `min..max` under `model`,
    /// tinted by `color` and textured from `tiles` when it holds a block.
    fn cuboid(
        &mut self,
        model: Mat4,
        min: Vec3,
        max: Vec3,
        color: [f32; 3],
        tiles: Option<[TileId; 6]>,
    ) {
        let white = tile_uv(T_WHITE, 0.5, 0.5);
        for (face, tile) in FACES.iter().zip(tiles.map_or([None; 6], |t| t.map(Some))) {
            let base = self.faces * 4;
            for (k, (corner, uv)) in face.corners.iter().zip(FACE_UV).enumerate() {
                let local = min + Vec3::from(*corner) * (max - min);
                let pos = model.transform_point3(local);
                self.verts[base + k] = HandVertex {
                    pos: pos.to_array(),
                    uv: tile.map_or(white, |t| tile_uv(t, uv[0], uv[1])),
                    color: color.map(|c| c * face.shade),
                };
            }
            let b = base as u16;
            let quad = self.faces * 6;
            self.idx[quad..quad + 6].copy_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
            self.faces += 1;
        }
    }
}

/// Geometry for the arm (skin and sleeve cuboids) and the held block at
/// swing phase `swing`, in view space.
fn build(block: BlockId, swing: f32) -> HandMesh {
    let mut out = HandMesh::new();
    let dir = (HAND_POS - SHOULDER_POS).normalize();
    let len = (HAND_POS - SHOULDER_POS).length();
    let assembly = Mat4::from_translation(HAND_POS)
        * swing_transform(swing)
        * Mat4::from_quat(Quat::from_rotation_arc(Vec3::Y, dir));
    out.cuboid(
        assembly,
        Vec3::new(-0.11, -len, -0.11),
        Vec3::new(0.11, -len * 0.5, 0.11),
        SLEEVE,
        None,
    );
    out.cuboid(
        assembly,
        Vec3::new(-0.1, -len * 0.5, -0.1),
        Vec3::new(0.1, 0.0, 0.1),
        SKIN,
        None,
    );
    let held = assembly
        * Mat4::from_translation(Vec3::new(0.0, HELD_HALF, 0.0))
        * Mat4::from_rotation_y(35f32.to_radians())
        * Mat4::from_rotation_x(-12f32.to_radians());
    out.cuboid(
        held,
        Vec3::splat(-HELD_HALF),
        Vec3::splat(HELD_HALF),
        [1.0; 3],
        Some(def(block).tiles),
    );
    out
}

/// Hand pipeline with fixed-size vertex, index and uniform buffers.
pub(crate) struct HandPass {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    uniform_buf: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

impl HandPass {
    /// Creates the hand resources for `format` from the shared [`Gpu`] core.
    pub(crate) fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let uniform_buf =
            gpu.uniform_buffer("hand-uniforms", std::mem::size_of::<HandUniforms>() as u64);
        let bind = gpu.atlas_bind_group("hand-bind", &uniform_buf);
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("hand-shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/hand.wgsl").into()),
            });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("hand-layout"),
                bind_group_layouts: &[Some(&gpu.scene_bgl)],
                immediate_size: 0,
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("hand-pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<HandVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: 0,
                                shader_location: 0,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x2,
                                offset: 12,
                                shader_location: 1,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: 20,
                                shader_location: 2,
                            },
                        ],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil(true, wgpu::CompareFunction::LessEqual)),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        let vertex_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hand-vb"),
            size: (VERTEX_COUNT * std::mem::size_of::<HandVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hand-ib"),
            size: (INDEX_COUNT * std::mem::size_of::<u16>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            uniform_buf,
            bind,
        }
    }

    /// Builds the hand, uploads it and records the draw; nothing is
    /// recorded while the view holds no hand.
    pub(crate) fn record(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        queue: &wgpu::Queue,
        frame: &FrameParams<'_>,
        width: f32,
        height: f32,
    ) {
        let Some(hand) = frame.hand else { return };
        let mesh = build(hand.block, hand.swing);
        let uniforms = HandUniforms {
            proj: projection(width / height.max(1.0)),
            sun_color: pad(frame.sun_color, 1.0),
        };
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&mesh.verts));
        queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&mesh.idx));
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind, &[]);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        rpass.draw_indexed(0..INDEX_COUNT as u32, 0, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_core::blocks::{GLASS, STONE};

    fn bits(verts: &[HandVertex]) -> Vec<u32> {
        verts
            .iter()
            .flat_map(|v| v.pos.into_iter().chain(v.uv).chain(v.color))
            .map(f32::to_bits)
            .collect()
    }

    #[test]
    fn build_emits_three_cuboids() {
        let mesh = build(STONE, 0.0);
        assert_eq!(mesh.faces, CUBOIDS * 6);
        assert_eq!(
            mesh.idx.iter().max().copied(),
            Some(VERTEX_COUNT as u16 - 1)
        );
    }

    #[test]
    fn idle_and_completed_swing_coincide() {
        let rest = build(STONE, 0.0);
        let done = build(STONE, 1.0);
        for (a, b) in rest.verts.iter().zip(&done.verts) {
            for (x, y) in a.pos.iter().zip(&b.pos) {
                assert!((x - y).abs() < 1e-5, "{x} vs {y}");
            }
        }
    }

    #[test]
    fn mid_swing_moves_geometry() {
        let rest = build(STONE, 0.0);
        let mid = build(STONE, 0.4);
        assert_ne!(bits(&rest.verts), bits(&mid.verts));
    }

    #[test]
    fn every_vertex_sits_in_front_of_the_camera() {
        for phase in [0.0, 0.25, 0.5, 0.75] {
            let mesh = build(GLASS, phase);
            assert!(mesh.verts.iter().all(|v| v.pos[2] < 0.0), "phase {phase}");
        }
    }

    #[test]
    fn held_block_samples_its_own_tiles() {
        let mesh = build(STONE, 0.0);
        let tiles = def(STONE).tiles;
        for (face, chunk) in mesh.verts[48..].chunks(4).enumerate() {
            let [u0, v1] = tile_uv(tiles[face], 0.0, 1.0);
            let [u1, v0] = tile_uv(tiles[face], 1.0, 0.0);
            for v in chunk {
                assert!(
                    v.uv[0] >= u0 && v.uv[0] <= u1,
                    "u {} in {u0}..{u1}",
                    v.uv[0]
                );
                assert!(
                    v.uv[1] >= v1 && v.uv[1] <= v0,
                    "v {} in {v1}..{v0}",
                    v.uv[1]
                );
            }
        }
    }

    /// The pipeline culls back faces, so every quad must wind
    /// counter-clockwise under the right-hand rule seen from outside its
    /// own cuboid; otherwise the hand shows its far side.
    #[test]
    fn every_face_winds_counter_clockwise_seen_from_outside() {
        let mesh = build(STONE, 0.35);
        for cuboid in mesh.verts.chunks(24) {
            let center: Vec3 = cuboid.iter().map(|v| Vec3::from(v.pos)).sum::<Vec3>() / 24.0;
            for face in cuboid.chunks(4) {
                let c: [Vec3; 4] = std::array::from_fn(|k| Vec3::from(face[k].pos));
                let normal = (c[1] - c[0]).cross(c[2] - c[0]);
                let centroid = (c[0] + c[1] + c[2] + c[3]) / 4.0;
                assert!(normal.dot(centroid - center) > 0.0, "{normal:?}");
            }
        }
    }

    #[test]
    fn projection_keeps_the_aspect_ratio() {
        let wide = projection(2.0);
        let square = projection(1.0);
        assert!((wide[0][0] * 2.0 - square[0][0]).abs() < 1e-6);
        assert!((wide[1][1] - square[1][1]).abs() < 1e-6);
    }
}
