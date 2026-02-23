use bytemuck::Pod;
use std::marker::PhantomData;

pub struct IBO<T: Pod> {
    pub buffer: wgpu::Buffer,
    pub count: usize,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Pod> IBO<T> {
    pub fn new(device: &wgpu::Device, count: usize) -> Self {
        let size = (std::mem::size_of::<T>() * count) as u64; 

        let buffer_descriptor = wgpu::BufferDescriptor {
            label: Some("IBO"),
            size: size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        };
        let buffer = device.create_buffer(&buffer_descriptor);

        Self {
            buffer,
            count,
            _marker: PhantomData,
        }
    }

    pub fn upload(&self, offset_index: usize, data: &[T], queue: &wgpu::Queue,) {
        let offset_bytes = offset_index as u64 * std::mem::size_of::<T>() as u64;
        queue.write_buffer(&self.buffer, offset_bytes, bytemuck::cast_slice(data));
    }
}
