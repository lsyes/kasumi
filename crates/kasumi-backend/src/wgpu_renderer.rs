//! wgpu window surface, atlas and instanced scene renderer.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::mem::size_of;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use bytemuck::{Pod, Zeroable};
use kasumi::Runtime;
use kasumi_core::{Color, DrawItem, DrawList, FontFaceId, ImageData, Size};
use kasumi_paint::TextEngine;
use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Theme as WinitTheme, Window};

use crate::WindowOptions;
use crate::atlas_shelf::{
    ATLAS_SIZE, AtlasEntry, AtlasRegion, GlyphKey, ShelfAllocator, expand_mask,
};

const ATTRIBUTES: usize = 7;
const FLOATS_PER_INSTANCE: usize = ATTRIBUTES * 4;
const MIN_INSTANCE_BYTES: u64 = (FLOATS_PER_INSTANCE * size_of::<f32>() * 64) as u64;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct Instance {
    rect: [f32; 4],
    color: [f32; 4],
    border_color: [f32; 4],
    radii: [f32; 4],
    uv: [f32; 4],
    clip: [f32; 4],
    meta: [f32; 4],
}

struct WgpuAtlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    entries: HashMap<GlyphKey, Option<AtlasEntry>>,
    images: HashMap<u64, Option<AtlasEntry>>,
    shelf: ShelfAllocator,
}

impl WgpuAtlas {
    fn new(device: &wgpu::Device) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("kasumi glyph atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("kasumi atlas sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            texture,
            view,
            sampler,
            entries: HashMap::new(),
            images: HashMap::new(),
            shelf: ShelfAllocator::default(),
        }
    }

    fn glyph(
        &mut self,
        queue: &wgpu::Queue,
        texts: &TextEngine,
        face: FontFaceId,
        glyph_id: u16,
        font_size: f32,
        scale: f32,
    ) -> Option<AtlasEntry> {
        let pixel_size = ((font_size * scale) * 4.0).round() / 4.0;
        if pixel_size < 1.0 {
            return None;
        }
        let key = GlyphKey {
            face,
            glyph_id,
            size_quarter: (pixel_size * 4.0).round() as u32,
        };
        if let Some(entry) = self.entries.get(&key) {
            return *entry;
        }
        let entry = texts
            .rasterize(face, glyph_id, pixel_size, 1.0)
            .and_then(|raster| {
                let data = expand_mask(&raster);
                let region = self.shelf.allocate(
                    raster.width,
                    raster.height,
                    raster.is_color,
                    raster.left,
                    raster.top,
                )?;
                self.upload(queue, region, &data);
                Some(region.entry)
            });
        self.entries.insert(key, entry);
        entry
    }

    fn image(&mut self, queue: &wgpu::Queue, image: &ImageData) -> Option<AtlasEntry> {
        if let Some(entry) = self.images.get(&image.id()) {
            return *entry;
        }
        let entry = self
            .shelf
            .allocate(image.width(), image.height(), true, 0, 0)
            .map(|region| {
                self.upload(queue, region, image.pixels());
                region.entry
            });
        self.images.insert(image.id(), entry);
        entry
    }

    fn upload(&self, queue: &wgpu::Queue, region: AtlasRegion, data: &[u8]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: region.x,
                    y: region.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(region.width * 4),
                rows_per_image: Some(region.height),
            },
            wgpu::Extent3d {
                width: region.width,
                height: region.height,
                depth_or_array_layers: 1,
            },
        );
    }
}

pub struct WgpuBackend {
    window: Arc<Window>,
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
    vertices: wgpu::Buffer,
    vertex_capacity: u64,
    atlas: WgpuAtlas,
    instances: Vec<Instance>,
    reconfigure_after_present: bool,
    pending_submissions: VecDeque<wgpu::SubmissionIndex>,
}

