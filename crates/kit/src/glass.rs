//! Liquid glass with a real backdrop blur, the way Apple and Windows Aero do it: a glass pane shows whatever was
//! drawn under it (the app's own content), blurred, lightly tinted, with light catching its top edge.
//!
//! How: a pane is a paint callback in egui's paint list. `Compositor::render` draws the list in pieces: everything
//! before a pane into a scene texture, then a dual-Kawase blur of that scene, then the pane sampling the blur, then
//! the rest on top. Backgrounds stay plain (the user rejected colour washes); content stays solid; only chrome over
//! content is glass. High contrast never uses glass.

use std::sync::Arc;

use egui::epaint::{ClippedPrimitive, Primitive};
use egui::{Color32, CornerRadius, Painter, Rect};
use egui_wgpu::ScreenDescriptor;

use crate::theme::Tokens;

/// The scene and blur textures' format. egui then works in gamma space, as it does everywhere else.
pub const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// How many times the scene is halved for the blur. Each level roughly doubles its reach.
const BLUR_LEVELS: usize = 4;
const BLUR_OFFSET: f32 = 1.6;

/// What a glass pane looks like. Made by `pane`; read by the compositor.
#[derive(Clone, Copy, Debug)]
pub struct GlassPane {
    pub radius: f32,
    /// The tint laid over the blur (alpha is how much).
    pub tint: Color32,
    pub dark: bool,
    /// How much brighter the colours get (1 keeps them, Apple uses about 1.3).
    pub saturation: f32,
    /// Whether it floats (and casts a soft shadow).
    pub floating: bool,
}

/// How see-through a pane is: chrome with words on it needs a little more tint than a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    /// Floating buttons, small chips.
    Light,
    /// Bars, sidebars, sheets: words sit on them.
    Regular,
}

