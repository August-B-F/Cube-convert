use eframe::egui;
use rfd::FileDialog;
use std::path::{Path, PathBuf};
use std::thread;

mod converters {
    use super::*;
    use std::process::Command;
    
    // Helper to call ffmpeg (assuming ffmpeg is in PATH or bundled like in the Python version)
    pub fn run_ffmpeg(args: &[&str]) -> Result<(), String> {
        let status = Command::new("ffmpeg")
            .args(args)
            .status()
            .map_err(|e| e.to_string())?;
        
        if status.success() {
            Ok(())
        } else {
            Err("ffmpeg command failed".into())
        }
    }

    pub fn convert_wind(file_path: &Path, _is_folder: bool) -> Result<(), String> {
        // Implement Wind PDF to MP3/WAV parsing using lopdf and hound instead of PyPDF2 and PyDub
        // For now, this is a placeholder where you put the port of WIND_TO_MP3.py logic
        println!("Converting Wind data for: {:?}", file_path);
        Ok(())
    }

    pub fn convert_bpm(file_path: &Path, _is_folder: bool) -> Result<(), String> {
        // Placeholder for BPM_MP3.py logic
        println!("Converting BPM data for: {:?}", file_path);
        Ok(())
    }

    pub fn convert_clouds(file_path: &Path, _is_folder: bool) -> Result<(), String> {
        // Placeholder for CLOUDS_TO_MP4.py logic
        println!("Converting Clouds data for: {:?}", file_path);
        Ok(())
    }

    pub fn convert_rgb(file_path: &Path, _is_folder: bool) -> Result<(), String> {
        // Placeholder for RGB_MP4.py logic
        println!("Converting RGB data for: {:?}", file_path);
        Ok(())
    }

    pub fn convert_text(file_path: &Path, _is_folder: bool, _color: [u8; 3]) -> Result<(), String> {
        // Placeholder for TEXT_TO_MP4.py logic using the RGB color array
        println!("Converting Text data for: {:?} with color {:?}", file_path, _color);
        Ok(())
    }
}

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
        // Check for messages from background conversion threads
        if let Ok(msg) = self.rx.try_recv() {
            self.is_converting = false;
            match msg {
                AppMessage::Success(m) => self.status_msg = format!("Success: {}", m),
                AppMessage::Error(e) => self.status_msg = format!("Error: {}", e),
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Cube-Convert");
            ui.add_space(10.0);

            // Top navigation tabs
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.selected_tab, ConversionType::Wind, "WIND");
                ui.selectable_value(&mut self.selected_tab, ConversionType::Bpm, "BPM");
                ui.selectable_value(&mut self.selected_tab, ConversionType::Clouds, "CLOUDS");
                ui.selectable_value(&mut self.selected_tab, ConversionType::Rgb, "RGB");
                ui.selectable_value(&mut self.selected_tab, ConversionType::Text, "TEXT");
            });
            ui.separator();

            let desc = match self.selected_tab {
                ConversionType::Wind => "Convert wind intensities into an MP3 file.",
                ConversionType::Bpm => "Convert BMP data into an MP3 file.",
                ConversionType::Clouds => "Convert cloud images into an MP4 file.",
                ConversionType::Rgb => "Convert RGB values into an MP4 file.",
                ConversionType::Text => "Convert text into an MP4 file.",
            };
            ui.label(desc);
            ui.add_space(20.0);

            // File selection replacing custom Pygame file browser with native OS dialog
            ui.horizontal(|ui| {
                if ui.button("Select File").clicked() {
                    if let Some(path) = FileDialog::new().pick_file() {
                        self.selected_path = Some(path);
                        self.is_folder = false;
                        self.status_msg.clear();
                    }
                }
                
                if ui.button("Select Folder").clicked() {
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

            // Color picker for TEXT mode
            if self.selected_tab == ConversionType::Text {
                ui.horizontal(|ui| {
                    ui.label("Color Picker:");
                    ui.color_edit_button_srgb(&mut self.rgb_color);
                });
            }

            ui.add_space(20.0);

            ui.add_enabled_ui(!self.is_converting && self.selected_path.is_some(), |ui| {
                if ui.button("Submit").clicked() {
                    self.is_converting = true;
                    self.status_msg = "Converting... Please wait.".to_string();
                    
                    let path = self.selected_path.clone().unwrap();
                    let is_folder = self.is_folder;
                    let tab = match self.selected_tab {
                        ConversionType::Wind => 0,
                        ConversionType::Bpm => 1,
                        ConversionType::Clouds => 2,
                        ConversionType::Rgb => 3,
                        ConversionType::Text => 4,
                    };
                    let color = self.rgb_color;
                    let tx = self.tx.clone();

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
                            Ok(_) => AppMessage::Success("Conversion completed successfully!".into()),
                            Err(e) => AppMessage::Error(e),
                        };
                        let _ = tx.send(msg);
                    });
                }
            });

            ui.add_space(10.0);

            if !self.status_msg.is_empty() {
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