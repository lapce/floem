use floem_vger::*;
use futures::executor::block_on;
use std::fs::File;
use wgpu::StoreOp;

pub async fn setup() -> (wgpu::Device, wgpu::Queue) {
    let instance_desc = wgpu::InstanceDescriptor::default();

    let instance = wgpu::Instance::new(&instance_desc);

    let adapter = wgpu::util::initialize_adapter_from_env_or_default(&instance, None)
        .await
        .expect("No suitable GPU adapters found on the system!");

    let adapter_info = adapter.get_info();
    println!("Using {} ({:?})", adapter_info.name, adapter_info.backend);

    adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .expect("Unable to find a suitable GPU adapter!")
}

// See https://github.com/gfx-rs/wgpu/blob/master/wgpu/examples/capture/main.rs

pub async fn create_png(
    png_output_path: &str,
    device: &wgpu::Device,
    output_buffer: wgpu::Buffer,
    texture_descriptor: &wgpu::TextureDescriptor<'_>,
) {
    let texture_extent = texture_descriptor.size;

    // Note that we're not calling `.await` here.
    let buffer_slice = output_buffer.slice(..);

    // Sets the buffer up for mapping, sending over the result of the mapping back to us when it is finished.
    let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());

    // Poll the device in a blocking manner so that our future resolves.
    // In an actual application, `device.poll(...)` should
    // be called in an event loop or on another thread.
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    // If a file system is available, write the buffer as a PNG
    let has_file_system_available = cfg!(not(target_arch = "wasm32"));
    if !has_file_system_available {
        return;
    }

    if let Some(Ok(())) = receiver.receive().await {
        let buffer_view = buffer_slice.get_mapped_range();

        let mut png_encoder = png::Encoder::new(
            File::create(png_output_path).unwrap(),
            texture_extent.width,
            texture_extent.height,
        );
        png_encoder.set_depth(png::BitDepth::Eight);
        png_encoder.set_color(match texture_descriptor.format {
            wgpu::TextureFormat::Rgba8UnormSrgb => png::ColorType::Rgba,
            wgpu::TextureFormat::R8Unorm => png::ColorType::Grayscale,
            _ => panic!("unsupported pixel format"),
        });
        let mut png_writer = png_encoder.write_header().unwrap();

        png_writer.write_image_data(&buffer_view).unwrap();

        // With the current interface, we have to make sure all mapped views are
        // dropped before we unmap the buffer.
        drop(buffer_view);

        output_buffer.unmap();
    }
}

fn get_texture_data(
    descriptor: &wgpu::TextureDescriptor,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> wgpu::Buffer {
    let texture_extent = descriptor.size;

    let bytes_per_pixel = match descriptor.format {
        wgpu::TextureFormat::Rgba8UnormSrgb => 4,
        wgpu::TextureFormat::R8Unorm => 1,
        _ => panic!("unsupported pixel format"),
    };

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (texture_extent.width * texture_extent.height * bytes_per_pixel) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    assert!((texture_extent.width * bytes_per_pixel) % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT == 0);

    let command_buffer = {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Copy the data from the texture to the buffer
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(texture_extent.width * bytes_per_pixel),
                    rows_per_image: None,
                },
            },
            texture_extent,
        );

        encoder.finish()
    };

    queue.submit(Some(command_buffer));

    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();

    output_buffer
}

pub fn render_test(
    vger: &mut Vger,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    name: &str,
    capture: bool,
) {
    if capture {
        unsafe {
            device.start_graphics_debugger_capture();
        }
    }

    let texture_size = wgpu::Extent3d {
        width: 512,
        height: 512,
        depth_or_array_layers: 1,
    };

    let texture_desc = wgpu::TextureDescriptor {
        size: texture_size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        label: Some("render_texture"),
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    };

    let render_texture = device.create_texture(&texture_desc);

    let view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let desc = wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };

    vger.encode(&desc);

    let output_buffer = get_texture_data(&texture_desc, device, queue, &render_texture);

    if capture {
        unsafe {
            device.stop_graphics_debugger_capture();
        }
    }

    block_on(create_png(name, device, output_buffer, &texture_desc));
}

pub fn save_png(
    texture: &wgpu::Texture,
    texture_desc: &wgpu::TextureDescriptor,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    name: &str,
) {
    let output_buffer = get_texture_data(texture_desc, device, queue, texture);
    block_on(create_png(name, device, output_buffer, texture_desc));
}

pub fn png_not_black(path: &str) -> bool {
    let decoder = png::Decoder::new(File::open(path).unwrap());

    let mut reader = match decoder.read_info() {
        Ok(result) => result,
        Err(decoding_error) => {
            println!("error: {:?}", decoding_error);
            return false;
        }
    };

    // Allocate the output buffer.
    let mut buf = vec![0; reader.output_buffer_size()];
    // Read the next frame. An APNG might contain multiple frames.
    reader.next_frame(&mut buf).unwrap();
    // Grab the bytes of the image.
    let bytes = &buf[..reader.output_buffer_size()];

    for (i, b) in bytes.iter().enumerate() {
        // Skip alpha values.
        if (i % 4 != 3) && (*b != 0) {
            return true;
        }
    }

    false
}
