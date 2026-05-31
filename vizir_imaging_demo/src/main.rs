// Copyright 2025 the VizIR Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Native demo using `winit`, `wgpu`, `imaging`, and `imaging_vello_hybrid`.

use std::sync::Arc;

use imaging::kurbo::{Affine, Rect, Vec2};
use imaging::peniko::Color;
use imaging::{Painter, record};
use imaging_vello_hybrid::{VelloHybridRenderer, wgpu};
use vizir_backend_imaging::{ImagingScene, ParleyTextPainter};
use vizir_demo_scenarios::{StaticScenario, static_scenarios};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SurfaceBytes {
    Rgba,
    Bgra,
}

struct GpuState {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    bytes: SurfaceBytes,
    renderer: VelloHybridRenderer,
}

struct App {
    window: Option<Arc<Window>>,
    window_id: Option<WindowId>,
    gpu: Option<GpuState>,
    imaging_scene: ImagingScene,
    text: ParleyTextPainter,
    view: Rect,
    chart_index: usize,
    content_chart_index: Option<usize>,
    presented_chart_index: Option<usize>,
    dirty: bool,
    redraw_pending: bool,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            window_id: None,
            gpu: None,
            imaging_scene: ImagingScene::new(),
            text: ParleyTextPainter::new(),
            view: Rect::new(0.0, 0.0, 1.0, 1.0),
            chart_index: 0,
            content_chart_index: None,
            presented_chart_index: None,
            dirty: true,
            redraw_pending: true,
        }
    }

    fn scenario_at(chart_index: usize) -> StaticScenario {
        static_scenarios()
            .get(chart_index)
            .copied()
            .unwrap_or_else(|| static_scenarios()[0])
    }

    fn set_chart(&mut self, chart_index: usize) {
        self.chart_index = chart_index % static_scenarios().len();
        self.dirty = true;
        self.request_redraw();
    }

    fn next_chart(&mut self) {
        self.set_chart(self.chart_index.wrapping_add(1));
    }

    fn prev_chart(&mut self) {
        let len = static_scenarios().len();
        let index = if self.chart_index == 0 {
            len.saturating_sub(1)
        } else {
            self.chart_index - 1
        };
        self.set_chart(index);
    }

    fn ensure_content(&mut self, chart_index: usize) {
        if !self.dirty && self.content_chart_index == Some(chart_index) {
            return;
        }
        let frame = Self::scenario_at(chart_index).build();
        self.view = frame.view;
        self.imaging_scene.clear();
        self.imaging_scene.apply_diffs(&frame.diffs);
        self.content_chart_index = Some(chart_index);
        self.dirty = false;
    }

    fn request_redraw(&mut self) {
        self.redraw_pending = true;
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn update_window_title(&self, chart_index: usize) {
        let Some(window) = &self.window else {
            return;
        };
        window.set_title(&format!(
            "vizir_imaging_demo - {}",
            Self::scenario_at(chart_index).title
        ));
    }

    fn recording(&mut self, chart_index: usize, width: u32, height: u32) -> record::Scene {
        self.ensure_content(chart_index);
        let width_f = f64::from(width.max(1));
        let height_f = f64::from(height.max(1));
        let transform = fit_transform(self.view, width_f, height_f);

        let mut scene = record::Scene::new();
        {
            let mut painter = Painter::new(&mut scene);
            painter.fill_rect(
                Rect::new(0.0, 0.0, width_f, height_f),
                Color::from_rgb8(0xff, 0xff, 0xff),
            );
        }
        self.imaging_scene
            .paint_transformed_with_text(&mut scene, transform, &mut self.text);
        scene
    }

    fn render(&mut self) {
        if self.window.is_none() {
            return;
        }
        let Some(mut gpu) = self.gpu.take() else {
            return;
        };

        let chart_index = self.chart_index;
        let width = gpu.config.width.max(1);
        let height = gpu.config.height.max(1);
        let recording = self.recording(chart_index, width, height);
        let render_width = u16::try_from(width).unwrap_or(u16::MAX);
        let render_height = u16::try_from(height).unwrap_or(u16::MAX);

        let native = gpu
            .renderer
            .encode_scene(&recording, render_width, render_height)
            .expect("encode imaging scene");
        let image = gpu
            .renderer
            .render(&native, render_width, render_height)
            .expect("render imaging scene");

        let (frame, reconfigure_after_present) = match gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Timeout => {
                self.gpu = Some(gpu);
                self.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Occluded => {
                self.redraw_pending = false;
                self.gpu = Some(gpu);
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Validation => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                self.gpu = Some(gpu);
                self.request_redraw();
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                let Some(window) = &self.window else {
                    self.gpu = Some(gpu);
                    return;
                };
                gpu.surface = gpu
                    .instance
                    .create_surface(window.clone())
                    .expect("recreate surface");
                gpu.surface.configure(&gpu.device, &gpu.config);
                self.gpu = Some(gpu);
                self.request_redraw();
                return;
            }
        };

        let mut data = image.data;
        if gpu.bytes == SurfaceBytes::Bgra {
            for pixel in data.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        let (upload, bytes_per_row) = aligned_upload_rows(&data, width, height);

        submit_surface_upload(&gpu, &frame.texture, &upload, bytes_per_row, width, height);
        frame.present();
        if reconfigure_after_present {
            gpu.surface.configure(&gpu.device, &gpu.config);
        }
        self.presented_chart_index = Some(chart_index);
        self.redraw_pending = false;
        self.gpu = Some(gpu);
        self.update_window_title(chart_index);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("vizir_imaging_demo")
                        .with_inner_size(PhysicalSize::new(1400_u32, 900_u32)),
                )
                .expect("create window"),
        );
        let size = window.inner_size();
        let gpu = pollster::block_on(init_gpu(window.clone(), size));

        self.window_id = Some(window.id());
        self.window = Some(window);
        self.gpu = Some(gpu);
        self.request_redraw();
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.redraw_pending
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if Some(id) != self.window_id {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                if width == 0 || height == 0 {
                    return;
                }
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.config.width = width;
                    gpu.config.height = height;
                    gpu.surface.configure(&gpu.device, &gpu.config);
                }
                self.request_redraw();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => event_loop.exit(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::ArrowRight),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                self.next_chart();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::ArrowLeft),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                self.prev_chart();
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }
}

