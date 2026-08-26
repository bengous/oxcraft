//! Shared GPU core owned by `Renderer` and lent to each pass at build
//! time: device, queue, texture atlas and bind group layouts.

use ox_core::atlas;

/// The single uniform-buffer entry shared by both layouts.
fn uniform_buffer_entry() -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding: 0,
        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Device, queue, uploaded atlas and the two bind group layouts reused by
/// all pipelines.
pub(crate) struct Gpu {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) atlas_view: wgpu::TextureView,
    pub(crate) atlas_sampler: wgpu::Sampler,
    pub(crate) scene_bgl: wgpu::BindGroupLayout,
    pub(crate) uniform_bgl: wgpu::BindGroupLayout,
}

impl Gpu {
    /// Creates a `COPY_DST` uniform buffer of `size` bytes.
    pub(crate) fn uniform_buffer(&self, label: &str, size: u64) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Composes a `scene_bgl` bind group binding `uniform_buf` plus the
    /// atlas sampler and view.
    pub(crate) fn atlas_bind_group(
        &self,
        label: &str,
        uniform_buf: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        self.texture_bind_group(label, uniform_buf, &self.atlas_view)
    }

    /// Composes a `scene_bgl` bind group binding `uniform_buf`, the atlas
    /// sampler and any filterable 2D `view`.
    pub(crate) fn texture_bind_group(
        &self,
        label: &str,
        uniform_buf: &wgpu::Buffer,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.scene_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.atlas_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(view),
                },
            ],
        })
    }

    /// Generates the atlas, uploads it to `device`/`queue` and creates the
    /// shared layouts.
    pub(crate) fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let atlas_data = atlas::generate();
        let atlas_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d {
                width: atlas::ATLAS_PX,
                height: atlas::ATLAS_PX,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas_data.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas::ATLAS_PX * 4),
                rows_per_image: Some(atlas::ATLAS_PX),
            },
            wgpu::Extent3d {
                width: atlas::ATLAS_PX,
                height: atlas::ATLAS_PX,
                depth_or_array_layers: 1,
            },
        );
        let atlas_view = atlas_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let scene_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene-bgl"),
            entries: &[
                uniform_buffer_entry(),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniform-bgl"),
            entries: &[uniform_buffer_entry()],
        });
        Self {
            device,
            queue,
            atlas_view,
            atlas_sampler,
            scene_bgl,
            uniform_bgl,
        }
    }
}
