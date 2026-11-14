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

/// How an app's window starts.
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub title: String,
    /// The desktop file's name without `.desktop` (`io.github.quezka.Quay`): the Wayland app id and the X11 class,
    /// so the dock puts the window under the app's own icon.
    pub app_id: String,
    /// The window icon as RGBA pixels and its side in pixels (X11 and Windows; Wayland takes it from the desktop file).
    pub icon: Option<(Vec<u8>, u32)>,
    /// Run without a window until the app asks for one (`ViewportCommand::Visible(true)`) or quits
    /// (`ViewportCommand::Close`): an app started at login that may have nothing to show.
    pub start_hidden: bool,
}

struct Runner<A: App> {
    app: A,
    options: Options,
    /// Still running without a window (see `Options::start_hidden`).
    hidden: bool,
    /// The app sent `ViewportCommand::Close`.
    closing: bool,
    /// Texture changes from frames run while hidden: uploaded with the first frame drawn.
    unseen_textures: egui::TexturesDelta,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    ctx: egui::Context,
    state: Option<egui_winit::State>,
    stats: FrameStats,
    #[cfg(target_os = "android")]
    android: Option<(winit::platform::android::activity::AndroidApp, crate::android::Typing)>,
    /// Events to add to the next frame's input (typing from a phone keyboard).
    pending: Vec<egui::Event>,
    frame_count: u64,
}

impl<A: App> Runner<A> {
    fn redraw(&mut self) {
        let started = std::time::Instant::now();
        let (Some(window), Some(gpu), Some(state)) = (&self.window, &mut self.gpu, &mut self.state) else { return };
        let Some(surface) = &gpu.surface else { return };
        let mut raw = state.take_egui_input(window);
        raw.events.append(&mut self.pending);
        self.frame_count += 1;
        #[cfg(target_os = "android")]
        if let Some((app, _)) = &self.android {
            // Where the bars and the keyboard are: checked every few frames (it costs a few calls into Java).
            if self.frame_count % 8 == 1 {
                if let Some(px) = crate::android::insets(app) {
                    let margins = crate::android::margins(px, window.scale_factor() as f32);
                    self.ctx.data_mut(|d| d.insert_temp(egui::Id::NULL.with("quezka-safe-area"), margins));
                }
            }
        }
        let (mut output, th) = frame(&self.ctx, &mut self.app, raw);
        if !self.unseen_textures.is_empty() {
            let mut all = std::mem::take(&mut self.unseen_textures);
            all.append(std::mem::take(&mut output.textures_delta));
            output.textures_delta = all;
        }
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
        let viewport = output.viewport_output.get(&egui::ViewportId::ROOT);
        let repaint = viewport.map(|v| v.repaint_delay);
        for command in viewport.map(|v| v.commands.clone()).unwrap_or_default() {
            match command {
                egui::ViewportCommand::Close => self.closing = true,
                egui::ViewportCommand::Minimized(on) => window.set_minimized(on),
                egui::ViewportCommand::Focus => window.focus_window(),
                _ => {}
            }
        }
        #[cfg(target_os = "android")]
        if let Some((app, typing)) = &mut self.android {
            let typed = typing.update(app, &self.ctx);
            if !typed.is_empty() {
                self.pending.extend(typed);
                window.request_redraw();
            }
            if self.ctx.egui_wants_keyboard_input() {
                // Keep looking at the keyboard's text while it's up.
                self.ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
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
        if self.hidden {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(std::time::Instant::now() + HIDDEN_TICK));
            return;
        }
        self.open_window(event_loop);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(gpu) = &mut self.gpu {
            gpu.surface = None;
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.closing {
            event_loop.exit();
            return;
        }
        if !self.hidden {
            return;
        }
        // No window yet: run the app's frame now and then, unseen, until it asks to be shown or to quit.
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WIDTH, HEIGHT))),
            ..Default::default()
        };
        let (mut output, _) = frame(&self.ctx, &mut self.app, raw);
        self.unseen_textures.append(std::mem::take(&mut output.textures_delta));
        let commands = output.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default();
        if commands.iter().any(|c| matches!(c, egui::ViewportCommand::Close)) {
            event_loop.exit();
        } else if commands.iter().any(|c| matches!(c, egui::ViewportCommand::Visible(true) | egui::ViewportCommand::Focus)) {
            self.hidden = false;
            self.open_window(event_loop);
        } else {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(std::time::Instant::now() + HIDDEN_TICK));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.on_window_event(event_loop, event);
    }
}

