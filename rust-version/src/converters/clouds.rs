use std::fs;
use std::path::{Path, PathBuf};
use image::{imageops, RgbImage};
use super::{shared, CancelFlag, ProgressTx};

fn list_images(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("read_dir {}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            ext == "png" || ext == "jpg" || ext == "jpeg"
        })
        .collect();
    files.sort();
    Ok(files)
}

pub fn convert_clouds(
    file_path: &Path,
    is_folder: bool,
    stitch_images: bool, // NEW: toggles between Image Stitching vs PDF Batch
    tx: ProgressTx,
    cancel: CancelFlag,
) -> Result<(), String> {
    if is_folder && stitch_images {
        let out = file_path.with_file_name(format!("{}_clouds.mp4", file_path.file_name().unwrap_or_default().to_string_lossy()));
        if out.exists() { return Ok(()); }

        let _ = tx.send(super::Progress::Init { total: 1 });
        let stem = file_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let _ = tx.send(super::Progress::Start { name: stem.clone() });

        let page_files = list_images(file_path)?;
        if page_files.is_empty() {
            return Err("No PNG/JPG images found in the selected folder".into());
        }

        let (w, h) = (750u32, 360u32);
        let total_w = w * (page_files.len() as u32 + 1);
        let mut strip = RgbImage::new(total_w, h);

        for (i, p) in page_files.iter().enumerate() {
            let img = image::open(p).map_err(|e| format!("open {}: {e}", p.display()))?.to_rgb8();
            let resized = imageops::resize(&img, w, h, imageops::FilterType::Triangle);
            imageops::replace(&mut strip, &resized, (i as i64) * w as i64, 0);
        }

        let tmp_dir = shared::make_temp_dir("clouds")?;
        let strip_png = tmp_dir.join("strip.png");
        image::DynamicImage::ImageRgb8(strip).save(&strip_png).map_err(|e| format!("save strip: {e}"))?;

        let video_dur = 12.0 * 60.0;
        let fps = 25.0;
        let total_frames = (video_dur * fps) as usize;
        let vf = format!("crop={w}:{h}:x=(iw-{w})*t/{video_dur}:y=0");
        
        let args: Vec<String> = vec![
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(), "-stats".into(),
            "-loop".into(), "1".into(), "-i".into(), strip_png.to_string_lossy().to_string(),
            "-vf".into(), vf, "-t".into(), video_dur.to_string(),
            "-r".into(), fps.to_string(), "-c:v".into(), "libx264".into(),
            "-preset".into(), shared::ffmpeg_preset(), "-pix_fmt".into(), "yuv420p".into(),
            out.to_string_lossy().to_string(),
        ];
        
        shared::run_ffmpeg(&args, Some(total_frames), &tx, &stem)?;

        let _ = tx.send(super::Progress::Done { name: stem });
        let _ = fs::remove_dir_all(&tmp_dir);
        Ok(())
    } else {
        // Standard PDF processing (Single PDF OR Folder of PDFs)
        shared::process_files(file_path, is_folder, tx, cancel, |pdf, name, prog_tx| {
            let out = pdf.with_file_name(format!("{name}.mp4"));
            if out.exists() {
                return Ok(());
            }

            let tmp_dir = shared::make_temp_dir("clouds")?;
            let pages_dir = tmp_dir.join("pages");
            fs::create_dir_all(&pages_dir).map_err(|e| e.to_string())?;

            let pdftoppm = shared::pdftoppm_bin();
            let dpi = shared::pdf_render_dpi();
            let prefix = pages_dir.join("page");
            let args: Vec<String> = vec![
                "-r".into(), dpi.to_string(), "-png".into(),
                pdf.to_string_lossy().to_string(), prefix.to_string_lossy().to_string(),
            ];
            shared::run_cmd(&pdftoppm, &args)?;

            let page_files = list_images(&pages_dir)?; 
            if page_files.is_empty() {
                let _ = fs::remove_dir_all(&tmp_dir);
                return Err("pdftoppm produced no PNGs".into());
            }

            let (w, h) = (750u32, 360u32);
            let total_w = w * (page_files.len() as u32 + 1);
            let mut strip = RgbImage::new(total_w, h);

            for (i, p) in page_files.iter().enumerate() {
                let img = image::open(p).map_err(|e| format!("open {}: {e}", p.display()))?.to_rgb8();
                let resized = imageops::resize(&img, w, h, imageops::FilterType::Triangle);
                imageops::replace(&mut strip, &resized, (i as i64) * w as i64, 0);
            }

            let strip_png = tmp_dir.join("strip.png");
            image::DynamicImage::ImageRgb8(strip).save(&strip_png).map_err(|e| format!("save strip: {e}"))?;

            let video_dur = 12.0 * 60.0;
            let fps = 25.0;
            let total_frames = (video_dur * fps) as usize;
            let vf = format!("crop={w}:{h}:x=(iw-{w})*t/{video_dur}:y=0");
            
            let mut args: Vec<String> = vec![
                "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(), "-stats".into(),
                "-loop".into(), "1".into(), "-i".into(), strip_png.to_string_lossy().to_string(),
                "-vf".into(), vf, "-t".into(), video_dur.to_string(),
                "-r".into(), fps.to_string(), "-c:v".into(), "libx264".into(),
                "-preset".into(), shared::ffmpeg_preset(), "-pix_fmt".into(), "yuv420p".into(),
            ];
            
            if is_folder {
                args.push("-threads".into());
                args.push("2".into());
            }
            args.push(out.to_string_lossy().to_string());
            
            shared::run_ffmpeg(&args, Some(total_frames), prog_tx, name)?;

            let _ = fs::remove_dir_all(&tmp_dir);
            Ok(())
        })
    }
}
