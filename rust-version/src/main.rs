#![windows_subsystem = "windows"]

use eframe::egui;
use rfd::FileDialog;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::thread;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::fs;

mod converters;
use converters::Progress;

#[derive(PartialEq)]
enum ConversionType {
    Wind,
    Bpm,
    Clouds,
    Rgb,
    Text,
    Slideshow,
}

#[derive(PartialEq, Clone, Copy)]
enum CloudsFolderMode {
    BatchPdf,
    StitchImages,
}

enum AppMessage {
    Progress(Progress),
    Finished,
}

struct CubeConvertApp {
    selected_tab: ConversionType,
    selected_path: Option<PathBuf>,
    last_dir: Option<PathBuf>,
    is_folder: bool,
    is_converting: bool,
    rgb_color: [u8; 3],
    color_history: Vec<[u8; 3]>,
    clouds_folder_mode: CloudsFolderMode,
    status_msg: String,
    show_error_popup: bool,
    popup_error_msg: String,

    // Progress state
    progress_current: usize,
    progress_total: usize,
    file_fractions: HashMap<String, f32>,
    current_file: String,

    // Concurrency
    tx: crossbeam_channel::Sender<AppMessage>,
    rx: crossbeam_channel::Receiver<AppMessage>,
    cancel_flag: Arc<AtomicBool>,
}

impl Default for CubeConvertApp {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        
        let mut color_history = Vec::new();
        let mut rgb_color = [255, 255, 255];
        if let Ok(data) = fs::read_to_string("cube_settings.json") {
            let cleaned = data.replace("[", "").replace("]", "").replace(" ", "").replace("\n", "");
            let parts: Vec<&str> = cleaned.split(',').collect();
            if parts.len() >= 3 {
                rgb_color[0] = parts[0].parse().unwrap_or(255);
                rgb_color[1] = parts[1].parse().unwrap_or(255);
                rgb_color[2] = parts[2].parse().unwrap_or(255);
                
                let mut i = 3;
                while i + 2 < parts.len() && color_history.len() < 5 {
                    color_history.push([
                        parts[i].parse().unwrap_or(255),
                        parts[i+1].parse().unwrap_or(255),
                        parts[i+2].parse().unwrap_or(255),
                    ]);
                    i += 3;
                }
            }
        }
        if color_history.is_empty() {
            color_history = vec![[255, 255, 255], [255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]];
        }

