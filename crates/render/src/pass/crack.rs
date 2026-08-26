//! Crack pass: break-progress overlay on the block under attack, an
//! inflated unit cube textured from the [`ox_core::cracks`] strip.

use ox_core::atlas::TILE_PX;
use ox_core::cracks::{STAGES, STRIP_PX, generate};
use ox_core::mesher::{FACE_UV, FACES};

use crate::FrameParams;
use crate::gpu::Gpu;
use crate::pass::{Geo, depth_stencil};

/// One crack vertex: unit-cube corner and stage-local UV.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct CrackVertex {
    /// Unit-cube corner.
    pub(crate) pos: [f32; 3],
    /// Texture coordinate inside one stage tile, inset from its edges.
    pub(crate) uv: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CrackUniforms {
    vp: [[f32; 4]; 4],
    offset: [f32; 4],
    stage_uv: [f32; 4],
}

/// Block being broken and how far along it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrackParams {
    /// World cell under attack.
    pub cell: [i32; 3],
    /// Fraction of the break time elapsed; outside `0.0..1.0` it clamps to
    /// the first or last stage.
    pub progress: f32,
}

const PAD: f32 = 0.03125;

/// Crack stage shown at `progress` of a break, in `0..STAGES`.
pub(crate) fn stage_of(progress: f32) -> u32 {
    ((progress * STAGES as f32) as u32).min(STAGES - 1)
}

/// The six faces of the unit cube with UVs inset by half a texel.
pub(crate) fn cube() -> (Vec<CrackVertex>, Vec<u16>) {
    let mut verts = Vec::with_capacity(24);
    let mut idx = Vec::with_capacity(36);
    for face in &FACES {
        let base = verts.len() as u16;
        for (corner, uv) in face.corners.iter().zip(FACE_UV) {
            verts.push(CrackVertex {
                pos: *corner,
                uv: [
                    PAD + uv[0] * (1.0 - 2.0 * PAD),
                    PAD + uv[1] * (1.0 - 2.0 * PAD),
                ],
            });
        }
        idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (verts, idx)
}

/// Crack pipeline with its strip texture, uniform buffer and cube batch.
pub(crate) struct CrackPass {
    buf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    cube: Geo,
}

impl CrackPass {
    /// Paints and uploads the crack strip and creates the overlay pipeline
    /// for `format`.
    pub(crate) fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let strip = generate();
        let size = wgpu::Extent3d {
            width: STRIP_PX,
            height: TILE_PX,
            depth_or_array_layers: 1,
        };
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cracks"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &strip.alpha,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(STRIP_PX),
                rows_per_image: Some(TILE_PX),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let buf = gpu.uniform_buffer(
            "crack-uniforms",
            std::mem::size_of::<CrackUniforms>() as u64,
        );
        let bind = gpu.texture_bind_group("crack-bind", &buf, &view);
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("crack-shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/crack.wgsl").into()),
            });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("crack-layout"),
                bind_group_layouts: &[Some(&gpu.scene_bgl)],
                immediate_size: 0,
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("crack-pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<CrackVertex>() as u64,
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
                        ],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(depth_stencil(false, wgpu::CompareFunction::LessEqual)),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        let (verts, idx) = cube();
        let cube = Geo::raw(&gpu.device, &verts, &idx);
        Self {
            buf,
            bind,
            pipeline,
            cube,
        }
    }

    /// Uploads the crack uniforms and records the overlay when a block is
    /// under attack.
    pub(crate) fn record(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        queue: &wgpu::Queue,
        frame: &FrameParams<'_>,
    ) {
        let Some(crack) = frame.crack else { return };
        let stage = stage_of(crack.progress) as f32;
        let uniforms = CrackUniforms {
            vp: frame.view_proj,
            offset: [
                crack.cell[0] as f32,
                crack.cell[1] as f32,
                crack.cell[2] as f32,
                0.0,
            ],
            stage_uv: [stage / STAGES as f32, 1.0 / STAGES as f32, 0.0, 0.0],
        };
        queue.write_buffer(&self.buf, 0, bytemuck::bytes_of(&uniforms));
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind, &[]);
        self.cube.draw(rpass);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_rises_with_progress_and_clamps_at_both_ends() {
        assert_eq!(stage_of(-1.0), 0);
        assert_eq!(stage_of(0.0), 0);
        assert_eq!(stage_of(0.5), STAGES / 2);
        assert_eq!(stage_of(1.0), STAGES - 1);
        assert_eq!(stage_of(9.0), STAGES - 1);
        assert!(stage_of(0.2) < stage_of(0.4));
        assert!(stage_of(0.4) < stage_of(0.8));
    }

    #[test]
    fn cube_has_24_inset_vertices() {
        let (verts, idx) = cube();
        assert_eq!(verts.len(), 24);
        assert_eq!(idx.len(), 36);
        assert!(verts.iter().all(|v| {
            v.uv.iter().all(|c| *c > 0.0 && *c < 1.0)
                && v.pos
                    .iter()
                    .all(|p| [0.0f32.to_bits(), 1.0f32.to_bits()].contains(&p.to_bits()))
        }));
    }
}
