use std::fs;
use std::path::{Path, PathBuf};

use image::{imageops, RgbImage};

use super::shared;

fn list_pngs(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("read_dir {}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("png"))
        .collect();
    files.sort();
    Ok(files)
}

pub fn convert_clouds(file_path: &Path, is_folder: bool) -> Result<(), String> {
    shared::each_pdf(file_path, is_folder, |pdf, name| {
        let out = pdf.with_file_name(format!("{name}.mp4"));
        if out.exists() {
            return Ok(());
        }

        let tmp_dir = shared::make_temp_dir("clouds")?;
        let pages_dir = tmp_dir.join("pages");
        fs::create_dir_all(&pages_dir).map_err(|e| e.to_string())?;

        // Render PDF pages -> PNGs
        let pdftoppm = shared::pdftoppm_bin();
        let dpi = shared::pdf_render_dpi();
        let prefix = pages_dir.join("page");
        let args: Vec<String> = vec![
            "-r".into(),
            dpi.to_string(),
            "-png".into(),
            pdf.to_string_lossy().to_string(),
            prefix.to_string_lossy().to_string(),
        ];
        shared::run_cmd(&pdftoppm, &args)?;

        let page_files = list_pngs(&pages_dir)?;
        if page_files.is_empty() {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Err(format!(
                "pdftoppm produced no PNGs in {} (input={})",
                pages_dir.display(),
                pdf.display()
            ));
        }

        // Stitch into one strip image (fast: only N pages, not per-frame)
        let (w, h) = (750u32, 360u32);
        let total_w = w * (page_files.len() as u32 + 1); // +1 blank at end
        let mut strip = RgbImage::new(total_w, h); // initialized black

        for (i, p) in page_files.iter().enumerate() {
            let img = image::open(p)
                .map_err(|e| format!("open {}: {e}", p.display()))?
                .to_rgb8();
            let resized = imageops::resize(&img, w, h, imageops::FilterType::Triangle);
            imageops::replace(&mut strip, &resized, (i as i64) * w as i64, 0);
        }

        let strip_png = tmp_dir.join("strip.png");
        image::DynamicImage::ImageRgb8(strip)
            .save(&strip_png)
            .map_err(|e| format!("save {}: {e}", strip_png.display()))?;

        // Let ffmpeg do the scrolling crop (much faster than generating 18k frames in Rust)
        let ffmpeg = shared::ffmpeg_bin();
        let preset = shared::ffmpeg_preset();
        let video_dur = 12.0 * 60.0;
        let vf = format!(
            "crop={w}:{h}:x=(iw-{w})*t/{video_dur}:y=0"
        );
        let args: Vec<String> = vec![
            "-y".into(),
            "-hide_banner".into(),
            "-loglevel".into(),
            (if shared::verbose() { "info" } else { "error" }).into(),
            "-loop".into(),
            "1".into(),
            "-i".into(),
            strip_png.to_string_lossy().to_string(),
            "-vf".into(),
            vf,
            "-t".into(),
            video_dur.to_string(),
            "-r".into(),
            "25".into(),
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            preset,
            "-pix_fmt".into(),
            "yuv420p".into(),
            out.to_string_lossy().to_string(),
        ];
        shared::run_cmd(&ffmpeg, &args)?;

        let _ = fs::remove_dir_all(&tmp_dir);
        Ok(())
    })
}
