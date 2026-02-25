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

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
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

// --- Custom Theme Colors ---
const COLOR_BG: egui::Color32 = egui::Color32::from_rgb(150, 158, 123);
const COLOR_TEXT: egui::Color32 = egui::Color32::from_rgb(33, 37, 13);
const COLOR_RED: egui::Color32 = egui::Color32::from_rgb(109, 74, 56);
const COLOR_ACCENT: egui::Color32 = egui::Color32::from_rgb(62, 84, 114);
const COLOR_FADED: egui::Color32 = egui::Color32::from_rgb(117, 122, 97);
const COLOR_WHITE: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);
const COLOR_BLACK: egui::Color32 = egui::Color32::from_rgb(0, 0, 0);

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

    // Animation states
    time_active: f32,
    tab_animations: HashMap<ConversionType, f32>,
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

        let mut tab_animations = HashMap::new();
        tab_animations.insert(ConversionType::Wind, 1.0);
        tab_animations.insert(ConversionType::Bpm, 0.0);
        tab_animations.insert(ConversionType::Clouds, 0.0);
        tab_animations.insert(ConversionType::Rgb, 0.0);
        tab_animations.insert(ConversionType::Text, 0.0);
        tab_animations.insert(ConversionType::Slideshow, 0.0);

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
            time_active: 0.0,
            tab_animations,
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

    fn apply_retro_theme(&self, ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();
        if let Ok(font_data) = std::fs::read("assets/pixel.ttf") {
            fonts.font_data.insert("pixel".to_owned(), egui::FontData::from_owned(font_data));
            fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "pixel".to_owned());
            fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "pixel".to_owned());
        }
        ctx.set_fonts(fonts);

        let mut visuals = egui::Visuals::light();
        
        visuals.window_fill = COLOR_BG;
        visuals.panel_fill = COLOR_BG;
        visuals.faint_bg_color = COLOR_BG;
        visuals.extreme_bg_color = COLOR_BG;
        
        visuals.override_text_color = Some(COLOR_TEXT);
        
        visuals.selection.bg_fill = COLOR_TEXT;
        visuals.selection.stroke = egui::Stroke::new(1.0, COLOR_TEXT);

        // Fix for white backgrounds: explicitly set widget backgrounds to COLOR_BG
        visuals.widgets.noninteractive.bg_fill = COLOR_BG;
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(2.0, COLOR_TEXT);
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(2.0, COLOR_TEXT);
        
        visuals.widgets.inactive.bg_fill = COLOR_BG;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(2.0, COLOR_TEXT);
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, COLOR_TEXT);
        visuals.widgets.inactive.rounding = egui::Rounding::same(0.0);

        visuals.widgets.hovered.bg_fill = COLOR_TEXT;
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(2.0, COLOR_TEXT);
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, COLOR_BG);
        visuals.widgets.hovered.rounding = egui::Rounding::same(0.0);

        visuals.widgets.active.bg_fill = COLOR_TEXT;
        visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, COLOR_TEXT);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, COLOR_BG);
        visuals.widgets.active.rounding = egui::Rounding::same(0.0);
        
        // Button-specific fixes for uniform styling
        visuals.widgets.open.bg_fill = COLOR_BG;
        visuals.widgets.open.bg_stroke = egui::Stroke::new(2.0, COLOR_TEXT);
        visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, COLOR_TEXT);
        visuals.widgets.open.rounding = egui::Rounding::same(0.0);
        
        visuals.window_rounding = egui::Rounding::same(0.0);
        visuals.window_stroke = egui::Stroke::new(3.0, COLOR_TEXT);
        visuals.popup_shadow = egui::epaint::Shadow::NONE;

        ctx.set_visuals(visuals);

        let mut style = (*ctx.style()).clone();
        style.spacing.button_padding = egui::vec2(16.0, 8.0);
        style.spacing.item_spacing = egui::vec2(12.0, 16.0);
        // Fix text wrapping and sizing
        style.wrap = Some(true);
        ctx.set_style(style);
    }

    fn custom_tab(&mut self, ui: &mut egui::Ui, tab: ConversionType, label: &str, ctx: &egui::Context) -> bool {
        let is_selected = self.selected_tab == tab;
        let mut clicked = false;

        let desired_size = egui::vec2(70.0, 30.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
        
        let anim_target = if is_selected { 1.0 } else { 0.0 };
        let current_anim = self.tab_animations.get(&tab).copied().unwrap_or(0.0);
        let dt = ctx.input(|i| i.stable_dt);
        
        // Animate the fill state
        let new_anim = current_anim + (anim_target - current_anim) * (dt * 15.0).min(1.0);
        self.tab_animations.insert(tab, new_anim);
        
        if response.clicked() {
            clicked = true;
        }

        if ui.is_rect_visible(rect) {
            let _visuals = ui.style().interact(&response);
            
            let bg_color = if is_selected {
                COLOR_BG
            } else if response.hovered() {
                COLOR_FADED
            } else {
                COLOR_TEXT
            };
            
            let text_color = if is_selected {
                COLOR_TEXT
            } else {
                COLOR_BG
            };

            // Draw background 
            ui.painter().rect_filled(rect, 0.0, bg_color);
            ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(2.0, COLOR_TEXT));
            
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(16.0),
                text_color,
            );
        }
        
        if new_anim > 0.01 && new_anim < 0.99 {
             ctx.request_repaint();
        }

        clicked
    }
}

