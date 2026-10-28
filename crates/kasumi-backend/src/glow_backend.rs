//! OpenGL window surface and renderer adapter.

use std::num::NonZeroU32;

use anyhow::{Context as _, Result, anyhow};
use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, GlProfile, PossiblyCurrentContext, Version,
};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::DisplayBuilder;
use kasumi::Runtime;
use kasumi_core::Color;
use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Theme as WinitTheme, Window};

use crate::{WindowOptions, renderer::Renderer};

pub struct GlowBackend {
    renderer: Renderer,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    window: Window,
    _display: glutin::display::Display,
}

impl GlowBackend {
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

        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_transparency(false)
            .with_depth_size(0)
            .with_stencil_size(0);

        let (window, config) = DisplayBuilder::new()
            .with_window_attributes(Some(attributes))
            .build(event_loop, template, |configs| {
                configs
                    .max_by_key(|config| {
                        (
                            config.hardware_accelerated() as u8,
                            config.srgb_capable() as u8,
                            config.alpha_size(),
                            (config.num_samples() == 0) as u8,
                        )
                    })
                    .expect("no suitable OpenGL framebuffer configuration")
            })
            .map_err(|error| anyhow!("failed to initialize the OpenGL display: {error}"))?;
        let window = window.ok_or_else(|| anyhow!("no window was created"))?;

        let raw_handle = window
            .window_handle()
            .map_err(|error| anyhow!("window handle unavailable: {error}"))?
            .as_raw();
        let display = config.display();

        let context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .with_profile(GlProfile::Core)
            .build(Some(raw_handle));
        let not_current = unsafe { display.create_context(&config, &context_attributes) }
            .map_err(|error| anyhow!("failed to create an OpenGL 3.3 context: {error}"))?;

        let initial = window.inner_size();
        let width = NonZeroU32::new(initial.width.max(1)).expect("width is non-zero");
        let height = NonZeroU32::new(initial.height.max(1)).expect("height is non-zero");
        let surface_attributes =
            SurfaceAttributesBuilder::<WindowSurface>::new().build(raw_handle, width, height);
        let surface = unsafe { display.create_window_surface(&config, &surface_attributes) }
            .map_err(|error| anyhow!("failed to create the window surface: {error}"))?;
        let context = not_current
            .make_current(&surface)
            .map_err(|error| anyhow!("failed to make the GL context current: {error}"))?;

        if options.vsync {
            let interval = SwapInterval::Wait(NonZeroU32::new(1).expect("1 is non-zero"));
            let _ = surface.set_swap_interval(&context, interval);
        }

        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|symbol| {
                display.get_proc_address(symbol).cast()
            })
        };

        if std::env::var_os("KASUMI_STATS").is_some() {
            use glow::HasContext;
            unsafe {
                eprintln!(
                    "kasumi: GL {} · {}",
                    gl.get_parameter_string(glow::VERSION),
                    gl.get_parameter_string(glow::RENDERER)
                );
            }
        }

        let renderer = Renderer::new(gl).context("initializing the OpenGL renderer")?;
        Ok(Self {
            renderer,
            surface,
            context,
            window,
            _display: display,
        })
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
            self.surface.resize(&self.context, width, height);
        }
    }

    pub fn render(
        &mut self,
        runtime: &Runtime,
        viewport: kasumi_core::Size,
        physical_size: (u32, u32),
        clear: Color,
    ) -> Result<()> {
        let texts = runtime.texts();
        self.renderer.render_draw_list(
            runtime.draw_list(),
            &texts,
            viewport,
            runtime.scale_factor(),
            physical_size,
            clear,
        );
        self.surface
            .swap_buffers(&self.context)
            .map_err(|error| anyhow!("failed to present the frame: {error}"))
    }

    pub fn poll(&mut self) {}
}
