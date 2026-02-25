use rect_packer::{Packer, Rect};
use wgpu::util::DeviceExt;

#[derive(Debug)]
struct ImageData {
    rect: Rect,
    data: Vec<u8>,
}

pub enum AtlasContent {
    Mask,
    Color,
}

pub struct Atlas {
    pub(crate) max_seen: u32,
    width: u32,
    height: u32,
    packer: Packer,
    new_data: Vec<ImageData>,
    pub atlas_texture: wgpu::Texture,
    area_used: i32,
    did_clear: bool,
    content: AtlasContent,
}

impl Atlas {
    pub const RECT_PADDING: i32 = 6;

    fn get_packer_config(width: u32, height: u32) -> rect_packer::Config {
        rect_packer::Config {
            width: width as i32,
            height: height as i32,

            border_padding: Atlas::RECT_PADDING,
            rectangle_padding: Atlas::RECT_PADDING,
        }
    }

    pub fn get_texture_desc(width: u32, height: u32) -> wgpu::TextureDescriptor<'static> {
        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING,
            label: Some("atlas_texture"),
            view_formats: &[wgpu::TextureFormat::R8Unorm],
        }
    }

    pub fn new(device: &wgpu::Device, content: AtlasContent, width: u32, height: u32) -> Self {
        let atlas_texture = Self::get_atlas_texture(device, &content, width, height);

        Self {
            max_seen: 0,
            width,
            height,
            packer: Packer::new(Atlas::get_packer_config(width, height)),
            new_data: vec![],
            atlas_texture,
            area_used: 0,
            did_clear: false,
            content,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.atlas_texture = Self::get_atlas_texture(device, &self.content, width, height);
        self.clear();
    }

    fn get_atlas_texture(
        device: &wgpu::Device,
        content: &AtlasContent,
        width: u32,
        height: u32,
    ) -> wgpu::Texture {
        let texture_size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let format = match content {
            AtlasContent::Mask => wgpu::TextureFormat::R8Unorm,
            AtlasContent::Color => wgpu::TextureFormat::Rgba8Unorm,
        };
        let desc = wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING,
            label: Some("atlas_texture"),
            view_formats: &[format],
        };
        device.create_texture(&desc)
    }

    pub fn add_region(&mut self, data: &[u8], width: u32, height: u32) -> Option<Rect> {
        let max_seen = width.max(height);
        if max_seen > self.max_seen {
            self.max_seen = max_seen;
        }
        if let Some(rect) = self.packer.pack(width as i32, height as i32, false) {
            self.new_data.push(ImageData {
                rect,
                data: data.into(),
            });
            self.area_used +=
                (rect.width + Atlas::RECT_PADDING) * (rect.height + Atlas::RECT_PADDING);

            Some(rect)
        } else {
            None
        }
    }

    pub fn update(&mut self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        if self.did_clear {
            // encoder.clear_texture(&self.atlas_texture, &wgpu::ImageSubresourceRange::default());

            let image_size = wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            };

            let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as i32;
            let width = (self.width * 4) as i32;
            let padding = (align - width % align) % align;
            let padded_width = width + padding;
            let padded_data = vec![0_u8; (padded_width as u32 * self.height) as usize];

            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("atlas temp buffer"),
                contents: &padded_data,
                usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::MAP_WRITE,
            });

            encoder.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_width as u32),
                        rows_per_image: None,
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas_texture,
                    mip_level: 0,
                    aspect: wgpu::TextureAspect::All,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
                },
                image_size,
            );

            self.did_clear = false;
        }

        for data in &self.new_data {
            // Pad data to wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
            let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as i32;
            let pixels = match self.content {
                AtlasContent::Mask => 1,
                AtlasContent::Color => 4,
            };
            let width = data.rect.width * pixels;
            let padding = (align - width % align) % align;
            let padded_width = width + padding;
            let mut padded_data = Vec::with_capacity((padded_width * data.rect.height) as usize);

            let mut i = 0;
            for _ in 0..data.rect.height {
                for _ in 0..width {
                    padded_data.push(data.data[i]);
                    i += 1;
                }
                while (padded_data.len() % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize) != 0 {
                    padded_data.push(0);
                }
            }

            assert!(padded_data.len() == (padded_width * data.rect.height) as usize);

            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("atlas temp buffer"),
                contents: &padded_data,
                usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::MAP_WRITE,
            });

            let image_size = wgpu::Extent3d {
                width: data.rect.width as u32,
                height: data.rect.height as u32,
                depth_or_array_layers: 1,
            };

            encoder.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_width as u32),
                        rows_per_image: None,
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas_texture,
                    mip_level: 0,
                    aspect: wgpu::TextureAspect::All,
                    origin: wgpu::Origin3d {
                        x: data.rect.x as u32,
                        y: data.rect.y as u32,
                        z: 0,
                    },
                },
                image_size,
            );
        }

        self.new_data.clear();
    }

    pub fn create_view(&self) -> wgpu::TextureView {
        self.atlas_texture
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    pub fn usage(&self) -> f32 {
        (self.area_used as f32) / ((self.width * self.height) as f32)
    }

    pub fn clear(&mut self) {
        self.packer = Packer::new(Atlas::get_packer_config(self.width, self.height));
        self.area_used = 0;
        self.new_data.clear();
        self.did_clear = true;
    }
}
