use eframe::egui;
use rfd::FileDialog;
use std::path::PathBuf;
use std::thread;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

mod converters;
use converters::Progress;

#[derive(PartialEq)]
enum ConversionType {
    Wind,
    Bpm,
    Clouds,
    Rgb,
    Text,
}

enum AppMessage {
    Progress(Progress),
    Finished,
}

struct CubeConvertApp {
    selected_tab: ConversionType,
    selected_path: Option<PathBuf>,
    is_folder: bool,
    is_converting: bool,
    rgb_color: [u8; 3],
    status_msg: String,

    // Progress state
    progress_current: usize,
    progress_total: usize,
    current_file: String,

    // Concurrency
    tx: crossbeam_channel::Sender<AppMessage>,
    rx: crossbeam_channel::Receiver<AppMessage>,
    cancel_flag: Arc<AtomicBool>,
}

impl Default for CubeConvertApp {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            selected_tab: ConversionType::Wind,
            selected_path: None,
            is_folder: false,
            is_converting: false,
            rgb_color: [255, 255, 255],
            status_msg: String::new(),
            progress_current: 0,
            progress_total: 0,
            current_file: String::new(),
            tx,
            rx,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl eframe::App for CubeConvertApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::Progress(p) => match p {
                    Progress::Init { total } => {
                        self.progress_total = total;
                        self.progress_current = 0;
                    }
                    Progress::Start { name } => {
                        self.current_file = name;
                    }
                    Progress::Done => {
                        self.progress_current += 1;
                    }
                    Progress::Error { name, error } => {
                        self.progress_current += 1;
                        self.status_msg = format!("Error in {}: {}", name, error);
                    }
                },
                AppMessage::Finished => {
                    self.is_converting = false;
                    self.current_file.clear();
                    if self.status_msg.is_empty() || self.status_msg == "Starting..." {
                        self.status_msg = if self.cancel_flag.load(Ordering::Relaxed) {
                            "Cancelled.".to_string()
                        } else {
                            "All done!".to_string()
                        };
                    }
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);

            ui.add_enabled_ui(!self.is_converting, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.selected_tab, ConversionType::Wind, "WIND");
                    ui.selectable_value(&mut self.selected_tab, ConversionType::Bpm, "BPM");
                    ui.selectable_value(&mut self.selected_tab, ConversionType::Clouds, "CLOUDS");
                    ui.selectable_value(&mut self.selected_tab, ConversionType::Rgb, "RGB");
                    ui.selectable_value(&mut self.selected_tab, ConversionType::Text, "TEXT");
                });
            });
            ui.separator();

            let desc = match self.selected_tab {
                ConversionType::Wind   => "Convert wind intensities (PDF) -> MP3",
                ConversionType::Bpm    => "Convert BPM data (PDF) -> MP3",
                ConversionType::Clouds => "Convert cloud images (PDF) -> scrolling MP4",
                ConversionType::Rgb    => "Convert RGB values (PDF) -> gradient MP4",
                ConversionType::Text   => "Convert text (PDF) -> scrolling text MP4",
            };
            ui.label(desc);
            ui.add_space(16.0);

            ui.add_enabled_ui(!self.is_converting, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("\u{1F4C4} Select File").clicked() {
                        if let Some(path) = FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() {
                            self.selected_path = Some(path);
                            self.is_folder = false;
                            self.status_msg.clear();
                        }
                    }
                    if ui.button("\u{1F4C1} Select Folder").clicked() {
                        if let Some(path) = FileDialog::new().pick_folder() {
                            self.selected_path = Some(path);
                            self.is_folder = true;
                            self.status_msg.clear();
                        }
                    }
                });
            });

            if let Some(path) = &self.selected_path {
                ui.label(format!("Selected: {}", path.display()));
            } else {
                ui.label("No file or folder selected.");
            }

            ui.add_space(10.0);

            if self.selected_tab == ConversionType::Text {
                ui.add_enabled_ui(!self.is_converting, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Text Color:");
                        ui.color_edit_button_srgb(&mut self.rgb_color);
                    });
                });
            }

            ui.add_space(16.0);

            ui.horizontal(|ui| {
                if !self.is_converting {
                    if ui.add_enabled(
                        self.selected_path.is_some(),
                        egui::Button::new("\u{25B6} Submit"),
                    ).clicked() {
                        self.start_conversion(ctx.clone());
                    }
                } else if ui.button("\u{2716} Cancel").clicked() {
                    self.cancel_flag.store(true, Ordering::Relaxed);
                    self.status_msg = "Cancelling...".to_string();
                }
            });

            if self.is_converting {
                ui.add_space(10.0);
                let progress = if self.progress_total > 0 {
                    self.progress_current as f32 / self.progress_total as f32
                } else {
                    0.0
                };
                ui.add(
                    egui::ProgressBar::new(progress)
                        .text(format!("{}/{} files done", self.progress_current, self.progress_total))
                        .animate(true),
                );
                if !self.current_file.is_empty() {
                    ui.label(format!("Processing: {}", self.current_file));
                }
            }

            if !self.status_msg.is_empty() {
                ui.add_space(8.0);
                ui.label(&self.status_msg);
            }
        });

        // Keep repainting while converting so progress bar stays live
        if self.is_converting {
            ctx.request_repaint();
        }
    }
}

