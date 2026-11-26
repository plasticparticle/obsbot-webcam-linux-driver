//! Live camera preview: a capture thread streams MJPEG frames from the
//! camera, decodes them and hands RGBA images to the UI thread.

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use std::io::Read;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};

enum PreviewSource {
    Camera,
    /// Finished frames from the effects worker (see `read_stream_frame`).
    Stream(Box<dyn Read + Send>),
    Paused,
}

pub struct Preview {
    rx: Receiver<ColorImage>,
    texture: Option<TextureHandle>,
    connected: bool,
    command_tx: Sender<PreviewSource>,
}

impl Preview {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::sync_channel::<ColorImage>(1);
        let (command_tx, command_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("camera-preview".into())
            .spawn(move || capture_loop(tx, command_rx))
            .expect("spawn preview thread");
        Preview {
            rx,
            texture: None,
            connected: false,
            command_tx,
        }
    }

    /// A preview with no capture thread, for exercising UI code in tests.
    #[cfg(test)]
    pub fn detached() -> Self {
        let (_tx, rx) = mpsc::sync_channel::<ColorImage>(1);
        let (command_tx, _command_rx) = mpsc::channel();
        Preview {
            rx,
            texture: None,
            connected: false,
            command_tx,
        }
    }

    pub fn pause(&mut self) {
        let _ = self.command_tx.send(PreviewSource::Paused);
        self.texture = None;
        self.connected = false;
    }

    pub fn show_camera(&mut self) {
        let _ = self.command_tx.send(PreviewSource::Camera);
        self.texture = None;
        self.connected = false;
    }

    /// Show frames produced by the effects worker instead of the camera.
    pub fn show_stream(&mut self, frames: Box<dyn Read + Send>) {
        let _ = self.command_tx.send(PreviewSource::Stream(frames));
        self.texture = None;
        self.connected = false;
    }

    /// Forget the GPU texture; call when the window closes (it belongs to it).
    pub fn forget_texture(&mut self) {
        self.texture = None;
    }

    /// Draw the latest frame. Returns the rectangle the picture occupies.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<egui::Rect> {
        loop {
            match self.rx.try_recv() {
                Ok(img) => {
                    self.connected = true;
                    match &mut self.texture {
                        Some(tex) => tex.set(img, TextureOptions::LINEAR),
                        None => {
                            self.texture = Some(ui.ctx().load_texture(
                                "preview",
                                img,
                                TextureOptions::LINEAR,
                            ));
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.connected = false;
                    break;
                }
            }
        }
        match &self.texture {
            Some(tex) => {
                let avail = ui.available_size();
                let tex_size = tex.size_vec2();
                let scale = (avail.x / tex_size.x).min(avail.y / tex_size.y);
                let size = tex_size * scale;
                let area = ui.available_rect_before_wrap();
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new((tex.id(), size)));
                });
                Some(egui::Rect::from_center_size(area.center(), size))
            }
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label("waiting for video…");
                });
                None
            }
        }
    }
}

fn capture_loop(tx: mpsc::SyncSender<ColorImage>, command_rx: Receiver<PreviewSource>) {
    use v4l::buffer::Type;
    use v4l::io::mmap::Stream;
    use v4l::io::traits::CaptureStream;
    use v4l::video::capture::Parameters;
    use v4l::video::Capture;
    use v4l::FourCC;

    let mut source = PreviewSource::Camera;
    loop {
        while let Ok(next) = command_rx.try_recv() {
            source = next;
        }
        if matches!(source, PreviewSource::Stream(_)) {
            // Take the reader; when the worker exits the stream ends and we
            // idle until told what to show next.
            let taken = std::mem::replace(&mut source, PreviewSource::Paused);
            if let PreviewSource::Stream(mut frames) = taken {
                while let Some(image) = read_stream_frame(&mut frames) {
                    let _ = tx.try_send(image);
                    if let Ok(next) = command_rx.try_recv() {
                        source = next;
                        break;
                    }
                }
            }
            continue;
        }
        let path = match &source {
            PreviewSource::Camera => {
                let Some(path) = find_obsbot_node() else {
                    match command_rx.recv_timeout(std::time::Duration::from_secs(1)) {
                        Ok(next) => source = next,
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    continue;
                };
                path
            }
            PreviewSource::Stream(_) => continue,
            PreviewSource::Paused => {
                match command_rx.recv() {
                    Ok(next) => source = next,
                    Err(_) => return,
                }
                continue;
            }
        };
        let run = (|| -> Result<Option<PreviewSource>, Box<dyn std::error::Error>> {
            let dev = v4l::Device::with_path(&path)?;
            let mut fmt = dev.format()?;
            fmt.width = 1280;
            fmt.height = 720;
            fmt.fourcc = FourCC::new(b"MJPG");
            let fmt = dev.set_format(&fmt)?;
            let _ = dev.set_params(&Parameters::with_fps(30));
            let mut stream = Stream::with_buffers(&dev, Type::VideoCapture, 3)?;
            loop {
                if let Ok(next) = command_rx.try_recv() {
                    return Ok(Some(next));
                }
                let (buf, meta) = stream.next()?;
                if meta.bytesused == 0 {
                    continue;
                }
                let Some(color) = decode_frame(
                    &buf[..meta.bytesused as usize],
                    fmt.fourcc.repr,
                    fmt.width as usize,
                    fmt.height as usize,
                    fmt.stride as usize,
                ) else {
                    continue;
                };
                // Drop the frame if the UI hasn't consumed the previous one.
                let _ = tx.try_send(color);
            }
        })();
        match run {
            Ok(Some(next)) => source = next,
            Ok(None) => {}
            Err(_) => match command_rx.recv_timeout(std::time::Duration::from_millis(500)) {
                Ok(next) => source = next,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            },
        }
    }
}

/// Read one worker frame: `b"OBSF"`, little-endian u32 width and height, RGB bytes.
/// Returns `None` at end of stream or on a malformed header.
fn read_stream_frame(frames: &mut impl Read) -> Option<ColorImage> {
    let mut header = [0u8; 12];
    frames.read_exact(&mut header).ok()?;
    if &header[..4] != b"OBSF" {
        return None;
    }
    let width = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    let height = u32::from_le_bytes([header[8], header[9], header[10], header[11]]) as usize;
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return None;
    }
    let mut rgb = vec![0u8; width * height * 3];
    frames.read_exact(&mut rgb).ok()?;
    Some(ColorImage::from_rgb([width, height], &rgb))
}

