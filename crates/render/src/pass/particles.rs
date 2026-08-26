//! Particle pass: one camera-facing textured square per live particle,
//! drawn through `terrain.wgsl` so fog, sun tint and cutout match the
//! blocks the debris came from.

use std::cell::RefCell;

use glam::{Mat4, Vec3};
use ox_core::atlas::tile_uv;
use ox_core::particles::{MAX, Particle, SUB_UV};

use crate::FrameParams;
use crate::gpu::Gpu;
use crate::pass::{GpuVertex, SceneUniforms, depth_stencil, gpu_vertex_layout};

const LIGHT: f32 = 0.85;

/// World-space camera right and up vectors recovered from the inverse
/// view-projection matrix.
pub(crate) fn billboard_basis(inv_view_proj: [[f32; 4]; 4]) -> ([f32; 3], [f32; 3]) {
    let m = Mat4::from_cols_array_2d(&inv_view_proj);
    let right = m.transform_vector3(Vec3::X).normalize();
    let up = m.transform_vector3(Vec3::Y).normalize();
    (right.to_array(), up.to_array())
}

/// Quad indices for every particle slot; the pattern never changes, so the
/// index buffer is written once and only the draw range moves.
fn quad_indices() -> Vec<u32> {
    (0..MAX as u32)
        .flat_map(|q| {
            let b = q * 4;
            [b, b + 1, b + 2, b, b + 2, b + 3]
        })
        .collect()
}

/// One quad per particle facing the camera, capped at [`MAX`]; `verts` is
/// cleared first and holds four vertices per quad on return.
pub(crate) fn build_quads(
    verts: &mut Vec<GpuVertex>,
    particles: &[Particle],
    right: [f32; 3],
    up: [f32; 3],
) {
    let right = Vec3::from(right);
    let up = Vec3::from(up);
    verts.clear();
    for p in particles.iter().take(MAX) {
        let center = Vec3::from(p.pos);
        let half = p.size * 0.5;
        let corners = [
            center - right * half - up * half,
            center + right * half - up * half,
            center + right * half + up * half,
            center - right * half + up * half,
        ];
        let uvs = [
            [p.sub_uv[0], p.sub_uv[1]],
            [p.sub_uv[0] + SUB_UV, p.sub_uv[1]],
            [p.sub_uv[0] + SUB_UV, p.sub_uv[1] + SUB_UV],
            [p.sub_uv[0], p.sub_uv[1] + SUB_UV],
        ];
        for (corner, uv) in corners.iter().zip(uvs) {
            verts.push(GpuVertex {
                pos: corner.to_array(),
                _pad: 0.0,
                uv: tile_uv(p.tile, uv[0], uv[1]),
                light: LIGHT,
            });
        }
    }
}

/// Particle pipeline with fixed-size vertex, index and uniform buffers.
pub(crate) struct ParticlesPass {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    scene_buf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    /// Reused staging vertices, so a frame builds the quads without
    /// allocating.
    verts: RefCell<Vec<GpuVertex>>,
}

