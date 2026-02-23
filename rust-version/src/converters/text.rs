use std::fs;
use std::path::Path;

use image::RgbImage;
use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale};

use super::shared;

pub fn convert_text(file_path: &Path, is_folder: bool, color: [u8; 3]) -> Result<(), String> {
    let font_path = Path::new("assets/JdLcdRoundedRegular-vXwE.ttf");
    if !font_path.exists() {
        return Err("Font file assets/JdLcdRoundedRegular-vXwE.ttf not found".into());
    }
    let font_data = fs::read(font_path).map_err(|e| e.to_string())?;
    let font = Font::try_from_vec(font_data).ok_or("Failed to load font")?;

    shared::each_pdf(file_path, is_folder, |pdf, name| {
        let out = pdf.with_file_name(format!("{name}.mp4"));
        if out.exists() {
            return Ok(());
        }

        let mut text = shared::extract_text(pdf)?;
        text = text.replace('\n', " ");
        if text.trim().is_empty() {
            return Err("No text found in PDF".into());
        }

        let chunk_size = 5;
        let chunks: Vec<String> = text
            .chars()
            .collect::<Vec<_>>()
            .chunks(chunk_size)
            .map(|c| c.iter().collect())
            .collect();

        let frame_w = 600u32;
        let frame_h = 225u32;
        let fps = 30.0f32;
        let speed_px_per_frame = 5.0f32; // old behavior
        let speed_px_per_sec = speed_px_per_frame * fps;

        let scale = Scale::uniform(frame_h as f32 * 0.6);

        let measure = |s: &str| -> f32 {
            let mut w = 0.0f32;
            let mut last = None;
            for ch in s.chars() {
                let g = font.glyph(ch);
                if let Some(prev) = last {
                    w += font.pair_kerning(scale, prev, g.id());
                }
                w += g.clone().scaled(scale).h_metrics().advance_width;
                last = Some(g.id());
            }
            w
        };

        let total_text_w: f32 = chunks.iter().map(|c| measure(c)).sum();
        let total_scroll_px = total_text_w + frame_w as f32;
        let duration = (total_scroll_px / speed_px_per_sec).max(1.0);

        // Render a single long strip image once, then let ffmpeg do the scrolling crop.
        let strip_w = (frame_w as f32 + total_text_w + frame_w as f32).ceil() as u32;
        let mut strip = RgbImage::new(strip_w, frame_h); // black background

        let mut x = frame_w as f32; // start off-screen to the right
        let text_color = image::Rgb(color);
        let y = (frame_h as f32 / 2.0 - scale.y / 2.0) as i32;

        for chunk in &chunks {
            let xi = x.round() as i32;
            draw_text_mut(&mut strip, text_color, xi, y, scale, &font, chunk);
            x += measure(chunk);
        }

        let tmp_dir = shared::make_temp_dir("text")?;
        let strip_png = tmp_dir.join("text_strip.png");
        image::DynamicImage::ImageRgb8(strip)
            .save(&strip_png)
            .map_err(|e| format!("save {}: {e}", strip_png.display()))?;

        let ffmpeg = shared::ffmpeg_bin();
        let preset = shared::ffmpeg_preset();
        let vf = format!(
            "crop={frame_w}:{frame_h}:x=(iw-{frame_w})*t/{duration}:y=0"
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
            duration.to_string(),
            "-r".into(),
            fps.to_string(),
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
