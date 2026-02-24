use crossbeam_channel::Sender;
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use super::{CancelFlag, Progress, ProgressTx};

pub fn ffmpeg_bin() -> String {
    std::env::var("CUBE_FFMPEG").unwrap_or_else(|_| "ffmpeg".to_string())
}

pub fn pdftoppm_bin() -> String {
    std::env::var("CUBE_PDFTOPPM").unwrap_or_else(|_| "pdftoppm".to_string())
}

pub fn pdftotext_bin() -> String {
    std::env::var("CUBE_PDFTOTEXT").unwrap_or_else(|_| "pdftotext".to_string())
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
    // Try using pdftotext CLI first (much more reliable for large/complex PDFs)
    // Poppler's pdftotext almost never drops pages unlike the rust pdf_extract crate.
    let program = pdftotext_bin();
    let mut cmd = Command::new(&program);
    cmd.arg("-enc").arg("UTF-8"); // Force UTF-8 output to prevent byte translation errors
    cmd.arg("-layout");
    cmd.arg(pdf_path);
    cmd.arg("-");
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    if let Ok(output) = cmd.output() {
        if output.status.success() {
            // Use from_utf8_lossy instead of from_utf8 so that a single weird character (like a smart quote)
            // won't cause the entire text extraction to fail and fallback to the broken pdf_extract crate.
            let text = String::from_utf8_lossy(&output.stdout).into_owned();
            if !text.trim().is_empty() {
                return Ok(text);
            }
        }
    }

    // Fallback to pdf_extract crate if pdftotext is missing
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
                if !cancel.load(Ordering::Relaxed) && e != "Cancelled." {
                    let _ = tx.send(Progress::Error {
                        name: stem,
                        error: e,
                    });
                }
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
    let mut cmd = Command::new(program);
    cmd.args(args);
    
    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW to hide console flashes

    let status = cmd.status().map_err(|e| {
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
    cancel: CancelFlag,
) -> Result<(), String> {
    use std::io::{BufRead, BufReader, Read};
    use std::sync::mpsc;
    use std::thread;
    use std::sync::Mutex;

    let program = ffmpeg_bin();
    let mut cmd = Command::new(&program);
    
    let mut opt_args = vec!["-thread_queue_size".to_string(), "512".to_string()];
    opt_args.extend_from_slice(args);
    
    cmd.args(&opt_args).stderr(Stdio::piped()).stdout(Stdio::null());

    #[cfg(windows)]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let mut child = cmd.spawn().map_err(|e| format!("failed to spawn '{program}': {e}"))?;
    let stderr = child.stderr.take().unwrap();
    
    let (err_tx, err_rx) = mpsc::channel();
    
    let last_error = Arc::new(Mutex::new(String::new()));
    let last_error_clone = last_error.clone();
    
    thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut buffer = Vec::new();
        while let Ok(bytes_read) = reader.read_until(b'\r', &mut buffer) {
            if bytes_read == 0 { 
                let mut rest = String::new();
                let _ = reader.read_to_string(&mut rest);
                if !rest.trim().is_empty() {
                    let mut lock = last_error_clone.lock().unwrap();
                    *lock = rest;
                }
                break; 
            }
            if let Ok(line) = String::from_utf8(buffer.clone()) {
                if line.contains("Error") || line.contains("Invalid") || line.contains("Could not") {
                    let mut lock = last_error_clone.lock().unwrap();
                    *lock = line.clone();
                }
                let _ = err_tx.send(line);
            }
            buffer.clear();
        }
    });

    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Cancelled.".to_string());
        }

        match err_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => {
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
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Ok(Some(status)) = child.try_wait() {
                    if status.success() {
                        return Ok(());
                    } else {
                        let err_msg = last_error.lock().unwrap().clone();
                        if err_msg.is_empty() {
                            return Err(format!("ffmpeg exited with {}", status));
                        } else {
                            return Err(format!("FFmpeg Error: {}", err_msg.trim()));
                        }
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        let err_msg = last_error.lock().unwrap().clone();
        if err_msg.is_empty() {
            Err(format!("ffmpeg exited with {}", status))
        } else {
            Err(format!("FFmpeg Error: {}", err_msg.trim()))
        }
    }
}

pub fn run_ffmpeg_stream<F>(
    args: &[String],
    tx: &ProgressTx,
    name: &str,
    cancel: CancelFlag,
    mut stream_fn: F,
) -> Result<(), String>
where
    F: FnMut(&mut std::process::ChildStdin) -> Result<(), String>,
{
    use std::io::{BufRead, BufReader, Read};
    use std::sync::Mutex;

    let program = ffmpeg_bin();
    let mut cmd = Command::new(&program);
    
    let mut opt_args = vec!["-thread_queue_size".to_string(), "512".to_string()];
    opt_args.extend_from_slice(args);
    
    cmd.args(&opt_args)
       .stdin(Stdio::piped())
       .stderr(Stdio::piped())
       .stdout(Stdio::null());

    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    let mut child = cmd.spawn().map_err(|e| format!("failed to spawn '{program}': {e}"))?;
    let stderr = child.stderr.take().unwrap();
    let mut stdin = child.stdin.take().unwrap();

    let last_error = Arc::new(Mutex::new(String::new()));
    let last_error_clone = last_error.clone();
    
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut buffer = Vec::new();
        while let Ok(bytes_read) = reader.read_until(b'\r', &mut buffer) {
            if bytes_read == 0 { 
                let mut rest = String::new();
                let _ = reader.read_to_string(&mut rest);
                if !rest.trim().is_empty() {
                    let mut lock = last_error_clone.lock().unwrap();
                    *lock = rest;
                }
                break; 
            }
            if let Ok(line) = String::from_utf8(buffer.clone()) {
                if line.contains("Error") || line.contains("Invalid") || line.contains("Could not") {
                    let mut lock = last_error_clone.lock().unwrap();
                    *lock = line.clone();
                }
            }
            buffer.clear();
        }
    });

    if let Err(e) = stream_fn(&mut stdin) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e);
    }
    drop(stdin); // Flush pipe

    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Cancelled.".to_string());
        }
        if let Ok(Some(status)) = child.try_wait() {
            if status.success() {
                return Ok(());
            } else {
                let err_msg = last_error.lock().unwrap().clone();
                if err_msg.is_empty() {
                    return Err(format!("ffmpeg exited with {}", status));
                } else {
                    return Err(format!("FFmpeg Error: {}", err_msg.trim()));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
