use glfw::Window;
use nalgebra_glm::ortho;

use crate::model::quad_tree::Body;
use crate::renderer::backend::{
    bind_group_layout,
    definitions::{ParticleInstance, Vertex},
    ibo::IBO,
    material::Material,
    mesh_builder, pipeline,
    ubo::{UBO, UBOGroup},
};

// Quad width as a multiple of the body radius; the glow reaches GLOW_SCALE / 2 radii.
// shader.wgsl's BODY_RADIUS is 2 / GLOW_SCALE, keep them in sync.
const GLOW_SCALE : f32 = 20.0;
const VIEW_SCALE : f32 = 3.0;

pub struct State<'a> {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: (i32, i32),
    pub window: &'a mut Window,
    glow_pipeline: wgpu::RenderPipeline,
    core_pipeline: wgpu::RenderPipeline,
    quad_mesh: mesh_builder::Mesh,
    quad_material: Material,
    ibo: Option<IBO<ParticleInstance>>,
    ubo: Option<UBOGroup>,
    projection_ubo: UBO,
}

impl<'a> State<'a> {
    pub async fn new(window: &'a mut Window) -> Self {
        let size = window.get_framebuffer_size();

        let instance_descriptor = wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        };
        let instance = wgpu::Instance::new(&instance_descriptor);
        let surface = instance.create_surface(window.render_context()).unwrap();

        let adapter_descriptor = wgpu::RequestAdapterOptionsBase {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        };
        let adapter = instance.request_adapter(&adapter_descriptor).await.unwrap();

        let device_descriptor = wgpu::DeviceDescriptor {
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            label: Some("Device"),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        };
        let (device, queue) = adapter.request_device(&device_descriptor).await.unwrap();

