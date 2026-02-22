use crate::renderer::backend::bind_group;
use crate::renderer::backend::mesh_builder::any_as_u8_slice;

pub struct UBOGroup {
    pub buffer: wgpu::Buffer,
    pub bind_groups: Vec<wgpu::BindGroup>,
    alignment: u64,
}

impl UBOGroup {
    pub fn new(device: &wgpu::Device, object_count: usize, layout: wgpu::BindGroupLayout) -> Self {
        let alignment = glm::max(
            device.limits().min_uniform_buffer_offset_alignment as u32,
            std::mem::size_of::<glm::Mat4>() as u32) as u64;
            
        let buffer_descriptor = wgpu::BufferDescriptor {
            label: Some("UBO"),
            size: object_count as u64 * alignment,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        };
        let buffer = device.create_buffer(&buffer_descriptor);

        let mut bind_groups: Vec<wgpu::BindGroup> = Vec::new();
        for i in 0..object_count {
            let mut builder = bind_group::Builder::new(device);
            builder.set_layout(&layout);
            builder.add_buffer(&buffer, i as u64 * alignment);
            bind_groups.push(builder.build("Matrix"));
        };

        Self { buffer, bind_groups, alignment }
    }

    pub fn upload(&mut self, i: u64, matrix: &glm::Mat4, queue: &wgpu::Queue) {
        let offset = i * self.alignment;
        let data = unsafe { any_as_u8_slice(matrix) };
        queue.write_buffer(&self.buffer, offset, data);
    }
}

pub struct UBO {
    pub buffer: wgpu::Buffer,
    pub bind_group:wgpu::BindGroup,
}

impl UBO {
    pub fn new(device: &wgpu::Device, layout: wgpu::BindGroupLayout) -> Self {
            
        let buffer_descriptor = wgpu::BufferDescriptor {
            label: Some("UBO"),
            size: std::mem::size_of::<glm::Mat4>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        };
        let buffer = device.create_buffer(&buffer_descriptor);

        let bind_group: wgpu::BindGroup;
        {
            let mut builder = bind_group::Builder::new(device);
            builder.set_layout(&layout);
            builder.add_buffer(&buffer, 0);
            bind_group = builder.build("Matrix");
        }

        Self { buffer, bind_group }
    }

    pub fn upload(&mut self, matrix: &glm::Mat4, queue: &wgpu::Queue) {
        let data = unsafe { any_as_u8_slice(matrix) };
        queue.write_buffer(&self.buffer, 0, data);
    }
}
