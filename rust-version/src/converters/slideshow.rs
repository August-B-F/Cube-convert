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
    
    // Validate dimensions and copy files
    let mut reference_dimensions: Option<(u32, u32)> = None;
    
    for (i, src_file) in files.iter().enumerate() {
        // Check dimensions using image crate
        if let Ok(img) = image::io::Reader::open(src_file) {
            if let Ok(img) = img.decode() {
                let (width, height) = (img.width(), img.height());
                
                if let Some((ref_w, ref_h)) = reference_dimensions {
                    if width != ref_w || height != ref_h {
                        let _ = fs::remove_dir_all(&tmp_dir);
                        return Err(format!(
                            "Image dimension mismatch: {} is {}x{}, but first image was {}x{}. All images must have the same dimensions.",
                            src_file.file_name().unwrap_or_default().to_string_lossy(),
                            width, height, ref_w, ref_h
                        ));
                    }
                } else {
                    reference_dimensions = Some((width, height));
                }
            }
        }
        
        // Copy with sequential naming: img_0001.png, img_0002.png, etc.
        let ext = src_file.extension().and_then(|e| e.to_str()).unwrap_or("png");
        let dest = tmp_dir.join(format!("img_{:04}.{}", i + 1, ext));
        
        if let Err(e) = fs::copy(src_file, &dest) {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Err(format!("Failed to copy image: {}", e));
        }
    }

    // Create concat file with relative paths
    let concat_file = tmp_dir.join("concat.txt");
    let mut content = String::new();
    
    for i in 0..files.len() {
        let ext = files[i].extension().and_then(|e| e.to_str()).unwrap_or("png");
        content.push_str(&format!("file 'img_{:04}.{}'\n", i + 1, ext));
        content.push_str("duration 4.0\n");
    }
    // Add last image again without duration for proper ending
    if let Some(last) = files.last() {
        let ext = last.extension().and_then(|e| e.to_str()).unwrap_or("png");
        content.push_str(&format!("file 'img_{:04}.{}'\n", files.len(), ext));
    }
    
    fs::write(&concat_file, content).map_err(|e| e.to_string())?;

    let total_frames = files.len() * 4 * 25; // 4 seconds per image at 25fps
    
    let args: Vec<String> = vec![
        "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(), "-stats".into(),
        "-f".into(), "concat".into(), "-safe".into(), "0".into(),
        "-i".into(), concat_file.to_string_lossy().to_string(),
        "-vsync".into(), "vfr".into(), // Variable frame rate for better concat handling
        "-r".into(), "25".into(), "-c:v".into(), "libx264".into(),
        "-preset".into(), shared::ffmpeg_preset(), "-pix_fmt".into(), "yuv420p".into(),
        partial_out.to_string_lossy().to_string()
    ];

    // Change working directory to temp dir for relative paths
    let original_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    std::env::set_current_dir(&tmp_dir).map_err(|e| e.to_string())?;
    
    let result = shared::run_ffmpeg(&args, Some(total_frames), &tx, &stem, cancel.clone());
    
    // Restore original directory
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