        Self {
            selected_tab: ConversionType::Wind,
            selected_path: None,
            last_dir: None,
            is_folder: false,
            is_converting: false,
            rgb_color,
            color_history,
            clouds_folder_mode: CloudsFolderMode::StitchImages,
            status_msg: String::new(),
            show_error_popup: false,
            popup_error_msg: String::new(),
            progress_current: 0,
            progress_total: 0,
            file_fractions: HashMap::new(),
            current_file: String::new(),
            tx,
            rx,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl CubeConvertApp {
    fn save_settings(&self) {
        let mut out = String::new();
        out.push_str(&format!("[{},{},{}]", self.rgb_color[0], self.rgb_color[1], self.rgb_color[2]));
        for c in &self.color_history {
            out.push_str(&format!(",\n[{},{},{}]", c[0], c[1], c[2]));
        }
        let _ = fs::write("cube_settings.json", format!("[{}]", out));
    }
}

impl eframe::App for CubeConvertApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut style = (*ctx.style()).clone();
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        ctx.set_style(style);

        // Render Error Popup if needed
        if self.show_error_popup {
            egui::Window::new("⚠️ Error")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(&self.popup_error_msg);
                    ui.add_space(10.0);
                    if ui.button("OK").clicked() {
                        self.show_error_popup = false;
                    }
                });
        }

        ctx.input(|i| {
            if !self.is_converting {
                if let Some(dropped) = i.raw.dropped_files.first() {
                    if let Some(path) = &dropped.path {
                        self.selected_path = Some(path.clone());
                        self.is_folder = path.is_dir();
                        if let Some(parent) = path.parent() {
                            self.last_dir = Some(parent.to_path_buf());
                        }
                        self.status_msg.clear();
                    }
                }
            }
        });

        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::Progress(p) => match p {
                    Progress::Init { total } => {
                        self.progress_total = total;
                        self.progress_current = 0;
                        self.file_fractions.clear();
                    }
                    Progress::Start { name } => {
                        self.current_file = name.clone();
                        self.file_fractions.insert(name, 0.0);
                    }
                    Progress::Update { name, fraction } => {
                        self.file_fractions.insert(name, fraction);
                    }
                    Progress::Done { name } => {
                        self.progress_current += 1;
                        self.file_fractions.remove(&name);
                        if self.current_file == name {
                            self.current_file.clear();
                        }
                    }
                    Progress::Error { name, error } => {
                        if name != "Batch" {
                            self.progress_current += 1;
                            self.file_fractions.remove(&name);
                        }
                        self.status_msg = "An error occurred.".to_string();
                        self.popup_error_msg = error;
                        self.show_error_popup = true;
                    }
                },
                AppMessage::Finished => {
                    self.is_converting = false;
                    self.current_file.clear();
                    if self.status_msg.is_empty() || self.status_msg == "Starting..." {
                        self.status_msg = if self.cancel_flag.load(Ordering::Relaxed) {
                            "Cancelled.".to_string()
                        } else {
                            "Done.".to_string()
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
                    ui.selectable_value(&mut self.selected_tab, ConversionType::Slideshow, "SLIDESHOW");
                });
            });
            ui.separator();

            let desc = match self.selected_tab {
                ConversionType::Wind      => "Convert wind intensities (PDF) -> MP3",
                ConversionType::Bpm       => "Convert BPM data (PDF) -> MP3",
                ConversionType::Clouds    => "Convert clouds (PDF) -> scrolling MP4",
                ConversionType::Rgb       => "Convert RGB values (PDF) -> gradient MP4",
                ConversionType::Text      => "Convert text (PDF) -> scrolling text MP4",
                ConversionType::Slideshow => "Convert folder of images -> 4s per drawing Slideshow MP4",
            };
            ui.label(desc);
            ui.add_space(16.0);

            ui.add_enabled_ui(!self.is_converting, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("\u{1F4C4} Select File").clicked() {
                        let mut dialog = FileDialog::new().add_filter("PDF", &["pdf"]);
                        if let Some(dir) = &self.last_dir { dialog = dialog.set_directory(dir); }
                        if let Some(path) = dialog.pick_file() {
                            if let Some(parent) = path.parent() { self.last_dir = Some(parent.to_path_buf()); }
                            self.selected_path = Some(path);
                            self.is_folder = false;
                            self.status_msg.clear();
                        }
                    }
                    if ui.button("\u{1F4C1} Select Folder").clicked() {
                        let mut dialog = FileDialog::new();
                        if let Some(dir) = &self.last_dir { dialog = dialog.set_directory(dir); }
                        if let Some(path) = dialog.pick_folder() {
                            self.last_dir = Some(path.clone());
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
            ui.label(egui::RichText::new("💡 Hint: You can also drag & drop files/folders here").italics().color(egui::Color32::DARK_GRAY));

            ui.add_space(10.0);

            if self.selected_tab == ConversionType::Clouds && self.is_folder {
                ui.add_enabled_ui(!self.is_converting, |ui| {
                    ui.label("Folder contents:");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.clouds_folder_mode, CloudsFolderMode::StitchImages, "Images (Stitch into 1 Video)");
                        ui.radio_value(&mut self.clouds_folder_mode, CloudsFolderMode::BatchPdf, "PDFs (Batch Convert)");
                    });
                });
                ui.add_space(10.0);
            }

            if self.selected_tab == ConversionType::Text {
                ui.add_enabled_ui(!self.is_converting, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Text Color:");
                        if ui.color_edit_button_srgb(&mut self.rgb_color).changed() {
                            if !self.color_history.is_empty() && self.color_history[0] != self.rgb_color {
                                self.color_history.insert(0, self.rgb_color);
                                self.color_history.truncate(5);
                                self.save_settings();
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Recent:");
                        for color in self.color_history.clone() {
                            let (r, g, b) = (color[0], color[1], color[2]);
                            let color32 = egui::Color32::from_rgb(r, g, b);
                            let button = egui::Button::new("  ").fill(color32);
                            if ui.add(button).clicked() {
                                self.rgb_color = color;
                                self.save_settings();
                            }
                        }
                    });
                });
                ui.add_space(10.0);
            }

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

                let fraction_sum: f32 = self.file_fractions.values().sum();
                let progress = if self.progress_total > 0 {
                    (self.progress_current as f32 + fraction_sum) / self.progress_total as f32
                } else {
                    0.0
                };
                let progress = progress.clamp(0.0, 1.0);

                let bar_label = if self.progress_total > 1 {
                    format!("{}/{} files done", self.progress_current, self.progress_total)
                } else {
                    format!("{}%", (progress * 100.0).round() as u32)
                };

                ui.add(
                    egui::ProgressBar::new(progress)
                        .text(bar_label)
                        .animate(self.is_folder && self.progress_total > 1),
                );

                if !self.current_file.is_empty() && self.progress_total > 1 {
                    ui.label(format!("Processing: {}", self.current_file));
                }
            }

            if !self.status_msg.is_empty() {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if self.status_msg == "Done." {
                        ui.colored_label(egui::Color32::from_rgb(0, 200, 0), &self.status_msg);
                        
                        if ui.button("📂 Open Output Folder").clicked() {
                            if let Some(path) = &self.selected_path {
                                let dir = if self.is_folder {
                                    path.clone()
                                } else {
                                    path.parent().unwrap_or(Path::new("")).to_path_buf()
                                };
                                #[cfg(target_os = "windows")]
                                let _ = std::process::Command::new("explorer").arg(dir).spawn();
                                #[cfg(target_os = "macos")]
                                let _ = std::process::Command::new("open").arg(dir).spawn();
                                #[cfg(target_os = "linux")]
                                let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                            }
                        }
                    } else if self.status_msg.starts_with("Error") || self.status_msg.starts_with("An error") || self.status_msg.starts_with("Cancelled") {
                        ui.colored_label(egui::Color32::from_rgb(200, 0, 0), &self.status_msg);
                    } else {
                        ui.label(&self.status_msg);
                    }
                });
            }
        });

        if self.is_converting {
            ctx.request_repaint();
        }
    }
}

