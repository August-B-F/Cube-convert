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

    let tmp_dir = shared::make_temp_dir("slideshow")?;
    
    // Copy files preserving their ORIGINAL extension, but with sequential naming.
    // FFmpeg's image2 demuxer gets confused if you rename a PNG to .jpg
    // We will use a concat demuxer again, but a properly generated one,
    // combined with the safety of the scale/pad filter.
    let concat_file = tmp_dir.join("concat.txt");
    let mut concat_content = String::new();

    for (i, src_file) in files.iter().enumerate() {
        let ext = src_file.extension().and_then(|e| e.to_str()).unwrap_or("png");
        let safe_name = format!("img_{:05}.{}", i + 1, ext);
        let dest = tmp_dir.join(&safe_name);
        
        if let Err(e) = fs::copy(src_file, &dest) {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Err(format!("Failed to copy image: {}", e));
        }

        concat_content.push_str(&format!("file '{}'\n", safe_name));
        concat_content.push_str("duration 4.0\n");
    }
    
    // Concat requires the last file to be repeated without a duration to finish correctly
    if let Some(last_file) = files.last() {
        let ext = last_file.extension().and_then(|e| e.to_str()).unwrap_or("png");
        concat_content.push_str(&format!("file 'img_{:05}.{}'\n", files.len(), ext));
    }

    fs::write(&concat_file, concat_content).map_err(|e| e.to_string())?;

    let total_frames = files.len() * 4 * 25; 
    
    // The previous scale filter was mathematically dangerous.
    // If it scaled a 699x241 image down, the resulting dimensions might be odd numbers.
    // pad=1920:1080 requires the input to have even dimensions or it crashes libx264.
    // Adding `scale=...:eval=init` and wrapping the iw/ih math in `ceil` or explicitly 
    // forcing the scale to output even numbers before padding fixes this.
    // We force the scale to output even numbers by using 'trunc(ow/2)*2:trunc(oh/2)*2' equivalent:
    let filter = "scale='min(1920,iw)':'min(1080,ih)':force_original_aspect_ratio=decrease,scale=trunc(iw/2)*2:trunc(ih/2)*2,pad=1920:1080:(1920-iw)/2:(1080-ih)/2,format=yuv420p";

    let args: Vec<String> = vec![
        "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(), "-stats".into(),
        "-f".into(), "concat".into(), "-safe".into(), "0".into(),
        "-i".into(), "concat.txt".into(),
        "-vf".into(), filter.into(),
        "-r".into(), "25".into(), 
        "-c:v".into(), "libx264".into(),
        "-preset".into(), shared::ffmpeg_preset(), 
        partial_out.to_string_lossy().to_string()
    ];

    let original_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    std::env::set_current_dir(&tmp_dir).map_err(|e| e.to_string())?;
    
    let result = shared::run_ffmpeg(&args, Some(total_frames), &tx, &stem, cancel.clone());
    
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