impl WgpuBackend {
    pub fn new(event_loop: &ActiveEventLoop, options: &WindowOptions) -> Result<Self> {
        let mut attributes = Window::default_attributes()
            .with_title(&options.title)
            .with_theme(Some(WinitTheme::Light))
            .with_inner_size(LogicalSize::new(
                options.logical_size.width,
                options.logical_size.height,
            ));
        if let Some(min_size) = options.min_size {
            attributes =
                attributes.with_min_inner_size(LogicalSize::new(min_size.width, min_size.height));
        }
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| anyhow!("failed to create the window: {error}"))?,
        );

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(event_loop.owned_display_handle()),
        ));
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(|error| anyhow!("failed to create the wgpu surface: {error}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .map_err(|error| anyhow!("no compatible wgpu adapter: {error}"))?;
        let adapter_limits = adapter.limits();
        if adapter_limits.max_texture_dimension_2d < ATLAS_SIZE {
            return Err(anyhow!(
                "wgpu adapter max_texture_dimension_2d {} is smaller than the {ATLAS_SIZE}px atlas",
                adapter_limits.max_texture_dimension_2d
            ));
        }
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("kasumi device"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter_limits,
            ..Default::default()
        }))
        .map_err(|error| anyhow!("failed to create the wgpu device: {error}"))?;
        device.on_uncaptured_error(std::sync::Arc::new(|error| {
            eprintln!("kasumi: wgpu uncaptured error: {error}");
        }));

        let initial = window.inner_size();
        let mut config = configure_surface(
            &surface,
            &adapter,
            &device,
            initial.width.max(1),
            initial.height.max(1),
            options.vsync,
        )?;
        let format = config.format;
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kasumi scene bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kasumi scene pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kasumi scene shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/scene.wgsl").into()),
        });
        let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("kasumi scene pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Instance>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4,
                        1 => Float32x4,
                        2 => Float32x4,
                        3 => Float32x4,
                        4 => Float32x4,
                        5 => Float32x4,
                        6 => Float32x4,
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        if let Some(error) = pollster::block_on(error_scope.pop()) {
            return Err(anyhow!("wgpu pipeline validation failed: {error}"));
        }

        let atlas = WgpuAtlas::new(&device);
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("kasumi viewport uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vertices = create_vertex_buffer(&device, MIN_INSTANCE_BYTES);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kasumi scene bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&atlas.sampler),
                },
            ],
        });

        if std::env::var_os("KASUMI_STATS").is_some() {
            let info = adapter.get_info();
            eprintln!(
                "kasumi: wgpu {} · {} · {}",
                info.backend, info.name, info.driver
            );
        }

        // Width and height were already applied before pipeline creation.
        config.width = initial.width.max(1);
        config.height = initial.height.max(1);
        Ok(Self {
            window,
            instance,
            surface,
            device,
            queue,
            config,
            pipeline,
            bind_group,
            uniforms,
            vertices,
            vertex_capacity: MIN_INSTANCE_BYTES,
            atlas,
            instances: Vec::new(),
            reconfigure_after_present: false,
            pending_submissions: VecDeque::new(),
        })
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(
        &mut self,
        runtime: &Runtime,
        viewport: Size,
        _physical_size: (u32, u32),
        clear: Color,
    ) -> Result<()> {
        self.queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::bytes_of(&[viewport.width, viewport.height, 0.0, 0.0]),
        );
        self.prepare_instances(runtime, viewport, runtime.scale_factor());

        let frame = match self.acquire_frame()? {
            Some(frame) => frame,
            None => return Ok(()),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("kasumi frame encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kasumi scene pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(clear.r),
                            g: f64::from(clear.g),
                            b: f64::from(clear.b),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !self.instances.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertices.slice(..));
                pass.draw(0..4, 0..self.instances.len() as u32);
            }
        }
        let submission = self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        self.pending_submissions.push_back(submission);
        if self.pending_submissions.len() > 2
            && let Some(oldest) = self.pending_submissions.pop_front()
        {
            let _ = self.device.poll(wgpu::PollType::Wait {
                submission_index: Some(oldest),
                timeout: Some(std::time::Duration::from_millis(100)),
            });
        }

        if self.reconfigure_after_present {
            self.reconfigure_after_present = false;
            self.surface.configure(&self.device, &self.config);
        }
        Ok(())
    }

    fn prepare_instances(&mut self, runtime: &Runtime, viewport: Size, scale: f32) {
        let scale = scale.max(0.01);
        self.instances.clear();
        let draw_list = runtime.draw_list();
        self.instances.reserve(draw_list.items.len());
        let texts = runtime.texts();
        append_instances(
            &mut self.instances,
            &mut self.atlas,
            &self.queue,
            &texts,
            draw_list,
            scale,
        );
        if self.instances.is_empty() {
            return;
        }
        let bytes = bytemuck::cast_slice(&self.instances);
        let required = bytes.len() as u64;
        if required > self.vertex_capacity {
            self.vertex_capacity = required.next_power_of_two();
            self.vertices = create_vertex_buffer(&self.device, self.vertex_capacity);
        }
        self.queue.write_buffer(&self.vertices, 0, bytes);

        let _ = viewport;
    }

    fn acquire_frame(&mut self) -> Result<Option<wgpu::SurfaceTexture>> {
        let texture = self.surface.get_current_texture();
        let frame = match texture {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.reconfigure_after_present = true;
                frame
            }
            wgpu::CurrentSurfaceTexture::Timeout => return Ok(None),
            wgpu::CurrentSurfaceTexture::Occluded => return Ok(None),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                    other => {
                        return Err(anyhow!("wgpu surface unavailable after resize: {other:?}"));
                    }
                }
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self
                    .instance
                    .create_surface(Arc::clone(&self.window))
                    .map_err(|error| anyhow!("failed to recreate the wgpu surface: {error}"))?;
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
                    other => {
                        return Err(anyhow!(
                            "wgpu surface unavailable after recreation: {other:?}"
                        ));
                    }
                }
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(anyhow!("wgpu surface validation error"));
            }
        };
        Ok(Some(frame))
    }

    pub fn poll(&mut self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
    }
}