impl eframe::App for CubeConvertApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_retro_theme(ctx);
        self.time_active += ctx.input(|i| i.stable_dt);

        // Render Error Popup if needed (Improved layout)
        if self.show_error_popup {
            egui::Window::new(egui::RichText::new("! ERROR !").color(COLOR_BG).background_color(COLOR_RED).size(16.0))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .frame(egui::Frame::window(&ctx.style()).fill(COLOR_BG).stroke(egui::Stroke::new(4.0, COLOR_TEXT)).inner_margin(16.0))
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new(&self.popup_error_msg).color(COLOR_TEXT).strong().size(14.0));
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new("[ OK ]").fill(COLOR_BG)).clicked() {
                                self.show_error_popup = false;
                            }
                        });
                    });
                });
        }

        let mut is_hovering_file = false;
        ctx.input(|i| {
            if !self.is_converting {
                if !i.raw.hovered_files.is_empty() {
                    is_hovering_file = true;
                }
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

        // Smooth fade-in for the drag & drop overlay
        let overlay_alpha = ctx.animate_bool(egui::Id::new("drop_overlay_anim"), is_hovering_file);

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
                        
                        // Prevent "Cancelled" from spawning an error popup
                        if self.cancel_flag.load(Ordering::Relaxed) || error == "Cancelled." {
                            self.status_msg = "Cancelled.".to_string();
                        } else {
                            self.status_msg = "An error occurred.".to_string();
                            self.popup_error_msg = error;
                            self.show_error_popup = true;
                        }
                    }
                },
                AppMessage::Finished => {
                    self.is_converting = false;
                    self.current_file.clear();
                    if self.status_msg.is_empty() || self.status_msg == "INITIALIZING..." || self.status_msg == "Starting..." {
                        self.status_msg = if self.cancel_flag.load(Ordering::Relaxed) {
                            "Cancelled.".to_string()
                        } else {
                            "Done.".to_string()
                        };
                    }
                }
            }
        }

        // TopPanel for execute button to prevent it from being stuck at the bottom edge and full width
        // We move the execution button and progress info here to have predictable spacing.
        egui::TopBottomPanel::bottom("execution_panel")
            .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(24.0, 16.0)).fill(COLOR_BG))
            .show(ctx, |ui| {
            // Manually draw a thick top border for the panel to cleanly separate it from the content
            let rect = ui.max_rect();
            ui.painter().hline(
                rect.min.x..=rect.max.x,
                ui.cursor().top() - 16.0,
                egui::Stroke::new(2.0, COLOR_TEXT),
            );

            // Bottom execution area
            ui.horizontal(|ui| {
                // Status & Progress line left aligned
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if self.is_converting {
                        let fraction_sum: f32 = self.file_fractions.values().sum();
                        let progress = if self.progress_total > 0 {
                            (self.progress_current as f32 + fraction_sum) / self.progress_total as f32
                        } else {
                            0.0
                        };
                        let progress = progress.clamp(0.0, 1.0);

                        ui.horizontal(|ui| {
                            // Retro blocky progress bar with integrated text
                            let desired_size = egui::vec2(400.0, 32.0);
                            let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());
                            
                            ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(2.0, COLOR_TEXT));
                            
                            // Draw segmented blocks
                            let num_blocks = 25.0;
                            let block_width = (rect.width() - 4.0) / num_blocks;
                            let blocks_to_fill = (progress * num_blocks) as i32;
                            
                            for i in 0..blocks_to_fill {
                                let mut block_rect = rect;
                                block_rect.min.x += 2.0 + (i as f32 * block_width);
                                block_rect.max.x = block_rect.min.x + block_width - 2.0;
                                block_rect.min.y += 2.0;
                                block_rect.max.y -= 2.0;
                                ui.painter().rect_filled(block_rect, 0.0, COLOR_TEXT);
                            }
                            
                            let percentage = (progress * 100.0).round() as u32;
                            let prog_text = if self.progress_total > 1 {
                                format!("{:02}%  [{}/{}]", percentage, self.progress_current, self.progress_total)
                            } else {
                                format!("{:02}%", percentage)
                            };

                            // Draw text in middle of progress bar with contrasting background badge
                            let text_pos = rect.center();
                            let galley = ui.painter().layout_no_wrap(
                                prog_text.clone(),
                                egui::FontId::proportional(16.0),
                                COLOR_TEXT,
                            );
                            let text_bg_rect = egui::Rect::from_center_size(text_pos, galley.size()).expand(4.0);
                            
                            ui.painter().rect_filled(text_bg_rect, 0.0, COLOR_BG);
                            ui.painter().rect_stroke(text_bg_rect, 0.0, egui::Stroke::new(2.0, COLOR_TEXT));
                            ui.painter().text(
                                text_pos,
                                egui::Align2::CENTER_CENTER,
                                prog_text,
                                egui::FontId::proportional(16.0),
                                COLOR_TEXT,
                            );
                        });

                        if !self.current_file.is_empty() && self.progress_total > 1 {
                            ui.add_space(8.0);
                            let frames = ["|", "/", "-", "\\\\"];
                            let frame_idx = ((self.time_active * 10.0) as usize) % frames.len();
                            ui.label(egui::RichText::new(format!("{} {}", frames[frame_idx], self.current_file)).color(COLOR_TEXT));
                        }
                    } else {
                        let blink = (self.time_active * 4.0).sin() > 0.0;
                        let cursor = if blink { "_" } else { " " };
                        
                        if self.status_msg == "Done." {
                            ui.label(egui::RichText::new("+++ SUCCESS +++").color(COLOR_ACCENT).strong().size(16.0));
                            ui.add_space(16.0);
                            if ui.add(egui::Button::new("[ OPEN DIR ]").fill(COLOR_BG)).clicked() {
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
                        } else if self.status_msg == "Cancelled." || self.status_msg == "ABORTING..." {
                            ui.label(egui::RichText::new(format!("> {}{}", self.status_msg, cursor)).color(COLOR_RED).strong().size(16.0));
                        } else if self.status_msg.starts_with("Error") || self.status_msg.starts_with("An error") {
                            ui.label(egui::RichText::new(format!("> {}{}", self.status_msg, cursor)).color(COLOR_RED).strong().size(16.0));
                        } else if !self.status_msg.is_empty() {
                            ui.label(egui::RichText::new(format!("> {}{}", self.status_msg, cursor)).color(COLOR_TEXT).strong().size(16.0));
                        } else {
                            // Provide continuous prompt instructions instead of empty space
                            let prompt = if self.selected_path.is_some() {
                                "> READY. CLICK EXECUTE TO START."
                            } else {
                                "> AWAITING DATA DROP OR SELECTION..."
                            };
                            ui.label(egui::RichText::new(format!("{}{}", prompt, cursor)).color(COLOR_TEXT).strong().size(16.0));
                        }
                    }
                });

                // Execute/Abort button right aligned
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.is_converting {
                        // Standard button, not filling width
                        let btn_text = egui::RichText::new("EXECUTE").size(18.0).strong().color(COLOR_TEXT);
                        if ui.add_enabled(self.selected_path.is_some(), egui::Button::new(btn_text).fill(COLOR_BG).min_size(egui::vec2(120.0, 40.0))).clicked() {
                            self.start_conversion(ctx.clone());
                        }
                    } else {
                        let btn_text = egui::RichText::new("ABORT").size(18.0).strong().color(COLOR_TEXT);
                        if ui.add(egui::Button::new(btn_text).fill(COLOR_BG).min_size(egui::vec2(120.0, 40.0))).clicked() {
                            self.cancel_flag.store(true, Ordering::Relaxed);
                            self.status_msg = "ABORTING...".to_string();
                        }
                    }
                });
            });
        });

        // Main Window Content
        egui::CentralPanel::default().frame(egui::Frame::none().fill(COLOR_BG)).show(ctx, |ui| {
            ui.add_space(20.0);

            // TABS
            ui.add_enabled_ui(!self.is_converting, |ui| {
                let mut tab_changed = false;
                ui.horizontal(|ui| {
                    ui.add_space(24.0); // Consistent left margin
                    if self.custom_tab(ui, ConversionType::Wind, "WIND", ctx) { self.selected_tab = ConversionType::Wind; tab_changed = true; }
                    if self.custom_tab(ui, ConversionType::Bpm, "BPM", ctx) { self.selected_tab = ConversionType::Bpm; tab_changed = true; }
                    if self.custom_tab(ui, ConversionType::Clouds, "CLOUDS", ctx) { self.selected_tab = ConversionType::Clouds; tab_changed = true; }
                    if self.custom_tab(ui, ConversionType::Rgb, "RGB", ctx) { self.selected_tab = ConversionType::Rgb; tab_changed = true; }
                    if self.custom_tab(ui, ConversionType::Slideshow, "SLIDE", ctx) { self.selected_tab = ConversionType::Slideshow; tab_changed = true; }
                    if self.custom_tab(ui, ConversionType::Text, "TEXT", ctx) { self.selected_tab = ConversionType::Text; tab_changed = true; }
                });

                if tab_changed {
                    self.status_msg.clear();
                    self.show_error_popup = false;
                }
            });

            // Thick line separating tabs from content
            let rect = ui.max_rect();
            ui.painter().hline(
                rect.min.x..=rect.max.x,
                ui.cursor().top() - 2.0,
                egui::Stroke::new(2.0, COLOR_TEXT),
            );
            ui.add_space(16.0);

            let desc = match self.selected_tab {
                ConversionType::Wind      => "Convert wind intensities (PDF) -> MP3",
                ConversionType::Bpm       => "Convert BPM data (PDF) -> MP3",
                ConversionType::Clouds    => "Convert clouds (PDF) -> scrolling MP4",
                ConversionType::Rgb       => "Convert RGB values (PDF) -> gradient MP4",
                ConversionType::Text      => "Convert text (PDF) -> scrolling text MP4",
                ConversionType::Slideshow => "Folder of images -> 4s Slideshow MP4",
            };
            
            // Typing animation for description text
            let char_count = (self.time_active * 30.0) as usize;
            let desc_to_show = if char_count > desc.len() { desc } else { &desc[..char_count] };
            
            ui.horizontal(|ui| {
               ui.add_space(24.0); // Consistent left margin
               ui.label(egui::RichText::new(desc_to_show).size(20.0).strong().color(COLOR_TEXT));
            });
            ctx.request_repaint(); // Keep repainting for the typing effect
            
            ui.add_space(16.0);

            // Input Area
            ui.horizontal(|ui| {
                 ui.add_space(24.0); // Consistent left margin
                 ui.add_enabled_ui(!self.is_converting, |ui| {
                    if ui.add(egui::Button::new("[ SELECT FILE ]").fill(COLOR_BG)).clicked() {
                        let mut dialog = FileDialog::new().add_filter("PDF", &["pdf"]);
                        if let Some(dir) = &self.last_dir { dialog = dialog.set_directory(dir); }
                        if let Some(path) = dialog.pick_file() {
                            if let Some(parent) = path.parent() { self.last_dir = Some(parent.to_path_buf()); }
                            self.selected_path = Some(path);
                            self.is_folder = false;
                            self.status_msg.clear();
                            self.time_active = 0.0; // Reset typing anim on change
                        }
                    }
                    ui.add_space(16.0);
                    if ui.add(egui::Button::new("[ SELECT FOLDER ]").fill(COLOR_BG)).clicked() {
                        let mut dialog = FileDialog::new();
                        if let Some(dir) = &self.last_dir { dialog = dialog.set_directory(dir); }
                        if let Some(path) = dialog.pick_folder() {
                            self.last_dir = Some(path.clone());
                            self.selected_path = Some(path);
                            self.is_folder = true;
                            self.status_msg.clear();
                            self.time_active = 0.0; // Reset typing anim on change
                        }
                    }
                });
            });

            ui.add_space(20.0);

            // Path display box
            ui.horizontal(|ui| {
                 ui.add_space(24.0); // Consistent left margin
                 if let Some(path) = &self.selected_path {
                    egui::Frame::none()
                        .fill(COLOR_BG)
                        .stroke(egui::Stroke::new(2.0, COLOR_TEXT))
                        .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            // Truncate path if it's too long, showing the end
                            let path_str = path.display().to_string();
                            let display_str = if path_str.len() > 60 {
                                format!("...{}", &path_str[path_str.len() - 57..])
                            } else {
                                path_str
                            };
                            ui.label(egui::RichText::new(display_str).color(COLOR_TEXT));
                        });
                } else {
                    ui.label(egui::RichText::new("> NO INPUT SELECTED.").color(COLOR_TEXT));
                }
            });
            
            ui.add_space(32.0);

            // Module specific options enclosed in retro frames
            if self.selected_tab == ConversionType::Clouds && self.is_folder {
                ui.horizontal(|ui| {
                     ui.add_space(24.0); // Consistent left margin
                     egui::Frame::none()
                        .stroke(egui::Stroke::new(2.0, COLOR_TEXT))
                        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                        .show(ui, |ui| {
                            ui.add_enabled_ui(!self.is_converting, |ui| {
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.label(egui::RichText::new("> CLOUD DIRECTORY MODE:").color(COLOR_TEXT));
                                    ui.add_space(16.0);
                                    ui.radio_value(&mut self.clouds_folder_mode, CloudsFolderMode::StitchImages, "[ STITCH ]");
                                    ui.add_space(16.0);
                                    ui.radio_value(&mut self.clouds_folder_mode, CloudsFolderMode::BatchPdf, "[ BATCH ]");
                                });
                            });
                        });
                });
            }

            if self.selected_tab == ConversionType::Text {
                ui.horizontal(|ui| {
                    ui.add_space(24.0); // Consistent left margin
                    egui::Frame::none()
                        .stroke(egui::Stroke::new(2.0, COLOR_TEXT))
                        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                        .show(ui, |ui| {
                        ui.add_enabled_ui(!self.is_converting, |ui| {
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new("> COLOR:").color(COLOR_TEXT));
                                ui.add_space(8.0);
                                ui.scope(|ui| {
                                    ui.spacing_mut().interact_size = egui::vec2(40.0, 24.0);
                                    ui.color_edit_button_srgb(&mut self.rgb_color);
                                });
                                
                                ui.add_space(24.0);
                                
                                ui.label(egui::RichText::new("PALETTE:").color(COLOR_TEXT));
                                ui.add_space(8.0);
                                for color in self.color_history.clone() {
                                    let (r, g, b) = (color[0], color[1], color[2]);
                                    let color32 = egui::Color32::from_rgb(r, g, b);
                                    
                                    let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
                                    if ui.is_rect_visible(rect) {
                                        let stroke_color = if response.hovered() {
                                            COLOR_WHITE
                                        } else {
                                            COLOR_TEXT
                                        };
                                        ui.painter().rect(rect, 0.0, color32, egui::Stroke::new(2.0, stroke_color));
                                    }
                                    
                                    if response.clicked() {
                                        self.rgb_color = color;
                                        // Move to front of history
                                        self.color_history.retain(|&c| c != color);
                                        self.color_history.insert(0, color);
                                        self.save_settings();
                                    }
                                }
                            });
                        });
                    });
                });
            }
        });

        // Draw animated drag & drop overlay
        if overlay_alpha > 0.0 {
            let rect = ctx.screen_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop_overlay")));
            
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_premultiplied(150, 158, 123, (220.0 * overlay_alpha) as u8));
            
            // Pixelated target bracket animation
            let time = ctx.input(|i| i.time);
            let pulse = ((time * 8.0).sin() as f32 * 0.5 + 0.5) * overlay_alpha;
            
            let center = rect.center();
            let box_size = 150.0 + (pulse * 20.0);
            
            // Draw 4 corner brackets
            let stroke = egui::Stroke::new(6.0, COLOR_TEXT);
            let l = 30.0;
            
            // Top Left
            painter.line_segment([center + egui::vec2(-box_size, -box_size), center + egui::vec2(-box_size + l, -box_size)], stroke);
            painter.line_segment([center + egui::vec2(-box_size, -box_size), center + egui::vec2(-box_size, -box_size + l)], stroke);
            
            // Top Right
            painter.line_segment([center + egui::vec2(box_size, -box_size), center + egui::vec2(box_size - l, -box_size)], stroke);
            painter.line_segment([center + egui::vec2(box_size, -box_size), center + egui::vec2(box_size, -box_size + l)], stroke);
            
            // Bottom Left
            painter.line_segment([center + egui::vec2(-box_size, box_size), center + egui::vec2(-box_size + l, box_size)], stroke);
            painter.line_segment([center + egui::vec2(-box_size, box_size), center + egui::vec2(-box_size, box_size - l)], stroke);
            
            // Bottom Right
            painter.line_segment([center + egui::vec2(box_size, box_size), center + egui::vec2(box_size - l, box_size)], stroke);
            painter.line_segment([center + egui::vec2(box_size, box_size), center + egui::vec2(box_size, box_size - l)], stroke);

            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "[ DROP DATA HERE ]",
                egui::FontId::proportional(24.0),
                COLOR_TEXT,
            );
            
            ctx.request_repaint(); 
        }
    }
}

