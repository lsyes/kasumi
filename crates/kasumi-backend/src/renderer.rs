//! The OpenGL renderer.
//!
//! All primitives share one instanced pipeline and one draw call. A rectangle
//! uses a rounded-box SDF in the fragment shader; a glyph samples the atlas.
//! Because both kinds end up in the same ordered instance buffer, z-order is
//! preserved without extra draw calls.

use std::mem::size_of;

use anyhow::{Context as _, Result, anyhow};
use glow::HasContext;
use kasumi_core::{Color, DrawItem, DrawList, Size};
use kasumi_paint::TextEngine;

use crate::atlas::GlyphAtlas;

const ATTRIBUTES: u32 = 7;
const FLOATS_PER_INSTANCE: usize = ATTRIBUTES as usize * 4;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Instance {
    rect: [f32; 4],
    color: [f32; 4],
    border_color: [f32; 4],
    radii: [f32; 4],
    uv: [f32; 4],
    clip: [f32; 4],
    /// x: primitive kind (0 rect, 1 mask glyph, 2 color glyph)
    /// y: border width in logical pixels
    meta: [f32; 4],
}

pub struct Renderer {
    gl: glow::Context,
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    atlas: GlyphAtlas,
    u_viewport: Option<glow::UniformLocation>,
    u_atlas: Option<glow::UniformLocation>,
    instances: Vec<Instance>,
}

impl Renderer {
    pub fn new(gl: glow::Context) -> Result<Self> {
        let vertex = compile_shader(
            &gl,
            glow::VERTEX_SHADER,
            include_str!("../shaders/scene.vert"),
        )
        .context("compiling scene vertex shader")?;
        let fragment = compile_shader(
            &gl,
            glow::FRAGMENT_SHADER,
            include_str!("../shaders/scene.frag"),
        )
        .context("compiling scene fragment shader")?;
        let program = link_program(&gl, vertex, fragment)?;
        unsafe {
            gl.detach_shader(program, vertex);
            gl.detach_shader(program, fragment);
            gl.delete_shader(vertex);
            gl.delete_shader(fragment);
        }

        let u_viewport = unsafe { gl.get_uniform_location(program, "u_viewport") };
        let u_atlas = unsafe { gl.get_uniform_location(program, "u_atlas") };

        let atlas = GlyphAtlas::new(&gl)?;

        let vao = unsafe {
            gl.create_vertex_array()
                .map_err(|error| anyhow!("create VAO: {error}"))?
        };
        let vbo = unsafe {
            gl.create_buffer()
                .map_err(|error| anyhow!("create instance buffer: {error}"))?
        };

        unsafe {
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                &[0u8; FLOATS_PER_INSTANCE * size_of::<f32>() * 64],
                glow::DYNAMIC_DRAW,
            );
            let stride = (FLOATS_PER_INSTANCE * size_of::<f32>()) as i32;
            for location in 0..ATTRIBUTES {
                gl.enable_vertex_attrib_array(location);
                gl.vertex_attrib_pointer_f32(
                    location,
                    4,
                    glow::FLOAT,
                    false,
                    stride,
                    (location as usize * 4 * size_of::<f32>()) as i32,
                );
                gl.vertex_attrib_divisor(location, 1);
            }
            gl.bind_vertex_array(None);
            gl.enable(glow::BLEND);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::SCISSOR_TEST);
            // The framework works in sRGB bytes and writes them directly.
            gl.disable(glow::FRAMEBUFFER_SRGB);
        }

        Ok(Self {
            gl,
            program,
            vao,
            vbo,
            atlas,
            u_viewport,
            u_atlas,
            instances: Vec::new(),
        })
    }

    pub fn render(
        &mut self,
        draw_list: &DrawList,
        texts: &TextEngine,
        viewport: Size,
        scale_factor: f32,
        physical_size: (u32, u32),
        clear: Color,
    ) {
        if physical_size.0 == 0 || physical_size.1 == 0 {
            return;
        }
        let scale = scale_factor.max(0.01);
        unsafe {
            self.gl
                .viewport(0, 0, physical_size.0 as i32, physical_size.1 as i32);
            self.gl.clear_color(clear.r, clear.g, clear.b, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }

        self.instances.clear();
        self.instances.reserve(draw_list.items.len());

        for item in &draw_list.items {
            match item {
                DrawItem::Rect(rect) => {
                    self.instances.push(Instance {
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
                    });
                }
                DrawItem::Glyph(glyph) => {
                    let Some(entry) = self.atlas.get_or_insert(
                        &self.gl,
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
                    let rect = [
                        left / scale,
                        top / scale,
                        entry.width as f32 / scale,
                        entry.height as f32 / scale,
                    ];
                    let color = if entry.is_color {
                        [1.0, 1.0, 1.0, glyph.color.a]
                    } else {
                        glyph.color.to_array()
                    };
                    self.instances.push(Instance {
                        rect,
                        color,
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
                    let Some(entry) = self.atlas.get_or_insert_image(&self.gl, &image.image) else {
                        continue;
                    };
                    self.instances.push(Instance {
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

        if self.instances.is_empty() {
            return;
        }

        let bytes = unsafe {
            // SAFETY: Instance is repr(C) and contains only f32 fields, so
            // viewing it as bytes for buffer upload is valid. The slice does
            // not outlive the call.
            std::slice::from_raw_parts(
                self.instances.as_ptr().cast::<u8>(),
                self.instances.len() * size_of::<Instance>(),
            )
        };

        unsafe {
            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            self.gl
                .buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::DYNAMIC_DRAW);
            self.gl.use_program(Some(self.program));
            self.gl
                .uniform_2_f32(self.u_viewport.as_ref(), viewport.width, viewport.height);
            self.gl.active_texture(glow::TEXTURE0);
            self.gl
                .bind_texture(glow::TEXTURE_2D, Some(self.atlas.texture()));
            self.gl.uniform_1_i32(self.u_atlas.as_ref(), 0);
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl
                .draw_arrays_instanced(glow::TRIANGLE_STRIP, 0, 4, self.instances.len() as i32);
            self.gl.bind_vertex_array(None);
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            let texture = self.atlas.texture();
            self.gl.delete_texture(texture);
            self.gl.delete_buffer(self.vbo);
            self.gl.delete_vertex_array(self.vao);
            self.gl.delete_program(self.program);
        }
    }
}

fn compile_shader(gl: &glow::Context, kind: u32, source: &str) -> Result<glow::Shader> {
    unsafe {
        let shader = gl
            .create_shader(kind)
            .map_err(|error| anyhow!("create shader: {error}"))?;
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if !gl.get_shader_compile_status(shader) {
            let log = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            return Err(anyhow!("shader compilation failed:\n{log}"));
        }
        Ok(shader)
    }
}

fn link_program(
    gl: &glow::Context,
    vertex: glow::Shader,
    fragment: glow::Shader,
) -> Result<glow::Program> {
    unsafe {
        let program = gl
            .create_program()
            .map_err(|error| anyhow!("create program: {error}"))?;
        gl.attach_shader(program, vertex);
        gl.attach_shader(program, fragment);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            return Err(anyhow!("shader link failed:\n{log}"));
        }
        Ok(program)
    }
}