impl CubeConvertApp {
    fn start_conversion(&mut self, ctx: egui::Context) {
        self.is_converting = true;
        self.status_msg = "Starting...".to_string();
        self.progress_current = 0;
        self.progress_total = 0;
        self.current_file.clear();

        self.cancel_flag.store(false, Ordering::Relaxed);
        let cancel = self.cancel_flag.clone();

        let path = self.selected_path.clone().unwrap();
        let is_folder = self.is_folder;
        let tab = match self.selected_tab {
            ConversionType::Wind   => 0,
            ConversionType::Bpm    => 1,
            ConversionType::Clouds => 2,
            ConversionType::Rgb    => 3,
            ConversionType::Text   => 4,
        };
        let color = self.rgb_color;

        // Internal progress channel: worker -> forwarding thread
        let (prog_tx, prog_rx) = crossbeam_channel::unbounded::<Progress>();

        // tx_fwd: owned by the forwarding thread
        let tx_fwd = self.tx.clone();
        let ctx_fwd = ctx.clone();
        thread::spawn(move || {
            // Exits automatically when prog_tx is dropped in the worker thread
            while let Ok(msg) = prog_rx.recv() {
                let _ = tx_fwd.send(AppMessage::Progress(msg));
                ctx_fwd.request_repaint();
            }
        });

        // tx_done: owned by the worker thread for the final Finished message
        let tx_done = self.tx.clone();

        thread::spawn(move || {
            let result = match tab {
                0 => converters::convert_wind(&path, is_folder, prog_tx.clone(), cancel),
                1 => converters::convert_bpm(&path, is_folder, prog_tx.clone(), cancel),
                2 => converters::convert_clouds(&path, is_folder, prog_tx.clone(), cancel),
                3 => converters::convert_rgb(&path, is_folder, prog_tx.clone(), cancel),
                4 => converters::convert_text(&path, is_folder, color, prog_tx.clone(), cancel),
                _ => Err("Unknown mode".into()),
            };

            // Report any top-level batch error through the progress channel
            if let Err(e) = result {
                let _ = prog_tx.send(Progress::Error {
                    name: "Batch".into(),
                    error: e,
                });
            }

            // Drop prog_tx so the forwarding thread's recv() returns Err and it exits cleanly
            drop(prog_tx);

            let _ = tx_done.send(AppMessage::Finished);
            ctx.request_repaint();
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Cube-Convert",
        options,
        Box::new(|_cc| Box::new(CubeConvertApp::default())),
    )
}