/// Lays a glass pane over what's been painted so far in `rect`, with a soft shadow under it when it floats. Paint its
/// contents after this call. In high contrast it's a solid surface with an outline instead.
pub fn pane(painter: &Painter, rect: Rect, radius: f32, th: &Tokens, weight: Weight, floating: bool) {
    let round = CornerRadius::same(radius.round().clamp(0.0, 255.0) as u8);
    if th.high_contrast {
        painter.rect_filled(rect, round, th.surface);
        painter.rect_stroke(rect, round, egui::Stroke::new(1.5, th.border), egui::StrokeKind::Inside);
        return;
    }
    let amount = match weight {
        Weight::Light => 0.16,
        Weight::Regular => 0.30,
    } + if th.dark { 0.12 } else { 0.0 };
    let base = th.surface;
    let tint = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), (amount * 255.0) as u8);
    painter.add(egui::PaintCallback {
        rect,
        callback: Arc::new(GlassPane { radius, tint, dark: th.dark, saturation: 1.3, floating }),
    });
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurUniform {
    half_pixel: [f32; 2],
    offset: f32,
    _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PaneUniform {
    rect: [f32; 4],
    screen: [f32; 4],
    tint: [f32; 4],
    look: [f32; 4],
    shadow: [f32; 4],
}

struct Level {
    view: wgpu::TextureView,
    size: [u32; 2],
}

struct Targets {
    size: [u32; 2],
    scene: wgpu::Texture,
    scene_view: wgpu::TextureView,
    levels: Vec<Level>,
}

/// Draws egui's paint list with real glass. One per window (or per offscreen picture).
pub struct Compositor {
    egui: egui_wgpu::Renderer,
    sampler: wgpu::Sampler,
    layout: wgpu::BindGroupLayout,
    down: wgpu::RenderPipeline,
    up: wgpu::RenderPipeline,
    pane: wgpu::RenderPipeline,
    copy: wgpu::RenderPipeline,
    output_format: wgpu::TextureFormat,
    targets: Option<Targets>,
}

fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    vertex: &str,
    fragment: &str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(layout),
        vertex: wgpu::VertexState { module, entry_point: Some(vertex), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(fragment),
            targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl Compositor {
    /// `output_format` is what `present` draws into (the window's surface); offscreen pictures use `SCENE_FORMAT`.
    pub fn new(device: &wgpu::Device, output_format: wgpu::TextureFormat) -> Self {
        let egui = egui_wgpu::Renderer::new(device, SCENE_FORMAT, egui_wgpu::RendererOptions::default());
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("glass"),
            source: wgpu::ShaderSource::Wgsl(include_str!("glass.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glass"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glass"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let premultiplied = Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glass"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            egui,
            down: pipeline(device, &pipeline_layout, &module, "vs_full", "fs_down", SCENE_FORMAT, None),
            up: pipeline(device, &pipeline_layout, &module, "vs_full", "fs_up", SCENE_FORMAT, None),
            pane: pipeline(device, &pipeline_layout, &module, "vs_pane", "fs_pane", SCENE_FORMAT, premultiplied),
            copy: pipeline(device, &pipeline_layout, &module, "vs_full", "fs_copy", output_format, None),
            sampler,
            layout,
            output_format,
            targets: None,
        }
    }

    pub fn output_format(&self) -> wgpu::TextureFormat {
        self.output_format
    }

    fn ensure_targets(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = |label: &str, s: [u32; 2]| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: s[0].max(1), height: s[1].max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: SCENE_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let scene = texture("scene", size);
        let scene_view = scene.create_view(&Default::default());
        let mut levels = Vec::new();
        let mut s = size;
        for _ in 0..BLUR_LEVELS {
            s = [(s[0] / 2).max(1), (s[1] / 2).max(1)];
            let t = texture("blur", s);
            levels.push(Level { view: t.create_view(&Default::default()), size: s });
        }
        self.targets = Some(Targets { size, scene, scene_view, levels });
    }

    fn bind(&self, device: &wgpu::Device, view: &wgpu::TextureView, uniform: &[u8]) -> wgpu::BindGroup {
        use wgpu::util::DeviceExt;
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("glass"),
            contents: uniform,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glass"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: buffer.as_entire_binding() },
            ],
        })
    }

    fn fullscreen(
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        pipeline: &wgpu::RenderPipeline,
        group: &wgpu::BindGroup,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("glass pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Blurs the scene into the first blur level: down through every level, then back up.
    fn blur(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        let Some(t) = &self.targets else { return };
        let uniform = |src: [u32; 2]| BlurUniform {
            half_pixel: [0.5 / src[0] as f32, 0.5 / src[1] as f32],
            offset: BLUR_OFFSET,
            _pad: 0.0,
        };
        let mut src_view = &t.scene_view;
        let mut src_size = t.size;
        for level in &t.levels {
            let group = self.bind(device, src_view, bytemuck::bytes_of(&uniform(src_size)));
            Self::fullscreen(encoder, &level.view, &self.down, &group);
            src_view = &level.view;
            src_size = level.size;
        }
        for i in (0..t.levels.len() - 1).rev() {
            let src = &t.levels[i + 1];
            let group = self.bind(device, &src.view, bytemuck::bytes_of(&uniform(src.size)));
            Self::fullscreen(encoder, &t.levels[i].view, &self.up, &group);
        }
    }

    fn draw_pane(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, pane: &GlassPane, rect: Rect, clip: Rect, ppp: f32) {
        let Some(t) = &self.targets else { return };
        let [w, h] = t.size;
        // The tint's own colour (Color32 stores it multiplied by its alpha) and how much of it to mix in.
        let c = pane.tint.to_srgba_unmultiplied().map(|v| f32::from(v) / 255.0);
        let uniform = PaneUniform {
            rect: [rect.min.x * ppp, rect.min.y * ppp, rect.max.x * ppp, rect.max.y * ppp],
            screen: [w as f32, h as f32, pane.radius * ppp, if pane.dark { 1.0 } else { 0.0 }],
            tint: [c[0], c[1], c[2], c[3]],
            look: [pane.saturation, if pane.dark { 0.28 } else { 0.85 }, if pane.dark { 0.16 } else { 0.10 }, ppp],
            shadow: if pane.floating { [4.0 * ppp, 18.0 * ppp, if pane.dark { 0.45 } else { 0.16 }, 0.0] } else { [0.0; 4] },
        };
        let group = self.bind(device, &t.levels[0].view, bytemuck::bytes_of(&uniform));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("glass pane"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.scene_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        let clip = clip.intersect(rect.expand(if pane.floating { 26.0 } else { 2.0 }));
        let x = (clip.min.x * ppp).floor().clamp(0.0, w as f32) as u32;
        let y = (clip.min.y * ppp).floor().clamp(0.0, h as f32) as u32;
        let x1 = (clip.max.x * ppp).ceil().clamp(0.0, w as f32) as u32;
        let y1 = (clip.max.y * ppp).ceil().clamp(0.0, h as f32) as u32;
        if x1 <= x || y1 <= y {
            return;
        }
        pass.set_scissor_rect(x, y, x1 - x, y1 - y);
        pass.set_pipeline(&self.pane);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..6, 0..1);
    }

    fn draw_segment(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prims: &[ClippedPrimitive],
        screen: &ScreenDescriptor,
        clear: Option<Color32>,
    ) -> wgpu::CommandEncoder {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("glass segment") });
        let extra = self.egui.update_buffers(device, queue, &mut encoder, prims, screen);
        if !extra.is_empty() {
            queue.submit(extra);
        }
        let t = self.targets.as_ref().expect("targets");
        let load = match clear {
            Some(c) => {
                let f = c.to_normalized_gamma_f32();
                wgpu::LoadOp::Clear(wgpu::Color { r: f[0] as f64, g: f[1] as f64, b: f[2] as f64, a: f[3] as f64 })
            }
            None => wgpu::LoadOp::Load,
        };
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.scene_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            self.egui.render(&mut pass.forget_lifetime(), prims, screen);
        }
        encoder
    }

    /// Draws a frame into the scene texture: egui's paint list with every glass pane composited for real.
    pub fn render_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: [u32; 2],
        pixels_per_point: f32,
        prims: &[ClippedPrimitive],
        mut textures: egui::TexturesDelta,
        background: Color32,
    ) {
        for (id, deltas) in &textures.set {
            for delta in deltas {
                self.egui.update_texture(device, queue, *id, delta);
            }
        }
        self.ensure_targets(device, size);
        let screen = ScreenDescriptor { size_in_pixels: size, pixels_per_point };
        let mut start = 0;
        let mut first = true;
        loop {
            let end = prims[start..]
                .iter()
                .position(|p| matches!(&p.primitive, Primitive::Callback(cb) if cb.callback.downcast_ref::<GlassPane>().is_some()))
                .map_or(prims.len(), |i| start + i);
            let mut encoder = self.draw_segment(device, queue, &prims[start..end], &screen, first.then_some(background));
            first = false;
            if end == prims.len() {
                queue.submit([encoder.finish()]);
                break;
            }
            let job = &prims[end];
            if let Primitive::Callback(cb) = &job.primitive {
                if let Some(pane) = cb.callback.downcast_ref::<GlassPane>() {
                    self.blur(device, &mut encoder);
                    self.draw_pane(device, &mut encoder, pane, cb.rect, job.clip_rect, pixels_per_point);
                }
            }
            queue.submit([encoder.finish()]);
            start = end + 1;
        }
        for id in &textures.free {
            self.egui.free_texture(id);
        }
        textures.clear();
    }

    /// Copies the scene to the window's surface.
    pub fn present(&self, device: &wgpu::Device, queue: &wgpu::Queue, output: &wgpu::TextureView) {
        let Some(t) = &self.targets else { return };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("present") });
        let group = self.bind(device, &t.scene_view, bytemuck::bytes_of(&BlurUniform { half_pixel: [0.0; 2], offset: 0.0, _pad: 0.0 }));
        Self::fullscreen(&mut encoder, output, &self.copy, &group);
        queue.submit([encoder.finish()]);
    }

    /// Reads the scene back as RGBA pixels (for screenshots and tests).
    pub fn read_scene(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Option<(u32, u32, Vec<u8>)> {
        let t = self.targets.as_ref()?;
        let [w, h] = t.size;
        let row = (w * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("read"),
            size: u64::from(row * h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("read") });
        encoder.copy_texture_to_buffer(
            t.scene.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let data = slice.get_mapped_range().ok()?;
        let mut pixels = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let start = (y * row) as usize;
            pixels.extend_from_slice(&data[start..start + (w * 4) as usize]);
        }
        Some((w, h, pixels))
    }
}
