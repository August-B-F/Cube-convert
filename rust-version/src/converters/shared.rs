use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Walk a single file or every PDF in a folder and call `f` on each.
pub fn each_pdf<F>(path: &Path, is_folder: bool, mut f: F) -> Result<(), String>
where
    F: FnMut(&Path, &str) -> Result<(), String>,
{
    if is_folder {
        for entry in fs::read_dir(path).map_err(|e| format!("read_dir {}: {e}", path.display()))? {
            let p = entry.map_err(|e| e.to_string())?.path();
            if p.extension().and_then(|e| e.to_str()) == Some("pdf") {
                let stem = p.file_stem().unwrap().to_string_lossy().to_string();
                f(&p, &stem)?;
            }
        }
    } else {
        if path.extension().and_then(|e| e.to_str()) != Some("pdf") {
            return Err("Selected file is not a PDF".into());
        }
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        f(path, &stem)?;
    }
    Ok(())
}

pub fn make_temp_dir(tag: &str) -> Result<PathBuf, String> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let dir = base.join(format!("cube-convert_{tag}_{pid}_{ts}"));
    fs::create_dir_all(&dir).map_err(|e| format!("create temp dir {}: {e}", dir.display()))?;
    Ok(dir)
}

pub fn run_cmd(program: &str, args: &[String]) -> Result<(), String> {
    if verbose() {
        eprintln!("[cube] RUN: {} {}", program, args.join(" "));
        eprintln!("[cube] CWD: {}", std::env::current_dir().ok().map(|p| p.display().to_string()).unwrap_or_else(|| "?".into()));
        eprintln!("[cube] PATH: {}", std::env::var("PATH").unwrap_or_default());
    }

    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| {
            format!(
                "failed to spawn '{program}': {e} (os error {}). Hint: install it or set env var CUBE_FFMPEG/CUBE_PDFTOPPM. PATH={}",
                e.raw_os_error().unwrap_or(-1),
                std::env::var("PATH").unwrap_or_default()
            )
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("'{program}' exited with status: {status}"))
    }
}
