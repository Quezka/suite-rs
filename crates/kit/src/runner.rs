//! Running an app: a window on the desktop (winit + wgpu), or an offscreen picture for checks and tests. Both go
//! through the glass `Compositor`, so what a screenshot shows is what the window shows.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::glass::{Compositor, SCENE_FORMAT};
use crate::theme::{self, Tokens};

/// What an app gives the kit: its screens, drawn each frame.
pub trait App {
    fn ui(&mut self, ui: &mut egui::Ui, th: &Tokens);
    /// Whether the user wants the dark look (`None` follows the system).
    fn dark(&self) -> Option<bool> {
        None
    }
    fn high_contrast(&self) -> bool {
        false
    }
}

fn tokens_for(ctx: &egui::Context, app: &dyn App) -> Tokens {
    let dark = app.dark().unwrap_or_else(|| ctx.input(|i| i.raw.system_theme).is_none_or(|t| t == egui::Theme::Dark));
    theme::tokens(dark, app.high_contrast())
}

fn frame(ctx: &egui::Context, app: &mut dyn App, raw: egui::RawInput) -> (egui::FullOutput, Tokens) {
    if ctx.data(|d| d.get_temp::<bool>(egui::Id::NULL.with("quezka-fonts")).is_none()) {
        crate::fonts::install(ctx);
        ctx.data_mut(|d| d.insert_temp(egui::Id::NULL.with("quezka-fonts"), true));
    }
    let mut th = theme::LIGHT;
    let output = ctx.run_ui(raw, |ui| {
        th = tokens_for(ui.ctx(), &*app);
        crate::style::apply(ui.ctx(), &th);
        app.ui(ui, &th);
    });
    (output, th)
}

async fn device_for(instance: &wgpu::Instance, surface: Option<&wgpu::Surface<'_>>) -> Option<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    // The low-power GPU first; then any GPU; then a software one, so screenshots work anywhere.
    let mut adapter = None;
    for (power, fallback) in [
        (wgpu::PowerPreference::LowPower, false),
        (wgpu::PowerPreference::HighPerformance, false),
        (wgpu::PowerPreference::None, true),
    ] {
        let options = wgpu::RequestAdapterOptions {
            power_preference: power,
            compatible_surface: surface,
            force_fallback_adapter: fallback,
            apply_limit_buckets: false,
        };
        if let Ok(found) = instance.request_adapter(&options).await {
            adapter = Some(found);
            break;
        }
    }
    let adapter = adapter?;
    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await.ok()?;
    Some((adapter, device, queue))
}

/// Renders `frames` frames of `app` offscreen and saves the last as a PNG. `size` is in points.
pub fn screenshot(app: &mut dyn App, size: [f32; 2], pixels_per_point: f32, frames: usize, path: &str) -> Result<(), String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let (_adapter, device, queue) = pollster::block_on(device_for(&instance, None)).ok_or("no graphics device")?;
    let mut compositor = Compositor::new(&device, SCENE_FORMAT);
    let ctx = egui::Context::default();
    let pixels = [(size[0] * pixels_per_point) as u32, (size[1] * pixels_per_point) as u32];
    for i in 0..frames.max(1) {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0], size[1]))),
            time: Some(i as f64 / 60.0),
            ..Default::default()
        };
        ctx.set_pixels_per_point(pixels_per_point);
        let (output, th) = frame(&ctx, app, raw);
        let prims = ctx.tessellate(output.shapes, output.pixels_per_point);
        compositor.render_scene(&device, &queue, pixels, output.pixels_per_point, &prims, output.textures_delta, th.bg);
    }
    let (w, h, data) = compositor.read_scene(&device, &queue).ok_or("couldn't read the picture back")?;
    image::RgbaImage::from_raw(w, h, data)
        .ok_or("bad picture size")?
        .save(path)
        .map_err(|e| e.to_string())
}

struct Gpu {
    instance: wgpu::Instance,
    _adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// None while the app is in the background on Android (the system takes its window away).
    surface: Option<wgpu::Surface<'static>>,
    config: wgpu::SurfaceConfiguration,
    compositor: Compositor,
}

/// Frame timing, reported every few seconds (logcat on Android; stderr on the desktop with `QUEZKA_FRAME_STATS=1`):
/// the time between frames while animating, and the time each frame took to build and draw.
struct FrameStats {
    last: Option<std::time::Instant>,
    gaps: Vec<f32>,
    work: Vec<f32>,
    since: std::time::Instant,
    on: bool,
}

impl FrameStats {
    fn new() -> Self {
        let on = cfg!(target_os = "android") || std::env::var_os("QUEZKA_FRAME_STATS").is_some();
        Self { last: None, gaps: Vec::new(), work: Vec::new(), since: std::time::Instant::now(), on }
    }

