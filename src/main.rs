use eframe::egui;
use gif::{Encoder as GifEncoder, Frame as GifFrame, Repeat};
use image::GenericImageView;
use std::fs::File;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::{fs, thread};
use tempfile::TempDir;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "Rust GIF Maker",
        options,
        Box::new(|_cc| Ok(Box::new(GifMakerApp::new()))),
    )
}

struct GifMakerApp {
    input_path: Option<PathBuf>,
    output_path: String,
    duration: String, // "00:00:05"

    is_running: bool,
    status: Arc<Mutex<String>>,
    progress: Arc<AtomicUsize>,
    cancel_flag: Arc<AtomicBool>,
}

impl GifMakerApp {
    fn new() -> Self {
        Self {
            input_path: None,
            output_path: "output.gif".to_string(),
            duration: "00:00:05".to_string(),
            is_running: false,
            status: Arc::new(Mutex::new("Idle".to_string())),
            progress: Arc::new(AtomicUsize::new(0)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    fn spawn_worker(&mut self) {
        if self.input_path.is_none() {
            let mut s = self.status.lock().unwrap();
            *s = "Please select an input video file.".to_string();
            return;
        }

        let input = self.input_path.as_ref().unwrap().clone();
        let duration = self.duration.clone();
        let output_path = self.output_path.clone();
        let status = Arc::clone(&self.status);
        let progress = Arc::clone(&self.progress);
        let cancel = Arc::clone(&self.cancel_flag);

        // set output path to same folder as input
        let output_path = if input.is_file() {
            let mut path = input.clone();
            path.set_extension("gif");
            path.to_string_lossy().to_string()
        } else {
            output_path
        };

        self.is_running = true;
        {
            let mut s = status.lock().unwrap();
            *s = "Processing...".to_string();
        }
        progress.store(0, Ordering::SeqCst);
        cancel.store(false, Ordering::SeqCst);

        thread::spawn(move || {
            if let Err(e) = worker_process(
                input,
                output_path,
                duration,
                &status,
                &progress,
                &cancel,
            ) {
                let mut s = status.lock().unwrap();
                *s = format!("Error: {}", e);
            }
            if !cancel.load(Ordering::SeqCst) {
                progress.store(100, Ordering::SeqCst);
            }
        });
    }
}

impl eframe::App for GifMakerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        use egui::{Slider};

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Rust GIF Maker");
            ui.separator();

            if ui.button("Select input video").clicked() {
                if let Some(f) = rfd::FileDialog::new()
                    .add_filter("Video", &["mp4", "mov", "mkv", "webm"])
                    .pick_file()
                {
                    self.input_path = Some(f);
                }
            }

            ui.label(format!(
                "Input: {}",
                self.input_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "Not selected".to_string())
            ));

            ui.horizontal(|ui| {
                ui.label("GIF Duration (HH:MM:SS):");
                ui.text_edit_singleline(&mut self.duration);
            });

            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.is_running, egui::Button::new("Start Conversion"))
                    .clicked()
                {
                    self.spawn_worker();
                }

                if ui
                    .add_enabled(self.is_running, egui::Button::new("Cancel"))
                    .clicked()
                {
                    self.cancel_flag.store(true, Ordering::SeqCst);
                    let mut s = self.status.lock().unwrap();
                    *s = "Cancelling...".to_string();
                    self.is_running = false;
                }
            });

            ui.add_space(8.0);

            let st = self.status.lock().unwrap().clone();
            ui.horizontal(|ui| {
                ui.label("Status:");
                ui.label(st);
            });

            let pct = self.progress.load(Ordering::SeqCst) as f32 / 100.0;
            ui.add(egui::ProgressBar::new(pct).show_percentage());
        });

        if self.progress.load(Ordering::SeqCst) >= 100 || self.cancel_flag.load(Ordering::SeqCst) {
            self.is_running = false;
        }

        ctx.request_repaint();
    }
}

fn worker_process(
    input: PathBuf,
    output_gif: String,
    duration: String,
    status: &Arc<Mutex<String>>,
    progress: &Arc<AtomicUsize>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let tmp = TempDir::new().map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let tmp_path = tmp.path();

    // ffmpeg extract frames
    {
        let mut s = status.lock().unwrap();
        *s = "Extracting frames...".to_string();
    }

    let out_pattern = tmp_path.join("frame_%05d.png");

    let status_cmd = Command::new("ffmpeg")
        .args(&[
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            &input.to_string_lossy(),
            "-t",
            &duration,
            "-vf",
            "fps=15",
            &out_pattern.to_string_lossy(),
            "-y",
        ])
        .status()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    if !status_cmd.success() {
        return Err("ffmpeg returned an error".to_string());
    }

    // collect frames
    let mut frames: Vec<PathBuf> = glob::glob(&format!("{}/frame_*.png", tmp_path.display()))
        .map_err(|e| format!("glob error: {}", e))?
        .filter_map(Result::ok)
        .collect();

    frames.sort();

    if frames.is_empty() {
        return Err("No frames found".to_string());
    }

    let first_img = image::open(&frames[0]).map_err(|e| format!("Failed to open frame: {}", e))?;
    let (w, h) = first_img.dimensions();

    let mut file = File::create(&output_gif).map_err(|e| format!("Failed to create output: {}", e))?;
    let mut encoder = GifEncoder::new(&mut file, w as u16, h as u16, &[]).map_err(|e| format!("GIF encoder failed: {}", e))?;
    let _ = encoder.set_repeat(Repeat::Infinite);

    let total = frames.len();
    for (i, p) in frames.into_iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            let mut s = status.lock().unwrap();
            *s = "Cancelled".to_string();
            return Ok(());
        }

        let mut img = image::open(&p).map_err(|e| format!("Failed to open frame: {}", e))?.to_rgba8();
        let delay_cs = (100u16 / 15).max(1);

        let mut gif_frame = GifFrame::from_rgba_speed(w as u16, h as u16, &mut img.into_raw(), 10);
        gif_frame.delay = delay_cs;

        encoder.write_frame(&gif_frame).map_err(|e| format!("Failed to write frame: {}", e))?;

        let pct = ((i + 1) * 100) / total;
        progress.store(pct, Ordering::SeqCst);
    }

    {
        let mut s = status.lock().unwrap();
        *s = format!("Completed: {}", output_gif);
    }
    progress.store(100, Ordering::SeqCst);

    Ok(())
}