impl CubeConvertApp {
    fn start_conversion(&mut self, ctx: egui::Context) {
        self.is_converting = true;
        self.status_msg = "Starting...".to_string();
        self.show_error_popup = false;
        self.progress_current = 0;
        self.progress_total = 0;
        self.file_fractions.clear();
        self.current_file.clear();

        self.cancel_flag.store(false, Ordering::Relaxed);
        let cancel = self.cancel_flag.clone();

        let path = self.selected_path.clone().unwrap();
        let is_folder = self.is_folder;
        let clouds_stitch = self.clouds_folder_mode == CloudsFolderMode::StitchImages;
        let tab = match self.selected_tab {
            ConversionType::Wind      => 0,
            ConversionType::Bpm       => 1,
            ConversionType::Clouds    => 2,
            ConversionType::Rgb       => 3,
            ConversionType::Text      => 4,
            ConversionType::Slideshow => 5,
        };
        let color = self.rgb_color;

        let (prog_tx, prog_rx) = crossbeam_channel::unbounded::<Progress>();

        let tx_fwd = self.tx.clone();
        let ctx_fwd = ctx.clone();
        thread::spawn(move || {
            while let Ok(msg) = prog_rx.recv() {
                let _ = tx_fwd.send(AppMessage::Progress(msg));
                ctx_fwd.request_repaint();
            }
        });

        let tx_done = self.tx.clone();
        thread::spawn(move || {
            let result = match tab {
                0 => converters::convert_wind(&path, is_folder, prog_tx.clone(), cancel),
                1 => converters::convert_bpm(&path, is_folder, prog_tx.clone(), cancel),
                2 => converters::convert_clouds(&path, is_folder, clouds_stitch, prog_tx.clone(), cancel),
                3 => converters::convert_rgb(&path, is_folder, prog_tx.clone(), cancel),
                4 => converters::convert_text(&path, is_folder, color, prog_tx.clone(), cancel),
                5 => converters::convert_slideshow(&path, is_folder, prog_tx.clone(), cancel),
                _ => Err("Unknown mode".into()),
            };

            if let Err(e) = result {
                let _ = prog_tx.send(Progress::Error {
                    name: "Batch".into(),
                    error: e,
                });
            }
            drop(prog_tx);
            let _ = tx_done.send(AppMessage::Finished);
            ctx.request_repaint();
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([600.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Cube-Convert",
        options,
        Box::new(|_cc| Box::new(CubeConvertApp::default())),
    )
}
