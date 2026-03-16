mod app;
mod egui_ui;
mod input;
mod menu;
mod renderer;

use std::sync::{Arc, Mutex};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use winit::event_loop::EventLoop;

use app::VibeApp;
use menu::{AppMenu, MenuAction};

fn build_audio_stream() -> (Arc<Mutex<Vec<f32>>>, f32, Option<Stream>) {
    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(4096)));
    let buf2 = buffer.clone();
    let mut sample_rate = 44100.0f32;

    let stream = cpal::default_host()
        .default_output_device()
        .and_then(|dev| {
            let config = dev.default_output_config().ok()?;
            sample_rate = config.sample_rate().0 as f32;
            let stream = dev.build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| {
                    let mut buf = buf2.lock().unwrap();
                    let mut idx = 0;
                    for s in data.iter_mut() {
                        *s = if idx < buf.len() { let v = buf[idx]; idx += 1; v } else { 0.0 };
                    }
                    if idx < buf.len() { buf.drain(0..idx); } else { buf.clear(); }
                },
                |e| eprintln!("Audio error: {e}"),
                None,
            ).ok()?;
            let _ = stream.play();
            Some(stream)
        });

    (buffer, sample_rate, stream)
}

pub fn launch_frontend(initial_rom: Option<String>) {
    #[cfg(target_os = "linux")]
    gtk::init().expect("GTK init failed");

    let event_loop = EventLoop::<MenuAction>::with_user_event().build().unwrap();
    let proxy = event_loop.create_proxy();
    let menu = AppMenu::build(proxy);
    let (audio_buf, sample_rate, stream) = build_audio_stream();
    let gilrs = gilrs::Gilrs::new().expect("Failed to init gilrs");
    let proxy2 = event_loop.create_proxy();
    let mut app = VibeApp::new(initial_rom, audio_buf, sample_rate, stream, gilrs, menu, proxy2);
    event_loop.run_app(&mut app).unwrap();
}
