//! One module per render pass; each owns its pipeline, bind groups and
//! buffers and is recorded by `Renderer::render_frame` in fixed order
//! (docs/adr/0005-renderer-pass-modules.md).

pub(crate) mod crack;
pub(crate) mod hand;
pub(crate) mod outline;
pub(crate) mod particles;
pub(crate) mod sky;
pub(crate) mod terrain;
pub mod ui;

pub(crate) use crack::CrackPass;
pub(crate) use hand::HandPass;
pub(crate) use outline::OutlinePass;
pub(crate) use particles::ParticlesPass;
pub(crate) use sky::SkyPass;
pub(crate) use terrain::TerrainPass;
pub(crate) use ui::UiPass;

use wgpu::util::DeviceExt;

use crate::FrameParams;

/// GPU-side copy of one mesher vertex: position, padded uv, light.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuVertex {
    pos: [f32; 3],
    _pad: f32,
    uv: [f32; 2],
    light: f32,
}

impl From<&ox_core::mesher::Vertex> for GpuVertex {
    fn from(v: &ox_core::mesher::Vertex) -> Self {
        Self {
            pos: v.pos,
            _pad: 0.0,
            uv: v.uv,
            light: v.light,
        }
    }
}

/// Vertex layout of [`GpuVertex`]: position, uv, light at locations 0..3.
pub(crate) const fn gpu_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<GpuVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 16,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 24,
                shader_location: 2,
            },
        ],
    }
}

/// Camera, sun and fog uniforms consumed by `terrain.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SceneUniforms {
    view_proj: [[f32; 4]; 4],
    sun_color: [f32; 4],
    fog_color: [f32; 4],
    fog_range: [f32; 4],
    cam_pos: [f32; 4],
}

impl SceneUniforms {
    /// The uniforms for one frame.
    pub(crate) const fn from_frame(frame: &FrameParams<'_>) -> Self {
        Self {
            view_proj: frame.view_proj,
            sun_color: pad(frame.sun_color, 1.0),
            fog_color: pad(frame.horizon, 1.0),
            fog_range: [frame.fog_near, frame.fog_far, 0.0, 0.0],
            cam_pos: pad(frame.cam_pos, 1.0),
        }
    }
}

/// Pads a color/direction triple to a vec4 uniform slot.
pub(crate) const fn pad(v: [f32; 3], w: f32) -> [f32; 4] {
    [v[0], v[1], v[2], w]
}

/// One uploaded geometry batch drawn as a single indexed call.
pub(crate) struct Geo {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    index_format: wgpu::IndexFormat,
}

impl Geo {
    /// Uploads one chunk mesh batch (u32-indexed).
    pub(crate) fn mesh(
        device: &wgpu::Device,
        verts: &[ox_core::mesher::Vertex],
        indices: &[u32],
    ) -> Self {
        let gpu_vertices: Vec<GpuVertex> = verts.iter().map(GpuVertex::from).collect();
        Self::create(
            device,
            bytemuck::cast_slice(&gpu_vertices),
            bytemuck::cast_slice(indices),
            indices.len() as u32,
            wgpu::IndexFormat::Uint32,
        )
    }

    /// Builds a fixed batch from plain vertices and u16 indices.
    pub(crate) fn raw<T: bytemuck::Pod>(
        device: &wgpu::Device,
        vertices: &[T],
        indices: &[u16],
    ) -> Self {
        Self::create(
            device,
            bytemuck::cast_slice(vertices),
            bytemuck::cast_slice(indices),
            indices.len() as u32,
            wgpu::IndexFormat::Uint16,
        )
    }

    fn create(
        device: &wgpu::Device,
        vertices: &[u8],
        indices: &[u8],
        index_count: u32,
        index_format: wgpu::IndexFormat,
    ) -> Self {
        Self {
            vertex_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: vertices,
                usage: wgpu::BufferUsages::VERTEX,
            }),
            index_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: indices,
                usage: wgpu::BufferUsages::INDEX,
            }),
            index_count,
            index_format,
        }
    }

    /// Records the indexed draw for this batch.
    pub(crate) fn draw(&self, rpass: &mut wgpu::RenderPass<'_>) {
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rpass.set_index_buffer(self.index_buffer.slice(..), self.index_format);
        rpass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}

/// Shared depth attachment state for the pass pipelines.
pub(crate) fn depth_stencil(
    write: bool,
    compare: wgpu::CompareFunction,
) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth24Plus,
        depth_write_enabled: Some(write),
        depth_compare: Some(compare),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}
