use std::fs;
use std::path::Path;
use rusttype::{Font, Scale};
use super::{shared, CancelFlag, ProgressTx};

pub fn convert_text(
    file_path: &Path,
    is_folder: bool,
    color: [u8; 3],
    tx: ProgressTx,
    cancel: CancelFlag,
) -> Result<(), String> {
    let font_path = Path::new("assets/JdLcdRoundedRegular-vXwE.ttf");
    if !font_path.exists() {
        return Err("Font file assets/JdLcdRoundedRegular-vXwE.ttf not found".into());
    }
    let font_data = fs::read(font_path).map_err(|e| e.to_string())?;
    let font = Font::try_from_vec(font_data).ok_or("Failed to load font")?;

    shared::process_files(file_path, is_folder, tx, cancel.clone(), |pdf, name, prog_tx| {
        let out = pdf.with_file_name(format!("{name}.mp4"));
        let partial_out = out.with_extension("tmp.mp4");
        if out.exists() {
            return Ok(());
        }

        let text_raw = shared::extract_text(pdf)?;
        
        let mut cleaned = String::with_capacity(text_raw.len());
        let mut last_was_space = false;
        for c in text_raw.chars() {
            if c.is_whitespace() {
                if !last_was_space {
                    cleaned.push(' ');
                    last_was_space = true;
                }
            } else {
                cleaned.push(c);
                last_was_space = false;
            }
        }
        let text = cleaned.trim().to_string();
        if text.trim().is_empty() { return Err("No text found".into()); }

        let frame_w = 600u32;
        let frame_h = 224u32;
        
        let fps = 30.0f32; 
        
        let speed_px_per_frame = 5; 
        let speed_px_per_sec = (speed_px_per_frame as f32) * fps; 
        
        let font_size_px = (frame_h as f32 * 0.6).round() as u32;
        let scale = Scale::uniform(font_size_px as f32);

        let mut total_text_w = 0.0f32;
        let mut last = None;
        for ch in text.chars() {
            let g = font.glyph(ch);
            if let Some(prev) = last { total_text_w += font.pair_kerning(scale, prev, g.id()); }
            total_text_w += g.clone().scaled(scale).h_metrics().advance_width;
            last = Some(g.id());
        }

        let total_scroll_px = total_text_w + frame_w as f32;
        let duration = (total_scroll_px / speed_px_per_sec) + 1.0;
        let total_frames = (duration * fps).ceil() as usize;

        let tmp_dir = shared::make_temp_dir("text")?;
        
        let text_file = tmp_dir.join("scroll_text.txt");
        fs::write(&text_file, &text).map_err(|e| e.to_string())?;

        let hex_color = format!("0x{:02x}{:02x}{:02x}", color[0], color[1], color[2]);
        
        let font_p = font_path.to_string_lossy().replace('\\', "/").replace(':', "\\:");
        let text_p = text_file.to_string_lossy().replace('\\', "/").replace(':', "\\:");

        // Added tmix to simulate motion blur. 1 2 1 weights the current frame twice as heavily 
        // as the adjacent frames so it stays readable, but gains a soft smeared edge to kill the wagon-wheel effect.
        let filter_str = format!(
            "color=c=black:s={frame_w}x{frame_h}:d={duration} [bg]; \
            [bg]drawtext=fontfile='{font_p}':textfile='{text_p}':\
            fontcolor={hex_color}:fontsize={fontsize}:y=(h-text_h)/2:\
            x=w-n*{speed} [txt]; \
            [txt]tmix=frames=3:weights=1 2 1 [out]",
            duration=duration,
            font_p=font_p,
            text_p=text_p,
            hex_color=hex_color,
            fontsize=font_size_px,
            speed=speed_px_per_frame
        );

        let mut args: Vec<String> = vec![
            "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(), "-stats".into(),
            "-filter_complex".into(), filter_str,
            "-map".into(), "[out]".into(),
            "-t".into(), duration.to_string(),
            "-r".into(), fps.to_string(), "-c:v".into(), "libx264".into(),
            
            "-preset".into(), "veryfast".into(), 
            "-crf".into(), "32".into(), 
            "-tune".into(), "animation".into(),
            "-g".into(), "150".into(),
            "-pix_fmt".into(), "yuv420p".into(),
        ];
        
        if is_folder {
            args.push("-threads".into());
            args.push("2".into());
        }
        args.push(partial_out.to_string_lossy().to_string());

        let result = shared::run_ffmpeg(&args, Some(total_frames), prog_tx, name, cancel.clone());

        let _ = fs::remove_dir_all(&tmp_dir);
        
        if result.is_ok() && !cancel.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = fs::rename(&partial_out, &out);
        } else {
            let _ = fs::remove_file(&partial_out);
        }

        result
    })
}
