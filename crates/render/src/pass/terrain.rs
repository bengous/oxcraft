//! Terrain pass: opaque chunk geometry, then translucent water on top;
//! both pipelines share the scene uniform bind group and `terrain.wgsl`.

use crate::FrameParams;
use crate::chunks::ChunkStore;
use crate::gpu::Gpu;
use crate::pass::{SceneUniforms, depth_stencil, gpu_vertex_layout};

/// Opaque and water pipelines with the shared scene uniform resources.
pub(crate) struct TerrainPass {
    scene_buf: wgpu::Buffer,
    scene_bind: wgpu::BindGroup,
    terrain_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
}

impl TerrainPass {
    /// Creates both pipelines and the scene uniform resources for `format`.
    pub(crate) fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let scene_buf = gpu.uniform_buffer(
            "scene-uniforms",
            std::mem::size_of::<SceneUniforms>() as u64,
        );
        let scene_bind = gpu.atlas_bind_group("scene-bind", &scene_buf);
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("terrain-shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/terrain.wgsl").into()),
            });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("terrain-layout"),
                bind_group_layouts: &[Some(&gpu.scene_bgl)],
                immediate_size: 0,
            });
        let terrain_pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("terrain-pipeline"),
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
                depth_stencil: Some(depth_stencil(true, wgpu::CompareFunction::LessEqual)),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        let water_pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("water-pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[gpu_vertex_layout()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_water"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(depth_stencil(true, wgpu::CompareFunction::LessEqual)),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        Self {
            scene_buf,
            scene_bind,
            terrain_pipeline,
            water_pipeline,
        }
    }

    /// Uploads the scene uniforms and records the opaque then water draws
    /// for every resident chunk.
    pub(crate) fn record(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        queue: &wgpu::Queue,
        frame: &FrameParams<'_>,
        chunks: &ChunkStore,
    ) {
        let scene = SceneUniforms::from_frame(frame);
        queue.write_buffer(&self.scene_buf, 0, bytemuck::bytes_of(&scene));

        rpass.set_pipeline(&self.terrain_pipeline);
        rpass.set_bind_group(0, &self.scene_bind, &[]);
        for chunk in chunks.iter() {
            chunk.opaque().draw(rpass);
        }

        rpass.set_pipeline(&self.water_pipeline);
        rpass.set_bind_group(0, &self.scene_bind, &[]);
        for chunk in chunks.iter() {
            if let Some(geo) = chunk.water() {
                geo.draw(rpass);
            }
        }
    }
}