        let surface_capabilities = surface.get_capabilities(&adapter);
        let surface_format = surface_capabilities
            .formats
            .iter()
            .copied()
            .filter(|f| f.is_srgb())
            .next()
            .unwrap_or(surface_capabilities.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.0 as u32,
            height: size.1 as u32,
            present_mode: surface_capabilities.present_modes[0],
            alpha_mode: surface_capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let quad_mesh = mesh_builder::make_quad(&device);

        let material_bind_group_layout;
        {
            let mut builder = bind_group_layout::Builder::new(&device);
            builder.add_material();
            material_bind_group_layout = builder.build("Material Bind Group Layout");
        }

        let ubo_bind_group_layout;
        {
            let mut builder = bind_group_layout::Builder::new(&device);
            builder.add_ubo();
            ubo_bind_group_layout = builder.build("Material Bind Group Layout");
        }

        // Two pipelines over the same shader and instances: the glows are added up first,
        // then the dark cores are blended over them so they block the light behind them.
        let glow_pipeline: wgpu::RenderPipeline;
        let core_pipeline: wgpu::RenderPipeline;
        {
            let mut builder = pipeline::Builder::new(&device);
            builder.set_shader_module("shader.wgsl", "vs_glow", "fs_glow");
            builder.set_pixel_format(config.format);
            builder.add_vertex_buffer_layout(Vertex::get_layout());
            builder.add_vertex_buffer_layout(ParticleInstance::get_layout());
            builder.add_bind_group_layout(&material_bind_group_layout);
            builder.add_bind_group_layout(&ubo_bind_group_layout);
            glow_pipeline = builder.build("Glow Pipeline");

            builder.set_shader_module("shader.wgsl", "vs_core", "fs_core");
            builder.add_vertex_buffer_layout(Vertex::get_layout());
            builder.add_vertex_buffer_layout(ParticleInstance::get_layout());
            builder.add_bind_group_layout(&material_bind_group_layout);
            builder.add_bind_group_layout(&ubo_bind_group_layout);
            // premultiplied alpha: the core covers what is behind it by its alpha
            builder.set_blend(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
            core_pipeline = builder.build("Core Pipeline");
        }

        let quad_material = Material::new(
            "Gigi_small.png",
            &device,
            &queue,
            "Quad Material",
            &material_bind_group_layout,
        );

        let projection_ubo = UBO::new(&device, ubo_bind_group_layout);

        Self {
            instance,
            window,
            surface,
            device,
            queue,
            config,
            size,
            glow_pipeline,
            core_pipeline,
            quad_mesh,
            quad_material: quad_material,
            ibo: None,
            ubo: None,
            projection_ubo: projection_ubo,
        }
    }

    pub fn build_ubos_for_objects(&mut self, object_count: usize) {
        let ubo_bind_group_layout;
        {
            let mut builder = bind_group_layout::Builder::new(&self.device);
            builder.add_ubo();
            ubo_bind_group_layout = builder.build("Material Bind Group Layout");
        }

        self.ubo = Some(UBOGroup::new(
            &self.device,
            object_count,
            ubo_bind_group_layout,
        ));
    }

    pub fn build_ibo_for_object(&mut self, object_count: usize) {
        self.ibo = Some(IBO::new(&self.device, object_count));
    }

    pub fn resize(&mut self, new_size: (i32, i32)) {
        if new_size.0 > 0 && new_size.1 > 0 {
            self.size = new_size;
            self.config.width = new_size.0 as u32;
            self.config.height = new_size.1 as u32;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn update_surface(&mut self) {
        self.surface = self
            .instance
            .create_surface(self.window.render_context())
            .unwrap();
    }

    fn update_projection(&mut self) {

        let screen_width = self.config.width as f32;
        let screen_height = self.config.height as f32;

        let aspect = screen_width / screen_height; 
        let ortho = if aspect >= 1.0 {
            ortho(-VIEW_SCALE * aspect, 
                  VIEW_SCALE * aspect, 
                  -1.0 * VIEW_SCALE, 
                  VIEW_SCALE, 
                  -1.0, 
                  1.0)
        } else {
            ortho(-1.0 * VIEW_SCALE, 
                VIEW_SCALE, 
                -1.0 * VIEW_SCALE / aspect, 
                VIEW_SCALE / aspect, 
                -1.0, 
                1.0)
        };
        // let ortho = ortho(-1.0, 1.0, -1.0, 1.0, -1.0, 1.0);

        self.projection_ubo.upload(&ortho, &self.queue);
    }

    fn update_transformations(&mut self, quads: &[Body], alpha: f32) {
        let mut particles: Vec<ParticleInstance> = Vec::new();
        for i in 0..quads.len() {
            let q = quads[i];
            let new_pos = q.prev_pos + (q.pos - q.prev_pos) * alpha;
            particles.push(ParticleInstance {
                position: [new_pos.x, new_pos.y],
                size: quads[i].radius * GLOW_SCALE * VIEW_SCALE * 0.5,
                color: [1.0, 0.9, 0.8],
            });
        }

        let ibo = self.ibo.as_mut().unwrap();
        ibo.reserve(&self.device, particles.len());
        ibo.upload(0, &particles, &self.queue);
    }

    pub fn render(&mut self, items: &[Body], alpha: f32) -> Result<(), wgpu::SurfaceError> {
        self.device.poll(wgpu::MaintainBase::Wait).ok();

        self.update_projection();
        // let quads = items.iter().map(|(item, _)| *item).collect();
        self.update_transformations(items, alpha);

        let event = self.queue.submit([]);
        let maintain = wgpu::MaintainBase::WaitForSubmissionIndex(event);
        self.device.poll(maintain).ok();

        let drawable = self.surface.get_current_texture()?;
        let image_view_descriptor = wgpu::TextureViewDescriptor::default();
        let image_view = drawable.texture.create_view(&image_view_descriptor);

        let command_encoder_descriptor = wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        };
        let mut command_encoder = self
            .device
            .create_command_encoder(&command_encoder_descriptor);

        let color_attachment = wgpu::RenderPassColorAttachment {
            view: &image_view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.005,
                    g: 0.005,
                    b: 0.005,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        };

        let render_pass_descriptor = wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(color_attachment)],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        };

        {
            let mut renderpass = command_encoder.begin_render_pass(&render_pass_descriptor);
            // Rendering the Quads
            renderpass.set_bind_group(0, &self.quad_material.bind_group, &[]);
            renderpass.set_bind_group(1, &self.projection_ubo.bind_group, &[]);
            renderpass.set_vertex_buffer(0, self.quad_mesh.buffer.slice(0..self.quad_mesh.offset));
            renderpass.set_vertex_buffer(1, self.ibo.as_ref().unwrap().buffer.slice(..));
            renderpass.set_index_buffer(
                self.quad_mesh.buffer.slice(self.quad_mesh.offset..),
                wgpu::IndexFormat::Uint16,
            );

            renderpass.set_pipeline(&self.glow_pipeline);
            renderpass.draw_indexed(0..6, 0, 0..items.len() as u32);
            renderpass.set_pipeline(&self.core_pipeline);
            renderpass.draw_indexed(0..6, 0, 0..items.len() as u32);
        }
        self.queue.submit(std::iter::once(command_encoder.finish()));

        drawable.present();

        Ok(())
    }
}
