use std::sync::{Arc, atomic::AtomicBool};
use std::process::Command;
use std::path::{Path, PathBuf};
use std::fs;

pub type ProgressTx = crossbeam_channel::Sender<Progress>;

#[derive(Clone, Debug)]
pub enum Progress {
    Init { total: usize },
    Start { name: String },
    Update { name: String, fraction: f32 },
    Done { name: String },
    Error { name: String, error: String },
}

pub fn check_ffmpeg() -> Result<(), String> {
    match Command::new("ffmpeg").arg("-version").output() {
        Ok(_) => Ok(()),
        Err(_) => Err("FFmpeg not found. Please install FFmpeg and make sure it's in your system PATH.".into())
    }
}

pub fn is_cancelled(cancel_flag: &Arc<AtomicBool>) -> Result<(), String> {
    if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
        Err("Cancelled.".into())
    } else {
        Ok(())
    }
}

pub fn get_files(path: &Path, is_folder: bool, ext: &str) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    if is_folder {
        let entries = fs::read_dir(path).map_err(|e| format!("Failed to read directory: {}", e))?;
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().unwrap_or_default().to_string_lossy().to_lowercase() == ext {
                    files.push(path);
                }
            }
        }
    } else {
        if path.extension().unwrap_or_default().to_string_lossy().to_lowercase() == ext {
             files.push(path.to_path_buf());
        } else {
             return Err(format!("Selected file is not a .{}", ext));
        }
    }
    
    if files.is_empty() {
        return Err(format!("No .{} files found", ext));
    }
    
    // Sort alphabetically for consistency
    files.sort();
    
    Ok(files)
}

pub fn parse_pdf_text(path: &Path, _tx: &ProgressTx, _name: &str) -> Result<Vec<String>, String> {
    let text = pdf_extract::extract_text(path).map_err(|e| format!("Failed to extract text from PDF: {}", e))?;
    let lines: Vec<String> = text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
        
    if lines.is_empty() {
        return Err("PDF is empty or contains no extractable text.".into());
    }
    Ok(lines)
}
