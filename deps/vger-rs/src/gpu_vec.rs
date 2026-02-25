use std::mem::size_of;
use std::ops::Index;
use wgpu::*;

pub struct GPUVec<T: Copy> {
    buffer: wgpu::Buffer,
    capacity: usize,
    data: Vec<T>,
    label: String,
}

impl<T: Copy> GPUVec<T> {
    pub fn new(device: &wgpu::Device, capacity: usize, label: &str) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (size_of::<T>() * capacity) as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            buffer,
            capacity,
            data: vec![],
            label: label.into(),
        }
    }

    pub fn new_uniforms(device: &wgpu::Device, label: &str) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: size_of::<T>() as _,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            buffer,
            capacity: 1,
            data: vec![],
            label: label.into(),
        }
    }

    /// Updates the underlying gpu buffer with self.data.
    ///
    /// We'd like to write directly to the mapped buffer, but that seemed
    /// tricky with wgpu.
    pub fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        let realloc = self.data.len() > self.capacity;
        if realloc {
            self.capacity = self.data.len().next_power_of_two();
            self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label.as_str()),
                size: (size_of::<T>() * self.capacity) as u64,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        let sz = self.data.len() * size_of::<T>();
        queue.write_buffer(&self.buffer, 0, unsafe {
            std::slice::from_raw_parts_mut(self.data[..].as_ptr() as *mut u8, sz)
        });
        realloc
    }

    pub fn bind_group_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: Some(std::num::NonZeroU64::new(size_of::<T>() as u64).unwrap()),
            },
            count: None,
        }
    }

    pub fn bind_group_entry(&'_ self, binding: u32) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &self.buffer,
                offset: 0,
                size: None,
            }),
        }
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn append(&mut self, values: &mut Vec<T>) {
        self.data.append(values);
    }

    pub fn push(&mut self, value: T) {
        self.data.push(value);
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }
}

impl<T: Copy> Index<usize> for GPUVec<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}
