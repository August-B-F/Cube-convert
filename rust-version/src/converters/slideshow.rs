use std::fs;
use std::path::{Path, PathBuf};

use super::{shared, CancelFlag, ProgressTx};

pub fn convert_slideshow(
    folder_path: &Path,
    is_folder: bool,
    tx: ProgressTx,
    cancel: CancelFlag,
) -> Result<(), String> {
    if !is_folder {
        return Err("Slideshow mode requires selecting a FOLDER containing images.".into());
    }

    let out = folder_path.with_file_name(format!("{}_slideshow.mp4", folder_path.file_name().unwrap_or_default().to_string_lossy()));
    
    let mut files: Vec<PathBuf> = fs::read_dir(folder_path)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            ext == "png" || ext == "jpg" || ext == "jpeg"
        })
        .collect();
    
    files.sort();
    if files.is_empty() {
        return Err("No PNG/JPG images found in the selected folder.".into());
    }

    let _ = tx.send(super::Progress::Init { total: 1 });
    let stem = folder_path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let _ = tx.send(super::Progress::Start { name: stem.clone() });

    let tmp_dir = shared::make_temp_dir("slideshow")?;
    let concat_file = tmp_dir.join("concat.txt");
    let mut content = String::new();
    
    for f in &files {
        content.push_str(&format!("file '{}'\n", f.to_string_lossy().replace("'", "'\\''")));
        content.push_str("duration 4.0\n");
    }
    if let Some(last) = files.last() {
        content.push_str(&format!("file '{}'\n", last.to_string_lossy().replace("'", "'\\''")));
    }
    
    fs::write(&concat_file, content).map_err(|e| e.to_string())?;

    let total_frames = files.len() * 4 * 25; 
    
    let args: Vec<String> = vec![
        "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(), "-stats".into(),
        "-f".into(), "concat".into(), "-safe".into(), "0".into(),
        "-i".into(), concat_file.to_string_lossy().to_string(),
        "-r".into(), "25".into(), "-c:v".into(), "libx264".into(),
        "-preset".into(), shared::ffmpeg_preset(), "-pix_fmt".into(), "yuv420p".into(),
        out.to_string_lossy().to_string()
    ];

    let result = shared::run_ffmpeg(&args, Some(total_frames), &tx, &stem, cancel.clone());
    
    if !cancel.load(std::sync::atomic::Ordering::Relaxed) {
        let _ = tx.send(super::Progress::Done { name: stem });
    }
    
    let _ = fs::remove_dir_all(&tmp_dir);
    
    result
}