async fn init_gpu(window: Arc<Window>, size: PhysicalSize<u32>) -> GpuState {
    let instance = wgpu::Instance::default();
    let surface = instance.create_surface(window).expect("create surface");
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await
        .expect("request adapter");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("vizir_imaging_demo device"),
            required_features: wgpu::Features::empty(),
            ..Default::default()
        })
        .await
        .expect("request device");

    let caps = surface.get_capabilities(&adapter);
    let (format, bytes) = choose_surface_format(&caps.formats).expect("RGBA/BGRA surface format");
    let present_mode = if caps.present_modes.contains(&wgpu::PresentMode::AutoVsync) {
        wgpu::PresentMode::AutoVsync
    } else {
        wgpu::PresentMode::Fifo
    };
    let alpha_mode = caps
        .alpha_modes
        .first()
        .copied()
        .unwrap_or(wgpu::CompositeAlphaMode::Auto);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode,
        alpha_mode,
        view_formats: Vec::new(),
        desired_maximum_frame_latency: 1,
    };
    surface.configure(&device, &config);

    let renderer = VelloHybridRenderer::new(device.clone(), queue.clone());
    GpuState {
        instance,
        surface,
        device,
        queue,
        config,
        bytes,
        renderer,
    }
}

fn choose_surface_format(
    formats: &[wgpu::TextureFormat],
) -> Option<(wgpu::TextureFormat, SurfaceBytes)> {
    let preferred = [
        (wgpu::TextureFormat::Rgba8Unorm, SurfaceBytes::Rgba),
        (wgpu::TextureFormat::Rgba8UnormSrgb, SurfaceBytes::Rgba),
        (wgpu::TextureFormat::Bgra8Unorm, SurfaceBytes::Bgra),
        (wgpu::TextureFormat::Bgra8UnormSrgb, SurfaceBytes::Bgra),
    ];

    preferred
        .into_iter()
        .find(|(format, _bytes)| formats.contains(format))
}