fn decode_frame(
    bytes: &[u8],
    format: [u8; 4],
    width: usize,
    height: usize,
    stride: usize,
) -> Option<ColorImage> {
    if format == *b"MJPG" {
        let image = image::load_from_memory(bytes).ok()?.to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        return Some(ColorImage::from_rgba_unmultiplied(size, image.as_raw()));
    }

    let channels = if matches!(&format, b"RGB3" | b"BGR3") {
        3
    } else if format == *b"YUYV" {
        2
    } else {
        return None;
    };
    let row_stride = stride.max(width * channels);
    if bytes.len() < row_stride * height {
        return None;
    }
    let mut rgb = Vec::with_capacity(width * height * 3);
    for row in bytes.chunks(row_stride).take(height) {
        match &format {
            b"RGB3" => rgb.extend_from_slice(&row[..width * 3]),
            b"BGR3" => {
                for pixel in row[..width * 3].as_chunks::<3>().0 {
                    rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
                }
            }
            b"YUYV" => {
                for pair in row[..width * 2].as_chunks::<4>().0 {
                    let y0 = pair[0] as i32;
                    let u = pair[1] as i32;
                    let y1 = pair[2] as i32;
                    let v = pair[3] as i32;
                    rgb.extend_from_slice(&yuv_to_rgb(y0, u, v));
                    rgb.extend_from_slice(&yuv_to_rgb(y1, u, v));
                }
            }
            _ => return None,
        }
    }
    Some(ColorImage::from_rgb([width, height], &rgb))
}

fn yuv_to_rgb(y: i32, u: i32, v: i32) -> [u8; 3] {
    let c = y - 16;
    let d = u - 128;
    let e = v - 128;
    let red = (298 * c + 409 * e + 128) >> 8;
    let green = (298 * c - 100 * d - 208 * e + 128) >> 8;
    let blue = (298 * c + 516 * d + 128) >> 8;
    [
        red.clamp(0, 255) as u8,
        green.clamp(0, 255) as u8,
        blue.clamp(0, 255) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_bgr_virtual_camera_frame() {
        let image = decode_frame(&[30, 20, 10, 60, 50, 40], *b"BGR3", 2, 1, 6).unwrap();
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(10, 20, 30));
        assert_eq!(image.pixels[1], egui::Color32::from_rgb(40, 50, 60));
    }

    #[test]
    fn reads_worker_frames_until_the_stream_ends() {
        let mut bytes = Vec::new();
        for shade in [10u8, 200] {
            bytes.extend_from_slice(b"OBSF");
            bytes.extend_from_slice(&2u32.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            bytes.extend_from_slice(&[shade, 0, 0, 0, shade, 0]);
        }
        let mut stream = std::io::Cursor::new(bytes);
        let first = read_stream_frame(&mut stream).unwrap();
        assert_eq!(first.size, [2, 1]);
        assert_eq!(first.pixels[0], egui::Color32::from_rgb(10, 0, 0));
        let second = read_stream_frame(&mut stream).unwrap();
        assert_eq!(second.pixels[1], egui::Color32::from_rgb(0, 200, 0));
        assert!(read_stream_frame(&mut stream).is_none());
    }

    #[test]
    fn rejects_a_stream_with_a_bad_header() {
        let mut stream = std::io::Cursor::new(b"JUNKxxxxyyyyzzzz".to_vec());
        assert!(read_stream_frame(&mut stream).is_none());
    }

    #[test]
    fn decodes_black_yuyv_pair() {
        let image = decode_frame(&[16, 128, 16, 128], *b"YUYV", 2, 1, 4).unwrap();
        assert_eq!(image.pixels, vec![egui::Color32::BLACK; 2]);
    }
}

/// The camera's first video node streams; find it by card name.
fn find_obsbot_node() -> Option<std::path::PathBuf> {
    let mut nodes: Vec<_> = std::fs::read_dir("/dev")
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("video"))
        })
        .collect();
    nodes.sort();
    for path in nodes {
        if let Ok(dev) = obsbot_core::Device::open(&path) {
            if dev.card().contains("OBSBOT") && !dev.card().contains("Virtual") {
                return Some(path);
            }
        }
    }
    None
}
