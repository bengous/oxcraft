//! Sky pass: one fullscreen triangle whose fragment shader rebuilds the
//! view ray through the inverse view-projection.

use crate::FrameParams;
use crate::gpu::Gpu;
use crate::pass::{depth_stencil, pad};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SkyUniforms {
    inv_vp: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    zenith: [f32; 4],
    horizon: [f32; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    viewport: [f32; 4],
}

/// Sky pipeline with its uniform buffer and bind group.
pub(crate) struct SkyPass {
    buf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

impl SkyPass {
    /// Creates the sky resources for `format` from the shared [`Gpu`] core.
    pub(crate) fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let buf = gpu.uniform_buffer("sky-uniforms", std::mem::size_of::<SkyUniforms>() as u64);
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky-bind"),
            layout: &gpu.uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sky-shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/sky.wgsl").into()),
            });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sky-layout"),
                bind_group_layouts: &[Some(&gpu.uniform_bgl)],
                immediate_size: 0,
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sky-pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
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
                depth_stencil: Some(depth_stencil(false, wgpu::CompareFunction::Always)),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        Self {
            buf,
            bind,
            pipeline,
        }
    }

    /// Uploads the sky uniforms and records the fullscreen draw.
    pub(crate) fn record(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        queue: &wgpu::Queue,
        frame: &FrameParams<'_>,
        width: f32,
        height: f32,
    ) {
        let sky = SkyUniforms {
            inv_vp: frame.inv_view_proj,
            cam_pos: pad(frame.cam_pos, 1.0),
            zenith: pad(frame.zenith, 1.0),
            horizon: pad(frame.horizon, 1.0),
            sun_dir: pad(frame.sun_dir, 0.0),
            sun_color: pad(frame.sun_color, 1.0),
            viewport: [width, height, 0.0, 0.0],
        };
        queue.write_buffer(&self.buf, 0, bytemuck::bytes_of(&sky));
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind, &[]);
        rpass.draw(0..3, 0..1);
    }
}
