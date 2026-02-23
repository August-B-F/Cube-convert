use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use super::shared;

fn lerp_color(a: [u8; 3], b: [u8; 3], steps: usize) -> Vec<[u8; 3]> {
    (0..steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            [
                (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
                (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
                (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
            ]
        })
        .collect()
}

pub fn convert_rgb(file_path: &Path, is_folder: bool) -> Result<(), String> {
    shared::each_pdf(file_path, is_folder, |pdf, name| {
        let out = pdf.with_file_name(format!("{name}.mp4"));
        if out.exists() {
            return Ok(());
        }

        let text = shared::extract_text(pdf)?;
        let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();

        let mut colors: Vec<[u8; 3]> = Vec::new();
        for chunk in digits.as_bytes().chunks(9) {
            if chunk.len() == 9 {
                let s = std::str::from_utf8(chunk).unwrap_or("");
                let r = s[0..3].parse::<u8>().unwrap_or(0);
                let g = s[3..6].parse::<u8>().unwrap_or(0);
                let b = s[6..9].parse::<u8>().unwrap_or(0);
                colors.push([r, g, b]);
            }
        }
        if colors.is_empty() {
            return Err("No RGB color data found in PDF".into());
        }

        let mut interpolated: Vec<[u8; 3]> = Vec::new();
        for w in colors.windows(2) {
            interpolated.extend(lerp_color(w[0], w[1], 3000));
        }

        // Keep the existing approach (rawvideo pipe), but use ultrafast preset.
        let num_frames = 25 * 720;
        let gradient: Vec<[u8; 3]> = (0..num_frames)
            .map(|i| {
                let idx = (i * interpolated.len()) / num_frames;
                interpolated[idx.min(interpolated.len() - 1)]
            })
            .collect();

        let ffmpeg = shared::ffmpeg_bin();
        let preset = shared::ffmpeg_preset();

        if shared::verbose() {
            eprintln!("[cube] RGB: {} frames", gradient.len());
        }

        let mut child = Command::new(&ffmpeg)
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                if shared::verbose() { "info" } else { "error" },
                "-f", "rawvideo",
                "-pix_fmt", "rgb24",
                "-s", "520x520",
                "-r", "25",
                "-i", "pipe:0",
                "-c:v", "libx264",
                "-preset", &preset,
                "-pix_fmt", "yuv420p",
                out.to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| format!("failed to spawn '{ffmpeg}': {e} (os error {})", e.raw_os_error().unwrap_or(-1)))?;

        {
            let stdin = child.stdin.as_mut().ok_or("ffmpeg stdin not available")?;
            let mut raw = vec![0u8; 520 * 520 * 3];
            for color in &gradient {
                for px in raw.chunks_mut(3) {
                    px[0] = color[0];
                    px[1] = color[1];
                    px[2] = color[2];
                }
                stdin.write_all(&raw).map_err(|e| e.to_string())?;
            }
        }
        child.wait().map_err(|e| e.to_string())?;
        Ok(())
    })
}