fn fit_transform(view: Rect, width: f64, height: f64) -> Affine {
    let view_w = view.width().max(1.0);
    let view_h = view.height().max(1.0);
    let scale = (width / view_w).min(height / view_h).max(1.0e-6);
    let content_w = view_w * scale;
    let content_h = view_h * scale;
    let pad_x = 0.5 * (width - content_w).max(0.0);
    let pad_y = 0.5 * (height - content_h).max(0.0);

    Affine::translate(Vec2::new(pad_x, pad_y))
        * Affine::scale(scale)
        * Affine::translate(Vec2::new(-view.x0, -view.y0))
}

fn submit_surface_upload(
    gpu: &GpuState,
    texture: &wgpu::Texture,
    upload: &[u8],
    bytes_per_row: u32,
    width: u32,
    height: u32,
) {
    let upload_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("vizir_imaging_demo surface upload"),
        size: u64::try_from(upload.len()).expect("surface upload size should fit in u64"),
        usage: wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: true,
    });
    {
        let mut mapped = upload_buffer.slice(..).get_mapped_range_mut();
        mapped.copy_from_slice(upload);
    }
    upload_buffer.unmap();

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("vizir_imaging_demo surface upload"),
        });
    encoder.copy_buffer_to_texture(
        wgpu::TexelCopyBufferInfo {
            buffer: &upload_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
}

fn aligned_upload_rows(data: &[u8], width: u32, height: u32) -> (Vec<u8>, u32) {
    let width_bytes = width.saturating_mul(4);
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let bytes_per_row = width_bytes.div_ceil(align) * align;
    if bytes_per_row == width_bytes {
        return (data.to_vec(), bytes_per_row);
    }

    let height_usize = usize::try_from(height).expect("surface height should fit in usize");
    let width_bytes_usize =
        usize::try_from(width_bytes).expect("surface row bytes should fit in usize");
    let bytes_per_row_usize =
        usize::try_from(bytes_per_row).expect("padded row bytes should fit in usize");
    let mut upload = vec![0; bytes_per_row_usize * height_usize];

    for (src, dst) in data
        .chunks_exact(width_bytes_usize)
        .zip(upload.chunks_exact_mut(bytes_per_row_usize))
    {
        dst[..width_bytes_usize].copy_from_slice(src);
    }

    (upload, bytes_per_row)
}

fn main() {
    let event_loop = EventLoop::new().expect("event loop");
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("run app");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_app_has_pending_redraw_and_content_builds() {
        let mut app = App::new();
        assert!(app.redraw_pending);

        app.ensure_content(0);

        assert!(!app.imaging_scene.is_empty());
        assert!(!app.dirty);
        assert_eq!(app.content_chart_index, Some(0));
        assert_eq!(app.presented_chart_index, None);
    }

    #[test]
    fn chart_switch_marks_content_dirty_without_advancing_presentation() {
        let mut app = App::new();
        app.ensure_content(0);
        app.presented_chart_index = Some(0);
        app.redraw_pending = false;

        app.next_chart();

        assert!(app.dirty);
        assert!(app.redraw_pending);
        assert_eq!(app.content_chart_index, Some(0));
        assert_eq!(app.presented_chart_index, Some(0));
        assert_eq!(
            App::scenario_at(app.chart_index).title,
            static_scenarios()[1].title
        );
    }

    #[test]
    fn ensure_content_replaces_retained_scene_for_requested_chart() {
        let mut app = App::new();
        app.ensure_content(0);
        let first_len = app.imaging_scene.len();

        app.next_chart();
        app.ensure_content(1);

        assert!(!app.imaging_scene.is_empty());
        assert_ne!(app.imaging_scene.len(), 0);
        assert_eq!(app.content_chart_index, Some(1));
        assert_eq!(app.presented_chart_index, None);
        assert_ne!(first_len, 0);
    }

    #[test]
    fn upload_rows_are_aligned_for_surface_copy() {
        let width = 3;
        let height = 2;
        let data: Vec<u8> = (0..width * height * 4)
            .map(|byte| u8::try_from(byte).expect("test byte fits"))
            .collect();

        let (upload, bytes_per_row) = aligned_upload_rows(&data, width, height);

        assert_eq!(bytes_per_row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
        assert_eq!(
            upload.len(),
            usize::try_from(bytes_per_row * height).expect("test size fits")
        );
        assert_eq!(&upload[..12], &data[..12]);
        let second_row_start = usize::try_from(bytes_per_row).expect("test row fits");
        assert_eq!(
            &upload[second_row_start..second_row_start + 12],
            &data[12..]
        );
    }
}
