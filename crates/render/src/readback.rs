//! GPU-to-CPU copy of a rendered texture into a PNG file, used by the
//! headless screenshot and test-server modes.

use crate::gpu::Gpu;

/// Copies `texture` (`width` x `height`, `format`) back to the CPU and
/// writes it to `path` as an RGBA PNG; BGRA formats are swizzled.
///
/// # Errors
///
/// Returns an error string when the GPU-to-CPU copy fails or the file
/// cannot be written.
///
/// # Panics
///
/// Panics if the map-completion channel's receiver disappeared early,
/// which cannot happen because `recv` below runs before returning.
pub(crate) fn save_png(
    gpu: &Gpu,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    path: &str,
) -> Result<(), String> {
    let bytes_per_row = (width * 4).div_ceil(256) * 256;
    let out = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture-out"),
        size: u64::from(bytes_per_row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &out,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    out.map_async(wgpu::MapMode::Read, .., move |r| {
        #[expect(
            clippy::expect_used,
            reason = "the receiver is still alive: recv() below runs before this fn returns"
        )]
        sender.send(r).expect("map channel");
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| e.to_string())?;
    receiver
        .recv()
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let data = out.get_mapped_range(..);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * bytes_per_row) as usize;
        rgba.extend_from_slice(&data[start..start + (width * 4) as usize]);
    }
    drop(data);
    out.unmap();
    if matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for px in rgba.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
    }
    image::save_buffer(path, &rgba, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())
}
