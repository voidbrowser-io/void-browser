//! Offscreen capture: render the current scene into a GPU texture and save
//! it as PNG. Used by `--auto` scripts (tests, CI, screenshots).

use std::path::Path;
use vello::peniko::Color;
use vello::util::DeviceHandle;
use vello::wgpu;
use vello::{AaConfig, RenderParams, Renderer, Scene};

pub fn capture(dev: &DeviceHandle, renderer: &mut Renderer, scene: &Scene, w: u32, h: u32, path: &Path) -> Result<(), String> {
    let (device, queue) = (&dev.device, &dev.queue);
    let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("void-capture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    renderer
        .render_to_texture(
            device,
            queue,
            scene,
            &view,
            &RenderParams { base_color: Color::WHITE, width: w, height: h, antialiasing_method: AaConfig::Area },
        )
        .map_err(|e| e.to_string())?;

    let padded = (w * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("void-capture-readback"),
        size: (padded * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: None },
        },
        size,
    );
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;

    let data = buffer.slice(..).get_mapped_range().map_err(|e| format!("{e:?}"))?;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h as usize {
        rgba.extend_from_slice(&data[row * padded as usize..][..(w * 4) as usize]);
    }
    drop(data);

    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().and_then(|mut wr| wr.write_image_data(&rgba)).map_err(|e| e.to_string())
}
