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
    let partial_out = out.with_extension("tmp.mp4");
    
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

    // Create temp directory and copy images with sequential names
    let tmp_dir = shared::make_temp_dir("slideshow")?;
    
    // Copy images with a unified extension for the image2 sequence demuxer
    for (i, src_file) in files.iter().enumerate() {
        // Force .jpg extension so FFmpeg's pattern matching reads them all seamlessly.
        // FFmpeg reads the actual file headers, so PNGs disguised as JPGs decode perfectly.
        let dest = tmp_dir.join(format!("img_{:05}.jpg", i + 1));
        
        if let Err(e) = fs::copy(src_file, &dest) {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Err(format!("Failed to copy image: {}", e));
        }
    }

    let total_frames = files.len() * 4 * 25; // 4 seconds per image at 25fps
    
    // Standardize to a 1080p canvas.
    // scale: shrinks large images or grows small ones to fit within 1920x1080 while maintaining aspect ratio.
    // pad: fills the remaining space with black borders to hit exactly 1920x1080.
    // format=yuv420p guarantees even dimensions, eliminating the "Invalid argument" libx264 error.
    let filter = "scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,format=yuv420p";

    let args: Vec<String> = vec![
        "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(), "-stats".into(),
        "-framerate".into(), "0.25".into(), // 1 frame every 4 seconds
        "-i".into(), "img_%05d.jpg".into(),
        "-vf".into(), filter.into(),
        "-r".into(), "25".into(), 
        "-c:v".into(), "libx264".into(),
        "-preset".into(), shared::ffmpeg_preset(), 
        partial_out.to_string_lossy().to_string()
    ];

    // Change working directory to temp dir so FFmpeg finds the sequence naturally
    let original_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    std::env::set_current_dir(&tmp_dir).map_err(|e| e.to_string())?;
    
    let result = shared::run_ffmpeg(&args, Some(total_frames), &tx, &stem, cancel.clone());
    
    // Restore original directory immediately
    let _ = std::env::set_current_dir(original_dir);
    let _ = fs::remove_dir_all(&tmp_dir);

    if result.is_ok() && !cancel.load(std::sync::atomic::Ordering::Relaxed) {
        let _ = tx.send(super::Progress::Done { name: stem });
        let _ = fs::rename(&partial_out, &out);
    } else {
        let _ = fs::remove_file(&partial_out);
    }
    
    result
}