impl ParticlesPass {
    /// Creates the particle resources for `format` from the shared [`Gpu`]
    /// core.
    pub(crate) fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let scene_buf = gpu.uniform_buffer(
            "particles-uniforms",
            std::mem::size_of::<SceneUniforms>() as u64,
        );
        let bind = gpu.atlas_bind_group("particles-bind", &scene_buf);
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("particles-shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/terrain.wgsl").into()),
            });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("particles-layout"),
                bind_group_layouts: &[Some(&gpu.scene_bgl)],
                immediate_size: 0,
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("particles-pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[gpu_vertex_layout()],
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
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(depth_stencil(false, wgpu::CompareFunction::LessEqual)),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        let vertex_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particles-vb"),
            size: (MAX * 4 * std::mem::size_of::<GpuVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particles-ib"),
            size: (MAX * 6 * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue
            .write_buffer(&index_buffer, 0, bytemuck::cast_slice(&quad_indices()));
        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            scene_buf,
            bind,
            verts: RefCell::new(Vec::with_capacity(MAX * 4)),
        }
    }

    /// Builds the billboards for `frame.particles`, uploads them and
    /// records the draw; nothing is recorded when no particle is alive.
    pub(crate) fn record(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        queue: &wgpu::Queue,
        frame: &FrameParams<'_>,
    ) {
        let (right, up) = billboard_basis(frame.inv_view_proj);
        let mut verts = self.verts.borrow_mut();
        build_quads(&mut verts, frame.particles, right, up);
        if verts.is_empty() {
            return;
        }
        let scene = SceneUniforms::from_frame(frame);
        queue.write_buffer(&self.scene_buf, 0, bytemuck::bytes_of(&scene));
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&verts));
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind, &[]);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        rpass.draw_indexed(0..(verts.len() / 4 * 6) as u32, 0, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perspective_view;
    use ox_core::atlas::T_STONE;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    fn particle(pos: [f32; 3], size: f32) -> Particle {
        Particle {
            pos,
            vel: [0.0; 3],
            age: 0.0,
            life: 1.0,
            tile: T_STONE,
            sub_uv: [0.25, 0.5],
            size,
        }
    }

    #[test]
    fn billboard_basis_matches_an_unrotated_camera() {
        let (_, inv) = perspective_view([1.0, 2.0, 3.0], 0.0, 0.0, 16.0 / 9.0, 75.0, 300.0);
        let (right, up) = billboard_basis(inv);
        assert!(close(right, [1.0, 0.0, 0.0]), "{right:?}");
        assert!(close(up, [0.0, 1.0, 0.0]), "{up:?}");
    }

    #[test]
    fn yaw_turns_the_right_vector() {
        let yaw = std::f32::consts::FRAC_PI_2;
        let (_, inv) = perspective_view([0.0; 3], yaw, 0.0, 16.0 / 9.0, 75.0, 300.0);
        let (right, _) = billboard_basis(inv);
        assert!(close(right, [0.0, 0.0, -1.0]), "{right:?}");
    }

    fn quads(particles: &[Particle]) -> Vec<GpuVertex> {
        let mut verts = Vec::new();
        build_quads(&mut verts, particles, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        verts
    }

    #[test]
    fn build_quads_emits_four_vertices_per_particle() {
        let verts = quads(&[particle([1.0; 3], 0.1), particle([2.0; 3], 0.2)]);
        assert_eq!(verts.len(), 8);
    }

    #[test]
    fn quad_indices_wind_every_slot_the_same_way() {
        let idx = quad_indices();
        assert_eq!(idx.len(), MAX * 6);
        assert_eq!(idx[..6], [0, 1, 2, 0, 2, 3]);
        assert_eq!(idx.iter().max().copied(), Some(MAX as u32 * 4 - 1));
    }

    #[test]
    fn build_quads_reuses_the_buffer() {
        let mut verts = Vec::new();
        build_quads(&mut verts, &[particle([1.0; 3], 0.1)], [1.0; 3], [0.0; 3]);
        build_quads(&mut verts, &[particle([2.0; 3], 0.2)], [1.0; 3], [0.0; 3]);
        assert_eq!(verts.len(), 4);
    }

    #[test]
    fn quads_are_centered_on_the_particle() {
        let verts = quads(&[particle([4.0, 5.0, 6.0], 0.2)]);
        let mut sum = [0.0f32; 3];
        for v in &verts {
            for (s, p) in sum.iter_mut().zip(v.pos) {
                *s += p * 0.25;
            }
        }
        assert!(close(sum, [4.0, 5.0, 6.0]), "{sum:?}");
        assert!(close(verts[0].pos, [3.9, 4.9, 6.0]));
        assert!(close(verts[2].pos, [4.1, 5.1, 6.0]));
    }

    #[test]
    fn build_quads_caps_at_max() {
        let verts = quads(&vec![particle([0.0; 3], 0.1); MAX + 10]);
        assert_eq!(verts.len(), MAX * 4);
    }
}
