use crossbeam_channel::Sender;
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{CancelFlag, Progress, ProgressTx};

pub fn verbose() -> bool {
    matches!(std::env::var("CUBE_VERBOSE").as_deref(), Ok("1") | Ok("true") | Ok("TRUE"))
}

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

/// Collect all PDF files to process.
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
    // Sort for deterministic order
    files.sort();
    Ok(files)
}

/// Process files in parallel using rayon.
pub fn process_files<F>(
    path: &Path,
    is_folder: bool,
    tx: ProgressTx,
    cancel: CancelFlag,
    process_fn: F,
) -> Result<(), String>
where
    F: Fn(&Path, &str) -> Result<(), String> + Sync + Send,
{
    let files = collect_pdfs(path, is_folder)?;
    if files.is_empty() {
        return Err("No PDF files found".into());
    }

    // Send total count
    let _ = tx.send(Progress::Init { total: files.len() });

    // Process in parallel
    files.par_iter().for_each(|pdf| {
        // Check cancellation
        if cancel.load(Ordering::Relaxed) {
            return;
        }

        let stem = pdf.file_stem().unwrap().to_string_lossy().to_string();
        let _ = tx.send(Progress::Start { name: stem.clone() });

        match process_fn(pdf, &stem) {
            Ok(_) => {
                let _ = tx.send(Progress::Done);
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
        Err("Operation cancelled".into())
    } else {
        Ok(())
    }
}

pub fn make_temp_dir(tag: &str) -> Result<PathBuf, String> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    // Use thread ID to avoid collisions in parallel execution
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
    if verbose() {
        eprintln!("[cube] RUN: {} {}", program, args.join(" "));
    }

    // When running many parallel jobs, we want to prevent ffmpeg from 
    // trying to use all cores for each job, which causes thrashing.
    // However, since we invoke this binary via Command::new, we can't easily set
    // environment variables inside the new process unless we add .env().
    // ffmpeg automatically detects core count.
    // It's usually better to let the OS scheduler handle it, or add -threads 1.
    // We'll let it be for now, assuming the user has enough cores.
    
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| {
            format!(
                "failed to spawn '{program}': {e} (os error {}). Hint: install it or set env var CUBE_FFMPEG/CUBE_PDFTOPPM.",
                e.raw_os_error().unwrap_or(-1)
            )
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("'{program}' exited with status: {status}"))
    }
}