fn configure_surface(
    surface: &wgpu::Surface<'_>,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    width: u32,
    height: u32,
    vsync: bool,
) -> Result<wgpu::SurfaceConfiguration> {
    let capabilities = surface.get_capabilities(adapter);
    let format = capabilities
        .formats
        .iter()
        .copied()
        .find(|format| {
            matches!(
                format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
            )
        })
        .or_else(|| capabilities.formats.first().copied())
        .ok_or_else(|| anyhow!("wgpu surface reports no compatible formats"))?;
    let mut config = surface
        .get_default_config(adapter, width.max(1), height.max(1))
        .ok_or_else(|| anyhow!("wgpu surface has no default configuration"))?;
    config.format = format;
    config.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
    config.present_mode = if vsync {
        wgpu::PresentMode::AutoVsync
    } else {
        wgpu::PresentMode::AutoNoVsync
    };
    config.desired_maximum_frame_latency = 2;
    if capabilities
        .alpha_modes
        .contains(&wgpu::CompositeAlphaMode::Opaque)
    {
        config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
    }
    surface.configure(device, &config);
    Ok(config)
}

fn create_vertex_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("kasumi scene instances"),
        size: size.max(4),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn append_instances(
    out: &mut Vec<Instance>,
    atlas: &mut WgpuAtlas,
    queue: &wgpu::Queue,
    texts: &TextEngine,
    draw_list: &DrawList,
    scale: f32,
) {
    for item in &draw_list.items {
        match item {
            DrawItem::Rect(rect) => out.push(Instance {
                rect: [
                    rect.rect.x(),
                    rect.rect.y(),
                    rect.rect.width(),
                    rect.rect.height(),
                ],
                color: rect.color.to_array(),
                border_color: rect.border_color.to_array(),
                radii: rect.radius.to_array(),
                uv: [0.0; 4],
                clip: [
                    rect.clip.x(),
                    rect.clip.y(),
                    rect.clip.width(),
                    rect.clip.height(),
                ],
                meta: [0.0, rect.border_width, 0.0, 0.0],
            }),
            DrawItem::Glyph(glyph) => {
                let Some(entry) = atlas.glyph(
                    queue,
                    texts,
                    glyph.face,
                    glyph.glyph_id,
                    glyph.font_size,
                    scale,
                ) else {
                    continue;
                };
                let left = (glyph.x * scale + entry.left as f32).round();
                let top = (glyph.baseline * scale - entry.top as f32).round();
                out.push(Instance {
                    rect: [
                        left / scale,
                        top / scale,
                        entry.width as f32 / scale,
                        entry.height as f32 / scale,
                    ],
                    color: if entry.is_color {
                        [1.0, 1.0, 1.0, glyph.color.a]
                    } else {
                        glyph.color.to_array()
                    },
                    border_color: [0.0; 4],
                    radii: [0.0; 4],
                    uv: entry.uv,
                    clip: [
                        glyph.clip.x(),
                        glyph.clip.y(),
                        glyph.clip.width(),
                        glyph.clip.height(),
                    ],
                    meta: [if entry.is_color { 2.0 } else { 1.0 }, 0.0, 0.0, 0.0],
                });
            }
            DrawItem::Image(image) => {
                let Some(entry) = atlas.image(queue, &image.image) else {
                    continue;
                };
                out.push(Instance {
                    rect: [
                        image.rect.x(),
                        image.rect.y(),
                        image.rect.width(),
                        image.rect.height(),
                    ],
                    color: [1.0, 1.0, 1.0, image.opacity],
                    border_color: [0.0; 4],
                    radii: image.radius.to_array(),
                    uv: entry.uv,
                    clip: [
                        image.clip.x(),
                        image.clip.y(),
                        image.clip.width(),
                        image.clip.height(),
                    ],
                    meta: [3.0, 0.0, 0.0, 0.0],
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn scene_shader_parses() {
        let source = include_str!("../shaders/scene.wgsl");
        wgpu::naga::front::wgsl::parse_str(source).expect("scene.wgsl must parse");
    }
}
