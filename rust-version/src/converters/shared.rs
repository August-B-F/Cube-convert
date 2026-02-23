use crossbeam_channel::Sender;
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{CancelFlag, Progress, ProgressTx};

pub fn ffmpeg_bin() -> String {
    std::env::var("CUBE_FFMPEG").unwrap_or_else(|_| "ffmpeg".to_string())
}

pub fn pdftoppm_bin() -> String {
    std::env::var("CUBE_PDFTOPPM").unwrap_or_else(|_| "pdftoppm".to_string())
}

pub fn ffmpeg_preset() -> String {
    std::env::var("CUBE_FFMPEG_PRESET").unwrap_or_else(|_| "ultrafast".to_string())
}

pub fn pdf_render_dpi() -> u32 {
    std::env::var("CUBE_PDF_DPI")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|&v| v >= 50 && v <= 600)
        .unwrap_or(120)
}

pub fn extract_text(pdf_path: &Path) -> Result<String, String> {
    let bytes = fs::read(pdf_path).map_err(|e| format!("read {}: {e}", pdf_path.display()))?;
    pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| format!("pdf_extract failed for {}: {e}", pdf_path.display()))
}

fn collect_pdfs(path: &Path, is_folder: bool) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    if is_folder {
        for entry in fs::read_dir(path).map_err(|e| format!("read_dir {}: {e}", path.display()))? {
            let p = entry.map_err(|e| e.to_string())?.path();
            if p.extension().and_then(|e| e.to_str()) == Some("pdf") {
                files.push(p);
            }
        }
    } else {
        if path.extension().and_then(|e| e.to_str()) != Some("pdf") {
            return Err("Selected file is not a PDF".into());
        }
        files.push(path.to_path_buf());
    }
    files.sort();
    Ok(files)
}

pub fn process_files<F>(
    path: &Path,
    is_folder: bool,
    tx: ProgressTx,
    cancel: CancelFlag,
    process_fn: F,
) -> Result<(), String>
where
    F: Fn(&Path, &str, &ProgressTx) -> Result<(), String> + Sync + Send,
{
    let files = collect_pdfs(path, is_folder)?;
    if files.is_empty() {
        return Err("No PDF files found".into());
    }

    let _ = tx.send(Progress::Init { total: files.len() });

    files.par_iter().for_each(|pdf| {
        if cancel.load(Ordering::Relaxed) {
            return;
        }

        let stem = pdf.file_stem().unwrap().to_string_lossy().to_string();
        let _ = tx.send(Progress::Start { name: stem.clone() });

        match process_fn(pdf, &stem, &tx) {
            Ok(_) => {
                let _ = tx.send(Progress::Done { name: stem });
            }
            Err(e) => {
                let _ = tx.send(Progress::Error {
                    name: stem,
                    error: e,
                });
            }
        }
    });

    if cancel.load(Ordering::Relaxed) {
        Err("Cancelled.".into())
    } else {
        Ok(())
    }
}

pub fn make_temp_dir(tag: &str) -> Result<PathBuf, String> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let tid = format!("{:?}", std::thread::current().id())
        .replace("ThreadId(", "")
        .replace(")", "");
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    
    let dir = base.join(format!("cube_{}_{}_{}_{}", tag, pid, tid, ts));
    fs::create_dir_all(&dir).map_err(|e| format!("create temp dir {}: {e}", dir.display()))?;
    Ok(dir)
}

pub fn run_cmd(program: &str, args: &[String]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| {
            format!(
                "failed to spawn '{program}': {e} (os error {}). Hint: install it.",
                e.raw_os_error().unwrap_or(-1)
            )
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("'{program}' exited with status: {status}"))
    }
}

pub fn run_ffmpeg(
    args: &[String],
    total_frames: Option<usize>,
    tx: &ProgressTx,
    name: &str,
) -> Result<(), String> {
    use std::io::{BufRead, BufReader};

    let program = ffmpeg_bin();
    let mut child = Command::new(&program)
        .args(args)
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to spawn '{program}': {e}"))?;

    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        // ffmpeg -stats usually prints progress with carriage returns (\r)
        for byte_line in reader.split(b'\r') {
            if let Ok(line) = String::from_utf8(byte_line.unwrap_or_default()) {
                let text = line.trim();
                if let Some(tf) = total_frames {
                    if text.starts_with("frame=") || text.contains("frame=") {
                        let parts: Vec<&str> = text.split_whitespace().collect();
                        if let Some(pos) = parts.iter().position(|&s| s.starts_with("frame=")) {
                            let val_str = if parts[pos] == "frame=" && pos + 1 < parts.len() {
                                parts[pos + 1]
                            } else {
                                &parts[pos][6..]
                            };
                            if let Ok(frame) = val_str.parse::<usize>() {
                                let fraction = (frame as f32 / tf as f32).clamp(0.0, 1.0);
                                let _ = tx.send(Progress::Update {
                                    name: name.to_string(),
                                    fraction,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("ffmpeg exited with {}", status))
    }
}