impl CubeConvertApp {
    fn start_conversion(&mut self, ctx: egui::Context) {
        self.is_converting = true;
        self.status_msg = "INITIALIZING...".to_string();
        self.show_error_popup = false;
        self.progress_current = 0;
        self.progress_total = 0;
        self.file_fractions.clear();
        self.current_file.clear();
        self.time_active = 0.0;

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
                0 => converters::convert_wind(&path, is_folder, prog_tx.clone(), cancel.clone()),
                1 => converters::convert_bpm(&path, is_folder, prog_tx.clone(), cancel.clone()),
                2 => converters::convert_clouds(&path, is_folder, clouds_stitch, prog_tx.clone(), cancel.clone()),
                3 => converters::convert_rgb(&path, is_folder, prog_tx.clone(), cancel.clone()),
                4 => converters::convert_text(&path, is_folder, color, prog_tx.clone(), cancel.clone()),
                5 => converters::convert_slideshow(&path, is_folder, prog_tx.clone(), cancel.clone()),
                _ => Err("Unknown mode".into()),
            };

            if let Err(e) = result {
                if !cancel.load(Ordering::Relaxed) && e != "Cancelled." {
                    let _ = prog_tx.send(Progress::Error {
                        name: "Batch".into(),
                        error: e,
                    });
                }
            }
            drop(prog_tx);
            let _ = tx_done.send(AppMessage::Finished);
            ctx.request_repaint();
        });
    }
}

fn load_icon() -> Option<egui::IconData> {
    if let Ok(img) = image::open("assets/icon.png") {
        let rgba = img.into_rgba8();
        let (width, height) = rgba.dimensions();
        return Some(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        });
    }
    None
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([700.0, 520.0]);
    
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Cube-Convert",
        options,
        Box::new(|_cc| Box::new(CubeConvertApp::default())),
    )
}