    fn record(&mut self, started: std::time::Instant, animating: bool) {
        if !self.on {
            return;
        }
        let now = std::time::Instant::now();
        self.work.push((now - started).as_secs_f32() * 1000.0);
        if let Some(last) = self.last {
            self.gaps.push((started - last).as_secs_f32() * 1000.0);
        }
        self.last = animating.then_some(now);
        if now - self.since > std::time::Duration::from_secs(3) && self.work.len() > 10 {
            let pct = |v: &mut Vec<f32>, p: f32| {
                v.sort_by(f32::total_cmp);
                v.get(((v.len() as f32 - 1.0) * p) as usize).copied().unwrap_or(0.0)
            };
            let frames = self.work.len();
            let slow = self.gaps.iter().filter(|g| **g > 17.5).count();
            let line = format!(
                "frames {frames}: work p50 {:.1} ms p90 {:.1} p99 {:.1}; gap p50 {:.1} p90 {:.1} p99 {:.1}; over 17.5 ms: {slow}",
                pct(&mut self.work, 0.5), pct(&mut self.work, 0.9), pct(&mut self.work, 0.99),
                pct(&mut self.gaps, 0.5), pct(&mut self.gaps, 0.9), pct(&mut self.gaps, 0.99),
            );
            crate::log(&line);
            self.work.clear();
            self.gaps.clear();
            self.since = now;
        }
    }
}

struct Runner<A: App> {
    app: A,
    title: String,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    ctx: egui::Context,
    state: Option<egui_winit::State>,
    stats: FrameStats,
}

impl<A: App> Runner<A> {
    fn redraw(&mut self) {
        let started = std::time::Instant::now();
        let (Some(window), Some(gpu), Some(state)) = (&self.window, &mut self.gpu, &mut self.state) else { return };
        let Some(surface) = &gpu.surface else { return };
        let raw = state.take_egui_input(window);
        let (output, th) = frame(&self.ctx, &mut self.app, raw);
        state.handle_platform_output(window, output.platform_output);
        let prims = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        let size = [gpu.config.width, gpu.config.height];
        gpu.compositor.render_scene(&gpu.device, &gpu.queue, size, output.pixels_per_point, &prims, output.textures_delta, th.bg);
        match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                let view = frame.texture.create_view(&Default::default());
                gpu.compositor.present(&gpu.device, &gpu.queue, &view);
                window.pre_present_notify();
                gpu.queue.present(frame);
            }
            _ => surface.configure(&gpu.device, &gpu.config),
        }
        let repaint = output.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.repaint_delay);
        let animating = repaint.is_some_and(|d| d.is_zero());
        if animating {
            window.request_redraw();
        }
        self.stats.record(started, animating);
    }

    fn attach_surface(&mut self) {
        let (Some(window), Some(gpu)) = (&self.window, &mut self.gpu) else { return };
        let surface = gpu.instance.create_surface(window.clone()).expect("a surface");
        let size = window.inner_size();
        gpu.config.width = size.width.max(1);
        gpu.config.height = size.height.max(1);
        surface.configure(&gpu.device, &gpu.config);
        gpu.surface = Some(surface);
        window.request_redraw();
    }
}

impl<A: App> ApplicationHandler for Runner<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            // Back from the background (Android): the window is new, so is its surface.
            self.attach_surface();
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(1100.0, 720.0));
        let window = Arc::new(event_loop.create_window(attributes).expect("a window"));
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(event_loop.owned_display_handle())));
        let surface = instance.create_surface(window.clone()).expect("a surface");
        let (adapter, device, queue) = pollster::block_on(device_for(&instance, Some(&surface))).expect("a graphics device");
        let caps = surface.get_capabilities(&adapter);
        // Plain (not sRGB) formats: egui and the glass work in gamma space, and the copy to the screen keeps it so.
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);
        crate::log(&format!("graphics: {:?} {}", adapter.get_info().backend, adapter.get_info().name));
        let compositor = Compositor::new(&device, format);
        let max_texture = device.limits().max_texture_dimension_2d as usize;
        self.state = Some(egui_winit::State::new(
            self.ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(max_texture),
        ));
        self.gpu = Some(Gpu { instance, _adapter: adapter, device, queue, surface: Some(surface), config, compositor });
        window.request_redraw();
        self.window = Some(window);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(gpu) = &mut self.gpu {
            gpu.surface = None;
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone() else { return };
        if let Some(state) = &mut self.state {
            let response = state.on_window_event(&window, &event);
            if response.repaint {
                window.request_redraw();
            }
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.config.width = size.width.max(1);
                    gpu.config.height = size.height.max(1);
                    if let Some(surface) = &gpu.surface {
                        surface.configure(&gpu.device, &gpu.config);
                    }
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }
}

fn run_loop<A: App + 'static>(event_loop: EventLoop<()>, title: &str, app: A) -> Result<(), String> {
    let mut runner = Runner {
        app,
        title: title.to_string(),
        window: None,
        gpu: None,
        ctx: egui::Context::default(),
        state: None,
        stats: FrameStats::new(),
    };
    event_loop.run_app(&mut runner).map_err(|e| e.to_string())
}

/// Opens the app's window and runs it until it's closed.
#[cfg(not(target_os = "android"))]
pub fn run<A: App + 'static>(title: &str, app: A) -> Result<(), String> {
    run_loop(EventLoop::new().map_err(|e| e.to_string())?, title, app)
}

/// Runs the app as an Android activity (call from the app's `android_main`).
#[cfg(target_os = "android")]
pub fn run_android<A: App + 'static>(android: winit::platform::android::activity::AndroidApp, title: &str, app: A) -> Result<(), String> {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    let event_loop = EventLoop::builder().with_android_app(android).build().map_err(|e| e.to_string())?;
    run_loop(event_loop, title, app)
}
