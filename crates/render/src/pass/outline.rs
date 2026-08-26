//! Outline pass: line-list box around the targeted block, drawn with
//! depth testing so hidden edges stay hidden.

use crate::FrameParams;
use crate::gpu::Gpu;
use crate::pass::{Geo, depth_stencil};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct OutlineUniforms {
    vp: [[f32; 4]; 4],
    offset: [f32; 4],
}

const OUTLINE_EDGES: [[f32; 3]; 24] = [
    [0.0, 0.0, 0.0],
    [1.0, 0.0, 0.0],
    [1.0, 0.0, 0.0],
    [1.0, 0.0, 1.0],
    [1.0, 0.0, 1.0],
    [0.0, 0.0, 1.0],
    [0.0, 0.0, 1.0],
    [0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [1.0, 1.0, 0.0],
    [1.0, 1.0, 0.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [0.0, 1.0, 1.0],
    [0.0, 1.0, 1.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [1.0, 0.0, 0.0],
    [1.0, 1.0, 0.0],
    [1.0, 0.0, 1.0],
    [1.0, 1.0, 1.0],
    [0.0, 0.0, 1.0],
    [0.0, 1.0, 1.0],
];

/// Outline pipeline with its uniform buffer, bind group and edge batch.
pub(crate) struct OutlinePass {
    buf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    edges: Geo,
}

impl OutlinePass {
    /// Creates the outline resources for `format` from the shared [`Gpu`]
    /// core.
    pub(crate) fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let buf = gpu.uniform_buffer(
            "outline-uniforms",
            std::mem::size_of::<OutlineUniforms>() as u64,
        );
        let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("outline-bind"),
            layout: &gpu.uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("outline-shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/outline.wgsl").into()),
            });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("outline-layout"),
                bind_group_layouts: &[Some(&gpu.uniform_bgl)],
                immediate_size: 0,
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("outline-pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 12,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        }],
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
                    topology: wgpu::PrimitiveTopology::LineList,
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil(true, wgpu::CompareFunction::LessEqual)),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        let indices: Vec<u16> = (0..24).collect();
        let edges = Geo::raw(&gpu.device, &OUTLINE_EDGES, &indices);
        Self {
            buf,
            bind,
            pipeline,
            edges,
        }
    }

    /// Uploads the outline uniforms and records the box edges when a block
    /// is highlighted.
    pub(crate) fn record(
        &self,
        rpass: &mut wgpu::RenderPass<'_>,
        queue: &wgpu::Queue,
        frame: &FrameParams<'_>,
    ) {
        if let Some([bx, by, bz]) = frame.highlight {
            let outline = OutlineUniforms {
                vp: frame.view_proj,
                offset: [bx as f32, by as f32, bz as f32, 0.0],
            };
            queue.write_buffer(&self.buf, 0, bytemuck::bytes_of(&outline));
            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &self.bind, &[]);
            self.edges.draw(rpass);
        }
    }
}