const WIDTH: f32 = 1100.0;
const HEIGHT: f32 = 720.0;
const HIDDEN_TICK: std::time::Duration = std::time::Duration::from_millis(250);

/// The window's first size: the app's own, or less on a small screen (never filling it, so the
/// window manager doesn't take it for a maximised window).
pub fn first_size(screen: Option<[f32; 2]>) -> [f32; 2] {
    match screen {
        Some([w, h]) => [WIDTH.min((w * 0.9).floor()), HEIGHT.min((h * 0.85).floor())],
        None => [WIDTH, HEIGHT],
    }
}

impl<A: App> Runner<A> {
    fn open_window(&mut self, event_loop: &ActiveEventLoop) {
        let screen = event_loop.primary_monitor().or_else(|| event_loop.available_monitors().next()).map(|m| {
            let size = m.size().to_logical::<f32>(m.scale_factor());
            [size.width, size.height]
        });
        let [w, h] = first_size(screen);
        #[allow(unused_mut)]
        let mut attributes = Window::default_attributes()
            .with_title(self.options.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(w, h))
            .with_min_inner_size(winit::dpi::LogicalSize::new(360.0_f32.min(w), 360.0_f32.min(h)));
        if let Some((rgba, side)) = &self.options.icon {
            attributes = attributes.with_window_icon(winit::window::Icon::from_rgba(rgba.clone(), *side, *side).ok());
        }
        #[cfg(all(unix, not(target_os = "android"), not(target_os = "macos"), not(target_os = "ios")))]
        if !self.options.app_id.is_empty() {
            let id = self.options.app_id.clone();
            attributes = winit::platform::wayland::WindowAttributesExtWayland::with_name(attributes, id.clone(), id.clone());
            attributes = winit::platform::x11::WindowAttributesExtX11::with_name(attributes, id.clone(), id);
        }
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

    fn on_window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
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

fn run_loop<A: App + 'static>(
    event_loop: EventLoop<()>,
    options: Options,
    app: A,
    #[cfg(target_os = "android")] android: winit::platform::android::activity::AndroidApp,
) -> Result<(), String> {
    let mut runner = Runner {
        app,
        hidden: options.start_hidden,
        closing: false,
        unseen_textures: Default::default(),
        options,
        window: None,
        gpu: None,
        ctx: egui::Context::default(),
        state: None,
        stats: FrameStats::new(),
        #[cfg(target_os = "android")]
        android: None,
        pending: Vec::new(),
        frame_count: 0,
    };
    #[cfg(target_os = "android")]
    {
        runner.android = Some((android, crate::android::Typing::default()));
    }
    event_loop.run_app(&mut runner).map_err(|e| e.to_string())
}

/// Opens the app's window and runs it until it's closed.
#[cfg(not(target_os = "android"))]
pub fn run<A: App + 'static>(title: &str, app: A) -> Result<(), String> {
    run_with(Options { title: title.to_string(), ..Default::default() }, app)
}

/// Like `run`, with the window's app id, icon and whether it starts hidden.
#[cfg(not(target_os = "android"))]
pub fn run_with<A: App + 'static>(options: Options, app: A) -> Result<(), String> {
    run_loop(EventLoop::new().map_err(|e| e.to_string())?, options, app)
}

/// Runs the app as an Android activity (call from the app's `android_main`).
#[cfg(target_os = "android")]
pub fn run_android<A: App + 'static>(android: winit::platform::android::activity::AndroidApp, title: &str, app: A) -> Result<(), String> {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    let event_loop = EventLoop::builder().with_android_app(android.clone()).build().map_err(|e| e.to_string())?;
    run_loop(event_loop, Options { title: title.to_string(), ..Default::default() }, app, android)
}

#[cfg(test)]
mod tests {
    use super::first_size;

    #[test]
    fn a_small_screen_gets_a_window_that_does_not_fill_it() {
        assert_eq!(first_size(None), [1100.0, 720.0]);
        assert_eq!(first_size(Some([1920.0, 1080.0])), [1100.0, 720.0]);
        let [w, h] = first_size(Some([1093.0, 614.0])); // 1366x768 at 125%
        assert!(w < 1093.0 && h < 614.0);
    }
}
