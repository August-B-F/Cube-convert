use eframe::egui;
use rfd::FileDialog;
use std::path::{Path, PathBuf};
use std::thread;

mod converters;

#[derive(PartialEq)]
enum ConversionType {
    Wind,
    Bpm,
    Clouds,
    Rgb,
    Text,
}

enum AppMessage {
    Success(String),
    Error(String),
}

struct CubeConvertApp {
    selected_tab: ConversionType,
    selected_path: Option<PathBuf>,
    is_folder: bool,
    is_converting: bool,
    rgb_color: [u8; 3],
    status_msg: String,
    tx: crossbeam_channel::Sender<AppMessage>,
    rx: crossbeam_channel::Receiver<AppMessage>,
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
            tx,
            rx,
        }
    }
}

impl eframe::App for CubeConvertApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Ok(msg) = self.rx.try_recv() {
            self.is_converting = false;
            match msg {
                AppMessage::Success(m) => self.status_msg = format!("\u{2714} {}", m),
                AppMessage::Error(e) => self.status_msg = format!("\u{2718} Error: {}", e),
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Cube-Convert");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.selected_tab, ConversionType::Wind, "WIND");
                ui.selectable_value(&mut self.selected_tab, ConversionType::Bpm, "BPM");
                ui.selectable_value(&mut self.selected_tab, ConversionType::Clouds, "CLOUDS");
                ui.selectable_value(&mut self.selected_tab, ConversionType::Rgb, "RGB");
                ui.selectable_value(&mut self.selected_tab, ConversionType::Text, "TEXT");
            });
            ui.separator();

            let desc = match self.selected_tab {
                ConversionType::Wind   => "Convert wind intensities (PDF) \u{2192} MP3",
                ConversionType::Bpm    => "Convert BPM data (PDF) \u{2192} MP3",
                ConversionType::Clouds => "Convert cloud images (PDF) \u{2192} scrolling MP4",
                ConversionType::Rgb    => "Convert RGB values (PDF) \u{2192} gradient MP4",
                ConversionType::Text   => "Convert text (PDF) \u{2192} scrolling text MP4",
            };
            ui.label(desc);
            ui.add_space(16.0);

            ui.horizontal(|ui| {
                if ui.button("\U0001F4C4 Select File").clicked() {
                    if let Some(path) = FileDialog::new().add_filter("PDF", &["pdf"]).pick_file() {
                        self.selected_path = Some(path);
                        self.is_folder = false;
                        self.status_msg.clear();
                    }
                }
                if ui.button("\U0001F4C1 Select Folder").clicked() {
                    if let Some(path) = FileDialog::new().pick_folder() {
                        self.selected_path = Some(path);
                        self.is_folder = true;
                        self.status_msg.clear();
                    }
                }
            });

            if let Some(path) = &self.selected_path {
                ui.label(format!("Selected: {}", path.display()));
            } else {
                ui.label("No file or folder selected.");
            }

            ui.add_space(10.0);

            if self.selected_tab == ConversionType::Text {
                ui.horizontal(|ui| {
                    ui.label("Text Color:");
                    ui.color_edit_button_srgb(&mut self.rgb_color);
                });
            }

            ui.add_space(16.0);

            ui.add_enabled_ui(!self.is_converting && self.selected_path.is_some(), |ui| {
                if ui.button("\u{25B6} Submit").clicked() {
                    self.is_converting = true;
                    self.status_msg = "Converting\u{2026} please wait.".to_string();

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
                    let tx = self.tx.clone();
                    let ctx = ctx.clone();

                    thread::spawn(move || {
                        let result = match tab {
                            0 => converters::convert_wind(&path, is_folder),
                            1 => converters::convert_bpm(&path, is_folder),
                            2 => converters::convert_clouds(&path, is_folder),
                            3 => converters::convert_rgb(&path, is_folder),
                            4 => converters::convert_text(&path, is_folder, color),
                            _ => Err("Unknown mode".into()),
                        };

                        let msg = match result {
                            Ok(_) => AppMessage::Success("Conversion completed!".into()),
                            Err(e) => AppMessage::Error(e),
                        };
                        let _ = tx.send(msg);
                        ctx.request_repaint();
                    });
                }
            });

            if self.is_converting {
                ui.add_space(8.0);
                ui.spinner();
            }

            if !self.status_msg.is_empty() {
                ui.add_space(8.0);
                ui.label(&self.status_msg);
            }
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
